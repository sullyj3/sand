use crate::sand::message::Command;
use serde::Deserialize;
use std::io::{self, BufRead, BufReader, LineWriter, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

pub struct DaemonConnection {
    read: BufReader<UnixStream>,
    write: LineWriter<UnixStream>,
}

impl DaemonConnection {
    pub fn new(sock_path: PathBuf) -> io::Result<Self> {
        let stream = UnixStream::connect(sock_path)?;

        let read = BufReader::new(stream.try_clone()?);
        let write = LineWriter::new(stream);

        Ok(Self { read, write })
    }

    // TODO make private and expose functions corresponding to messages
    pub fn send(&mut self, cmd: Command) -> io::Result<()> {
        let str = serde_json::to_string(&cmd).expect("failed to serialize Command {cmd}");
        writeln!(self.write, "{str}")
    }

    // TODO make private
    pub fn recv<T: for<'de> Deserialize<'de>>(&mut self) -> io::Result<T> {
        let mut recv_buf = String::with_capacity(128);
        self.read.read_line(&mut recv_buf)?;
        let resp: T = serde_json::from_str(&recv_buf)
            .expect("Bug: failed to deserialize response from daemon");
        Ok(resp)
    }
}
