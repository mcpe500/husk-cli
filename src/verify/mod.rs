//! Verification gate for worker output (prompt1.md pillar 2).
//!
//! Never trust the small model's self-assessment: deterministic checks decide
//! whether a worker result may enter the supervisor's context. Any violation
//! escalates the task to DeepSeek instead.

use crate::tokenutil::estimate_tokens;

#[derive(Debug, Clone, Default)]
pub struct VerifySpec {
    /// When true, the output must contain a JSON object with all required fields.
    pub expect_json: bool,
    /// Top-level JSON fields that must be present (when expect_json).
    pub required_fields: Vec<String>,
    /// Identifiers (file paths, symbols, ids) that must appear in the output.
    pub required_identifiers: Vec<String>,
    /// Maximum estimated tokens; 0 = unlimited.
    pub max_tokens: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Violation {
    NotJson,
    MissingField(String),
    MissingIdentifier(String),
    OverBudget { actual: usize, max: usize },
    Empty,
}

/// Strip a markdown ```json fence if present, returning the JSON payload.
pub fn extract_json(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if start < end {
                return Some(&trimmed[start..=end]);
            }
        }
    }
    None
}

pub fn verify(output: &str, spec: &VerifySpec) -> Result<(), Vec<Violation>> {
    let mut violations = Vec::new();

    if output.trim().is_empty() {
        violations.push(Violation::Empty);
        return Err(violations);
    }

    if spec.max_tokens > 0 {
        let actual = estimate_tokens(output);
        if actual > spec.max_tokens {
            violations.push(Violation::OverBudget { actual, max: spec.max_tokens });
        }
    }

    if spec.expect_json {
        match extract_json(output) {
            Some(json) => match serde_json::from_str::<serde_json::Value>(json) {
                Ok(value) => {
                    for field in &spec.required_fields {
                        if value.get(field).is_none() {
                            violations.push(Violation::MissingField(field.clone()));
                        }
                    }
                }
                Err(_) => violations.push(Violation::NotJson),
            },
            None => violations.push(Violation::NotJson),
        }
    }

    for ident in &spec.required_identifiers {
        if !output.contains(ident.as_str()) {
            violations.push(Violation::MissingIdentifier(ident.clone()));
        }
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> VerifySpec {
        VerifySpec {
            expect_json: true,
            required_fields: vec!["key_findings".into(), "source_references".into()],
            required_identifiers: vec!["src/main.rs".into()],
            max_tokens: 500,
        }
    }

    #[test]
    fn valid_summary_passes() {
        let out = r#"```json
{"key_findings": ["auth uses jwt"], "source_references": ["src/main.rs:10"]}
``` path src/main.rs"#;
        assert!(verify(out, &spec()).is_ok());
    }

    #[test]
    fn missing_field_fails() {
        let out = r#"{"key_findings": ["x"]}"#;
        let errs = verify(out, &spec()).unwrap_err();
        assert!(errs.contains(&Violation::MissingField("source_references".into())));
    }

    #[test]
    fn missing_identifier_fails() {
        let out = r#"{"key_findings": ["x"], "source_references": ["other.rs"]}"#;
        let errs = verify(out, &spec()).unwrap_err();
        assert!(errs.contains(&Violation::MissingIdentifier("src/main.rs".into())));
    }

    #[test]
    fn broken_json_fails() {
        let out = "here you go: {key_findings: no quotes, path src/main.rs}";
        let errs = verify(out, &spec()).unwrap_err();
        assert!(errs.contains(&Violation::NotJson));
    }

    #[test]
    fn over_budget_fails() {
        let mut spec = spec();
        spec.max_tokens = 5;
        let out = r#"{"key_findings": ["a much longer summary than five tokens"], "source_references": ["src/main.rs"]}"#;
        let errs = verify(out, &spec).unwrap_err();
        assert!(matches!(errs[0], Violation::OverBudget { .. }));
    }

    #[test]
    fn empty_output_fails_fast() {
        assert_eq!(verify("   \n\t", &spec()), Err(vec![Violation::Empty]));
    }

    #[test]
    fn plain_text_passes_when_json_not_expected() {
        let spec = VerifySpec {
            expect_json: false,
            required_identifiers: vec!["TODO".into()],
            ..Default::default()
        };
        assert!(verify("the file contains TODO items", &spec).is_ok());
    }
}
