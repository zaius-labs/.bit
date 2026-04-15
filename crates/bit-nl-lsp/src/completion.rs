use lsp_types::{CompletionParams, CompletionResponse};

/// Stub: returns None — full implementation is PR17.
pub fn completions(_params: &CompletionParams) -> Option<CompletionResponse> {
    // TODO(PR17): implement NL→.bit completions
    None
}
