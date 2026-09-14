//! `husk stats` — token accounting and savings report from husk.db.

use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::db::Database;
use crate::sessions_cmd::latest_id;

pub fn run(session: &Option<String>) -> Result<()> {
    let db = Database::open(&Config::db_path())?;
    let id = match session {
        Some(id) => id.clone(),
        None => latest_id(&db)?.ok_or_else(|| {
            anyhow::anyhow!("no sessions yet — run `husk chat` first")
        })?,
    };
    print_session(&db, &id)
}

pub fn print_session(db: &Database, session_id: &str) -> Result<()> {
    let ledger = db.session_ledger(session_id)?;
    let pricing = crate::agents::session::DEEPSEEK_FLASH_PRICING;

    println!("{}", "▶ Token accounting".bold().cyan());
    println!(
        "   {} {:>10} prompt | {:>8} cached | {:>8} out",
        "supervisor (deepseek):".dimmed(),
        ledger.supervisor.prompt_tokens,
        ledger.supervisor.cached_tokens,
        ledger.supervisor.completion_tokens
    );
    println!(
        "   {} {:>10} in      | {:>8} out (local, free)",
        "worker (minicpm):".dimmed(),
        ledger.worker_input,
        ledger.worker_output
    );
    println!(
        "   {} {:>10} raw     | {:>8} folded",
        "context folding:".dimmed(),
        ledger.context_raw,
        ledger.context_compressed
    );
    let saved = ledger.saved_tokens();
    println!("   {} {saved} tokens never reached the paid model", "saved:".green().bold());
    println!("   {} ${:.4}", "cloud cost:".yellow(), ledger.cloud_cost(&pricing));
    // Honest estimate: what those saved tokens would have cost as input.
    let baseline =
        crate::tokenutil::TokenUsage::new(ledger.supervisor.prompt_tokens + saved, 0, ledger.supervisor.completion_tokens);
    let baseline_cost = pricing.cost_of(&baseline);
    println!(
        "   {} ${:.4} (deepseek-only baseline ≈ ${:.4} → saved ${:.4})",
        "with husk:".yellow(),
        ledger.cloud_cost(&pricing),
        baseline_cost,
        baseline_cost - ledger.cloud_cost(&pricing)
    );
    Ok(())
}
