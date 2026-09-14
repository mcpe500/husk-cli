//! Content-aware output compression — never let raw CLI output reach the
//! model's context window (RTK / token-saver style, prompt1.md layer 1).
//!
//! Terminal output is written for humans: progress bars, ANSI colors,
//! dependency trees, audit spam. Processors keep errors, diffs and actionable
//! summaries; everything else is dropped deterministically. A hard token cap
//! is enforced last, preserving head+tail (build errors live at the end,
//! headers at the start).

use crate::tokenutil::estimate_tokens;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Compressed {
    pub text: String,
    pub raw_tokens: u64,
    pub compressed_tokens: u64,
    pub processor: &'static str,
}

impl Compressed {
    /// 0.0 = nothing saved, 1.0 = everything dropped.
    pub fn saved_ratio(&self) -> f64 {
        if self.raw_tokens == 0 {
            return 0.0;
        }
        1.0 - (self.compressed_tokens as f64 / self.raw_tokens as f64)
    }
}

/// Strip ANSI escape sequences (colors, cursor moves, progress-bar redraws).
pub fn strip_ansi(input: &str) -> String {
    let re = Regex::new("\x1b\\[[0-9;?]*[A-Za-z]|\x1b\\][^\x07\x1b]*(\x07|\x1b\\\\)|\r").unwrap();
    let cleaned = re.replace_all(input, "");
    // Collapse progress-bar artifacts: repeated identical lines.
    let mut out = String::with_capacity(cleaned.len());
    let mut prev: Option<&str> = None;
    for line in cleaned.lines() {
        if prev == Some(line) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        prev = Some(line);
    }
    out
}

struct Processor {
    name: &'static str,
    command: &'static str,
    keep: &'static [&'static str],
    drop: &'static [&'static str],
    /// When a line matches, keep the following lines too (up to the cap) —
    /// for failure blocks whose body lines alone are meaningless.
    sticky: &'static [&'static str],
    sticky_max: usize,
}

const CARGO: Processor = Processor {
    name: "cargo",
    command: r"^\s*(cargo|rustc)\b",
    keep: &[
        r"^error",
        r"error\[E[0-9]+\]",
        r"^warning",
        r"panicked at",
        r"^---- ",
        r"^failures:",
        r"^test result:",
        r"^thread '",
        r"^\s*\|",
        r"^\s*-->",
        r"^note: ",
        r"^stack backtrace:",
    ],
    drop: &[
        r"^\s*Compiling ",
        r"^\s*Finished",
        r"^\s*Running",
        r"^\s*Downloaded",
        r"^\s*Downloading",
        r"^\s*Updating ",
        r"^\s*Locking ",
        r"^\s*Adding ",
        r"^\s*Fresh ",
        r"^\s*Doc-tests",
    ],
    sticky: &[r"^failures:", r"^---- ", r"^error(\[|:)"],
    sticky_max: 40,
};

const GIT: Processor = Processor {
    name: "git",
    command: r"^\s*git\b",
    keep: &[
        r"^diff --git",
        r"^index ",
        r"^\+\+\+",
        r"^---",
        r"^@@",
        r"^fatal:",
        r"^error:",
        r"^hint:",
        r"^On branch",
        r"^Merge:",
        r"^\s*(modified|new file|deleted|renamed):",
        r"^(Untracked|Changes to be committed|Changes not staged)",
        r"^commit [0-9a-f]{7,}",
        r"^Author:",
        r"^Date:",
        r"^    ",
    ],
    drop: &[],
    sticky: &[r"^diff --git"],
    sticky_max: 80,
};

const NPM: Processor = Processor {
    name: "npm",
    command: r"^\s*(npm|npx|pnpm|yarn)\b",
    keep: &[
        r"npm (ERR!|error|warn)",
        r"^added \d+ packages?",
        r"^removed \d+ packages?",
        r"^changed \d+ packages?",
        r"^\d+ packages? are looking for funding",
        r"^\d+ (high|moderate|low|critical|info) severity",
        r"^\d+ vulnerabilities",
        r"^(Error|TypeError|SyntaxError|ReferenceError)",
        r"^\s*at ",
        r"^EXIT CODE",
        r"^ELIFECYCLE",
    ],
    drop: &[
        r"^npm (WARN )?deprecated",
        r"^\s*\d+[smb]?\s*$", // spinner/progress lines
        r"^$",
    ],
    sticky: &[r"npm ERR!", r"^(Error|TypeError|SyntaxError|ReferenceError)"],
    sticky_max: 30,
};

const TEST: Processor = Processor {
    name: "test",
    command: r"^\s*(pytest|python -m pytest|jest|vitest|go test|go vet|make test)\b",
    keep: &[
        r"^FAILED",
        r"^ERROR",
        r"^PASSED TO CONTINUE",
        r"=+ FAILURES =+",
        r"^E\s",
        r"^assert",
        r"^---?",
        r"^\+",
        r"short test summary",
        r"^ok\s",
        r"^FAIL",
        r"^--- FAIL",
        r"^=== RUN",
    ],
    drop: &[r"^$"],
    sticky: &[r"=+ FAILURES =+", r"^--- FAIL", r"^FAILED"],
    sticky_max: 50,
};

const PROCESSORS: [&Processor; 4] = [&CARGO, &GIT, &NPM, &TEST];

pub struct OutputCompressor {
    /// Hard token ceiling for any compressed result.
    max_output_tokens: usize,
    compiled: Vec<(CompiledProcessor, &'static str)>,
}

struct CompiledProcessor {
    command: Regex,
    keep: Vec<Regex>,
    drop: Vec<Regex>,
    sticky: Vec<Regex>,
    sticky_max: usize,
}

fn compile(p: &Processor) -> CompiledProcessor {
    CompiledProcessor {
        command: Regex::new(p.command).unwrap(),
        keep: p.keep.iter().map(|r| Regex::new(r).unwrap()).collect(),
        drop: p.drop.iter().map(|r| Regex::new(r).unwrap()).collect(),
        sticky: p.sticky.iter().map(|r| Regex::new(r).unwrap()).collect(),
        sticky_max: p.sticky_max,
    }
}

impl OutputCompressor {
    pub fn new(max_output_tokens: usize) -> Self {
        Self {
            max_output_tokens,
            compiled: PROCESSORS.iter().map(|p| (compile(p), p.name)).collect(),
        }
    }

    /// Default budget tuned for an 8k-token local window and cheap cloud turns.
    pub fn with_defaults() -> Self {
        Self::new(2_000)
    }

    /// Compress `output` produced by `command`. Falls back to generic
    /// head+tail truncation when no processor matches.
    pub fn compress(&self, command: &str, output: &str) -> Compressed {
        let raw_tokens = estimate_tokens(output) as u64;
        let cleaned = strip_ansi(output);
        let name = self
            .compiled
            .iter()
            .find(|(p, _)| p.command.is_match(command))
            .map(|(_, n)| *n)
            .unwrap_or("generic");
        let mut text = match name {
            "generic" => self.head_tail(&cleaned, 80, 120),
            _ => {
                let (p, _) = self.compiled.iter().find(|(cp, _)| cp.command.is_match(command)).unwrap();
                self.filter(p, &cleaned)
            }
        };
        text = self.enforce_cap(&text);
        let compressed_tokens = estimate_tokens(&text) as u64;
        Compressed { text, raw_tokens, compressed_tokens, processor: name }
    }

    fn filter(&self, p: &CompiledProcessor, cleaned: &str) -> String {
        let mut out: Vec<&str> = Vec::new();
        let mut sticky_left = 0usize;
        for line in cleaned.lines() {
            if sticky_left > 0 {
                out.push(line);
                sticky_left -= 1;
                continue;
            }
            if p.drop.iter().any(|r| r.is_match(line)) {
                continue;
            }
            if p.keep.iter().any(|r| r.is_match(line)) {
                out.push(line);
                if p.sticky.iter().any(|r| r.is_match(line)) {
                    sticky_left = p.sticky_max;
                }
            }
        }
        // Trim trailing sticky spillover.
        while out.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
            out.pop();
        }
        out.join("\n")
    }

    fn head_tail(&self, cleaned: &str, head: usize, tail: usize) -> String {
        let lines: Vec<&str> = cleaned.lines().collect();
        if lines.len() <= head + tail {
            return lines.join("\n");
        }
        let elided = lines.len() - head - tail;
        let mut out: Vec<String> = lines[..head].iter().map(|s| s.to_string()).collect();
        out.push(format!("[... {elided} lines elided (husk): see `husk search` for the full output]"));
        out.extend(lines[lines.len() - tail..].iter().map(|s| s.to_string()));
        out.join("\n")
    }

    /// Last-resort hard cap by estimated tokens, preserving head+tail.
    fn enforce_cap(&self, text: &str) -> String {
        if estimate_tokens(text) <= self.max_output_tokens {
            return text.to_string();
        }
        // ~4 ascii chars per token; char-based so long lines are handled too.
        let cap_chars = self.max_output_tokens * 4;
        let total = text.chars().count();
        if total <= cap_chars {
            return text.to_string();
        }
        let head = cap_chars * 2 / 5;
        let tail = cap_chars - head;
        let elided = total - head - tail;
        let head_s: String = text.chars().take(head).collect();
        let tail_s: String = text.chars().skip(total - tail).collect();
        format!(
            "{head_s}\n[... {elided} chars elided (husk): token cap {}]\n{tail_s}",
            self.max_output_tokens
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compressor() -> OutputCompressor {
        OutputCompressor::with_defaults()
    }

    #[test]
    fn strips_ansi_and_dedupes_progress_lines() {
        let raw = "\x1b[32m\u{2713} built\x1b[0m\r\n\u{1b}[1Gdownloading...\r\ndownloading...\r\ndownloading...\r\nnext\n";
        let out = strip_ansi(raw);
        assert!(!out.contains('\x1b'));
        assert!(!out.contains('\r'));
        let count = out.lines().filter(|l| *l == "downloading...").count();
        assert_eq!(count, 1, "progress redraws must collapse: {out}");
    }

    #[test]
    fn cargo_output_keeps_errors_drops_noise() {
        let out = "\
   Compiling serde v1.0.0
   Compiling tokio v1.0.0
    Finished dev [unoptimized] target(s) in 12.34s
     Running unittests src/lib.rs
error[E0308]: mismatched types
 --> src/main.rs:10:5
  |
10 |     let x: u32 = \"s\";
  |                  ^^ expected u32, found `&str`
thread 'tests::basic' panicked at src/lib.rs:42:
failures:
---- tests::basic stdout ----
assertion failed: cfg.enabled
test result: FAILED. 1 passed; 2 failed; 0 ignored; 12 measured
";
        let c = compressor().compress("cargo test --workspace", out);
        assert_eq!(c.processor, "cargo");
        assert!(c.text.contains("error[E0308]"));
        assert!(c.text.contains("test result: FAILED"));
        assert!(c.text.contains("---- tests::basic stdout ----"));
        assert!(c.text.contains("assertion failed"));
        assert!(!c.text.contains("Compiling serde"));
        assert!(!c.text.contains("Finished dev"));
        assert!(c.saved_ratio() > 0.0);
    }

    #[test]
    fn git_diff_keeps_headers_and_hunks() {
        let mut out = String::from("diff --git a/src/x.rs b/src/x.rs\nindex abc..def 100644\n--- a/src/x.rs\n+++ b/src/x.rs\n");
        for i in 0..300 {
            out.push_str(&format!("+line {i} of a very large refactor\n"));
        }
        out.push_str("fatal: not a git repository\n");
        let c = compressor().compress("git diff main...HEAD", &out);
        assert_eq!(c.processor, "git");
        assert!(c.text.contains("diff --git a/src/x.rs"));
        assert!(c.text.contains("fatal: not a git repository"));
        // 300 identical-ish lines: sticky cap (80) must bound them.
        let kept_plus = c.text.lines().filter(|l| l.starts_with("+line")).count();
        assert!(kept_plus <= 90, "diff hunk must be capped, got {kept_plus}");
        assert!(c.saved_ratio() > 0.5, "ratio {}", c.saved_ratio());
    }

    #[test]
    fn npm_output_keeps_summary_and_errors() {
        let out = "\
npm WARN deprecated har-validator@5.1.5: this library is no longer supported

added 234 packages, and audited 235 packages in 8s

23 packages are looking for funding
run `npm fund` for details

8 vulnerabilities (2 moderate, 6 high)

To address all issues, run:
  npm audit fix
";
        let c = compressor().compress("npm install", out);
        assert_eq!(c.processor, "npm");
        assert!(c.text.contains("added 234 packages"));
        assert!(c.text.contains("8 vulnerabilities"));
        assert!(!c.text.contains("deprecated har-validator"), "deprecation spam is dropped");
        assert!(c.saved_ratio() >= 0.0);
    }

    #[test]
    fn generic_output_is_head_and_tail() {
        let out: String = (0..500).map(|i| format!("filler line {i}\n")).collect();
        let c = compressor().compress("./weird-tool --dump", &out);
        assert_eq!(c.processor, "generic");
        assert!(c.text.contains("filler line 0"));
        assert!(c.text.contains("filler line 499"));
        assert!(c.text.contains("lines elided (husk)"));
        assert!(c.saved_ratio() > 0.5);
    }

    #[test]
    fn hard_token_cap_is_enforced() {
        // Few but very long lines: head/tail alone leaves far more than the budget.
        let out: String = (0..300).map(|i| format!("long line {i}: {}\n", "x".repeat(300))).collect();
        let c = OutputCompressor::with_defaults().compress("./huge", &out);
        assert!(c.compressed_tokens <= 2_500, "cap breached: {}", c.compressed_tokens);
        assert!(c.text.contains("token cap"));
    }

    #[test]
    fn empty_output_stays_empty_and_cheap() {
        let c = compressor().compress("cargo build", "");
        assert_eq!(c.compressed_tokens, 0);
        assert_eq!(c.saved_ratio(), 0.0);
    }
}
