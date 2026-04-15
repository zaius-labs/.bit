use crate::eval::{self, EvalResult};
use crate::index::DocIndex;
use crate::mutate::RecordStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub passed: bool,
    pub score: Option<f64>,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateContext {
    pub store: RecordStore,
    pub index: DocIndex,
    pub vars: HashMap<String, String>,
    pub completed_tasks: Vec<String>,
    pub task_results: HashMap<String, String>,
    pub submitted_forms: Vec<String>,
    #[serde(default)]
    pub task_scores: HashMap<String, f64>,
}

pub fn eval_gate(gate_body: &str, ctx: &GateContext) -> GateResult {
    let trimmed = gate_body.trim();

    if trimmed.starts_with("when:") || trimmed.starts_with("when ") {
        return eval_when(trimmed, ctx);
    }
    if trimmed.starts_with("unless:") || trimmed.starts_with("unless ") {
        return eval_unless(trimmed, ctx);
    }
    if trimmed.starts_with("after:") || trimmed.starts_with("after ") {
        return eval_after(trimmed, ctx);
    }
    if trimmed.starts_with("needs:") || trimmed.starts_with("needs ") {
        return eval_needs(trimmed, ctx);
    }
    if trimmed.starts_with("all:") || trimmed.starts_with("all ") {
        return eval_all(trimmed, ctx);
    }
    if trimmed.starts_with("any:") || trimmed.starts_with("any ") {
        return eval_any(trimmed, ctx);
    }
    if trimmed.starts_with("not:") || trimmed.starts_with("not ") {
        return eval_not(trimmed, ctx);
    }
    if trimmed.starts_with("intake") {
        return eval_intake(trimmed, ctx);
    }

    if trimmed.contains("score:") {
        return eval_score_gate(trimmed, ctx);
    }
    if trimmed.contains("result:") {
        return eval_result_gate(trimmed, ctx);
    }

    eval_validator_gate(trimmed, ctx)
}

fn eval_when(body: &str, ctx: &GateContext) -> GateResult {
    let expr = body
        .trim_start_matches("when:")
        .trim_start_matches("when ")
        .trim();
    let expr = strip_pipes(expr);
    let result = eval::eval_compute(&expr, &ctx.store, &ctx.vars);
    let passed = is_truthy(&result);
    GateResult {
        passed,
        score: None,
        details: format!("when: {} -> {}", expr, passed),
    }
}

fn eval_unless(body: &str, ctx: &GateContext) -> GateResult {
    let expr = body
        .trim_start_matches("unless:")
        .trim_start_matches("unless ")
        .trim();
    let expr = strip_pipes(expr);
    let result = eval::eval_compute(&expr, &ctx.store, &ctx.vars);
    let passed = !is_truthy(&result);
    GateResult {
        passed,
        score: None,
        details: format!("unless: {} -> {}", expr, passed),
    }
}

fn eval_after(body: &str, ctx: &GateContext) -> GateResult {
    let task_ref = body
        .trim_start_matches("after:")
        .trim_start_matches("after ")
        .trim();

    let result_check = task_ref.find(" result:");
    let (raw_name, expected_result) = if let Some(pos) = result_check {
        let name = task_ref[..pos].trim();
        let result = task_ref[pos + 8..].trim().trim_start_matches(':');
        (name, Some(result))
    } else {
        (task_ref, None)
    };

    let task_name = raw_name.trim_matches(|c: char| c == '[' || c == ']');

    let task_done = ctx
        .completed_tasks
        .iter()
        .any(|t| t == task_name || t.contains(task_name));

    let passed = if let Some(expected) = expected_result {
        let expected_clean = expected.trim_start_matches(':');
        task_done
            && ctx.task_results.get(task_name).is_some_and(|r| {
                let actual_clean = r.trim_start_matches(':');
                actual_clean == expected_clean
            })
    } else {
        task_done
    };

    GateResult {
        passed,
        score: None,
        details: format!("after: {} -> {}", task_name, passed),
    }
}

fn eval_needs(body: &str, ctx: &GateContext) -> GateResult {
    let resource = body
        .trim_start_matches("needs:")
        .trim_start_matches("needs ")
        .trim();

    // Strip leading @ for variable lookup
    let key = resource.trim_start_matches('@');
    let passed = ctx.vars.contains_key(key);

    GateResult {
        passed,
        score: None,
        details: format!("needs: {} -> {}", resource, passed),
    }
}

fn eval_all(body: &str, ctx: &GateContext) -> GateResult {
    let inner = body
        .trim_start_matches("all:")
        .trim_start_matches("all ")
        .trim();
    let sub_gates = extract_sub_gates(inner);
    let results: Vec<GateResult> = sub_gates.iter().map(|g| eval_gate(g, ctx)).collect();
    let passed = results.iter().all(|r| r.passed);

    GateResult {
        passed,
        score: None,
        details: format!(
            "all: {}/{} passed",
            results.iter().filter(|r| r.passed).count(),
            results.len()
        ),
    }
}

fn eval_any(body: &str, ctx: &GateContext) -> GateResult {
    let inner = body
        .trim_start_matches("any:")
        .trim_start_matches("any ")
        .trim();
    let sub_gates = extract_sub_gates(inner);
    let results: Vec<GateResult> = sub_gates.iter().map(|g| eval_gate(g, ctx)).collect();
    let passed = results.iter().any(|r| r.passed);

    GateResult {
        passed,
        score: None,
        details: format!(
            "any: {}/{} passed",
            results.iter().filter(|r| r.passed).count(),
            results.len()
        ),
    }
}

fn eval_not(body: &str, ctx: &GateContext) -> GateResult {
    let inner = body
        .trim_start_matches("not:")
        .trim_start_matches("not ")
        .trim();
    let inner_result = eval_gate(inner, ctx);
    GateResult {
        passed: !inner_result.passed,
        score: None,
        details: format!(
            "not: inner={} -> {}",
            inner_result.passed, !inner_result.passed
        ),
    }
}

fn eval_intake(body: &str, ctx: &GateContext) -> GateResult {
    let rest = body.trim_start_matches("intake").trim();

    // Parse optional `where: <expr>` suffix
    let (form_name, where_expr) = if let Some(idx) = rest.find("where:") {
        let name = rest[..idx].trim();
        let expr = rest[idx + "where:".len()..].trim();
        (name, Some(expr))
    } else {
        (rest, None)
    };

    // Check if the named form (or any form) was submitted
    let form_submitted = if form_name.is_empty() {
        !ctx.submitted_forms.is_empty()
    } else {
        ctx.submitted_forms.iter().any(|f| f == form_name)
    };

    if !form_submitted {
        return GateResult {
            passed: false,
            score: None,
            details: format!("intake: '{}' not submitted", form_name),
        };
    }

    // If there's a where: clause, evaluate it as a compute expression
    if let Some(expr) = where_expr {
        let expr = strip_pipes(expr);
        let result = crate::eval::eval_compute(&expr, &ctx.store, &ctx.vars);
        let passed = is_truthy(&result);
        return GateResult {
            passed,
            score: None,
            details: format!(
                "intake: '{}' submitted, where: {} -> {}",
                form_name, expr, passed
            ),
        };
    }

    GateResult {
        passed: true,
        score: None,
        details: format!("intake: '{}' submitted", form_name),
    }
}

fn eval_score_gate(body: &str, ctx: &GateContext) -> GateResult {
    // {validator_name score: > N}
    let parts: Vec<&str> = body.splitn(2, "score:").collect();
    let name = parts[0].trim();
    let threshold_str = parts.get(1).unwrap_or(&"0").trim();

    let (op, val) = if let Some(rest) = threshold_str.strip_prefix(">=") {
        (">=", rest.trim().parse::<f64>().unwrap_or(0.0))
    } else if let Some(rest) = threshold_str.strip_prefix("<=") {
        ("<=", rest.trim().parse::<f64>().unwrap_or(0.0))
    } else if let Some(rest) = threshold_str.strip_prefix('>') {
        (">", rest.trim().parse::<f64>().unwrap_or(0.0))
    } else if let Some(rest) = threshold_str.strip_prefix('<') {
        ("<", rest.trim().parse::<f64>().unwrap_or(0.0))
    } else {
        (
            ">=",
            threshold_str
                .trim_start_matches('=')
                .trim()
                .parse::<f64>()
                .unwrap_or(0.0),
        )
    };

    // Look up the actual score for this validator from the context.
    // If no score recorded yet, the gate fails (score not yet available).
    let score = match ctx.task_scores.get(name) {
        Some(&s) => s,
        None => {
            return GateResult {
                passed: false,
                score: None,
                details: format!("score: no score recorded for '{}'", name),
            };
        }
    };

    let passed = match op {
        ">" => score > val,
        "<" => score < val,
        ">=" => score >= val,
        "<=" => score <= val,
        _ => score >= val,
    };

    GateResult {
        passed,
        score: Some(score),
        details: format!("score: {} {} {} {} -> {}", name, score, op, val, passed),
    }
}

fn eval_result_gate(body: &str, ctx: &GateContext) -> GateResult {
    // {task_name result: :value}
    let parts: Vec<&str> = body.splitn(2, "result:").collect();
    let name = parts[0].trim();
    let expected = parts.get(1).unwrap_or(&"").trim().trim_start_matches(':');

    let passed = ctx
        .task_results
        .get(name)
        .is_some_and(|r| r.trim_start_matches(':') == expected);

    GateResult {
        passed,
        score: None,
        details: format!("result: {} expected :{} -> {}", name, expected, passed),
    }
}

fn eval_validator_gate(name: &str, ctx: &GateContext) -> GateResult {
    let exists = ctx.index.validators.iter().any(|v| v == name);
    GateResult {
        passed: exists,
        score: None,
        details: format!("validator: {} exists={}", name, exists),
    }
}

fn extract_sub_gates(s: &str) -> Vec<String> {
    let mut gates = Vec::new();
    let mut depth: usize = 0;
    let mut current = String::new();

    for c in s.chars() {
        match c {
            '{' => {
                if depth > 0 {
                    current.push(c);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth > 0 {
                    current.push(c);
                } else if depth == 0 {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        gates.push(trimmed);
                    }
                    current.clear();
                }
            }
            _ => {
                if depth > 0 {
                    current.push(c);
                }
            }
        }
    }

    if gates.is_empty() && !s.is_empty() {
        gates.push(s.to_string());
    }

    gates
}

fn strip_pipes(s: &str) -> String {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix("||").and_then(|s| s.strip_suffix("||")) {
        inner.to_string()
    } else if let Some(inner) = s.strip_prefix('|').and_then(|s| s.strip_suffix('|')) {
        inner.to_string()
    } else {
        s.to_string()
    }
}

fn is_truthy(result: &EvalResult) -> bool {
    match result {
        EvalResult::Bool { value } => *value,
        EvalResult::Number { value } => *value != 0.0,
        EvalResult::String { value } => !value.is_empty(),
        EvalResult::Null => false,
        EvalResult::Error { .. } => false,
        EvalResult::List { items } => !items.is_empty(),
        EvalResult::Map { entries } => !entries.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_ctx() -> GateContext {
        GateContext {
            store: RecordStore::new(),
            index: DocIndex::default(),
            vars: HashMap::new(),
            completed_tasks: Vec::new(),
            task_results: HashMap::new(),
            submitted_forms: Vec::new(),
            task_scores: HashMap::new(),
        }
    }

    // ── is_truthy ──

    #[test]
    fn truthy_bool_true() {
        assert!(is_truthy(&EvalResult::Bool { value: true }));
    }

    #[test]
    fn truthy_bool_false() {
        assert!(!is_truthy(&EvalResult::Bool { value: false }));
    }

    #[test]
    fn truthy_number_nonzero() {
        assert!(is_truthy(&EvalResult::Number { value: 42.0 }));
    }

    #[test]
    fn truthy_number_zero() {
        assert!(!is_truthy(&EvalResult::Number { value: 0.0 }));
    }

    #[test]
    fn truthy_string_nonempty() {
        assert!(is_truthy(&EvalResult::String {
            value: "hi".to_string(),
        }));
    }

    #[test]
    fn truthy_string_empty() {
        assert!(!is_truthy(&EvalResult::String {
            value: "".to_string(),
        }));
    }

    #[test]
    fn truthy_null() {
        assert!(!is_truthy(&EvalResult::Null));
    }

    #[test]
    fn truthy_error() {
        assert!(!is_truthy(&EvalResult::Error {
            message: "err".to_string(),
        }));
    }

    #[test]
    fn truthy_list_nonempty() {
        assert!(is_truthy(&EvalResult::List {
            items: vec![EvalResult::Null],
        }));
    }

    #[test]
    fn truthy_list_empty() {
        assert!(!is_truthy(&EvalResult::List { items: vec![] }));
    }

    // ── strip_pipes ──

    #[test]
    fn strip_single_pipes() {
        assert_eq!(strip_pipes("|x > 0|"), "x > 0");
    }

    #[test]
    fn strip_double_pipes() {
        assert_eq!(strip_pipes("||x > 0||"), "x > 0");
    }

    #[test]
    fn strip_no_pipes() {
        assert_eq!(strip_pipes("x > 0"), "x > 0");
    }

    // ── extract_sub_gates ──

    #[test]
    fn extract_single_sub_gate() {
        let gates = extract_sub_gates("{when: x > 0}");
        assert_eq!(gates, vec!["when: x > 0"]);
    }

    #[test]
    fn extract_multiple_sub_gates() {
        let gates = extract_sub_gates("{when: x > 0}{after: build}");
        assert_eq!(gates, vec!["when: x > 0", "after: build"]);
    }

    #[test]
    fn extract_no_braces_fallback() {
        let gates = extract_sub_gates("some plain text");
        assert_eq!(gates, vec!["some plain text"]);
    }

    #[test]
    fn extract_nested_braces() {
        let gates = extract_sub_gates("{all: {when: a}{when: b}}");
        assert_eq!(gates.len(), 1);
        assert!(gates[0].starts_with("all:"));
    }

    // ── eval_gate: when ──

    #[test]
    fn gate_when_true() {
        let mut ctx = default_ctx();
        ctx.vars.insert("x".to_string(), "10".to_string());
        let result = eval_gate("when: |x > 5|", &ctx);
        assert!(result.passed);
    }

    #[test]
    fn gate_when_false() {
        let mut ctx = default_ctx();
        ctx.vars.insert("x".to_string(), "2".to_string());
        let result = eval_gate("when: |x > 5|", &ctx);
        assert!(!result.passed);
    }

    // ── eval_gate: unless ──

    #[test]
    fn gate_unless_true_expr_blocks() {
        let mut ctx = default_ctx();
        ctx.vars.insert("blocked".to_string(), "true".to_string());
        let result = eval_gate("unless: |blocked|", &ctx);
        assert!(!result.passed);
    }

    #[test]
    fn gate_unless_false_expr_passes() {
        let mut ctx = default_ctx();
        ctx.vars.insert("blocked".to_string(), "false".to_string());
        let result = eval_gate("unless: |blocked|", &ctx);
        assert!(result.passed);
    }

    // ── eval_gate: after ──

    #[test]
    fn gate_after_completed() {
        let mut ctx = default_ctx();
        ctx.completed_tasks.push("build".to_string());
        let result = eval_gate("after: build", &ctx);
        assert!(result.passed);
    }

    #[test]
    fn gate_after_not_completed() {
        let ctx = default_ctx();
        let result = eval_gate("after: build", &ctx);
        assert!(!result.passed);
    }

    #[test]
    fn gate_after_with_result() {
        let mut ctx = default_ctx();
        ctx.completed_tasks.push("review".to_string());
        ctx.task_results
            .insert("review".to_string(), ":approved".to_string());
        let result = eval_gate("after: review result: :approved", &ctx);
        assert!(result.passed);
    }

    #[test]
    fn gate_after_with_wrong_result() {
        let mut ctx = default_ctx();
        ctx.completed_tasks.push("review".to_string());
        ctx.task_results
            .insert("review".to_string(), ":rejected".to_string());
        let result = eval_gate("after: review result: :approved", &ctx);
        assert!(!result.passed);
    }

    // ── eval_gate: needs ──

    #[test]
    fn gate_needs_passes_when_var_exists() {
        let mut ctx = default_ctx();
        ctx.vars
            .insert("database".to_string(), "available".to_string());
        let result = eval_gate("needs: @database", &ctx);
        assert!(result.passed);
    }

    #[test]
    fn gate_needs_fails_when_var_missing() {
        let ctx = default_ctx();
        let result = eval_gate("needs: @database", &ctx);
        assert!(!result.passed);
    }

    // ── eval_gate: all ──

    #[test]
    fn gate_all_both_pass() {
        let mut ctx = default_ctx();
        ctx.completed_tasks.push("a".to_string());
        ctx.completed_tasks.push("b".to_string());
        let result = eval_gate("all: {after: a}{after: b}", &ctx);
        assert!(result.passed);
    }

    #[test]
    fn gate_all_one_fails() {
        let mut ctx = default_ctx();
        ctx.completed_tasks.push("a".to_string());
        let result = eval_gate("all: {after: a}{after: b}", &ctx);
        assert!(!result.passed);
    }

    // ── eval_gate: any ──

    #[test]
    fn gate_any_one_passes() {
        let mut ctx = default_ctx();
        ctx.completed_tasks.push("a".to_string());
        let result = eval_gate("any: {after: a}{after: b}", &ctx);
        assert!(result.passed);
    }

    #[test]
    fn gate_any_none_pass() {
        let ctx = default_ctx();
        let result = eval_gate("any: {after: a}{after: b}", &ctx);
        assert!(!result.passed);
    }

    // ── eval_gate: not ──

    #[test]
    fn gate_not_inverts_true_to_false() {
        let mut ctx = default_ctx();
        ctx.completed_tasks.push("build".to_string());
        let result = eval_gate("not: after: build", &ctx);
        assert!(!result.passed);
    }

    #[test]
    fn gate_not_inverts_false_to_true() {
        let ctx = default_ctx();
        let result = eval_gate("not: after: build", &ctx);
        assert!(result.passed);
    }

    // ── eval_gate: intake ──

    #[test]
    fn gate_intake_form_submitted() {
        let mut ctx = default_ctx();
        ctx.submitted_forms.push("onboarding".to_string());
        let result = eval_gate("intake onboarding", &ctx);
        assert!(result.passed);
    }

    #[test]
    fn gate_intake_form_not_submitted() {
        let ctx = default_ctx();
        let result = eval_gate("intake onboarding", &ctx);
        assert!(!result.passed);
    }

    #[test]
    fn gate_intake_any_form() {
        let mut ctx = default_ctx();
        ctx.submitted_forms.push("any-form".to_string());
        let result = eval_gate("intake", &ctx);
        assert!(result.passed);
    }

    // ── eval_gate: result ──

    #[test]
    fn gate_result_match() {
        let mut ctx = default_ctx();
        ctx.task_results
            .insert("review".to_string(), ":approved".to_string());
        let result = eval_gate("review result: :approved", &ctx);
        assert!(result.passed);
    }

    #[test]
    fn gate_result_mismatch() {
        let mut ctx = default_ctx();
        ctx.task_results
            .insert("review".to_string(), ":rejected".to_string());
        let result = eval_gate("review result: :approved", &ctx);
        assert!(!result.passed);
    }

    // ── eval_gate: score ──

    #[test]
    fn gate_score_passes() {
        let mut ctx = default_ctx();
        ctx.task_scores.insert("quality".to_string(), 1.0);
        let result = eval_gate("quality score: >= 0.5", &ctx);
        assert!(result.passed);
        assert!(result.score.is_some());
    }

    // ── eval_gate: validator ──

    #[test]
    fn gate_validator_exists() {
        let mut ctx = default_ctx();
        ctx.index.validators.push("code-review".to_string());
        let result = eval_gate("code-review", &ctx);
        assert!(result.passed);
    }

    #[test]
    fn gate_validator_not_found() {
        let ctx = default_ctx();
        let result = eval_gate("nonexistent", &ctx);
        assert!(!result.passed);
    }
}
