use crate::{openapi::generate::Gen, Command, CommandResult};
use structopt::StructOpt;

mod generate;
mod schema;

/// OpenAPI v3 generation
///
/// See: https://github.com/OAI/OpenAPI-Specification/blob/master/versions/3.0.2.md
#[derive(StructOpt, Debug)]
pub(crate) struct OpenapiArgs {
    #[structopt(subcommand)]
    cmd: OpenapiCommand,
}

#[derive(StructOpt, Debug)]
pub(crate) enum OpenapiCommand {
    Gen(<Gen as Command>::Args),
}

pub(crate) struct Openapi {
    pub args: <Openapi as Command>::Args,
}

impl Command for Openapi {
    type Args = OpenapiArgs;

    fn new(args: Self::Args) -> Self {
        Self { args }
    }

    fn run<'b>(self) -> CommandResult {
        match self.args.cmd {
            OpenapiCommand::Gen(args) => {
                let gen = Gen::new(args);
                gen.run()?;
            }
        }
        Ok(())
    }
}
