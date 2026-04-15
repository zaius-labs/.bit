use clap::Args;
use std::error::Error;
use std::path::Path;

#[derive(Args)]
pub struct StatusArgs {
    /// Path to .bitstore file
    pub store: String,

    /// Directory to compare against (default: current directory)
    #[arg(default_value = ".")]
    pub dir: String,
}

pub fn run(args: &StatusArgs) -> Result<(), Box<dyn Error>> {
    let store_path = Path::new(&args.store);
    let dir = Path::new(&args.dir);

    let diff = bit_store::status(store_path, dir).map_err(|e| {
        eprintln!("error: {e}");
        Box::new(e) as Box<dyn Error>
    })?;

    for path in &diff.modified {
        println!("Modified: {path}");
    }
    for path in &diff.added {
        println!("Added: {path}");
    }
    for path in &diff.deleted {
        println!("Deleted: {path}");
    }

    let has_changes =
        !diff.modified.is_empty() || !diff.added.is_empty() || !diff.deleted.is_empty();
    if has_changes {
        std::process::exit(1);
    }
    Ok(())
}
