use std::io::{self, BufRead, BufReader, LineWriter, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use serde::Deserialize;

use crate::cli;
use crate::sand::cli::StartArgs;
use crate::sand::duration::DurationExt;
use crate::sand::message::{
    self, AddTimerResponse, Command, ListResponse, PauseTimerResponse, ResumeTimerResponse,
};
use crate::sand::socket;
use crate::sand::timer::{TimerId, TimerInfoForClient};

struct DaemonConnection {
    read: BufReader<UnixStream>,
    write: LineWriter<UnixStream>,
}

impl DaemonConnection {
    fn new(sock_path: PathBuf) -> io::Result<Self> {
        let stream = UnixStream::connect(sock_path)?;

        let read = BufReader::new(stream.try_clone()?);
        let write = LineWriter::new(stream);

        Ok(Self { read, write })
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

fn display_timer_info(timers: &[TimerInfoForClient]) -> String {
    if timers.len() == 0 {
        "No timers running.".into()
    } else {
        timers
            .iter()
            .map(TimerInfoForClient::display)
            .intersperse("\n".to_string())
            .collect()
    }
}

fn exit_timer_not_found(id: TimerId) -> ! {
    println!("Timer {id} not found.");
    exit(1)
}

pub fn main(cmd: cli::CliCommand) -> io::Result<()> {
    let Some(sock_path) = socket::get_sock_path() else {
        eprintln!("socket not provided and runtime directory does not exist.");
        eprintln!("no socket to use.");
        std::process::exit(1)
    };

    let mut conn = match DaemonConnection::new(sock_path) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Error establishing connection with daemon: {e}");
            std::process::exit(1);
        }
    };

    // TODO: make sure to parse Error Messages. we should prob move sending,
    // receiving, and parsing fully into DaemonConnection, and present
    // Command -> Result<CmdResponse, Error> type api
    match cmd {
        cli::CliCommand::Start(StartArgs { durations }) => {
            let dur: Duration = durations.iter().sum();
            conn.send(Command::AddTimer {
                duration: dur.as_millis() as u64,
            })?;
            let AddTimerResponse::Ok { id } = conn.recv::<AddTimerResponse>()?;

            let dur_string = dur.format_colon_separated();
            println!("Timer {id} created for {dur_string}.");
            Ok(())
        }
        cli::CliCommand::Ls => {
            conn.send(Command::List)?;
            let ListResponse::Ok { ref timers } = conn.recv::<ListResponse>()?;
            println!("{}", display_timer_info(timers));
            Ok(())
        }
        cli::CliCommand::Pause { timer_id } => {
            let timer_id = TimerId::parse_or_quit(&timer_id);
            conn.send(Command::PauseTimer(timer_id))?;
            match conn.recv::<PauseTimerResponse>()? {
                PauseTimerResponse::Ok => {
                    println!("Paused timer {timer_id}.");
                    Ok(())
                }
                PauseTimerResponse::TimerNotFound => exit_timer_not_found(timer_id),
                PauseTimerResponse::AlreadyPaused => {
                    println!("Timer {timer_id} is already paused.");
                    exit(1);
                }
            }
        }
        cli::CliCommand::Resume { timer_id } => {
            let timer_id = TimerId::parse_or_quit(&timer_id);
            conn.send(Command::ResumeTimer(timer_id))?;
            use ResumeTimerResponse as Resp;
            match conn.recv::<ResumeTimerResponse>()? {
                Resp::Ok => {
                    println!("Resumed timer {timer_id}.");
                    Ok(())
                }
                Resp::TimerNotFound => exit_timer_not_found(timer_id),
                Resp::AlreadyRunning => {
                    println!("Timer {timer_id} is already running.");
                    exit(1);
                }
            }
        }
        cli::CliCommand::Cancel { timer_id } => {
            let timer_id = TimerId::parse_or_quit(&timer_id);
            conn.send(Command::CancelTimer(timer_id))?;
            use message::CancelTimerResponse as Resp;
            match conn.recv::<Resp>()? {
                Resp::Ok => {
                    println!("Cancelled timer {timer_id}.");
                    Ok(())
                }
                Resp::TimerNotFound => exit_timer_not_found(timer_id),
            }
        }
        cli::CliCommand::Daemon(_) => unreachable!("handled in top level main"),
    }
}
