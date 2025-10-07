use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use crate::sand;

#[derive(Args, Clone)]
pub struct DaemonArgs {}

// TODO: default to `Start` when no subcommand provided 
// https://stackoverflow.com/a/79564853
#[derive(Parser)]
#[clap(
    name = "sand",
    about = "Command line countdown timers that don't take up a terminal.",
    version
)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: CliCommand,
}

impl Cli {
    pub fn command(&self) -> CliCommand {
        self.command.clone()
    }
}

#[derive(Args, Clone)]
pub struct StartArgs {
    #[clap(
        name = "DURATION",
        value_parser = sand::duration::parse_duration_component,
        num_args = 1..,
        help = "One or more durations (e.g. 5m, 90, 1h 30m).",
            long_help = r#"The timer duration.

A duration is a number with an optional unit suffix. 
Provide multiple DURATION values to combine them (for example `1h 30m`).

Accepted suffixes:
  seconds: s, sec, secs, seconds
  minutes: m, min, mins, minutes
  hours:   h, hr, hrs, hours
  millis:  ms, milli, millis, milliseconds

When no suffix is present the value is interpreted as seconds."#
    )]
    pub durations: Vec<Duration>,
}

#[derive(Parser)]
#[clap(
        about = "Start timers when no subcommand is provided",
    long_about = r#"Start one or more timers by specifying durations directly.

Examples:
  sand 5m            Start a 5 minute timer
  sand 1h 30m        Start a 1 hour 30 minute timer
  sand 90            Start a 90 second timer (the 's' suffix is optional)"#
)]
pub struct CliDefault {
    #[command(flatten)]
    pub start: StartArgs,
}

#[derive(Subcommand, Clone)]
pub enum CliCommand {
    /// Start a new timer for the given duration
    Start(StartArgs),
    /// List active timers
    #[clap(alias = "list")]
    Ls,
    /// Pause the timer with the given ID
    Pause {
        timer_id: String,
    },
    /// Resume the timer with the given ID
    Resume {
        timer_id: String,
    },
    /// Cancel the timer with the given ID
    Cancel {
        timer_id: String,
    },
    Version,

    /// Launch the daemon
    Daemon(DaemonArgs),
}
