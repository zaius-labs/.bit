#![allow(clippy::useless_conversion)] // pyo3 proc macros generate .into() on PyErr

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Parse .bit source text, return JSON AST string.
#[pyfunction]
fn parse(source: &str) -> PyResult<String> {
    let doc =
        bit_core::parse_source(source).map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    serde_json::to_string_pretty(&doc).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Format .bit source text with consistent indentation.
#[pyfunction]
fn fmt(source: &str) -> PyResult<String> {
    bit_core::fmt(source).map_err(|e| PyValueError::new_err(format!("{}", e)))
}

/// Render a JSON AST back to .bit text.
#[pyfunction]
fn render(doc_json: &str) -> PyResult<String> {
    let doc: bit_core::Document =
        serde_json::from_str(doc_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(bit_core::render_doc(&doc))
}

/// Convert JSON to .bit text.
#[pyfunction]
fn from_json(json: &str) -> PyResult<String> {
    let doc = bit_core::from_json(json).map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    Ok(bit_core::render_doc(&doc))
}

/// Convert Markdown to .bit text.
#[pyfunction]
fn from_markdown(md: &str) -> PyResult<String> {
    let doc = bit_core::from_markdown(md).map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    Ok(bit_core::render_doc(&doc))
}

/// Convert .bit source to JSON string.
#[pyfunction]
fn to_json(source: &str) -> PyResult<String> {
    let doc =
        bit_core::parse_source(source).map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    bit_core::to_json(&doc).map_err(|e| PyValueError::new_err(format!("{}", e)))
}

/// Build document index as JSON string.
#[pyfunction]
fn build_index(source: &str) -> PyResult<String> {
    let doc =
        bit_core::parse_source(source).map_err(|e| PyValueError::new_err(format!("{}", e)))?;
    let idx = bit_core::build_index(&doc);
    serde_json::to_string_pretty(&idx).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pymodule]
fn bit_lang(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(fmt, m)?)?;
    m.add_function(wrap_pyfunction!(render, m)?)?;
    m.add_function(wrap_pyfunction!(from_json, m)?)?;
    m.add_function(wrap_pyfunction!(from_markdown, m)?)?;
    m.add_function(wrap_pyfunction!(to_json, m)?)?;
    m.add_function(wrap_pyfunction!(build_index, m)?)?;
    Ok(())
}
