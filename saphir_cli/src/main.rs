use structopt::StructOpt;

mod docgen;
mod openapi;

use crate::docgen::DocGen;

type CommandResult = std::result::Result<(), String>;

trait Command: Sized {
    type Args;
    fn new(args: Self::Args) -> Self;
    fn run(&mut self) -> CommandResult;
}

/// Saphir web framework's CLI utility.
#[derive(StructOpt, Debug)]
struct SaphirCli {
    #[structopt(subcommand)]
    cmd: SaphirCliCommand,
}

#[derive(StructOpt, Debug)]
enum SaphirCliCommand {
    DocGen(<DocGen as Command>::Args),
}

fn main() {
    let cli = SaphirCli::from_args();
    if let Err(e) = match cli.cmd {
        SaphirCliCommand::DocGen(a) => {
            let mut doc = DocGen::new(a);
            doc.run()
        }
    } {
        eprintln!("{}", e);
    }
}
