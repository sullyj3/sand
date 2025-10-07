#![feature(iter_intersperse)]

use std::io;

use clap::{Parser, CommandFactory};
use sand::cli::CliCommand;
use sand::cli;

mod client;
mod daemon;
mod sand;

fn main() -> io::Result<()> {
    // All this nonsense is to support treating invocations like `sand 5m` as `sand start 5m`

    // Try parsing the full CLI. If that fails with InvalidSubcommand, parse the
    // fallback `CliDefault` which flattens `StartArgs` and map it into a `Cli`.
    let cli_result = cli::Cli::try_parse().or_else(|err| {
        use clap::error::ErrorKind;

        if let ErrorKind::InvalidSubcommand = err.kind() {
            // Try the fallback parser which expects bare start args
            match sand::cli::CliDefault::try_parse() {
                Ok(default) => {
                    return Ok(cli::Cli {
                        command: sand::cli::CliCommand::Start(default.start),
                    });
                }
                Err(_) => {
                    // Check if any arguments contain numbers
                    let args: Vec<String> = std::env::args().skip(1).collect();
                    if args.iter().any(|arg| arg.chars().any(|c| c.is_numeric())) {
                        // If numbers are found, show usage for Start
                        let mut start_help = Vec::new();
                        let _ = cli::CliDefault::command().write_long_help(&mut start_help);
                        eprintln!("{}", String::from_utf8_lossy(&start_help));
                        std::process::exit(1);
                    }
                }
            }
        }

        // If no numbers are found or another error occurs, return the original error
        Err(err)
    });

    match cli_result {
        Ok(cli) => match cli.command() {
            CliCommand::Version => {
                println!("{}", sand::VERSION);
                Ok(())
            }
            CliCommand::Daemon(args) => daemon::main(args),
            cmd => client::main(cmd),
        },
        Err(e) => {
            // If the error is a help/version display, forward it and exit
            use clap::error::ErrorKind;
            match e.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    e.print()?;
                    std::process::exit(0);
                }
                _ => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
