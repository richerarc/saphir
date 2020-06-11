use structopt::StructOpt;

mod openapi;

use crate::openapi::Openapi;

type CommandResult = std::result::Result<(), String>;

pub(crate) trait Command: Sized {
    type Args;
    fn new(args: Self::Args) -> Self;
    fn run(self) -> CommandResult;
}

/// Saphir web framework's CLI utility.
#[derive(StructOpt, Debug)]
struct SaphirCli {
    #[structopt(subcommand)]
    cmd: SaphirCliCommand,
}

#[derive(StructOpt, Debug)]
enum SaphirCliCommand {
    Openapi(<Openapi as Command>::Args),
}

fn main() {
    let cli = SaphirCli::from_args();
    if let Err(e) = match cli.cmd {
        SaphirCliCommand::Openapi(args) => {
            let openapi = Openapi::new(args);
            openapi.run()
        }
    } {
        eprintln!("{}", e);
    }
}
