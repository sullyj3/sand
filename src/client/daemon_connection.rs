use crate::sand::message::*;
use crate::sand::timer::TimerId;
use serde::Deserialize;
use std::io::{self, BufRead, BufReader, LineWriter, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

pub struct DaemonConnection {
    read: BufReader<UnixStream>,
    write: LineWriter<UnixStream>,
}

impl DaemonConnection {
    pub fn new(sock_path: impl AsRef<Path>) -> io::Result<Self> {
        let stream = UnixStream::connect(sock_path)?;

        let read = BufReader::new(stream.try_clone()?);
        let write = LineWriter::new(stream);

        Ok(Self { read, write })
    }

    pub fn add_timer(
        &mut self,
        duration: Duration,
        message: Option<String>,
    ) -> io::Result<StartTimerResponse> {
        self.send(Command::StartTimer { duration, message })?;
        self.recv::<StartTimerResponse>()
    }

    pub fn list(&mut self) -> io::Result<ListResponse> {
        self.send(Command::List)?;
        self.recv::<ListResponse>()
    }

    pub fn pause_timer(&mut self, timer_id: TimerId) -> io::Result<PauseTimerResponse> {
        self.send(Command::PauseTimer(timer_id))?;
        self.recv::<PauseTimerResponse>()
    }

    pub fn resume_timer(&mut self, timer_id: TimerId) -> io::Result<ResumeTimerResponse> {
        self.send(Command::ResumeTimer(timer_id))?;
        self.recv::<ResumeTimerResponse>()
    }

    pub fn cancel_timer(&mut self, timer_id: TimerId) -> io::Result<CancelTimerResponse> {
        self.send(Command::CancelTimer(timer_id))?;
        self.recv::<CancelTimerResponse>()
    }

    pub fn again(&mut self) -> io::Result<AgainResponse> {
        self.send(Command::Again)?;
        self.recv::<AgainResponse>()
    }

    fn send(&mut self, cmd: Command) -> io::Result<()> {
        let str = serde_json::to_string(&cmd).expect("failed to serialize Command {cmd}");
        writeln!(self.write, "{str}")
    }

    fn recv<T: for<'de> Deserialize<'de>>(&mut self) -> io::Result<T> {
        let mut recv_buf = String::with_capacity(128);
        let bytes_read = self.read.read_line(&mut recv_buf)?;

        if bytes_read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Connection to daemon closed before a response was received",
            ));
        }

        if let Ok(resp) = serde_json::from_str::<T>(&recv_buf) {
            return Ok(resp);
        }

        // `Response::Error` serializes as a bare JSON string, since `Response`
        // is `#[serde(untagged)]`. The daemon sends this when it couldn't
        // handle our request, e.g. a client/daemon protocol mismatch.
        if let Ok(err_msg) = serde_json::from_str::<String>(&recv_buf) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, err_msg));
        }

        Err(io::Error::other(format!(
            "failed to parse response from daemon: {recv_buf:?}"
        )))
    }
}
