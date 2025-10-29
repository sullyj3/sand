#![feature(iter_intersperse)]

use std::io;

use clap::Parser;
use sand::cli;
use sand::cli::CliCommand;

mod client;
mod daemon;
mod sand;

fn main() -> io::Result<()> {
    let cli = cli::Cli::parse();

    match cli.command() {
        CliCommand::Daemon(args) => daemon::main(args),
        CliCommand::ClientCommand(cmd) => client::main(cmd),
    }
}
