use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::Url;
use bit_nl_core::{CompileResult, SpanIndex, UserProfile};

pub struct DocumentState {
    pub uri: Url,
    pub text: String,
    pub compile_result: Option<CompileResult>,
    pub profile: UserProfile,
    pub span_index: SpanIndex,
    pub last_compile: std::time::Instant,
}

impl DocumentState {
    pub fn new(uri: Url, text: String) -> Self {
        Self {
            uri,
            text,
            compile_result: None,
            profile: UserProfile::new(),
            span_index: SpanIndex::new(),
            last_compile: std::time::Instant::now(),
        }
    }
}

pub struct Backend {
    pub client: tower_lsp::Client,
    pub documents: Arc<RwLock<HashMap<Url, DocumentState>>>,
    pub models_dir: PathBuf,
}
