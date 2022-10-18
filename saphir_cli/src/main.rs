#![allow(clippy::match_like_matches_macro)]
use clap::{Parser, Subcommand};

mod openapi;

use crate::openapi::Openapi;

type CommandResult = std::result::Result<(), String>;

pub(crate) trait Command: Sized {
    type Args;
    fn new(args: Self::Args) -> Self;
    fn run(self) -> CommandResult;
}

/// Saphir web framework's CLI utility.
#[derive(Parser, Debug)]
#[command(name = "saphir")]
#[command(bin_name = "saphir")]
// #[command(about = "Saphir web framework's CLI utility.", long_about = None)]
struct SaphirCli {
    #[command(subcommand)]
    cmd: SaphirCliCommand,
}

#[derive(Subcommand, Debug)]
enum SaphirCliCommand {
    Openapi(<Openapi as Command>::Args),
}

fn main() {
    let cli = SaphirCli::parse();
    if let Err(e) = match cli.cmd {
        SaphirCliCommand::Openapi(args) => {
            let openapi = Openapi::new(args);
            openapi.run()
        }
    } {
        eprintln!("{}", e);
    }
}
