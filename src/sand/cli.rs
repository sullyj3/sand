use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use indoc::indoc;

use crate::sand::{self, timer::TimerId};

#[derive(Args, Clone)]
pub struct DaemonArgs {}

// TODO better header formatting to match the clap generated help
const AFTER_HELP: &str = indoc! {"
SHORTER COMMAND ALIASES
    You can use any prefix of a subcommand, as long as it is unambiguous. For
    example:
        `sand s 10m` is equivalent to `sand start 10m`.
        `sand l` is equivalent to `sand list`.
        `sand p 1` is equivalent to `sand pause 1`.

CUSTOM SOUNDS
    To use a custom timer sound, place an audio file at

        $XDG_DATA_HOME/sand-timer/timer_sound.{mp3,wav,flac,aac,m4a}.

    XDG_DATA_HOME defaults to ~/.local/share/"
};

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
    /// Launch the daemon
    Daemon(DaemonArgs),

    #[clap(flatten)]
    ClientCommand(ClientCommand),
}

#[derive(Subcommand, Clone)]
pub enum ClientCommand {
    /// Start a new timer for the given duration
    Start(StartArgs),
    /// List active timers
    #[clap(alias = "list")]
    Ls,
    /// Show the next due running timer if any, exit failure otherwise.
    ///
    /// This can be useful for scripting or usage in other programs, such as
    /// starship.
    NextDue,
    /// Pause the timers with the given IDs
    Pause { timer_ids: Vec<TimerId> },
    /// Resume the timers with the given IDs
    Resume { timer_ids: Vec<TimerId> },
    /// Cancel the timers with the given IDs
    Cancel { timer_ids: Vec<TimerId> },
    /// Start a new timer with the same duration as the most recently started one.
    Again,
}
