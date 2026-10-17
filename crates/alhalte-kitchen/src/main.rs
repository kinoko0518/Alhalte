//! # alhalte-kitchen CLI binary

#![forbid(unsafe_code)]

use clap::Parser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = alhalte_kitchen::CliArgs::parse();
    alhalte_kitchen::run(args)?;
    Ok(())
}
