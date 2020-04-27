use structopt::StructOpt;

mod docgen;
mod openapi;

use self::docgen::{DocGenArgs};
use std::error::Error;
use std::fmt::Display;
use futures::future::BoxFuture;
use crate::docgen::DocGen;

type CommandResult = std::result::Result<(), String>;

trait Command {
    type Args;
    fn new(args: Self::Args) -> Self;
    fn run<'a>(self) -> BoxFuture<'a, CommandResult>;
}

/// Saphir web framework's CLI utility.
#[derive(StructOpt, Debug)]
struct SaphirCli {
    #[structopt(subcommand)]
    cmd: SaphirCliCommand
}

#[derive(StructOpt, Debug)]
enum SaphirCliCommand {
    DocGen(<DocGen as Command>::Args),
}

#[tokio::main]
async fn main() {
    let cli = SaphirCli::from_args();
    if let Err(e) = match cli.cmd {
        SaphirCliCommand::DocGen(a) => {
            let doc = DocGen::new(a);
            doc.run().await
        },
    } {
        eprintln!("{}", e);
    }
}