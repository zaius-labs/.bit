use clap::Args;
use std::error::Error;

use super::super::read_input;

#[derive(Args)]
pub struct RenderArgs {
    /// File to render (use "-" for stdin)
    pub file: String,
}

pub fn run(args: &RenderArgs) -> Result<(), Box<dyn Error>> {
    let source = read_input(&args.file)?;

    let doc = bit_core::parse_source(&source).map_err(|e| {
        eprintln!("error: {}", e);
        e
    })?;

    let output = bit_core::render_doc(&doc);
    print!("{output}");

    Ok(())
}
