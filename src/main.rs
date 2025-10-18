#![feature(iter_intersperse)]

use std::io;

use clap::Parser;
use sand::cli::CliCommand;
use sand::cli;

mod client;
mod daemon;
mod sand;

fn main() -> io::Result<()> {
    let cli = cli::Cli::parse();

    match cli.command() {
        CliCommand::Daemon(args) => daemon::main(args),
        cmd => {
            client::main(cmd)
        }
    }
}

#[cfg(test)]
mod tests {
    // intentionally failing test to ensure CI works
    #[test]
    fn intentionally_failing() {
        assert!(false);
    }
}