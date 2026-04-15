use clap::Args;
use std::error::Error;

use super::super::read_input;

#[derive(Args)]
pub struct ValidateArgs {
    /// File to validate (use "-" for stdin)
    pub file: String,

    /// Schema file to validate against
    #[arg(long)]
    pub schema: Option<String>,
}

pub fn run(args: &ValidateArgs) -> Result<(), Box<dyn Error>> {
    let source = read_input(&args.file)?;

    let doc = bit_core::parse_source(&source).map_err(|e| {
        eprintln!("parse error: {}", e);
        e
    })?;

    // Build schema registry: external schema file if provided, plus self-extracted definitions
    let mut schemas = if let Some(schema_path) = &args.schema {
        let schema_source = read_input(schema_path)?;
        bit_core::load_schemas(&[schema_source.as_str()])?
    } else {
        bit_core::SchemaRegistry::new()
    };
    schemas.extract_from_doc(&doc);

    let result = bit_core::validate_doc(&doc, &schemas);

    for w in &result.warnings {
        eprintln!("warning: {w}");
    }
    for e in &result.errors {
        eprintln!("error[{}]: {} ({})", e.code, e.message, e.kind);
    }

    if !result.valid() {
        eprintln!(
            "{} error(s), {} warning(s)",
            result.errors.len(),
            result.warnings.len()
        );
        std::process::exit(1);
    }

    eprintln!("ok");
    Ok(())
}
