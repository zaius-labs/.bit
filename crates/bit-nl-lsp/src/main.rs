mod state;
mod tokens;
mod hover;
mod diagnostics;
mod inlay;
mod actions;
mod completion;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use bit_nl_core::{compile, position_to_offset, offset_to_position};
use state::{Backend, DocumentState};
use tokens::{TOKEN_TYPES, TOKEN_MODIFIERS, segment_kind_to_token_type, segment_to_modifiers};

/// Apply an LSP incremental text change to a mutable string.
fn apply_change(text: &mut String, change: &TextDocumentContentChangeEvent) {
    if let Some(range) = change.range {
        let start = position_to_offset(text, range.start.line, range.start.character);
        let end = position_to_offset(text, range.end.line, range.end.character);
        text.replace_range(start..end, &change.text);
    } else {
        *text = change.text.clone();
    }
}

/// Compile a document and publish diagnostics.
async fn compile_and_publish(client: &Client, uri: &Url, text: &str, documents: &Arc<RwLock<HashMap<Url, DocumentState>>>) {
    let text_owned = text.to_string();
    let uri_clone = uri.clone();

    let result = tokio::task::spawn_blocking(move || {
        compile(&text_owned, None)
    }).await;

    match result {
        Ok(compile_result) => {
            // Build diagnostics before moving compile_result
            let diags = diagnostics::compile_result_to_diagnostics(&compile_result, text);
            let span_index = compile_result.span_index.clone();

            {
                let mut docs = documents.write().await;
                if let Some(state) = docs.get_mut(uri) {
                    state.span_index = span_index;
                    state.last_compile = std::time::Instant::now();
                    state.compile_result = Some(compile_result);
                }
            }

            client
                .publish_diagnostics(uri_clone, diags, None)
                .await;
        }
        Err(_) => {
            // spawn_blocking panicked — publish empty diagnostics
            client.publish_diagnostics(uri_clone, vec![], None).await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "bit-nl-lsp".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        SemanticTokensRegistrationOptions {
                            text_document_registration_options: TextDocumentRegistrationOptions {
                                document_selector: Some(vec![DocumentFilter {
                                    language: Some("nl".to_string()),
                                    scheme: None,
                                    pattern: Some("**/*.nl".to_string()),
                                }]),
                            },
                            semantic_tokens_options: SemanticTokensOptions {
                                work_done_progress_options: Default::default(),
                                legend: SemanticTokensLegend {
                                    token_types: TOKEN_TYPES.to_vec(),
                                    token_modifiers: TOKEN_MODIFIERS.to_vec(),
                                },
                                range: Some(true),
                                full: Some(SemanticTokensFullOptions::Bool(true)),
                            },
                            static_registration_options: Default::default(),
                        },
                    ),
                ),
                inlay_hint_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("bit-nl".to_string()),
                        inter_file_dependencies: false,
                        workspace_diagnostics: false,
                        work_done_progress_options: Default::default(),
                    },
                )),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["@".to_string(), ":".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "bit-nl-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();

        {
            let mut docs = self.documents.write().await;
            docs.insert(uri.clone(), DocumentState::new(uri.clone(), text.clone()));
        }

        compile_and_publish(&self.client, &uri, &text, &self.documents).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        let text = {
            let mut docs = self.documents.write().await;
            if let Some(state) = docs.get_mut(&uri) {
                for change in &params.content_changes {
                    apply_change(&mut state.text, change);
                }
                state.text.clone()
            } else {
                return;
            }
        };

        // 50ms debounce
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        compile_and_publish(&self.client, &uri, &text, &self.documents).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
        // Clear diagnostics on close
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let docs = self.documents.read().await;
        let state = match docs.get(&uri) {
            Some(s) => s,
            None => return Ok(None),
        };
        let compile_result = match &state.compile_result {
            Some(r) => r,
            None => return Ok(None),
        };

        let tokens = encode_semantic_tokens(&compile_result.segments, &state.text);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let docs = self.documents.read().await;
        let state = match docs.get(&uri) {
            Some(s) => s,
            None => return Ok(None),
        };
        let compile_result = match &state.compile_result {
            Some(r) => r,
            None => return Ok(None),
        };

        // Filter segments that fall within the requested range
        let filtered: Vec<_> = compile_result
            .segments
            .iter()
            .filter(|seg| {
                let (sl, sc) = offset_to_position(&state.text, seg.segment.span.start);
                let seg_pos = Position { line: sl, character: sc };
                position_in_range(&seg_pos, &range)
            })
            .cloned()
            .collect();

        let tokens = encode_semantic_tokens(&filtered, &state.text);
        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }

    async fn inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let docs = self.documents.read().await;
        let state = match docs.get(&uri) {
            Some(s) => s,
            None => return Ok(None),
        };
        let compile_result = match &state.compile_result {
            Some(r) => r,
            None => return Ok(None),
        };

        let hints = inlay::build_inlay_hints(&compile_result.segments, &state.text, &range);
        Ok(Some(hints))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        let state = match docs.get(&uri) {
            Some(s) => s,
            None => return Ok(None),
        };
        let compile_result = match &state.compile_result {
            Some(r) => r,
            None => return Ok(None),
        };

        let offset = position_to_offset(&state.text, pos.line, pos.character) as u32;
        let construct_id = state.span_index.find_nl_construct(offset);
        let construct_id = match construct_id {
            Some(id) => id,
            None => return Ok(None),
        };

        // Find the matching classified segment
        let seg = compile_result
            .segments
            .iter()
            .find(|s| &s.construct_id == construct_id);
        let seg = match seg {
            Some(s) => s,
            None => return Ok(None),
        };

        let impl_loc = state.span_index.impl_locations.get(construct_id);
        Ok(Some(hover::build_hover(seg, impl_loc)))
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let docs = self.documents.read().await;
        let doc = match docs.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let result = match &doc.compile_result {
            Some(r) => r,
            None => return Ok(None),
        };

        // Convert cursor position to byte offset
        let cursor_offset = position_to_offset(
            &doc.text,
            params.range.start.line,
            params.range.start.character,
        ) as u32;

        // Find the segment at cursor
        let seg = result.segments.iter().find(|s| {
            s.segment.span.contains(cursor_offset)
        });

        let seg = match seg {
            Some(s) => s,
            None => return Ok(Some(vec![])),
        };

        let actions = actions::code_actions_for_segment(&uri, &doc.text, seg);
        let response: CodeActionResponse = actions
            .into_iter()
            .map(CodeActionOrCommand::CodeAction)
            .collect();

        Ok(Some(response))
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        Ok(completion::completions(&params))
    }
}

/// Encode all segments as LSP semantic tokens using delta encoding.
fn encode_semantic_tokens(
    segments: &[bit_nl_core::ClassifiedSegment],
    source: &str,
) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for seg in segments {
        let (line, start_char) = offset_to_position(source, seg.segment.span.start);
        let (end_line, end_char) = offset_to_position(source, seg.segment.span.end);

        let token_type = segment_kind_to_token_type(seg.kind, seg.confidence);
        let has_impl = false; // impl locations checked via span_index at higher level
        let modifiers = segment_to_modifiers(seg.segment.locked, has_impl);

        // LSP semantic tokens work per-line; if a segment spans multiple lines,
        // emit a token per line.
        if line == end_line {
            let length = end_char.saturating_sub(start_char);
            let delta_line = line - prev_line;
            let delta_start = if delta_line == 0 { start_char - prev_start } else { start_char };
            tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: modifiers,
            });
            prev_line = line;
            prev_start = start_char;
        } else {
            // Multi-line segment: emit one token per line
            let lines: Vec<&str> = source.lines().collect();
            for l in line..=end_line {
                let (line_start, line_len) = if l == line {
                    (start_char, lines.get(l as usize).map(|s| s.len() as u32).unwrap_or(0).saturating_sub(start_char))
                } else if l == end_line {
                    (0, end_char)
                } else {
                    (0, lines.get(l as usize).map(|s| s.len() as u32).unwrap_or(0))
                };
                if line_len == 0 { continue; }
                let delta_line = l - prev_line;
                let delta_start = if delta_line == 0 { line_start - prev_start } else { line_start };
                tokens.push(SemanticToken {
                    delta_line,
                    delta_start,
                    length: line_len,
                    token_type,
                    token_modifiers_bitset: modifiers,
                });
                prev_line = l;
                prev_start = line_start;
            }
        }
    }

    tokens
}

fn position_in_range(pos: &Position, range: &Range) -> bool {
    let after_start = pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character);
    let before_end = pos.line < range.end.line
        || (pos.line == range.end.line && pos.character < range.end.character);
    after_start && before_end
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Arc::new(RwLock::new(HashMap::new())),
        models_dir: dirs::home_dir()
            .unwrap_or_default()
            .join(".bit/models"),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
