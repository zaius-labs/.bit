// bit-lang-core — .bit language parser, IR, interpreter, and toolkit

/// The .bit language schema — defines syntax, constructs, and field types.
/// Embedded as a system schema in bitstores via `@_system:schema`.
pub const LANGUAGE_SCHEMA: &str = include_str!("schema.bit");

// Foundation layer
pub mod bit_errors;
pub mod bit_types;
pub mod grid;
pub mod lex;
pub mod span;
pub mod trit;
pub mod tst;
pub mod types;

// Parser layer
pub mod format;
pub mod ir;
pub mod mutate;
pub mod parse;
pub mod render;
pub mod schema;

// Execution layer
pub mod context;
pub mod eval;
pub mod index;
pub mod interpret;
pub mod query;
pub mod validate;
pub mod workflow;

// Integration layer
pub mod check;
pub mod gate;

// Converters
pub mod convert;

// ── Public API ──────────────────────────────────────────────────

// Re-export key types for consumers
pub use index::DocIndex;
pub use ir::BitIR;
pub use mutate::RecordStore;
pub use schema::SchemaRegistry;
pub use span::ByteSpan;
pub use types::Document;
pub use types::ParseError;
pub use validate::ValidationResult;

/// Parse .bit source text into an AST Document.
pub fn parse_source(source: &str) -> Result<Document, ParseError> {
    parse::parse(source)
}

/// Compile .bit source into executable IR.
pub fn compile(source: &str) -> Result<BitIR, ParseError> {
    let doc = parse::parse(source)?;
    Ok(ir::BitIR::from_document(&doc))
}

/// Format .bit source text with consistent indentation.
pub fn fmt(source: &str) -> Result<String, ParseError> {
    let doc = parse::parse(source)?;
    Ok(format::format(&doc))
}

/// Render a Document AST back to .bit text.
pub fn render_doc(doc: &Document) -> String {
    render::render(doc)
}

/// Validate a document against schemas.
pub fn validate_doc(doc: &Document, schemas: &SchemaRegistry) -> ValidationResult {
    validate::validate(doc, schemas)
}

/// Build an index from a document (extract tasks, groups, entities).
pub fn build_index(doc: &Document) -> DocIndex {
    index::DocIndex::build(doc)
}

/// Load schemas from .bit source texts.
pub fn load_schemas(sources: &[&str]) -> Result<SchemaRegistry, ParseError> {
    let mut registry = SchemaRegistry::new();
    for source in sources {
        let doc = parse::parse(source)?;
        registry.extract_from_doc(&doc);
    }
    Ok(registry)
}

/// Convert a JSON string into a .bit Document.
pub fn from_json(json: &str) -> Result<Document, convert::ConvertError> {
    convert::from_json(json)
}

/// Convert Markdown text into a .bit Document.
pub fn from_markdown(md: &str) -> Result<Document, convert::ConvertError> {
    convert::from_markdown(md)
}

/// Convert a .bit Document into a JSON string.
pub fn to_json(doc: &Document) -> Result<String, convert::ConvertError> {
    convert::to_json(doc)
}
