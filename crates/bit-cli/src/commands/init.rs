use clap::Args;
use std::error::Error;
use std::path::Path;

#[derive(Args)]
pub struct InitArgs {
    /// Directory to initialize (default: current directory)
    #[arg(default_value = ".")]
    pub dir: String,
}

pub fn run(args: &InitArgs) -> Result<(), Box<dyn Error>> {
    let dir = Path::new(&args.dir);
    std::fs::create_dir_all(dir)?;

    // Write schema.bit from embedded language schema
    let schema_dest = dir.join("schema.bit");
    if !schema_dest.exists() {
        std::fs::write(&schema_dest, bit_core::LANGUAGE_SCHEMA)?;
    }

    eprintln!(
        "Initialized .bit project in {}",
        dir.canonicalize()?.display()
    );
    Ok(())
}
