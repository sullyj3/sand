mod daemon_connection;

use std::fmt::{self, Display, Formatter};
use std::io;
use std::time::Duration;

use crate::cli;
use crate::client::daemon_connection::DaemonConnection;
use crate::sand::cli::StartArgs;
use crate::sand::duration::DurationExt;
use crate::sand::message::*;
use crate::sand::socket;
use crate::sand::timer::{TimerId, TimerInfoForClient, TimerState};

#[derive(Debug)]
enum ClientError {
    Io(io::Error),
    TimerNotFound(TimerId),
    AlreadyPaused(TimerId),
    AlreadyRunning(TimerId),
}

impl Display for ClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Io(err) => write!(f, "I/O error: {}", err),
            ClientError::TimerNotFound(timer_id) => write!(f, "Timer {timer_id} not found."),
            ClientError::AlreadyPaused(timer_id) => {
                write!(f, "Timer {timer_id} is already paused.")
            }
            ClientError::AlreadyRunning(timer_id) => {
                write!(f, "Timer {timer_id} is already running.")
            }
        }
    }
}

impl From<io::Error> for ClientError {
    fn from(value: io::Error) -> Self {
        ClientError::Io(value)
    }
}

type ClientResult<T> = Result<T, ClientError>;

fn display_timer_info(mut timers: Vec<TimerInfoForClient>) -> String {
    if timers.len() == 0 {
        return "There are currently no timers.".into();
    };

    timers.sort_by(TimerInfoForClient::cmp_by_next_due);
    let (running, paused): (Vec<_>, Vec<_>) = timers
        .iter()
        .partition(|ti| ti.state == TimerState::Running);

    let first_column_width = {
        let max_id = timers
            .iter()
            .map(|ti| ti.id)
            .max()
            .expect("timers.len() != 0");
        max_id.to_string().len()
    };
    let mut output = String::new();
    if running.len() > 0 {
        display_timer_info_table(&mut output, first_column_width, &running);
        if paused.len() > 0 {
            output.push_str("\n");
        }
    }
    if paused.len() > 0 {
        display_timer_info_table(&mut output, first_column_width, &paused);
    }

    output
}

// Used separately for running and paused timers
// timers must be nonempty
fn display_timer_info_table(
    output: &mut String,
    first_column_width: usize,
    timers: &[&TimerInfoForClient],
) -> () {
    for timer in timers {
        output.push_str(&timer.display(first_column_width));
        output.push('\n');
    }
}

pub fn main(cmd: cli::CliCommand) -> io::Result<()> {
    let Some(sock_path) = socket::get_sock_path() else {
        eprintln!("socket not provided and runtime directory does not exist.");
        eprintln!("no socket to use.");
        std::process::exit(1)
    };

    let conn = match DaemonConnection::new(sock_path) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Error establishing connection with daemon: {e}");
            std::process::exit(1);
        }
    };

    match handle_cli_command(cmd, conn) {
        Ok(()) => Ok(()),
        Err(_) => std::process::exit(1),
    }
}

/// Handles a CLI command by sending it to the daemon and processing the response.
///
/// this function will handle all printing of success and errors. The returned result
/// does not need to be displayed, and is only used to determine the exit code.
fn handle_cli_command(cmd: cli::CliCommand, mut conn: DaemonConnection) -> ClientResult<()> {
    // TODO: for multi-id commands, it's a bit wack to only return one of the errors.
    // This needs to be re-worked somehow.

    // TODO: support passing multiple IDs in protocol
    match cmd {
        cli::CliCommand::Start(StartArgs { durations }) => {
            start(&mut conn, durations).inspect_err(|err| eprintln!("{err}"))
        }
        cli::CliCommand::Ls => ls(&mut conn).inspect_err(|err| eprintln!("{err}")),
        cli::CliCommand::Pause { timer_ids } => {
            let mut ret = Ok(());
            for result in timer_ids.iter().map(|id| pause(&mut conn, *id)) {
                if let Err(err) = result {
                    eprintln!("{err}");
                    ret = Err(err);
                }
            }
            ret
        }
        cli::CliCommand::Resume { timer_ids } => {
            let mut ret = Ok(());
            for result in timer_ids.iter().map(|id| resume(&mut conn, *id)) {
                if let Err(err) = result {
                    eprintln!("{err}");
                    ret = Err(err);
                }
            }
            ret
        }
        cli::CliCommand::Cancel { timer_ids } => {
            let mut ret = Ok(());
            for result in timer_ids.iter().map(|id| cancel(&mut conn, *id)) {
                if let Err(err) = result {
                    eprintln!("{err}");
                    ret = Err(err);
                }
            }
            ret
        }
        cli::CliCommand::Daemon(_) => unreachable!("handled in top level main"),
    }
}

fn start(conn: &mut DaemonConnection, durations: Vec<Duration>) -> ClientResult<()> {
    let dur: Duration = durations.iter().sum();
    let AddTimerResponse::Ok { id } = conn.add_timer(dur)?;

    let dur_string = dur.format_colon_separated();
    println!("Timer {id} created for {dur_string}.");
    Ok(())
}

fn ls(conn: &mut DaemonConnection) -> ClientResult<()> {
    let ListResponse::Ok { timers } = conn.list()?;
    print!("{}", display_timer_info(timers));
    Ok(())
}

fn pause(conn: &mut DaemonConnection, timer_id: TimerId) -> ClientResult<()> {
    match conn.pause_timer(timer_id)? {
        PauseTimerResponse::Ok => {
            println!("Paused timer {timer_id}.");
            Ok(())
        }
        PauseTimerResponse::TimerNotFound => Err(ClientError::TimerNotFound(timer_id)),
        PauseTimerResponse::AlreadyPaused => Err(ClientError::AlreadyPaused(timer_id)),
    }
}

fn resume(conn: &mut DaemonConnection, timer_id: TimerId) -> ClientResult<()> {
    use ResumeTimerResponse as Resp;
    match conn.resume_timer(timer_id)? {
        Resp::Ok => {
            println!("Resumed timer {timer_id}.");
            Ok(())
        }
        Resp::TimerNotFound => Err(ClientError::TimerNotFound(timer_id)),
        Resp::AlreadyRunning => Err(ClientError::AlreadyRunning(timer_id)),
    }
}

fn cancel(conn: &mut DaemonConnection, timer_id: TimerId) -> ClientResult<()> {
    use CancelTimerResponse as Resp;
    match conn.cancel_timer(timer_id)? {
        Resp::Ok => {
            println!("Cancelled timer {timer_id}.");
            Ok(())
        }
        Resp::TimerNotFound => Err(ClientError::TimerNotFound(timer_id)),
    }
}
