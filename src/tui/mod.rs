//! husk TUI — opencode-style three-zone layout.
//!
//! ┌ top bar: mode · session · model · worker · token stats ┐
//! │ conversation scrollback                                │
//! └ input box (modal Normal/Insert, readline shortcuts)    ┘
//!
//! Leader key Ctrl+X: n new session · t theme · q quit. Tab toggles the
//! local MiniCPM worker for subsequent turns. Slash commands: /new
//! /sessions /stats /model <id> /quit. Prefixes: @file and !cmd route raw
//! material through the worker fold instead of the supervisor window.

pub mod input;
pub mod theme;

use std::io::{self, Stdout};
use std::sync::Arc;

use anyhow::Result;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::agents::prefix;
use crate::agents::session::SupervisorWorkerSession;
use crate::config::Config;
use crate::db::Database;
use crate::local;
use crate::tokenutil::TokenLedger;

use input::{InputState, Mode};
use theme::Theme;

const MAX_SCROLLBACK: usize = 500;

#[derive(Debug)]
pub enum TuiEvent {
    Delta(String),
    Status(String),
    Done { worker_used: bool, escalated: bool, ledger: TokenLedger },
    Failed(String),
}

struct App {
    db: Arc<Database>,
    session: Arc<tokio::sync::Mutex<SupervisorWorkerSession>>,
    session_id: String,
    session_title: String,
    config: Config,
    theme: Theme,
    input: InputState,
    scrollback: Vec<(Span<'static>, String)>,
    scroll: usize,
    busy: bool,
    streaming: bool,
    worker_on: bool,
    ledger: TokenLedger,
    pending_leader: bool,
    last_ctrl_c: Option<std::time::Instant>,
    model_field: String,
}

impl App {
    fn push(&mut self, kind: Span<'static>, text: String) {
        self.scrollback.push((kind, text));
        if self.scrollback.len() > MAX_SCROLLBACK {
            self.scrollback.remove(0);
        }
        self.scroll = 0; // stick to bottom on new content
    }

    fn role_span(&self, label: &'static str, color_key: &'static str) -> Span<'static> {
        Span::styled(label.to_string(), Style::default().fg(self.theme.color(color_key)))
    }

    fn sys_span(&self) -> Span<'static> {
        self.role_span("sys", "dim")
    }

    fn assistant_span(&self) -> Span<'static> {
        self.role_span("husk", "accent")
    }

    fn new_session(&mut self) {
        if let Ok(s) = self.db.create_session("husk", &self.config.model, None) {
            self.session_title = s.title.clone();
            self.session_id = s.id.clone();
            self.push(self.sys_span(), format!("new session {}", s.id));
        }
    }
}

pub fn run_tui() -> Result<()> {
    let config = Config::load()?;
    let db = Arc::new(Database::open(&Config::db_path())?);
    let session_rec = db.create_session("husk", &config.model, None)?;

    let worker = local::create_local_engine(&config.local);
    let worker_on = worker.is_some();
    let provider = crate::providers::create_provider(&config)?;
    let session = Arc::new(tokio::sync::Mutex::new(
        SupervisorWorkerSession::new(provider, worker, db.clone())
            .with_supervisor_model(config.model.clone()),
    ));

    let mut app = App {
        db,
        session,
        session_id: session_rec.id,
        session_title: session_rec.title,
        model_field: config.model.clone(),
        config,
        theme: Theme::default_theme(),
        input: InputState::default(),
        scrollback: Vec::new(),
        scroll: 0,
        busy: false,
        streaming: false,
        worker_on,
        ledger: TokenLedger::default(),
        pending_leader: false,
        last_ctrl_c: None,
    };
    app.push(app.sys_span(), "welcome to husk — Ctrl+X q quit · i/a edit · Esc scroll · @file !cmd".into());

    let terminal = setup_terminal()?;
    let result = event_loop(terminal, &mut app);
    restore_terminal()?;
    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal() -> Result<()> {
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::LeaveAlternateScreen)?;
    crossterm::terminal::disable_raw_mode()?;
    Ok(())
}

fn event_loop(mut terminal: Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    let (event_tx, mut event_rx): (UnboundedSender<TuiEvent>, UnboundedReceiver<TuiEvent>) =
        unbounded_channel();

    loop {
        terminal.draw(|f| draw(f, app))?;

        if crossterm::event::poll(std::time::Duration::from_millis(80))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    handle_key(app, key, &event_tx);
                }
            }
        }

        // Drain worker events.
        while let Ok(event) = event_rx.try_recv() {
            match event {
                TuiEvent::Delta(delta) => {
                    if !app.streaming {
                        app.push(app.assistant_span(), "husk: ".into());
                        app.streaming = true;
                    }
                    if let Some((_, last)) = app.scrollback.last_mut() {
                        last.push_str(&delta);
                    }
                }
                TuiEvent::Status(status) => app.push(app.sys_span(), status),
                TuiEvent::Done { worker_used, escalated, ledger } => {
                    app.ledger = ledger;
                    app.busy = false;
                    app.streaming = false;
                    app.push(
                        app.sys_span(),
                        format!(
                            "─ worker:{} escalated:{} saved:{}tok cost:${:.4}",
                            if worker_used { "on" } else { "off" },
                            if escalated { "yes" } else { "no" },
                            ledger.saved_tokens(),
                            ledger.cloud_cost(&crate::agents::session::DEEPSEEK_FLASH_PRICING)
                        ),
                    );
                    let _ = escalated;
                }
                TuiEvent::Failed(err) => {
                    app.busy = false;
                    app.streaming = false;
                    app.push(
                        Span::styled("err", Style::default().fg(app.theme.color("error"))),
                        err,
                    );
                }
            }
        }
    }
}

fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    event_tx: &UnboundedSender<TuiEvent>,
) {
    use crossterm::event::KeyCode;

    // Ctrl+C twice to quit.
    if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
        && key.code == KeyCode::Char('c')
    {
        let now = std::time::Instant::now();
        if app.last_ctrl_c.map(|t| now.duration_since(t).as_millis() < 1200).unwrap_or(false) {
            std::process::exit(0);
        }
        app.last_ctrl_c = Some(now);
        return;
    }

    // Leader handling: Ctrl+X then one key.
    if app.pending_leader {
        app.pending_leader = false;
        match key.code {
            KeyCode::Char('n') => app.new_session(),
            KeyCode::Char('t') => {
                app.theme = app.theme.cycle();
                app.push(app.sys_span(), format!("theme → {}", app.theme.name));
            }
            KeyCode::Char('q') => {
                restore_terminal().ok();
                std::process::exit(0);
            }
            other => app.push(app.sys_span(), format!("unknown leader key {other:?}")),
        }
        return;
    }

    if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('x') => {
                app.pending_leader = true;
                return;
            }
            KeyCode::Char('a') => return app.input.home(),
            KeyCode::Char('e') => return app.input.end(),
            KeyCode::Char('k') => return app.input.kill_to_end(),
            KeyCode::Char('u') => return app.input.kill_to_start(),
            _ => {}
        }
    }

    match key.code {
        KeyCode::Esc => {
            app.input.handle_esc();
        }
        KeyCode::Tab => {
            if app.input.mode == Mode::Normal {
                toggle_worker(app);
            }
        }
        KeyCode::Enter => {
            if app.input.mode == Mode::Insert && !app.busy {
                let raw = app.input.submit();
                submit(app, &raw, event_tx);
            }
        }
        KeyCode::Backspace => {
            if app.input.mode == Mode::Insert {
                app.input.backspace();
            }
        }
        KeyCode::Delete => app.input.delete(),
        KeyCode::Left => app.input.left(),
        KeyCode::Right => app.input.right(),
        KeyCode::Up => app.scroll = (app.scroll + 1).min(app.scrollback.len()),
        KeyCode::Down => app.scroll = app.scroll.saturating_sub(1),
        KeyCode::Char(c) => {
            if app.input.mode == Mode::Normal {
                match c {
                    'k' => app.scroll = (app.scroll + 1).min(app.scrollback.len()),
                    'j' | 'G' => app.scroll = 0,
                    _ => {
                        app.input.handle_char(c);
                    }
                }
            } else {
                app.input.handle_char(c);
            }
        }
        _ => {}
    }
}

fn toggle_worker(app: &mut App) {
    if app.worker_on {
        app.worker_on = false;
        if let Ok(mut session) = app.session.try_lock() {
            session.set_worker(None);
        }
        app.push(app.sys_span(), "worker → off (cloud-only)".into());
    } else {
        match local::create_local_engine(&app.config.local) {
            Some(engine) => {
                let describe = engine.describe();
                app.worker_on = true;
                if let Ok(mut session) = app.session.try_lock() {
                    session.set_worker(Some(engine));
                }
                app.push(app.sys_span(), format!("worker → on ({describe})"));
            }
            None => {
                app.worker_on = false;
                app.push(
                    Span::styled("warn", Style::default().fg(app.theme.color("warn"))),
                    "local worker unavailable (needs --features local + enabled config)".into(),
                );
            }
        }
    }
}

fn submit(app: &mut App, raw: &str, event_tx: &UnboundedSender<TuiEvent>) {
    let raw = raw.trim();
    if raw.is_empty() {
        return;
    }

    // Slash commands.
    if let Some(rest) = raw.strip_prefix('/') {
        let mut parts = rest.split_whitespace();
        match parts.next() {
            Some("quit" | "q") => {
                restore_terminal().ok();
                std::process::exit(0);
            }
            Some("new") => app.new_session(),
            Some("sessions") => {
                if let Ok(list) = app.db.list_sessions(10) {
                    for s in list {
                        app.push(app.sys_span(), format!("{}  {} [{}]", s.id, s.title, s.updated_at));
                    }
                }
            }
            Some("stats") => {
                if let Ok(l) = app.db.session_ledger(&app.session_id) {
                    app.ledger = l;
                    app.push(app.sys_span(), format!("saved {} tokens so far", app.ledger.saved_tokens()));
                }
            }
            Some("model") => {
                let model = parts.collect::<Vec<_>>().join(" ");
                if !model.is_empty() {
                    app.config.model = model.clone();
                    if let Ok(mut s) = app.session.try_lock() {
                        s.supervisor_model = model.clone();
                    }
                    app.model_field = model;
                    let _ = app.config.save();
                    app.push(app.sys_span(), "model updated".into());
                }
            }
            other => app.push(app.sys_span(), format!("unknown command {other:?} — /new /sessions /stats /model /quit")),
        }
        return;
    }

    // Parse @file / !cmd prefixes.
    let parsed = prefix::parse(raw);
    let user_span = app.role_span("you", "ok");
    app.push(user_span, format!("you: {}", parsed.text));

    app.busy = true;
    let session = app.session.clone();
    let session_id = app.session_id.clone();
    let docs = parsed.docs;
    let text = parsed.text;
    let tx = event_tx.clone();
    let (delta_tx, mut delta_rx) = unbounded_channel::<String>();

    // Relay provider deltas into TUI events.
    let relay_tx = tx.clone();
    tokio::spawn(async move {
        while let Some(delta) = delta_rx.recv().await {
            let _ = relay_tx.send(TuiEvent::Delta(delta));
        }
    });

    tokio::spawn(async move {
        let mut session = session.lock().await;
        match session.send_streamed(&session_id, &text, &docs, Some(delta_tx)).await {
            Ok(outcome) => {
                let _ = tx.send(TuiEvent::Done {
                    worker_used: outcome.worker_used,
                    escalated: outcome.worker_escalated,
                    ledger: outcome.ledger,
                });
            }
            Err(e) => {
                let _ = tx.send(TuiEvent::Failed(format!("{e:#}")));
            }
        }
    });
}

fn draw(f: &mut ratatui::Frame, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(3),
    ])
    .split(f.area());

    // --- top bar ---
    let saved = app.ledger.saved_tokens();
    let top = Line::from(vec![
        Span::styled(
            format!(" {} ", if app.input.mode == Mode::Insert { "INSERT" } else { "NORMAL" }),
            Style::default().fg(app.theme.color("accent")).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" session:{} ", truncate(&app.session_title, 24)),
            Style::default().fg(app.theme.color("text")),
        ),
        Span::styled(
            format!(" model:{} ", truncate(&app.model_field, 34)),
            Style::default().fg(app.theme.color("dim")),
        ),
        Span::styled(
            format!(" worker:{} ", if app.worker_on { "on" } else { "off" }),
            Style::default().fg(app.theme.color(if app.worker_on { "ok" } else { "dim" })),
        ),
        Span::styled(
            format!(" sup:{}tok saved:{saved}tok cost:${:.4} ", app.ledger.supervisor.prompt_tokens, app.ledger.cloud_cost(&crate::agents::session::DEEPSEEK_FLASH_PRICING)),
            Style::default().fg(app.theme.color("warn")),
        ),
    ]);
    f.render_widget(Paragraph::new(top), chunks[0]);

    // --- scrollback ---
    let area_height = chunks[1].height.saturating_sub(2) as usize;
    let total = app.scrollback.len();
    let visible_end = total.saturating_sub(app.scroll);
    let visible_start = visible_end.saturating_sub(area_height * 3);
    let lines: Vec<Line> = app.scrollback[visible_start..visible_end.max(visible_start)]
        .iter()
        .map(|(label, text)| {
            Line::from(vec![label.clone(), Span::styled(format!(" {text}"), Style::default().fg(app.theme.color("text")))])
        })
        .collect();
    let scroll_from_end = area_height.saturating_sub(1);
    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll as u16, 0))
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(para, chunks[1]);
    let _ = scroll_from_end;

    // --- input ---
    let mode_hint = if app.input.mode == Mode::Insert {
        "type — Enter send · Esc scroll · Ctrl+X leader"
    } else {
        "normal — j/k scroll · i edit · Tab worker"
    };
    let prompt_label = if app.input.mode == Mode::Insert { "> " } else { ": " };
    let input_text = Line::from(vec![
        Span::styled(prompt_label, Style::default().fg(app.theme.color("accent")).add_modifier(Modifier::BOLD)),
        Span::styled(app.input.text.clone(), Style::default().fg(app.theme.color("text"))),
        Span::styled("▌", Style::default().fg(app.theme.color("accent"))),
        Span::styled(format!("  {mode_hint}"), Style::default().fg(app.theme.color("dim"))),
    ]);
    f.render_widget(
        Paragraph::new(input_text).block(Block::default().borders(Borders::TOP)),
        chunks[2],
    );
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max.saturating_sub(1)]
    }
}

// Keep the compiler happy: the event loop is sync but awaits briefly via the
// runtime handle owned by main.
async fn _unused() {}
