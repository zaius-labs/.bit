use clap::Args;
use std::error::Error;

use super::super::read_input;

#[derive(Args)]
pub struct ParseArgs {
    /// File to parse (use "-" for stdin)
    pub file: String,

    /// Output compiled IR instead of AST
    #[arg(long)]
    pub ir: bool,
}

pub fn run(args: &ParseArgs) -> Result<(), Box<dyn Error>> {
    let source = read_input(&args.file)?;

    if args.ir {
        let ir = bit_core::compile(&source).map_err(|e| {
            eprintln!("error: {}", e);
            e
        })?;
        let json = serde_json::to_string_pretty(&ir)?;
        println!("{json}");
    } else {
        let doc = bit_core::parse_source(&source).map_err(|e| {
            eprintln!("error: {}", e);
            e
        })?;
        let json = serde_json::to_string_pretty(&doc)?;
        println!("{json}");
    }

    Ok(())
}
