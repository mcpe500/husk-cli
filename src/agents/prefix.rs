//! Prompt prefix parsing — opencode-style `@file` and `!command` attachers.
//!
//! `@<path>`  → file contents become a context doc (folded by the worker).
//! `!<cmd>`   → shell command runs locally; raw output is compressed by the
//!              output compressor and stored as a context doc (full output
//!              also lands in husk.db via the session layer).
//! Remaining tokens form the actual prompt text.

use crate::compress::OutputCompressor;

#[derive(Debug, PartialEq)]
pub struct ParsedPrompt {
    pub text: String,
    /// (name, content) pairs, in order of appearance.
    pub docs: Vec<(String, String)>,
}

pub fn parse(input: &str) -> ParsedPrompt {
    parse_with(input, &mut |_| -> Option<String> { None })
}

/// Testable core: `shell_runner` executes bang-commands (production wires
/// std::process + compressor; tests inject canned outputs).
pub fn parse_with(
    input: &str,
    shell_runner: &mut dyn FnMut(&str) -> Option<String>,
) -> ParsedPrompt {
    let compressor = OutputCompressor::with_defaults();
    let mut text_parts: Vec<String> = Vec::new();
    let mut docs: Vec<(String, String)> = Vec::new();

    for token in input.split_whitespace() {
        if let Some(path) = token.strip_prefix('@') {
            match std::fs::read_to_string(path) {
                Ok(content) => docs.push((path.to_string(), content)),
                Err(_) => text_parts.push(token.to_string()),
            }
        } else if let Some(command) = token.strip_prefix('!') {
            if let Some(raw) = shell_runner(command) {
                let compressed = compressor.compress(command, &raw);
                docs.push((format!("!{command}"), compressed.text));
            } else {
                text_parts.push(token.to_string());
            }
        } else {
            text_parts.push(token.to_string());
        }
    }

    ParsedPrompt { text: text_parts.join(" "), docs }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_prompt_passes_through() {
        let p = parse("fix the bug in auth");
        assert_eq!(p.text, "fix the bug in auth");
        assert!(p.docs.is_empty());
    }

    #[test]
    fn at_file_attaches_content() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.md");
        std::fs::write(&file, "the secret is 42").unwrap();
        let p = parse(&format!("explain @{} please", file.display()));
        // The @token is consumed into docs, not repeated in the prompt text.
        assert_eq!(p.text, "explain please");
        assert_eq!(p.docs.len(), 1);
        assert_eq!(p.docs[0].1, "the secret is 42");
    }

    #[test]
    fn missing_at_file_stays_in_text() {
        let p = parse("review @definitely/not/here.rs now");
        assert!(p.text.contains("@definitely/not/here.rs"));
        assert!(p.docs.is_empty());
    }

    #[test]
    fn bang_command_output_is_compressed_and_attached() {
        let p = parse_with("summarize !echo", &mut |cmd| {
            if cmd == "echo" {
                Some("hello-world\n".to_string())
            } else {
                None
            }
        });
        assert_eq!(p.text, "summarize");
        assert_eq!(p.docs.len(), 1);
        assert!(p.docs[0].0.starts_with("!echo"));
        assert!(p.docs[0].1.contains("hello-world"));
    }

    #[test]
    fn failed_command_stays_in_text() {
        let p = parse_with("run !false-thing now", &mut |_| None);
        assert!(p.text.contains("!false-thing"));
        assert!(p.docs.is_empty());
    }
}
