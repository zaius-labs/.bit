use clap::Args;
use std::error::Error;

use super::super::read_input;

#[derive(Args)]
pub struct ConvertArgs {
    /// File to convert (use "-" for stdin)
    pub file: String,

    /// Input format (auto-detected from extension if not set)
    #[arg(long)]
    pub from: Option<String>,

    /// Output format (default: bit)
    #[arg(long, default_value = "bit")]
    pub to: String,
}

pub fn run(args: &ConvertArgs) -> Result<(), Box<dyn Error>> {
    let format = match &args.from {
        Some(f) => f.clone(),
        None => {
            if args.file == "-" {
                eprintln!("error: --from is required when reading from stdin");
                return Err("missing --from".into());
            }
            detect_format(&args.file).ok_or_else(|| {
                eprintln!("error: cannot detect format from extension, use --from");
                "unknown format"
            })?
        }
    };

    let input = read_input(&args.file)?;

    let doc = match format.as_str() {
        "json" => bit_core::from_json(&input).map_err(|e| {
            eprintln!("error: {}", e);
            Box::new(e) as Box<dyn Error>
        })?,
        "md" | "markdown" => bit_core::from_markdown(&input).map_err(|e| {
            eprintln!("error: {}", e);
            Box::new(e) as Box<dyn Error>
        })?,
        other => {
            eprintln!("error: unsupported input format '{}'", other);
            return Err(format!("unsupported format: {}", other).into());
        }
    };

    match args.to.as_str() {
        "bit" => {
            let output = bit_core::render_doc(&doc);
            print!("{output}");
        }
        "json" => {
            let output = serde_json::to_string_pretty(&doc)?;
            println!("{output}");
        }
        other => {
            eprintln!("error: unsupported output format '{}'", other);
            return Err(format!("unsupported output format: {}", other).into());
        }
    }

    Ok(())
}

fn detect_format(path: &str) -> Option<String> {
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    match ext {
        "json" => Some("json".to_string()),
        "md" | "markdown" => Some("md".to_string()),
        "toml" => Some("toml".to_string()),
        _ => None,
    }
}
