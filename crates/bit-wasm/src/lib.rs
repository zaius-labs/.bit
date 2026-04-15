// bit-lang-wasm — WASM bindings for the .bit language

use wasm_bindgen::prelude::*;

/// Parse .bit source text into a JSON AST.
#[wasm_bindgen]
pub fn parse(source: &str) -> Result<JsValue, JsValue> {
    let doc = bit_core::parse_source(source).map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    serde_wasm_bindgen::to_value(&doc).map_err(|e| JsValue::from_str(&format!("{}", e)))
}

/// Compile .bit source to executable IR.
#[wasm_bindgen]
pub fn compile(source: &str) -> Result<JsValue, JsValue> {
    let ir = bit_core::compile(source).map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    serde_wasm_bindgen::to_value(&ir).map_err(|e| JsValue::from_str(&format!("{}", e)))
}

/// Format .bit source text with consistent indentation.
#[wasm_bindgen]
pub fn fmt(source: &str) -> Result<String, JsValue> {
    bit_core::fmt(source).map_err(|e| JsValue::from_str(&format!("{}", e)))
}

/// Render a Document (as JSON string) back to .bit text.
#[wasm_bindgen]
pub fn render(doc_json: &str) -> Result<String, JsValue> {
    let doc: bit_core::Document =
        serde_json::from_str(doc_json).map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    Ok(bit_core::render_doc(&doc))
}

/// Convert a JSON string to .bit text.
#[wasm_bindgen]
pub fn from_json(json: &str) -> Result<String, JsValue> {
    let doc = bit_core::from_json(json).map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;
    Ok(bit_core::render_doc(&doc))
}

/// Convert Markdown text to .bit text.
#[wasm_bindgen]
pub fn from_markdown(md: &str) -> Result<String, JsValue> {
    let doc = bit_core::from_markdown(md).map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;
    Ok(bit_core::render_doc(&doc))
}

/// Convert .bit source to a JSON string.
#[wasm_bindgen]
pub fn to_json(source: &str) -> Result<String, JsValue> {
    let doc = bit_core::parse_source(source).map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    bit_core::to_json(&doc).map_err(|e| JsValue::from_str(&format!("{:?}", e)))
}

/// Validate .bit source against a schema source. Returns JSON array of diagnostics.
#[wasm_bindgen]
pub fn validate(source: &str, schema_source: &str) -> Result<JsValue, JsValue> {
    let doc = bit_core::parse_source(source).map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    let schemas = bit_core::load_schemas(&[schema_source])
        .map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    let result = bit_core::validate_doc(&doc, &schemas);
    serde_wasm_bindgen::to_value(&result).map_err(|e| JsValue::from_str(&format!("{}", e)))
}

/// Build an index from .bit source (extract tasks, groups, entities). Returns JSON.
#[wasm_bindgen]
pub fn build_index(source: &str) -> Result<JsValue, JsValue> {
    let doc = bit_core::parse_source(source).map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    let idx = bit_core::build_index(&doc);
    serde_wasm_bindgen::to_value(&idx).map_err(|e| JsValue::from_str(&format!("{}", e)))
}
