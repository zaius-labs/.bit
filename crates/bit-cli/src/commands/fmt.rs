use clap::Args;
use std::error::Error;
use std::fs;

use super::super::read_input;

#[derive(Args)]
pub struct FmtArgs {
    /// File to format (use "-" for stdin)
    pub file: String,

    /// Overwrite file in place
    #[arg(long)]
    pub write: bool,
}

pub fn run(args: &FmtArgs) -> Result<(), Box<dyn Error>> {
    let source = read_input(&args.file)?;

    let formatted = bit_core::fmt(&source).map_err(|e| {
        eprintln!("error: {}", e);
        e
    })?;

    if args.write && args.file != "-" {
        fs::write(&args.file, &formatted)?;
        eprintln!("formatted {}", args.file);
    } else {
        print!("{formatted}");
    }

    Ok(())
}
