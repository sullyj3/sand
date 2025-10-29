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

    pub fn add_timer(&mut self, duration: Duration) -> io::Result<AddTimerResponse> {
        self.send(Command::AddTimer {
            duration: duration.as_millis() as u64,
        })?;
        self.recv::<AddTimerResponse>()
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

    fn send(&mut self, cmd: Command) -> io::Result<()> {
        let str = serde_json::to_string(&cmd).expect("failed to serialize Command {cmd}");
        writeln!(self.write, "{str}")
    }

    fn recv<T: for<'de> Deserialize<'de>>(&mut self) -> io::Result<T> {
        let mut recv_buf = String::with_capacity(128);
        self.read.read_line(&mut recv_buf)?;
        let resp: T = serde_json::from_str(&recv_buf)
            .expect("Bug: failed to deserialize response from daemon");
        Ok(resp)
    }
}
