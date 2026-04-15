use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use typeaway_editor::classifier_v2::ClassifierV2;
use typeaway_editor::semantic_map::{SemanticMap, SpanRole};
use std::sync::Mutex;

struct Backend {
    client: Client,
    classifier: Mutex<ClassifierV2>,
    documents: Mutex<std::collections::HashMap<Url, String>>,
}

// Custom semantic token types matching our SpanRoles
const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,     // 0: Command
    SemanticTokenType::CLASS,       // 1: Object
    SemanticTokenType::PROPERTY,    // 2: Descriptor
    SemanticTokenType::NUMBER,      // 3: Value
    SemanticTokenType::VARIABLE,    // 4: Reference
    SemanticTokenType::COMMENT,     // 5: Connector
    SemanticTokenType::OPERATOR,    // 6: Relationship
];

fn role_to_token_type(role: &SpanRole) -> u32 {
    match role {
        SpanRole::Command => 0,
        SpanRole::Object => 1,
        SpanRole::Descriptor => 2,
        SpanRole::Value => 3,
        SpanRole::Reference => 4,
        SpanRole::Connector => 5,
        SpanRole::Relationship => 6,
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: TOKEN_TYPES.to_vec(),
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "Zaius LSP initialized").await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.documents.lock().unwrap().insert(uri, text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().last() {
            self.documents.lock().unwrap().insert(uri, change.text);
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let docs = self.documents.lock().unwrap();
        let text = match docs.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        let mut map = SemanticMap::new();
        self.classifier.lock().unwrap().classify(&text, &mut map);

        // Convert annotations to LSP semantic tokens
        // LSP tokens are encoded as deltas: [deltaLine, deltaStart, length, tokenType, tokenModifiers]
        let mut tokens: Vec<SemanticToken> = Vec::new();
        let mut prev_line = 0u32;
        let mut prev_start = 0u32;

        // Sort annotations by position
        let mut annotations: Vec<_> = map.all_annotations().to_vec();
        annotations.sort_by_key(|a| (a.span.0, a.span.1));

        for ann in &annotations {
            // Convert byte offset to line/column
            let (line, col) = offset_to_line_col(&text, ann.span.0);
            let length = (ann.span.1 - ann.span.0) as u32;

            let delta_line = line - prev_line;
            let delta_start = if delta_line == 0 { col - prev_start } else { col };

            tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type: role_to_token_type(&ann.role),
                token_modifiers_bitset: 0,
            });

            prev_line = line;
            prev_start = col;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }
}

fn offset_to_line_col(text: &str, offset: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if i >= offset { break; }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        classifier: Mutex::new(ClassifierV2::new()),
        documents: Mutex::new(std::collections::HashMap::new()),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
