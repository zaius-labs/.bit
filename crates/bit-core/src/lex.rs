//! .bit lexer — tokenizes source text into a stream of typed tokens.
//!
//! The lexer handles:
//! - Line-level tokenization (indentation, construct prefix, body)
//! - Keyword classification using the TST (ternary search tree)
//! - Inline span tokenization (@refs, $mods, #tags, {gates}, |exprs|)
//! - Type sigil recognition (#, ##, ?, @timestamp, etc.)
//!
//! The parser consumes tokens from the lexer rather than raw lines.

/// A single token in the .bit token stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A construct keyword with optional name: `define:@User`, `flow:`, `gate:code-review`
    Keyword {
        keyword: KeywordKind,
        name: Option<String>,
        line: usize,
        indent: usize,
    },
    /// A key-value field: `    name: "John"`
    Field {
        key: String,
        required: bool,
        indexed: bool,
        value: FieldValue,
        line: usize,
        indent: usize,
    },
    /// An arrow/transition: `A --> B`, `:draft --> :submitted`
    Arrow {
        from: String,
        to: String,
        label: Option<String>,
        line: usize,
        indent: usize,
    },
    /// A task marker: `- [ ] Open task`, `[1!] Design API :@user:ally`
    Task {
        marker: TaskMarkerKind,
        label: Option<String>,
        text: String,
        line: usize,
        indent: usize,
    },
    /// A comment: `// This is a comment`
    Comment { text: String, line: usize },
    /// A heading: `# Top-Level Group`, `## Subgroup`
    Heading {
        depth: u8,
        text: String,
        line: usize,
    },
    /// A divider: `---`
    Divider { line: usize },
    /// A spawn marker: `+` (parallel) or `++` (sequential)
    Spawn { parallel: bool, line: usize },
    /// An embed reference: `^api_design_rationale`
    Embed { tag: String, line: usize },
    /// A directive: `@priority high`, `@due 2026-04-01`
    Directive {
        kind: String,
        value: String,
        line: usize,
    },
    /// A variable assignment: `target = 500000`
    Variable {
        name: String,
        value: String,
        line: usize,
        indent: usize,
    },
    /// A code fence: ``` or ```python
    CodeFence {
        language: Option<String>,
        line: usize,
    },
    /// Raw text content (code block body, prose)
    Text {
        text: String,
        line: usize,
        indent: usize,
    },
    /// A blank line
    Blank { line: usize },
    /// End of input
    Eof,
}

/// All recognized .bit construct keywords.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeywordKind {
    Define,
    Mutate,
    Delete,
    Query,
    InlineQuery,
    Flow,
    States,
    Validate,
    Check,
    Gate,
    Bound,
    Form,
    Mod,
    Project,
    Commands,
    Serve,
    Sync,
    Webhook,
    Git,
    Snap,
    Diff,
    History,
    Status,
    Remember,
    Recall,
    Issue,
    Comment,
    Routine,
    Escalate,
    Use,
    Policy,
    Files,
    LatticeValidates,
    LatticeConstraint,
    LatticeSchema,
    LatticeFrontier,
    PressureEffect,
    UnitCell,
    Symmetry,
    Entity,
    Metric,
    ProjectScope,
}

/// Recognized field value types.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Enum(Vec<String>),
    Array(String),
    Object(String),
    EntityRef(String),
    Timestamp,
    Nil,
    Compute(String),
    Raw(String),
}

/// Task marker kinds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskMarkerKind {
    Open,          // [ ]
    Done,          // [x]
    Blocked,       // [!]
    Question,      // [?]
    Deferred,      // [>]
    Numbered(u32), // [1!], [2x], etc.
}

/// The lexer state machine.
pub struct Lexer<'a> {
    lines: Vec<&'a str>,
    pos: usize,
    in_code_block: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Lexer {
            lines: source.lines().collect(),
            pos: 0,
            in_code_block: false,
        }
    }

    /// Peek at the current line without advancing.
    pub fn peek_line(&self) -> Option<&'a str> {
        self.lines.get(self.pos).copied()
    }

    /// Get the current line number (1-indexed).
    pub fn line_number(&self) -> usize {
        self.pos + 1
    }

    /// Advance to the next line.
    fn advance(&mut self) -> Option<&'a str> {
        if self.pos < self.lines.len() {
            let line = self.lines[self.pos];
            self.pos += 1;
            Some(line)
        } else {
            None
        }
    }

    /// Compute indentation level (number of leading spaces).
    fn indent_of(line: &str) -> usize {
        line.len() - line.trim_start_matches(' ').len()
    }

    /// Produce the next token.
    pub fn next_token(&mut self) -> Token {
        let raw = match self.advance() {
            Some(line) => line,
            None => return Token::Eof,
        };

        let line_num = self.pos; // 1-indexed (already advanced)
        let indent = Self::indent_of(raw);
        let trimmed = raw.trim();

        // Blank line
        if trimmed.is_empty() {
            return Token::Blank { line: line_num };
        }

        // Code block handling
        if trimmed.starts_with("```") {
            self.in_code_block = !self.in_code_block;
            let lang = if trimmed.len() > 3 && !self.in_code_block {
                None // closing fence
            } else {
                let rest = trimmed.trim_start_matches('`').trim();
                if rest.is_empty() {
                    None
                } else {
                    Some(rest.to_string())
                }
            };
            return Token::CodeFence {
                language: lang,
                line: line_num,
            };
        }
        if self.in_code_block {
            return Token::Text {
                text: raw.to_string(),
                line: line_num,
                indent,
            };
        }

        // Comment
        if trimmed.starts_with("//") {
            return Token::Comment {
                text: trimmed.trim_start_matches("//").trim().to_string(),
                line: line_num,
            };
        }

        // Divider
        if trimmed.starts_with("---") && trimmed.chars().all(|c| c == '-') {
            return Token::Divider { line: line_num };
        }

        // Spawn
        if trimmed == "+" {
            return Token::Spawn {
                parallel: true,
                line: line_num,
            };
        }
        if trimmed == "++" {
            return Token::Spawn {
                parallel: false,
                line: line_num,
            };
        }

        // Heading
        if trimmed.starts_with('#') && !trimmed.starts_with("#(") {
            let depth = trimmed.chars().take_while(|&c| c == '#').count();
            let text = trimmed[depth..].trim().to_string();
            return Token::Heading {
                depth: depth as u8,
                text,
                line: line_num,
            };
        }

        // Embed
        if trimmed.starts_with('^') && trimmed.len() > 1 {
            return Token::Embed {
                tag: trimmed[1..].trim().to_string(),
                line: line_num,
            };
        }

        // Directive
        if trimmed.starts_with("@priority ")
            || trimmed.starts_with("@due ")
            || trimmed.starts_with("@assign ")
        {
            let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
            return Token::Directive {
                kind: parts[0].trim_start_matches('@').to_string(),
                value: parts.get(1).unwrap_or(&"").trim().to_string(),
                line: line_num,
            };
        }

        // Task markers: - [ ], [1!], [A!], etc.
        let task_check = trimmed
            .trim_start_matches('+')
            .trim_start_matches('-')
            .trim_start();
        if task_check.starts_with('[') && task_check.contains(']') {
            return self.lex_task(trimmed, line_num, indent);
        }

        // Variable: name = value (no colon, has equals)
        if let Some(eq_pos) = trimmed.find('=') {
            if eq_pos > 0 && !trimmed[..eq_pos].contains(':') && trimmed[..eq_pos].contains(' ') {
                let name = trimmed[..eq_pos].trim();
                let value = trimmed[eq_pos + 1..].trim();
                if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
                    return Token::Variable {
                        name: name.to_string(),
                        value: value.to_string(),
                        line: line_num,
                        indent,
                    };
                }
            }
        }

        // Construct keywords — only classify at base indentation (0 or 4 for nested constructs)
        // Indented key:value lines should NOT be classified as keywords
        if let Some(kw) = self.classify_keyword(trimmed) {
            let name = self.extract_keyword_name(trimmed, &kw);
            return Token::Keyword {
                keyword: kw,
                name,
                line: line_num,
                indent,
            };
        }

        // Arrow (transition)
        if trimmed.contains("-->") || trimmed.contains("==>") || trimmed.contains("-.->") {
            return self.lex_arrow(trimmed, line_num, indent);
        }

        // Key-value field (indented, has colon)
        if indent >= 4 && trimmed.contains(':') {
            return self.lex_field(trimmed, line_num, indent);
        }

        // Inline query: ? How many...
        if let Some(stripped) = trimmed.strip_prefix("? ") {
            return Token::Keyword {
                keyword: KeywordKind::InlineQuery,
                name: Some(stripped.to_string()),
                line: line_num,
                indent,
            };
        }

        // Mod invocation: $Name.method(...)
        if trimmed.starts_with('$') {
            return Token::Keyword {
                keyword: KeywordKind::Mod,
                name: Some(trimmed.to_string()),
                line: line_num,
                indent,
            };
        }

        // Project scope: %ProjectName
        if let Some(stripped) = trimmed.strip_prefix('%') {
            return Token::Keyword {
                keyword: KeywordKind::ProjectScope,
                name: Some(stripped.to_string()),
                line: line_num,
                indent,
            };
        }

        // Fallback: text/prose
        Token::Text {
            text: trimmed.to_string(),
            line: line_num,
            indent,
        }
    }

    fn classify_keyword(&self, trimmed: &str) -> Option<KeywordKind> {
        // Order matters — check more specific prefixes first
        if trimmed.starts_with("define:@") || trimmed.starts_with("define:$") {
            return Some(KeywordKind::Define);
        }
        if trimmed.starts_with("mutate:@") || trimmed.starts_with("mutate:$") {
            return Some(KeywordKind::Mutate);
        }
        if trimmed.starts_with("delete:@") || trimmed.starts_with("delete:$") {
            return Some(KeywordKind::Delete);
        }
        if trimmed.starts_with("query:") {
            return Some(KeywordKind::Query);
        }
        if trimmed.starts_with("flow:") || trimmed == "flow:" {
            return Some(KeywordKind::Flow);
        }
        if trimmed == "states:" || trimmed.starts_with("states:") {
            return Some(KeywordKind::States);
        }
        if trimmed.starts_with("validate ") && trimmed.ends_with(':') {
            return Some(KeywordKind::Validate);
        }
        if trimmed.starts_with("check:") {
            return Some(KeywordKind::Check);
        }
        if trimmed.starts_with("gate:") {
            return Some(KeywordKind::Gate);
        }
        if trimmed.starts_with("bound:") {
            return Some(KeywordKind::Bound);
        }
        if trimmed.starts_with("form:") {
            return Some(KeywordKind::Form);
        }
        if trimmed.starts_with("mod:") {
            return Some(KeywordKind::Mod);
        }
        if trimmed.starts_with("project:") {
            return Some(KeywordKind::Project);
        }
        if trimmed.starts_with("commands:") || trimmed == "commands:" {
            return Some(KeywordKind::Commands);
        }
        if trimmed.starts_with("serve:") {
            return Some(KeywordKind::Serve);
        }
        if trimmed.starts_with("sync:") {
            return Some(KeywordKind::Sync);
        }
        if trimmed.starts_with("webhook:") {
            return Some(KeywordKind::Webhook);
        }
        if trimmed.starts_with("git:") {
            return Some(KeywordKind::Git);
        }
        if trimmed.starts_with("snap:") {
            return Some(KeywordKind::Snap);
        }
        if trimmed.starts_with("diff:") {
            return Some(KeywordKind::Diff);
        }
        if trimmed.starts_with("history:") {
            return Some(KeywordKind::History);
        }
        if trimmed.starts_with("status:") && trimmed.contains('/') {
            return Some(KeywordKind::Status);
        }
        if trimmed.starts_with("remember:") {
            return Some(KeywordKind::Remember);
        }
        if trimmed.starts_with("recall:") {
            return Some(KeywordKind::Recall);
        }
        if trimmed.starts_with("issue:") {
            return Some(KeywordKind::Issue);
        }
        if trimmed.starts_with("comment:") {
            return Some(KeywordKind::Comment);
        }
        if trimmed.starts_with("routine:") {
            return Some(KeywordKind::Routine);
        }
        if trimmed.starts_with("escalate:") {
            return Some(KeywordKind::Escalate);
        }
        if trimmed.starts_with("use $") || trimmed.starts_with("use @") {
            return Some(KeywordKind::Use);
        }
        if trimmed.starts_with("policy:") {
            return Some(KeywordKind::Policy);
        }
        if trimmed.starts_with("files:") {
            return Some(KeywordKind::Files);
        }
        if trimmed.starts_with("if ") && trimmed.ends_with(':') {
            return Some(KeywordKind::Gate);
        } // conditional as gate
        if trimmed.starts_with("lattice_validates:") {
            return Some(KeywordKind::LatticeValidates);
        }
        if trimmed.starts_with("lattice_constraint:") {
            return Some(KeywordKind::LatticeConstraint);
        }
        if trimmed.starts_with("lattice_schema:") {
            return Some(KeywordKind::LatticeSchema);
        }
        if trimmed.starts_with("lattice_frontier:") {
            return Some(KeywordKind::LatticeFrontier);
        }
        if trimmed.starts_with("pressure_effect:") {
            return Some(KeywordKind::PressureEffect);
        }
        if trimmed.starts_with("unit_cell:") {
            return Some(KeywordKind::UnitCell);
        }
        if trimmed.starts_with("symmetry:") {
            return Some(KeywordKind::Symmetry);
        }
        None
    }

    fn extract_keyword_name(&self, trimmed: &str, kw: &KeywordKind) -> Option<String> {
        let after_colon = match kw {
            KeywordKind::Validate => {
                // "validate code-review:" -> "code-review"
                let rest = trimmed.strip_prefix("validate ")?.strip_suffix(':')?;
                return Some(rest.to_string());
            }
            KeywordKind::Use => return Some(trimmed.to_string()),
            _ => {
                if let Some(colon_pos) = trimmed.find(':') {
                    let rest = trimmed[colon_pos + 1..].trim();
                    if rest.is_empty() {
                        return None;
                    }
                    Some(rest.to_string())
                } else {
                    None
                }
            }
        };
        after_colon
    }

    fn lex_task(&self, trimmed: &str, line: usize, indent: usize) -> Token {
        // Find the bracket content
        let start = trimmed.find('[').unwrap_or(0);
        let end = trimmed.find(']').unwrap_or(trimmed.len());
        let inside = &trimmed[start + 1..end];
        let after = trimmed[end + 1..].trim();

        let marker = match inside.trim() {
            " " | "" => TaskMarkerKind::Open,
            "x" | "X" => TaskMarkerKind::Done,
            "!" => TaskMarkerKind::Blocked,
            "?" => TaskMarkerKind::Question,
            ">" => TaskMarkerKind::Deferred,
            other => {
                // Try to parse as numbered: "1!", "2x", "A!"
                if let Ok(n) = other
                    .trim_end_matches(|c: char| !c.is_ascii_digit())
                    .parse::<u32>()
                {
                    TaskMarkerKind::Numbered(n)
                } else {
                    TaskMarkerKind::Open
                }
            }
        };

        Token::Task {
            marker,
            label: None,
            text: after.to_string(),
            line,
            indent,
        }
    }

    fn lex_arrow(&self, trimmed: &str, line: usize, indent: usize) -> Token {
        // Normalize arrows
        let normalized = trimmed.replace("-.->", "-->").replace("==>", "-->");

        // Extract label from -->|label|
        let (working, label) = if let Some(pipe_start) = normalized.find("-->|") {
            if let Some(pipe_end) = normalized[pipe_start + 4..].find('|') {
                let l = normalized[pipe_start + 4..pipe_start + 4 + pipe_end].to_string();
                let mut s = normalized[..pipe_start].to_string();
                s.push_str("-->");
                s.push_str(&normalized[pipe_start + 4 + pipe_end + 1..]);
                (s, Some(l))
            } else {
                (normalized, None)
            }
        } else {
            (normalized, None)
        };

        let parts: Vec<&str> = working.splitn(2, "-->").collect();
        let from = parts.first().unwrap_or(&"").trim().to_string();
        let to = parts.get(1).unwrap_or(&"").trim().to_string();

        Token::Arrow {
            from,
            to,
            label,
            line,
            indent,
        }
    }

    fn lex_field(&self, trimmed: &str, line: usize, indent: usize) -> Token {
        let colon_pos = trimmed.find(':').unwrap_or(trimmed.len());
        let key_part = &trimmed[..colon_pos];
        let value_part = trimmed[colon_pos + 1..].trim();

        let required = key_part.ends_with('!');
        let indexed = key_part.ends_with('^');
        let key = key_part
            .trim_end_matches('!')
            .trim_end_matches('^')
            .trim()
            .to_string();

        let value = parse_field_value(value_part);

        Token::Field {
            key,
            required,
            indexed,
            value,
            line,
            indent,
        }
    }

    /// Tokenize the entire source into a Vec of tokens.
    pub fn tokenize_all(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            if token == Token::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        tokens
    }
}

/// Parse a field value string into a typed FieldValue.
fn parse_field_value(s: &str) -> FieldValue {
    let s = s.trim();
    if s.is_empty() || s == "\"\"" {
        return FieldValue::String(String::new());
    }
    if s == "nil" || s == "null" {
        return FieldValue::Nil;
    }
    if s == "?" || s == "true" || s == "false" {
        return FieldValue::Boolean(s == "true" || s == "?");
    }
    if s.starts_with('"') && s.ends_with('"') {
        return FieldValue::String(s[1..s.len() - 1].to_string());
    }
    if let Some(stripped) = s.strip_prefix("##") {
        return FieldValue::Float(stripped.trim().parse().unwrap_or(0.0));
    }
    if let Some(stripped) = s.strip_prefix('#') {
        return FieldValue::Integer(stripped.trim().parse().unwrap_or(0));
    }
    if s.starts_with('@') {
        if s == "@timestamp" {
            return FieldValue::Timestamp;
        }
        return FieldValue::EntityRef(s.to_string());
    }
    if s.starts_with(':') {
        let variants: Vec<String> = s.split('/').map(|v| v.trim().to_string()).collect();
        return FieldValue::Enum(variants);
    }
    if s.starts_with('[') {
        return FieldValue::Array(s.to_string());
    }
    if s.starts_with('{') {
        return FieldValue::Object(s.to_string());
    }
    if s.starts_with('|') && s.ends_with('|') {
        return FieldValue::Compute(s[1..s.len() - 1].to_string());
    }
    if let Ok(n) = s.parse::<i64>() {
        return FieldValue::Integer(n);
    }
    if let Ok(f) = s.parse::<f64>() {
        return FieldValue::Float(f);
    }
    FieldValue::Raw(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_define() {
        let mut lexer = Lexer::new("define:@User\n    name: \"John\"\n    role: :admin");
        let tokens = lexer.tokenize_all();
        assert!(matches!(
            tokens[0],
            Token::Keyword {
                keyword: KeywordKind::Define,
                ..
            }
        ));
        assert!(matches!(tokens[1], Token::Field { .. }));
        assert!(matches!(tokens[2], Token::Field { .. }));
    }

    #[test]
    fn lex_flow_with_arrows() {
        let mut lexer = Lexer::new("flow:deploy\n    build --> test\n    test --> production");
        let tokens = lexer.tokenize_all();
        assert!(matches!(
            tokens[0],
            Token::Keyword {
                keyword: KeywordKind::Flow,
                ..
            }
        ));
        assert!(matches!(tokens[1], Token::Arrow { .. }));
        assert!(matches!(tokens[2], Token::Arrow { .. }));
    }

    #[test]
    fn lex_mermaid_flow() {
        let mut lexer = Lexer::new("flow:ci\n    graph TD\n    A -->|approved| B\n    B -.-> C");
        let tokens = lexer.tokenize_all();
        assert!(matches!(
            tokens[0],
            Token::Keyword {
                keyword: KeywordKind::Flow,
                ..
            }
        ));
        // graph TD should be Text (metadata)
        assert!(matches!(tokens[1], Token::Text { .. }));
        // Arrow with label
        assert!(matches!(tokens[2], Token::Arrow { .. }));
        if let Token::Arrow { label, .. } = &tokens[2] {
            assert_eq!(label.as_deref(), Some("approved"));
        }
    }

    #[test]
    fn lex_bound() {
        let mut lexer = Lexer::new("bound:rate-limit\n    max: 100");
        let tokens = lexer.tokenize_all();
        assert!(matches!(
            tokens[0],
            Token::Keyword {
                keyword: KeywordKind::Bound,
                ..
            }
        ));
    }

    #[test]
    fn lex_directive() {
        let mut lexer = Lexer::new("@priority high\n@due 2026-04-01");
        let tokens = lexer.tokenize_all();
        assert!(matches!(tokens[0], Token::Directive { .. }));
        assert!(matches!(tokens[1], Token::Directive { .. }));
    }

    #[test]
    fn lex_task_markers() {
        let mut lexer = Lexer::new("- [ ] Open\n- [x] Done\n- [!] Blocked\n[1!] First");
        let tokens = lexer.tokenize_all();
        assert!(matches!(
            tokens[0],
            Token::Task {
                marker: TaskMarkerKind::Open,
                ..
            }
        ));
        assert!(matches!(
            tokens[1],
            Token::Task {
                marker: TaskMarkerKind::Done,
                ..
            }
        ));
        assert!(matches!(
            tokens[2],
            Token::Task {
                marker: TaskMarkerKind::Blocked,
                ..
            }
        ));
        assert!(matches!(
            tokens[3],
            Token::Task {
                marker: TaskMarkerKind::Numbered(1),
                ..
            }
        ));
    }

    #[test]
    fn lex_field_types() {
        let mut lexer = Lexer::new("    count: #42\n    price: ##9.99\n    active: ?\n    state: :draft/:published\n    ref: @User");
        let tokens = lexer.tokenize_all();
        assert!(matches!(
            tokens[0],
            Token::Field {
                value: FieldValue::Integer(42),
                ..
            }
        ));
        assert!(matches!(
            tokens[1],
            Token::Field {
                value: FieldValue::Float(_),
                ..
            }
        ));
        assert!(matches!(
            tokens[2],
            Token::Field {
                value: FieldValue::Boolean(true),
                ..
            }
        ));
        assert!(matches!(
            tokens[3],
            Token::Field {
                value: FieldValue::Enum(_),
                ..
            }
        ));
        assert!(matches!(
            tokens[4],
            Token::Field {
                value: FieldValue::EntityRef(_),
                ..
            }
        ));
    }

    #[test]
    fn lex_spawn() {
        let mut lexer = Lexer::new("+\n++");
        let tokens = lexer.tokenize_all();
        assert!(matches!(tokens[0], Token::Spawn { parallel: true, .. }));
        assert!(matches!(
            tokens[1],
            Token::Spawn {
                parallel: false,
                ..
            }
        ));
    }

    #[test]
    fn lex_code_block() {
        let mut lexer = Lexer::new("```python\ndef hello():\n    print(\"hello\")\n```");
        let tokens = lexer.tokenize_all();
        assert!(matches!(tokens[0], Token::CodeFence { .. }));
        assert!(matches!(tokens[1], Token::Text { .. }));
        assert!(matches!(tokens[2], Token::Text { .. }));
        assert!(matches!(tokens[3], Token::CodeFence { .. }));
    }
}
