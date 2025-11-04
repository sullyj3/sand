use std::time::Duration;
use std::time::Instant;

use crate::sand::message::AgainResponse;
use crate::sand::message::CancelTimerResponse;
use crate::sand::message::ListResponse;
use crate::sand::message::PauseTimerResponse;
use crate::sand::message::ResumeTimerResponse;
use crate::sand::message::StartTimerResponse;
use crate::sand::message::{Command, Response};
use crate::sand::timer::TimerId;
use serde_json::Error;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::LinesStream;

use super::ctx::DaemonCtx;

struct CmdHandlerCtx {
    now: Instant,
    state: DaemonCtx,
}

impl CmdHandlerCtx {
    fn new(state: DaemonCtx) -> Self {
        let now = Instant::now();
        Self { now, state }
    }

    fn list(&self) -> ListResponse {
        ListResponse::ok(self.state.get_timerinfo_for_client(self.now))
    }

    async fn start_timer(&self, duration: u64) -> StartTimerResponse {
        let duration = Duration::from_millis(duration);
        let id = self.state.start_timer(self.now, duration).await;
        StartTimerResponse::ok(id)
    }

    fn pause_timer(&self, id: TimerId) -> PauseTimerResponse {
        self.state.pause_timer(id, self.now)
    }

    fn resume_timer(&self, id: TimerId) -> ResumeTimerResponse {
        self.state.resume_timer(id, self.now)
    }

    fn cancel_timer(&self, id: TimerId) -> CancelTimerResponse {
        self.state.cancel_timer(id, self.now)
    }

    async fn again(&self) -> AgainResponse {
        self.state.again(self.now).await
    }
}

async fn handle_command(cmd: Command, state: &DaemonCtx) -> Response {
    let ctx = CmdHandlerCtx::new(state.clone());
    match cmd {
        Command::List => ctx.list().into(),
        Command::StartTimer { duration } => ctx.start_timer(duration).await.into(),
        Command::PauseTimer(id) => ctx.pause_timer(id).into(),
        Command::ResumeTimer(id) => ctx.resume_timer(id).into(),
        Command::CancelTimer(id) => ctx.cancel_timer(id).into(),
        Command::Again => ctx.again().await.into(),
    }
}

pub async fn handle_client(mut stream: UnixStream, state: DaemonCtx) {
    log::debug!("Handling client.");

    let (read_half, mut write_half) = stream.split();

    let br = BufReader::new(read_half);

    let mut lines = LinesStream::new(br.lines());

    while let Some(rline) = lines.next().await {
        let line: String = match rline {
            Ok(line) => line,
            Err(e) => {
                log::error!("Error reading line from client: {e}");
                continue;
            }
        };
        let line: &str = line.trim();
        let rcmd: Result<Command, Error> = serde_json::from_str(&line);

        let resp: Response = match rcmd {
            Ok(cmd) => handle_command(cmd, &state).await,
            Err(e) => {
                let err_msg: String = format!("Failed to parse client message as Command: {e}");
                log::error!("{err_msg}");
                Response::Error(err_msg)
            }
        };
        let mut resp_str: String = serde_json::to_string(&resp).unwrap();
        resp_str.push('\n');
        write_half.write_all(resp_str.as_bytes()).await.unwrap();
    }

    log::debug!("Client disconnected");
}
