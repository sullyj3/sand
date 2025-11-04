use std::time::Instant;

use crate::sand::message::ListResponse;
use crate::sand::message::StartTimerResponse;
use crate::sand::message::{Command, Response};
use serde_json::Error;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::LinesStream;

use super::ctx::DaemonCtx;

async fn handle_command(cmd: Command, ctx: &DaemonCtx) -> Response {
    let now = Instant::now();
    match cmd {
        Command::List => ListResponse::ok(ctx.get_timerinfo_for_client(now)).into(),
        Command::StartTimer { duration } => {
            StartTimerResponse::ok(ctx.start_timer(now, duration).await).into()
        }
        Command::PauseTimer(id) => ctx.pause_timer(id, now).into(),
        Command::ResumeTimer(id) => ctx.resume_timer(id, now).into(),
        Command::CancelTimer(id) => ctx.cancel_timer(id, now).into(),
        Command::Again => ctx.again(now).await.into(),
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
