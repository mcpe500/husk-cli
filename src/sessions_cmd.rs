//! `husk sessions` — browse the history stored in husk.db.

use anyhow::Result;
use colored::Colorize;

use crate::cli::SessionCommands;
use crate::config::Config;
use crate::db::{Database, Role};

pub fn run(action: SessionCommands) -> Result<()> {
    let db = Database::open(&Config::db_path())?;
    match action {
        SessionCommands::List => {
            println!("{}", "▶ Recent sessions (husk.db):".bold().cyan());
            let sessions = db.list_sessions(25)?;
            if sessions.is_empty() {
                println!("   (empty)");
                return Ok(());
            }
            for s in sessions {
                println!("   {}  {}  [{}]", s.id.dimmed(), s.title, s.updated_at);
            }
        }
        SessionCommands::Show { id } => {
            let Some(session) = db.get_session(&id)? else {
                println!("{} session {id} not found", "error:".red().bold());
                return Ok(());
            };
            println!("{} {} — model {}", "▶".cyan(), session.title.bold(), session.model);
            for m in db.recent_messages(&id, 500)? {
                let role = match m.role {
                    Role::User => "you".green(),
                    Role::Assistant => "husk".cyan(),
                    Role::System => "sys".dimmed(),
                    Role::Tool => "tool".yellow(),
                };
                println!("{role:>5}: {}", m.content);
            }
        }
        SessionCommands::Rm { id } => {
            db.delete_session(&id)?;
            println!("{} deleted {id}", "✔".green());
        }
    }
    Ok(())
}

/// Most recent session id, if any.
pub fn latest_id(db: &Database) -> Result<Option<String>> {
    Ok(db.list_sessions(1)?.into_iter().next().map(|s| s.id))
}

/// `husk search <query>` — FTS5 search across stored messages and tool parts.
pub fn run_search(query: &str, limit: usize) -> Result<()> {
    let db = Database::open(&Config::db_path())?;
    println!("{}", format!("▶ Search: {query:?}").bold().cyan());
    let hits = db.search(query, limit)?;
    if hits.is_empty() {
        println!("   (no matches)");
        return Ok(());
    }
    for (kind, title, body) in hits {
        println!("   {} {title} — {body}", kind.yellow());
    }
    Ok(())
}
