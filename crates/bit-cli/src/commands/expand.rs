use clap::Args;
use std::error::Error;
use std::path::Path;

#[derive(Args)]
pub struct ExpandArgs {
    /// Path to .bitstore file
    pub store: String,

    /// Output directory for expanded files
    #[arg(short, long, default_value = ".")]
    pub output: String,
}

pub fn run(args: &ExpandArgs) -> Result<(), Box<dyn Error>> {
    let store_path = Path::new(&args.store);
    let output = Path::new(&args.output);

    let count = bit_store::expand(store_path, output).map_err(|e| {
        eprintln!("error: {e}");
        e
    })?;
    eprintln!("Expanded {count} files to {}", output.display());

    Ok(())
}
