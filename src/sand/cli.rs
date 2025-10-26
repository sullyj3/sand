use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use crate::sand::{self, timer::TimerId};

#[derive(Args, Clone)]
pub struct DaemonArgs {}

const AFTER_HELP: &str = "To use a custom timer sound, place a flac at

    $XDG_DATA_HOME/sand-timer/timer_sound.flac.

XDG_DATA_HOME defaults to ~/.local/share/";

#[derive(Parser)]
#[command(
    name = "sand",
    about = "Command line countdown timers that don't take up a terminal.",
    after_help = AFTER_HELP,
    infer_subcommands = true,
    version
)]
pub struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

impl Cli {
    pub fn command(&self) -> CliCommand {
        self.command.clone()
    }
}

#[derive(Args, Clone)]
pub struct StartArgs {
    #[clap(name = "DURATION", value_parser = sand::duration::parse_duration_component, num_args = 1..)]
    pub durations: Vec<Duration>,
}

#[derive(Subcommand, Clone)]
pub enum CliCommand {
    /// Start a new timer for the given duration
    Start(StartArgs),
    /// List active timers
    #[clap(alias = "list")]
    Ls,
    /// Pause the timer with the given ID
    Pause { timer_id: TimerId },
    /// Resume the timer with the given ID
    Resume { timer_id: TimerId },
    /// Cancel the timer with the given ID
    Cancel { timer_id: TimerId },

    /// Launch the daemon
    Daemon(DaemonArgs),
}
