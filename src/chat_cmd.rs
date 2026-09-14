//! `husk chat` — interactive supervisor-worker session (REPL or one-shot).

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::agents::prefix;
use crate::agents::session::SupervisorWorkerSession;
use crate::config::Config;
use crate::db::Database;
use crate::local;
use crate::providers::create_provider;

fn run_shell(command: &str) -> Option<String> {
    // cmd.exe is guaranteed on Windows 7+; POSIX sh everywhere else.
    #[cfg(windows)]
    let output = std::process::Command::new("cmd").args(["/C", command]).output().ok()?;
    #[cfg(not(windows))]
    let output = std::process::Command::new("sh").arg("-c").arg(command).output().ok()?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    let err = String::from_utf8_lossy(&output.stderr);
    if !err.is_empty() {
        text.push_str(&err);
    }
    Some(text)
}

pub async fn run(
    config: &mut Config,
    prompt: Option<String>,
    session_id: Option<String>,
    local: bool,
    files: Vec<PathBuf>,
) -> Result<()> {
    if local {
        config.local.enabled = true;
    }
    let db = Arc::new(Database::open(&Config::db_path())?);

    let session_rec = match &session_id {
        Some(id) => db.get_session(id)?.with_context(|| format!("session {id} not found"))?,
        None => db.create_session("husk", &config.model, None)?,
    };

    let provider = create_provider(config)?;
    let worker = local::create_local_engine(&config.local);
    let worker_desc = match &worker {
        Some(engine) => engine.describe(),
        None => "worker: off (cloud-only; --local or local.enabled=true to enable)".to_string(),
    };

    let mut session = SupervisorWorkerSession::new(provider, worker, db.clone())
        .with_supervisor_model(config.model.clone());

    println!("{}", "▶ husk session started".bold().cyan());
    println!("   {} {} ({})", "supervisor:".dimmed(), config.model, config.base_url);
    println!("   {} {}", "local:".dimmed(), worker_desc);
    println!("   {} {}", "history:".dimmed(), Config::db_path().display());
    println!("   {} /new /sessions /stats /quit — prefixes: @file !cmd", "commands:".dimmed());

    let mut attached: Vec<(String, String)> = Vec::new();
    for file in &files {
        match std::fs::read_to_string(file) {
            Ok(content) => attached.push((file.display().to_string(), content)),
            Err(e) => println!("{} cannot read {file:?}: {e}", "warn:".yellow()),
        }
    }

    // One-shot prompt → single turn then exit.
    if let Some(first) = prompt {
        let parsed = prefix::parse(&first);
        attached.extend(parsed.docs);
        turn(&mut session, &session_rec.id, &parsed.text, &attached).await?;
        return Ok(());
    }

    let stdin = std::io::stdin();
    loop {
        print!("{} ", "husk>".green().bold());
        std::io::stdout().flush()?;
        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        match line {
            "" => continue,
            "/quit" | "/exit" | "/q" => break,
            "/new" => {
                let s = db.create_session("husk", &config.model, None)?;
                println!("{} new session {}", "✔".green(), s.id);
                continue;
            }
            "/sessions" => {
                for s in db.list_sessions(10)? {
                    println!("   {}  {}  [{}]", s.id.dimmed(), s.title, s.updated_at);
                }
                continue;
            }
            "/stats" => {
                crate::stats_cmd::print_session(&db, &session_rec.id)?;
                continue;
            }
            _ => {}
        }

        let parsed = prefix::parse_with(line, &mut run_shell);
        attached.extend(parsed.docs);
        if turn(&mut session, &session_rec.id, &parsed.text, &attached).await.is_err() {
            println!("{} turn failed — check api key / network", "error:".red().bold());
        }
        attached.clear();
    }
    Ok(())
}

async fn turn(
    session: &mut SupervisorWorkerSession,
    session_id: &str,
    text: &str,
    docs: &[(String, String)],
) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let printer = tokio::spawn(async move {
        let mut stdout = std::io::stdout();
        while let Some(delta) = rx.recv().await {
            print!("{delta}");
            let _ = stdout.flush();
        }
    });

    let outcome = session.send_streamed(session_id, text, docs, Some(tx)).await?;
    let _ = printer.await;

    let saved = outcome.ledger.saved_tokens();
    println!(
        "\n{} worker:{} escalated:{} saved:{saved}tok cost:${:.4}",
        "─".repeat(4).dimmed(),
        if outcome.worker_used { "on" } else { "off" },
        if outcome.worker_escalated { "yes" } else { "no" },
        outcome.ledger.cloud_cost(&session.pricing)
    );
    Ok(())
}
