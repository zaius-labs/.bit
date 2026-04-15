use clap::Args;
use std::error::Error;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub struct CollapseArgs {
    /// Source directory containing .bit files
    #[arg(default_value = ".")]
    pub source: String,

    /// Output .bitstore file path
    #[arg(short, long)]
    pub output: Option<String>,
}

pub fn run(args: &CollapseArgs) -> Result<(), Box<dyn Error>> {
    let source = Path::new(&args.source);
    let output: PathBuf = match &args.output {
        Some(p) => PathBuf::from(p),
        None => {
            let dir_name = source
                .canonicalize()?
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "store".to_string());
            PathBuf::from(format!("{dir_name}.bitstore"))
        }
    };

    let mut store = bit_store::collapse(source, &output).map_err(|e| {
        eprintln!("error: {e}");
        e
    })?;
    let info = store.info()?;
    eprintln!(
        "Collapsed {} blobs into {} ({} pages)",
        info.blob_count,
        output.display(),
        info.page_count
    );
    Ok(())
}
