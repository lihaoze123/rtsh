use std::io::stdout;

use clap::Parser;
use nix::unistd::dup2_stderr;

use crate::{shell::Shell, signal::SignalSource};

mod jobs;
mod parser;
mod shell;
mod signal;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    // emit additional diagnostic info
    #[arg(short, long)]
    verbose: bool,

    // don't print a prompt
    #[arg(short = 'p', long)]
    dont_emit_prompt: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    dup2_stderr(stdout())?;

    let signals = SignalSource::new()?;
    let mut shell = Shell::new(signals);

    shell.run(!args.dont_emit_prompt)
}
