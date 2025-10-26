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

/////////////////////////////////////////////////////////////////////////////////////////
// Main
/////////////////////////////////////////////////////////////////////////////////////////

pub fn main(cli_cmd: cli::CliCommand) -> io::Result<()> {
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

    // TODO: for multi-id commands, it's a bit wack to only return one of the errors.
    // This needs to be re-worked somehow.

    // TODO: support passing multiple IDs in protocol
    let result: ClientResult<()> = match cli_cmd {
        cli::CliCommand::Start(StartArgs { durations }) => {
            start(&mut conn, durations).inspect_err(|err| eprintln!("{err}"))
        }
        cli::CliCommand::Ls => ls(&mut conn),
        cli::CliCommand::Pause { timer_ids } => pause(&mut conn, timer_ids),
        cli::CliCommand::Resume { timer_ids } => resume(&mut conn, timer_ids),
        cli::CliCommand::Cancel { timer_ids } => cancel(&mut conn, timer_ids),
        cli::CliCommand::Daemon(_) => unreachable!("handled in top level main"),
    };
    // the individual command handler functions do all printing of success and
    // errors. The result does not need to be displayed, and is only used to determine the exit code.
    match result {
        Ok(()) => Ok(()),
        Err(_) => std::process::exit(1),
    }
}

/////////////////////////////////////////////////////////////////////////////////////////
// Command handler functions
/////////////////////////////////////////////////////////////////////////////////////////

fn start(conn: &mut DaemonConnection, durations: Vec<Duration>) -> ClientResult<()> {
    let dur: Duration = durations.iter().sum();
    let AddTimerResponse::Ok { id } = conn.add_timer(dur)?;

    let dur_string = dur.format_colon_separated();
    println!("Timer {id} created for {dur_string}.");
    Ok(())
}

fn ls(conn: &mut DaemonConnection) -> ClientResult<()> {
    match conn.list() {
        Ok(resp) => {
            let ListResponse::Ok { timers } = resp;
            print!("{}", display_timer_info(timers));
            Ok(())
        }
        Err(err) => {
            eprintln!("{err}");
            Err(err.into())
        }
    }
}

fn pause(conn: &mut DaemonConnection, timer_ids: Vec<TimerId>) -> ClientResult<()> {
    let mut ret = Ok(());
    for timer_id in timer_ids {
        let result = match conn.pause_timer(timer_id)? {
            PauseTimerResponse::Ok => Ok(()),
            PauseTimerResponse::TimerNotFound => Err(ClientError::TimerNotFound(timer_id)),
            PauseTimerResponse::AlreadyPaused => Err(ClientError::AlreadyPaused(timer_id)),
        };

        match result {
            Err(err) => {
                eprintln!("{err}");
                ret = Err(err);
            }
            Ok(()) => println!("Paused timer {timer_id}."),
        }
    }
    ret
}

fn resume(conn: &mut DaemonConnection, timer_ids: Vec<TimerId>) -> ClientResult<()> {
    let mut ret = Ok(());
    for timer_id in timer_ids {
        let result = match conn.resume_timer(timer_id)? {
            ResumeTimerResponse::Ok => Ok(()),
            ResumeTimerResponse::TimerNotFound => Err(ClientError::TimerNotFound(timer_id)),
            ResumeTimerResponse::AlreadyRunning => Err(ClientError::AlreadyRunning(timer_id)),
        };

        match result {
            Err(err) => {
                eprintln!("{err}");
                ret = Err(err);
            }
            Ok(()) => println!("Resumed timer {timer_id}."),
        }
    }
    ret
}

fn cancel(conn: &mut DaemonConnection, timer_ids: Vec<TimerId>) -> ClientResult<()> {
    let mut ret = Ok(());
    for timer_id in timer_ids {
        let result = match conn.cancel_timer(timer_id)? {
            CancelTimerResponse::Ok => Ok(()),
            CancelTimerResponse::TimerNotFound => Err(ClientError::TimerNotFound(timer_id)),
        };

        match result {
            Err(err) => {
                eprintln!("{err}");
                ret = Err(err);
            }
            Ok(()) => println!("Cancelled timer {timer_id}."),
        }
    }
    ret
}

/////////////////////////////////////////////////////////////////////////////////////////
// Helpers
/////////////////////////////////////////////////////////////////////////////////////////

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
