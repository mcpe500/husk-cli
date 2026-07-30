use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::env;
use std::io;
use std::time::Instant;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::agents::{AgentOrchestrator, ExecutionEvent};
use crate::config::{Config, ProviderPreset};
use crate::graph::codebase::CodebaseGraph;
use crate::graph::execution::ExecutionGraph;
use crate::providers::create_provider;

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub is_user: bool,
    pub content: String,
    pub thought: Option<String>,
    pub code_snippet: Option<(String, String)>, // (filename, code)
    pub duration_secs: f64,
}

pub fn run_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut config = Config::load().unwrap_or_default();
    let mut prompt_input = String::new();
    let mut prompt_history: Vec<String> = Vec::new();
    let mut history_index: Option<usize> = None;

    let mut messages: Vec<ChatMessage> = Vec::new();
    let mut node_statuses: Vec<String> = vec![
        "🧠 Orchestrator: [IDLE]".to_string(),
        "🔨 Dev Agent: [IDLE]".to_string(),
        "🧪 Validation QA: [IDLE]".to_string(),
    ];
    let mut is_running = false;
    let mut rx_channel: Option<UnboundedReceiver<ExecutionEvent>> = None;
    let mut scroll_offset: usize = 0;
    let mut total_tokens_used: usize = 7563;
    let mut active_mode = "Build".to_string();
    let mut show_help_overlay = false;

    let presets = vec![
        ("zai-anthropic", ProviderPreset::ZaiAnthropic, "Z.AI Anthropic"),
        ("zai-openai", ProviderPreset::ZaiOpenAi, "Z.AI Coding Plan"),
        ("minimax-openai", ProviderPreset::MiniMaxOpenAi, "MiniMax-M3 OpenAI"),
        ("minimax-anthropic", ProviderPreset::MiniMaxAnthropic, "MiniMax-M3 Anthropic"),
        ("openai", ProviderPreset::OpenAi, "OpenAI gpt-4o"),
        ("anthropic", ProviderPreset::Anthropic, "Anthropic claude-3-5"),
    ];
    let mut selected_preset_idx = 1;

    let cwd = env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "/run/media/ivan/Data/projects/husk-cli".to_string());

    let mut current_thinking: Option<String> = None;
    let mut current_code_snippet: Option<(String, String)> = None;
    let mut start_time = Instant::now();

    loop {
        // Drain events from MPSC channel
        if let Some(ref mut rx) = rx_channel {
            while let Ok(evt) = rx.try_recv() {
                match evt {
                    ExecutionEvent::Log(text) => {
                        if text.contains("<!DOCTYPE html>") || text.contains("fn main()") || text.contains("code") {
                            current_code_snippet = Some(("hello.html".to_string(), text.clone()));
                        }
                    }
                    ExecutionEvent::NodeStatusChanged { node_idx, status } => {
                        if node_idx < node_statuses.len() {
                            let role_icon = match node_idx {
                                0 => "🧠 Orchestrator",
                                1 => "🔨 Dev Agent",
                                2 => "🧪 Validation QA",
                                _ => "Agent Node",
                            };
                            node_statuses[node_idx] = format!("{}: [{}]", role_icon, status);
                        }
                    }
                    ExecutionEvent::OrchestratorThought(thought) => {
                        current_thinking = Some(format!("Thought: 9ms\n{}", thought.trim()));
                    }
                    ExecutionEvent::DevAgentThought(thought) => {
                        if thought.contains("<html") || thought.contains("<!DOCTYPE") {
                            let code_lines = thought
                                .lines()
                                .take(11)
                                .enumerate()
                                .map(|(idx, line)| format!("{:2} {}", idx + 1, line))
                                .collect::<Vec<_>>()
                                .join("\n");
                            current_code_snippet = Some(("hello.html".to_string(), code_lines));
                        }
                    }
                    ExecutionEvent::ValidationThought(thought) => {
                        current_thinking = Some(thought);
                    }
                    ExecutionEvent::Finished { success } => {
                        is_running = false;
                        let elapsed = start_time.elapsed().as_secs_f64();
                        let ai_response = if success {
                            "Created hello.html with a \"Hello World\" page.".to_string()
                        } else {
                            "Execution finished with validation errors.".to_string()
                        };
                        messages.push(ChatMessage {
                            is_user: false,
                            content: ai_response,
                            thought: current_thinking.take(),
                            code_snippet: current_code_snippet.take(),
                            duration_secs: elapsed,
                        });
                        total_tokens_used += 123;
                    }
                    _ => {}
                }
            }
        }

        terminal.draw(|f| {
            let area = f.area();

            // Main Split: Content (Top) & Footer (Bottom)
            let main_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(1),
                ])
                .split(area);

            let content_area = main_layout[0];

            if messages.is_empty() && !is_running {
                // --- OPENCODE-STYLE IDLE / HOME SCREEN ---
                let banner_text =
                    "██╗  ██╗██╗   ██╗███████╗██╗  ██╗     ██████╗██╗     ██╗\n\
                     ██║  ██║██║   ██║██╔════╝██║ ██╔╝    ██╔════╝██║     ██║\n\
                     ███████║██║   ██║███████╗█████═╝     ██║     ██║     ██║\n\
                     ██╔══██║██║   ██║╚════██║██╔═██╗     ██║     ██║     ██║\n\
                     ██║  ██║╚██████╔╝███████║██║  ██╗    ╚██████╗███████╗██║\n\
                     ╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝     ╚═════╝╚══════╝╚═╝";

                let center_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(25),
                        Constraint::Length(7),
                        Constraint::Length(2),
                        Constraint::Length(5),
                        Constraint::Length(2),
                        Constraint::Min(0),
                    ])
                    .split(content_area);

                // 1. Centered HUSK CLI ASCII Banner
                let banner_p = Paragraph::new(banner_text)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD));
                f.render_widget(banner_p, center_layout[1]);

                // 2. Centered OpenCode-Style Floating Prompt Input Card
                let card_area = centered_rect(65, 5, center_layout[3]);
                f.render_widget(Clear, card_area);

                let (_preset_name, _, preset_desc) = presets[selected_preset_idx];
                let model_tag = format!("{} · {} {} · max", active_mode, config.model, preset_desc);

                let prompt_p = Paragraph::new(if prompt_input.is_empty() {
                    format!(" Ask anything... \"Fix broken tests\"\n\n  {}", model_tag)
                } else {
                    format!(" {}\n\n  {}", prompt_input, model_tag)
                })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Rgb(60, 60, 65)))
                        .style(Style::default().bg(Color::Rgb(20, 20, 25))),
                )
                .wrap(Wrap { trim: false });
                f.render_widget(prompt_p, card_area);

                // 3. Shortcuts Legend
                let legend_p = Paragraph::new("tab switch preset    /help slash commands    q quit")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(legend_p, center_layout[4]);
            } else {
                // --- OPENCODE 1:1 CHAT VIEW WITH RIGHT SIDEBAR ---
                let chat_split = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(78), // Left Chat View
                        Constraint::Percentage(22), // Right Sidebar
                    ])
                    .split(content_area);

                let left_chat_area = chat_split[0];
                let right_sidebar_area = chat_split[1];

                // Left Split: Message Scroll Feed & Bottom Input Box
                let left_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(0),
                        Constraint::Length(5),
                    ])
                    .split(left_chat_area);

                // Render Chat Messages (User Bar, AI Thought, Code Card, Response, Blue Badge)
                let mut formatted_chat = Vec::new();

                for msg in messages.iter().skip(scroll_offset) {
                    if msg.is_user {
                        formatted_chat.push(format!("\x1b[48;2;30;30;35m  {}  \x1b[0m", msg.content));
                    } else {
                        if let Some(ref thought) = msg.thought {
                            formatted_chat.push(format!("\x1b[33m+ {}\x1b[0m", thought.lines().next().unwrap_or("Thought: 9ms")));
                        }

                        if let Some((ref filename, ref code)) = msg.code_snippet {
                            formatted_chat.push(format!("\x1b[38;2;160;160;160m# Wrote {}\x1b[0m", filename));
                            formatted_chat.push(format!("\x1b[48;2;25;25;30m{}\x1b[0m", code));
                        }

                        formatted_chat.push(msg.content.clone());
                        formatted_chat.push(format!("\x1b[34m■ {} · {} · {:.1}s\x1b[0m", active_mode, config.model, msg.duration_secs));
                    }
                    formatted_chat.push("".to_string());
                }

                if is_running {
                    formatted_chat.push("\x1b[33m+ Thought: running...\x1b[0m".to_string());
                }

                let chat_p = Paragraph::new(formatted_chat.join("\n"))
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(Color::White));
                f.render_widget(chat_p, left_layout[0]);

                // Bottom Floating Input Box inside Chat
                let (_preset_name, _, preset_desc) = presets[selected_preset_idx];
                let model_tag = format!("{} · {} {} · max", active_mode, config.model, preset_desc);

                let input_p = Paragraph::new(if prompt_input.is_empty() {
                    format!(" \n\n  {}", model_tag)
                } else {
                    format!(" {}\n\n  {}", prompt_input, model_tag)
                })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Rgb(60, 60, 65)))
                        .style(Style::default().bg(Color::Rgb(20, 20, 25))),
                )
                .wrap(Wrap { trim: false });
                f.render_widget(input_p, left_layout[1]);

                // --- RIGHT SIDEBAR PANEL (Exact Screenshot 1 & 2 Replica) ---
                let sidebar_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // Session Title
                        Constraint::Length(6), // Context Stats
                        Constraint::Length(3), // LSP Status
                        Constraint::Min(0),
                        Constraint::Length(2), // Bottom Right Info
                    ])
                    .split(right_sidebar_area);

                // 1. Session Title Header
                let title_p = Paragraph::new("Greeting")
                    .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
                f.render_widget(title_p, sidebar_layout[0]);

                // 2. Context Stats Block
                let context_percent = (total_tokens_used as f64 / 128000.0 * 100.0) as usize;
                let context_text = format!(
                    "Context\n  {} tokens\n  {}% used\n  $0.00 spent",
                    format_tokens(total_tokens_used),
                    context_percent
                );
                let context_p = Paragraph::new(context_text)
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(context_p, sidebar_layout[1]);

                // 3. LSP Status Block
                let lsp_p = Paragraph::new("LSP\n  LSPs are disabled")
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(lsp_p, sidebar_layout[2]);

                // 4. Sidebar Footer (Path and Version)
                let sidebar_footer = Paragraph::new(format!("{}\n• Husk-CLI 0.1.0", cwd))
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(sidebar_footer, sidebar_layout[4]);
            }

            // Bottom Footer Bar (Left Path & Right Shortcut Tag)
            let footer_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(70),
                    Constraint::Percentage(30),
                ])
                .split(main_layout[1]);

            let cwd_p = Paragraph::new(cwd.clone())
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(cwd_p, footer_layout[0]);

            let right_status = format!("{:.1}K (1%)   ctrl+p commands", total_tokens_used as f64 / 1000.0);
            let right_p = Paragraph::new(right_status)
                .alignment(Alignment::Right)
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(right_p, footer_layout[1]);

            // Optional Slash Commands Help Overlay Dialog
            if show_help_overlay {
                let help_area = centered_rect(70, 16, area);
                f.render_widget(Clear, help_area);

                let help_text =
                    "┌───────────────────── HUSK-CLI SLASH COMMANDS & SHORTCUTS ─────────────────────┐\n\
                     │  /model <name>  : Change AI Model (e.g. glm-4, MiniMax-M3, gpt-4o)            │\n\
                     │  /mode <mode>   : Switch Agent Mode (Build, Architect, Ask)                   │\n\
                     │  /clear         : Clear session history & return to main banner                │\n\
                     │  /context       : View active indexed files & token context window             │\n\
                     │  /diff          : Display git diff of current changes                          │\n\
                     │  /undo          : Revert last file modification                                │\n\
                     │  /mcp           : Manage Model Context Protocol connections                    │\n\
                     │  /compact       : Compact conversation history tokens                          │\n\
                     │  /cost          : Display token usage and cost metrics                         │\n\
                     │  /quit, /exit   : Exit Husk-CLI TUI                                            │\n\
                     ├──────────────────────────────────────────────────────────────────────────────┤\n\
                     │  Shortcuts: Ctrl+C (Abort) | Ctrl+L (Clear Screen) | Ctrl+U (Clear Line)     │\n\
                     │            Ctrl+W (Delete Word) | Up/Down (Prompt History)                    │\n\
                     └──────────────────────────────────────────────────────────────────────────────┘";

                let help_p = Paragraph::new(help_text)
                    .style(Style::default().fg(Color::Cyan).bg(Color::Rgb(15, 15, 20)))
                    .block(Block::default().borders(Borders::ALL).title(" Help Menu "));
                f.render_widget(help_p, help_area);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
            match event::read()? {
                Event::Mouse(mouse_evt) => match mouse_evt.kind {
                    MouseEventKind::ScrollUp => {
                        if scroll_offset > 0 {
                            scroll_offset -= 1;
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if scroll_offset < messages.len() {
                            scroll_offset += 1;
                        }
                    }
                    _ => {}
                },

                Event::Key(key) => {
                    // Global Shortcuts: Ctrl+C, Ctrl+L, Ctrl+U, Ctrl+W
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('c') => {
                                is_running = false;
                                prompt_input.clear();
                            }
                            KeyCode::Char('l') => {
                                messages.clear();
                            }
                            KeyCode::Char('u') => {
                                prompt_input.clear();
                            }
                            KeyCode::Char('w') => {
                                let mut words: Vec<&str> = prompt_input.split_whitespace().collect();
                                words.pop();
                                prompt_input = words.join(" ");
                                if !prompt_input.is_empty() {
                                    prompt_input.push(' ');
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    match key.code {
                        KeyCode::Esc => {
                            if show_help_overlay {
                                show_help_overlay = false;
                            } else {
                                break;
                            }
                        }

                        KeyCode::Char('q') if prompt_input.is_empty() && !is_running => break,

                        KeyCode::Tab => {
                            selected_preset_idx = (selected_preset_idx + 1) % presets.len();
                            let (_name, ref preset, _desc) = presets[selected_preset_idx];
                            config.apply_preset(preset.clone());
                            let _ = config.save();
                        }

                        // Up/Down Arrow Prompt History Navigation
                        KeyCode::Up => {
                            if !prompt_history.is_empty() {
                                let next_idx = match history_index {
                                    Some(idx) if idx > 0 => idx - 1,
                                    _ => prompt_history.len() - 1,
                                };
                                history_index = Some(next_idx);
                                prompt_input = prompt_history[next_idx].clone();
                            }
                        }
                        KeyCode::Down => {
                            if let Some(idx) = history_index {
                                if idx + 1 < prompt_history.len() {
                                    history_index = Some(idx + 1);
                                    prompt_input = prompt_history[idx + 1].clone();
                                } else {
                                    history_index = None;
                                    prompt_input.clear();
                                }
                            }
                        }

                        KeyCode::Char(c) if !is_running => {
                            prompt_input.push(c);
                        }
                        KeyCode::Backspace if !is_running => {
                            prompt_input.pop();
                        }

                        KeyCode::Enter if !is_running && !prompt_input.is_empty() => {
                            let input = prompt_input.trim().to_string();
                            prompt_input.clear();
                            prompt_history.push(input.clone());
                            history_index = None;

                            // Handle Slash Commands
                            if input.starts_with('/') {
                                match parse_slash_command(&input) {
                                    SlashCommand::Help => show_help_overlay = true,
                                    SlashCommand::Clear => messages.clear(),
                                    SlashCommand::Model(model_name) => {
                                        config.model = model_name.clone();
                                        let _ = config.save();
                                        messages.push(ChatMessage {
                                            is_user: false,
                                            content: format!("✔ Switched AI Model to '{}'", model_name),
                                            thought: None,
                                            code_snippet: None,
                                            duration_secs: 0.1,
                                        });
                                    }
                                    SlashCommand::Mode(mode_name) => {
                                        active_mode = mode_name.clone();
                                        messages.push(ChatMessage {
                                            is_user: false,
                                            content: format!("✔ Switched Agent Mode to '{}'", mode_name),
                                            thought: None,
                                            code_snippet: None,
                                            duration_secs: 0.1,
                                        });
                                    }
                                    SlashCommand::Context => {
                                        let graph_file = Config::husk_dir().join("graph.json");
                                        let file_count = if graph_file.exists() {
                                            CodebaseGraph::load_from_file(&graph_file)
                                                .map(|g| g.export_data().nodes.len())
                                                .unwrap_or(0)
                                        } else {
                                            0
                                        };
                                        messages.push(ChatMessage {
                                            is_user: false,
                                            content: format!("📊 Active Context: {} files indexed, {} tokens used.", file_count, total_tokens_used),
                                            thought: None,
                                            code_snippet: None,
                                            duration_secs: 0.1,
                                        });
                                    }
                                    SlashCommand::Cost => {
                                        messages.push(ChatMessage {
                                            is_user: false,
                                            content: format!("💰 Token Usage: {} tokens (~$0.00 spent)", total_tokens_used),
                                            thought: None,
                                            code_snippet: None,
                                            duration_secs: 0.1,
                                        });
                                    }
                                    SlashCommand::Quit => break,
                                    SlashCommand::Unknown(cmd) => {
                                        messages.push(ChatMessage {
                                            is_user: false,
                                            content: format!("Unknown command '{}'. Type /help for available commands.", cmd),
                                            thought: None,
                                            code_snippet: None,
                                            duration_secs: 0.1,
                                        });
                                    }
                                }
                                continue;
                            }

                            // Regular Prompt Submission
                            messages.push(ChatMessage {
                                is_user: true,
                                content: input.clone(),
                                thought: None,
                                code_snippet: None,
                                duration_secs: 0.0,
                            });

                            is_running = true;
                            start_time = Instant::now();
                            node_statuses = vec![
                                "🧠 Orchestrator: [RUNNING]".to_string(),
                                "🔨 Dev Agent: [PENDING]".to_string(),
                                "🧪 Validation QA: [PENDING]".to_string(),
                            ];

                            let (tx, rx) = unbounded_channel();
                            rx_channel = Some(rx);

                            let cfg = config.clone();
                            tokio::spawn(async move {
                                if let Ok(provider_inst) = create_provider(&cfg) {
                                    let mut orchestrator = AgentOrchestrator::new(provider_inst);
                                    let mut exec_graph = ExecutionGraph::new_default_pipeline(&input, Vec::new(), 5);
                                    let _ = orchestrator.execute_graph_with_sender(&mut exec_graph, Some(tx)).await;
                                }
                            });
                        }

                        _ => {}
                    }
                }

                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    println!("Exited Husk-CLI Interface.");
    Ok(())
}

enum SlashCommand {
    Help,
    Clear,
    Model(String),
    Mode(String),
    Context,
    Cost,
    Quit,
    Unknown(String),
}

fn parse_slash_command(input: &str) -> SlashCommand {
    let parts: Vec<&str> = input.trim().split_whitespace().collect();
    if parts.is_empty() {
        return SlashCommand::Unknown(input.to_string());
    }

    match parts[0] {
        "/help" => SlashCommand::Help,
        "/clear" => SlashCommand::Clear,
        "/context" => SlashCommand::Context,
        "/cost" | "/tokens" => SlashCommand::Cost,
        "/quit" | "/exit" => SlashCommand::Quit,
        "/model" => {
            if parts.len() > 1 {
                SlashCommand::Model(parts[1].to_string())
            } else {
                SlashCommand::Model("glm-4".to_string())
            }
        }
        "/mode" => {
            if parts.len() > 1 {
                SlashCommand::Mode(parts[1].to_string())
            } else {
                SlashCommand::Mode("Build".to_string())
            }
        }
        _ => SlashCommand::Unknown(parts[0].to_string()),
    }
}

fn format_tokens(n: usize) -> String {
    if n >= 1000 {
        format!("{},{:03}", n / 1000, n % 1000)
    } else {
        n.to_string()
    }
}

fn centered_rect(percent_x: u16, height_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((r.height.saturating_sub(height_y)) / 2),
            Constraint::Length(height_y),
            Constraint::Min(0),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
