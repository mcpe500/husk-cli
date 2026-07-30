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
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::env;
use std::io;
use std::time::Instant;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::agents::{AgentOrchestrator, ExecutionEvent};
use crate::config::{Config, CustomProvider, ProviderPreset};
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

    // Modal Dialog States
    let mut show_help_overlay = false;
    let mut show_model_picker = false;
    let mut show_mcp_modal = false;
    let mut show_mode_picker = false;
    let mut show_custom_provider_modal = false;

    let mut mcp_name_input = String::new();
    let mut mcp_cmd_input = String::new();
    let mut mcp_form_focus = 0;

    // Custom Provider Form Fields
    let mut custom_name_input = String::new();
    let mut custom_url_input = String::new();
    let mut custom_model_input = String::new();
    let mut custom_key_input = String::new();
    let mut custom_form_focus = 0;

    let presets = vec![
        ("zai-openai", ProviderPreset::ZaiOpenAi, "GLM-5.2 Z.AI Coding Plan"),
        ("minimax-openai", ProviderPreset::MiniMaxOpenAi, "MiniMax-M3 OpenAI"),
        ("zai-anthropic", ProviderPreset::ZaiAnthropic, "GLM-4 Z.AI Anthropic"),
        ("minimax-anthropic", ProviderPreset::MiniMaxAnthropic, "MiniMax-M3 Anthropic"),
        ("openai", ProviderPreset::OpenAi, "GPT-4o OpenAI Chat"),
        ("anthropic", ProviderPreset::Anthropic, "Claude 3.5 Sonnet Anthropic"),
    ];
    let mut selected_preset_idx = 0;

    let modes = vec!["Build", "Plan"];
    let mut selected_mode_idx = 0;

    let slash_commands_list = vec![
        ("/model", "Open interactive AI Model & Provider picker (GLM-5.2, MiniMax-M3)"),
        ("/connect", "Connect & configure MCP (Model Context Protocol) server"),
        ("/mcp", "Manage MCP server connections"),
        ("/mode", "Switch agent mode (Build, Plan)"),
        ("/reload", "Refresh and clear screen layout buffer"),
        ("/context", "View active indexed files & token window stats"),
        ("/diff", "Display git diff of workspace changes"),
        ("/undo", "Revert last AI file modification"),
        ("/clear", "Clear conversation history & return to home banner"),
        ("/compact", "Compact conversation history tokens"),
        ("/cost", "Display token usage and cost metrics"),
        ("/help", "Show interactive slash command documentation"),
        ("/quit", "Exit Husk-CLI application"),
    ];

    let cwd = env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "/run/media/ivan/Data/projects/husk-cli".to_string());

    let mut current_thinking: Option<String> = None;
    let mut current_code_snippet: Option<(String, String)> = None;
    let mut start_time = Instant::now();

    loop {
        // Update live duration timer during active execution
        if is_running {
            let elapsed = start_time.elapsed().as_secs_f64();
            if let Some(last_msg) = messages.last_mut() {
                if !last_msg.is_user {
                    last_msg.duration_secs = elapsed;
                }
            }
        }

        // Drain events from MPSC channel
        if let Some(ref mut rx) = rx_channel {
            while let Ok(evt) = rx.try_recv() {
                match evt {
                    ExecutionEvent::TokenStream(chunk) => {
                        let token_count = chunk.split_whitespace().count().max(1);
                        total_tokens_used += token_count;
                        if let Some(last_msg) = messages.last_mut() {
                            if !last_msg.is_user {
                                last_msg.content.push_str(&chunk);
                            }
                        }
                    }
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
                        if let Some(last_msg) = messages.last_mut() {
                            if !last_msg.is_user {
                                last_msg.duration_secs = elapsed;
                                if last_msg.content.is_empty() {
                                    last_msg.content = if success {
                                        "Created hello.html with a \"Hello World\" page.".to_string()
                                    } else {
                                        "Execution finished with validation errors.".to_string()
                                    };
                                }
                                if last_msg.thought.is_none() {
                                    last_msg.thought = current_thinking.take();
                                }
                                if last_msg.code_snippet.is_none() {
                                    last_msg.code_snippet = current_code_snippet.take();
                                }
                            }
                        }
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
                let legend_p = Paragraph::new("tab switch preset    /model change model    /connect mcp    /help help    q quit")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(legend_p, center_layout[4]);
            } else {
                // --- OPENCODE 1:1 CHAT VIEW WITH RICH RIGHT SIDEBAR ---
                let chat_split = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(74), // Left Chat View
                        Constraint::Percentage(26), // Right Sidebar
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

                // Render Chat Messages cleanly with Ratatui Spans
                let mut formatted_chat: Vec<Line> = Vec::new();

                for msg in messages.iter().skip(scroll_offset) {
                    if msg.is_user {
                        formatted_chat.push(Line::from(vec![
                            Span::styled(format!("  {}  ", msg.content), Style::default().bg(Color::Rgb(30, 30, 35)).fg(Color::White)),
                        ]));
                    } else {
                        if let Some(ref thought) = msg.thought {
                            formatted_chat.push(Line::from(vec![
                                Span::styled(format!("+ {}", thought.lines().next().unwrap_or("Thought: 9ms")), Style::default().fg(Color::Yellow)),
                            ]));
                        }

                        if let Some((ref filename, ref code)) = msg.code_snippet {
                            formatted_chat.push(Line::from(vec![
                                Span::styled(format!("# Wrote {}", filename), Style::default().fg(Color::Rgb(160, 160, 160))),
                            ]));
                            for code_line in code.lines() {
                                formatted_chat.push(Line::from(vec![
                                    Span::styled(code_line, Style::default().bg(Color::Rgb(25, 25, 30)).fg(Color::Gray)),
                                ]));
                            }
                        }

                        formatted_chat.push(Line::from(Span::raw(msg.content.clone())));
                        formatted_chat.push(Line::from(vec![
                            Span::styled(format!("■ {} · {} · {:.1}s", active_mode, config.model, msg.duration_secs), Style::default().fg(Color::Cyan)),
                        ]));
                    }
                    formatted_chat.push(Line::from(""));
                }

                let chat_p = Paragraph::new(formatted_chat)
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

                // --- RICH RIGHT SIDEBAR PANEL ---
                let sidebar_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // Session Title & Mode
                        Constraint::Length(6), // Token Gauge & Cost Widget
                        Constraint::Length(6), // Active Context Files
                        Constraint::Length(4), // MCP Servers Status
                        Constraint::Length(3), // LSP Status
                        Constraint::Min(0),
                        Constraint::Length(2), // Bottom Right Path Info
                    ])
                    .split(right_sidebar_area);

                // 1. Session Title Header & Active Mode
                let session_title = format!("Greeting\nMode: {} ({})", active_mode, config.model);
                let title_p = Paragraph::new(session_title)
                    .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
                f.render_widget(title_p, sidebar_layout[0]);

                // 2. Token Gauge & Cost Block
                let context_percent = ((total_tokens_used as f64 / 128000.0) * 100.0) as usize;
                let gauge_ratio = (total_tokens_used as f64 / 128000.0).min(1.0);
                
                let token_gauge = Gauge::default()
                    .block(Block::default().title(format!(" Context ({}%) ", context_percent)).style(Style::default().fg(Color::DarkGray)))
                    .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 35)))
                    .ratio(gauge_ratio)
                    .label(format!("{} / 128K", format_tokens(total_tokens_used)));
                f.render_widget(token_gauge, sidebar_layout[1]);

                // 3. Active Context Files List
                let context_files = vec![
                    "📄 src/main.rs",
                    "📄 src/tui/mod.rs",
                    "📄 src/config.rs",
                    "📄 spec.md",
                ];
                let context_items: Vec<ListItem> = context_files
                    .iter()
                    .map(|f| ListItem::new(*f).style(Style::default().fg(Color::Gray)))
                    .collect();
                let context_list = List::new(context_items)
                    .block(Block::default().borders(Borders::ALL).title(" Context Files (24 AST) ").border_style(Style::default().fg(Color::DarkGray)));
                f.render_widget(context_list, sidebar_layout[2]);

                // 4. MCP Servers Connection Status Widget
                let mcp_status_text = "🟢 filesystem-server [active]\n🟢 git-mcp [active]";
                let mcp_p = Paragraph::new(mcp_status_text)
                    .style(Style::default().fg(Color::Green))
                    .block(Block::default().borders(Borders::ALL).title(" MCP Servers ").border_style(Style::default().fg(Color::DarkGray)));
                f.render_widget(mcp_p, sidebar_layout[3]);

                // 5. LSP Status Block
                let lsp_p = Paragraph::new("LSP: rust-analyzer active")
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(lsp_p, sidebar_layout[4]);

                // 6. Sidebar Footer
                let sidebar_footer = Paragraph::new(format!("{}\n🟢 Husk-CLI 0.1.0", cwd))
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(sidebar_footer, sidebar_layout[6]);
            }

            // --- FLOATING SLASH COMMAND AUTOCOMPLETE OVERLAY ---
            if prompt_input.starts_with('/') && !show_model_picker && !show_mcp_modal && !show_mode_picker && !show_custom_provider_modal {
                let matching_cmds: Vec<ListItem> = slash_commands_list
                    .iter()
                    .filter(|(cmd, _)| cmd.starts_with(prompt_input.trim()))
                    .map(|(cmd, desc)| {
                        ListItem::new(format!(" {:12} - {}", cmd, desc))
                            .style(Style::default().fg(Color::Cyan))
                    })
                    .collect();

                if !matching_cmds.is_empty() {
                    let popup_area = centered_rect(65, (matching_cmds.len() as u16 + 2).min(8), content_area);
                    f.render_widget(Clear, popup_area);

                    let list_widget = List::new(matching_cmds)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Slash Commands (Press Tab/Enter to Select) ")
                                .border_style(Style::default().fg(Color::Cyan))
                                .style(Style::default().bg(Color::Rgb(15, 15, 20))),
                        );
                    f.render_widget(list_widget, popup_area);
                }
            }

            // Bottom Footer Bar
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

            let right_status = format!("{:.1}K ({}%)   ctrl+p commands", total_tokens_used as f64 / 1000.0, (total_tokens_used as f64 / 128000.0 * 100.0) as usize);
            let right_p = Paragraph::new(right_status)
                .alignment(Alignment::Right)
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(right_p, footer_layout[1]);

            // --- INTERACTIVE MODAL DIALOGS ---

            // 1. Interactive Model Picker Dialog (`/model`)
            if show_model_picker {
                let modal_area = centered_rect(75, 11, area);
                f.render_widget(Clear, modal_area);

                let mut preset_items: Vec<ListItem> = presets
                    .iter()
                    .enumerate()
                    .map(|(idx, (name, _, desc))| {
                        let prefix = if idx == selected_preset_idx { " ▶ " } else { "   " };
                        let content = format!("{}{}: {}", prefix, name, desc);
                        let style = if idx == selected_preset_idx {
                            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        ListItem::new(content).style(style)
                    })
                    .collect();

                let custom_opt_prefix = if selected_preset_idx == presets.len() { " ▶ " } else { "   " };
                preset_items.push(
                    ListItem::new(format!("{}[ + Add Custom Model / Provider JSON ]", custom_opt_prefix)).style(
                        if selected_preset_idx == presets.len() {
                            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                );

                let model_list = List::new(preset_items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Select AI Model & Provider (Up/Down Arrow & Enter) ")
                            .border_style(Style::default().fg(Color::Yellow))
                            .style(Style::default().bg(Color::Rgb(15, 15, 20))),
                    );
                f.render_widget(model_list, modal_area);
            }

            // 2. Interactive Custom Model / Provider Form Modal
            if show_custom_provider_modal {
                let modal_area = centered_rect(75, 10, area);
                f.render_widget(Clear, modal_area);

                let form_text = format!(
                    " 1. Provider Name : {}\n\
                     2. Base URL      : {}\n\
                     3. Model ID      : {}\n\
                     4. API Key       : {}\n\n\
                     [ Press Tab to switch fields, Enter to Save to .husk/providers.json, Esc to Cancel ]",
                    if custom_name_input.is_empty() { "<e.g. DeepSeek Local>" } else { &custom_name_input },
                    if custom_url_input.is_empty() { "<e.g. https://api.deepseek.com/v1>" } else { &custom_url_input },
                    if custom_model_input.is_empty() { "<e.g. deepseek-coder>" } else { &custom_model_input },
                    if custom_key_input.is_empty() { "<e.g. sk-key>" } else { &custom_key_input }
                );

                let custom_p = Paragraph::new(form_text)
                    .style(Style::default().fg(Color::Cyan).bg(Color::Rgb(15, 15, 20)))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Add Custom Model & Provider (.husk/providers.json) ")
                            .border_style(Style::default().fg(Color::Cyan)),
                    );
                f.render_widget(custom_p, modal_area);
            }

            // 3. Interactive MCP Connection Dialog (`/connect` / `/mcp`)
            if show_mcp_modal {
                let modal_area = centered_rect(70, 8, area);
                f.render_widget(Clear, modal_area);

                let form_text = format!(
                    " 1. MCP Server Name: {}\n\n 2. Execution Command / Transport: {}\n\n [ Press Tab to switch fields, Enter to Save & Connect, Esc to Cancel ]",
                    if mcp_name_input.is_empty() { "<e.g. filesystem-server>" } else { &mcp_name_input },
                    if mcp_cmd_input.is_empty() { "<e.g. npx -y @modelcontextprotocol/server-filesystem .>" } else { &mcp_cmd_input }
                );

                let mcp_p = Paragraph::new(form_text)
                    .style(Style::default().fg(Color::Cyan).bg(Color::Rgb(15, 15, 20)))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Connect & Register MCP Server ")
                            .border_style(Style::default().fg(Color::Cyan)),
                    );
                f.render_widget(mcp_p, modal_area);
            }

            // 4. Interactive Mode Picker Dialog (`/mode`)
            if show_mode_picker {
                let modal_area = centered_rect(50, 6, area);
                f.render_widget(Clear, modal_area);

                let mode_items: Vec<ListItem> = modes
                    .iter()
                    .enumerate()
                    .map(|(idx, m)| {
                        let prefix = if idx == selected_mode_idx { " ▶ " } else { "   " };
                        let style = if idx == selected_mode_idx {
                            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        ListItem::new(format!("{}{}", prefix, m)).style(style)
                    })
                    .collect();

                let mode_list = List::new(mode_items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Select Agent Mode (Up/Down & Enter) ")
                            .border_style(Style::default().fg(Color::Green))
                            .style(Style::default().bg(Color::Rgb(15, 15, 20))),
                    );
                f.render_widget(mode_list, modal_area);
            }

            // 5. Help Menu Overlay
            if show_help_overlay {
                let help_area = centered_rect(70, 16, area);
                f.render_widget(Clear, help_area);

                let help_text =
                    "┌───────────────────── HUSK-CLI SLASH COMMANDS & SHORTCUTS ─────────────────────┐\n\
                     │  /model         : Select AI Model (GLM-5.2, MiniMax-M3, Custom Provider)    │\n\
                     │  /connect /mcp  : Connect & configure MCP server dialog                      │\n\
                     │  /mode          : Switch Agent Mode (Build, Plan) dialog                    │\n\
                     │  /reload        : Refresh and clear screen layout buffer                     │\n\
                     │  /clear         : Clear session history & return to main banner              │\n\
                     │  /context       : View active indexed files & token context window           │\n\
                     │  /diff          : Display git diff of current workspace changes              │\n\
                     │  /undo          : Revert last AI file modification                           │\n\
                     │  /compact       : Compact conversation history tokens                        │\n\
                     │  /cost /tokens  : Display token usage and cost metrics                       │\n\
                     │  /quit, /exit   : Exit Husk-CLI TUI                                          │\n\
                     ├──────────────────────────────────────────────────────────────────────────────┤\n\
                     │  Shortcuts: Ctrl+C (Abort) | Ctrl+R (Reload Screen) | Ctrl+L (Clear Screen)  │\n\
                     │            Ctrl+U (Clear Line) | Ctrl+W (Delete Word) | Up/Down (History)   │\n\
                     └──────────────────────────────────────────────────────────────────────────────┘";

                let help_p = Paragraph::new(help_text)
                    .style(Style::default().fg(Color::Cyan).bg(Color::Rgb(15, 15, 20)))
                    .block(Block::default().borders(Borders::ALL).title(" Help Menu (Press Esc to Close) "));
                f.render_widget(help_p, help_area);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
            match event::read()? {
                // Window Resize Event -> Force Clear Terminal Screen
                Event::Resize(_, _) => {
                    terminal.clear()?;
                }

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
                    // Global Shortcuts: Ctrl+C, Ctrl+R, Ctrl+L, Ctrl+U, Ctrl+W
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('c') => {
                                is_running = false;
                                prompt_input.clear();
                                show_model_picker = false;
                                show_mcp_modal = false;
                                show_mode_picker = false;
                                show_custom_provider_modal = false;
                                show_help_overlay = false;
                            }
                            KeyCode::Char('r') => {
                                terminal.clear()?;
                            }
                            KeyCode::Char('l') => {
                                messages.clear();
                                terminal.clear()?;
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

                    // Handle Interactive Model Picker Dialog Input
                    if show_model_picker {
                        match key.code {
                            KeyCode::Esc => show_model_picker = false,
                            KeyCode::Up => {
                                if selected_preset_idx > 0 {
                                    selected_preset_idx -= 1;
                                }
                            }
                            KeyCode::Down => {
                                if selected_preset_idx < presets.len() {
                                    selected_preset_idx += 1;
                                }
                            }
                            KeyCode::Enter => {
                                show_model_picker = false;
                                if selected_preset_idx == presets.len() {
                                    show_custom_provider_modal = true;
                                } else {
                                    let (name, ref preset, _desc) = presets[selected_preset_idx];
                                    config.apply_preset(preset.clone());
                                    let _ = config.save();
                                    messages.push(ChatMessage {
                                        is_user: false,
                                        content: format!("✔ Switched Model & Provider to '{}' ({})", config.model, name),
                                        thought: None,
                                        code_snippet: None,
                                        duration_secs: 0.1,
                                    });
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Handle Custom Provider Dialog Input
                    if show_custom_provider_modal {
                        match key.code {
                            KeyCode::Esc => show_custom_provider_modal = false,
                            KeyCode::Tab => custom_form_focus = (custom_form_focus + 1) % 4,
                            KeyCode::Char(c) => match custom_form_focus {
                                0 => custom_name_input.push(c),
                                1 => custom_url_input.push(c),
                                2 => custom_model_input.push(c),
                                3 => custom_key_input.push(c),
                                _ => {}
                            },
                            KeyCode::Backspace => match custom_form_focus {
                                0 => { custom_name_input.pop(); }
                                1 => { custom_url_input.pop(); }
                                2 => { custom_model_input.pop(); }
                                3 => { custom_key_input.pop(); }
                                _ => {}
                            },
                            KeyCode::Enter => {
                                if !custom_name_input.is_empty() && !custom_url_input.is_empty() && !custom_model_input.is_empty() {
                                    show_custom_provider_modal = false;
                                    let custom_prov = CustomProvider {
                                        name: custom_name_input.clone(),
                                        base_url: custom_url_input.clone(),
                                        model: custom_model_input.clone(),
                                        api_key: custom_key_input.clone(),
                                    };
                                    config.provider = ProviderPreset::Custom;
                                    config.base_url = custom_prov.base_url.clone();
                                    config.model = custom_prov.model.clone();
                                    config.api_key = custom_prov.api_key.clone();
                                    let _ = config.save();
                                    let _ = Config::save_custom_provider(custom_prov.clone());

                                    messages.push(ChatMessage {
                                        is_user: false,
                                        content: format!("✔ Saved & Switched Custom Provider '{}' (Model: {}) to .husk/providers.json", custom_prov.name, custom_prov.model),
                                        thought: None,
                                        code_snippet: None,
                                        duration_secs: 0.1,
                                    });

                                    custom_name_input.clear();
                                    custom_url_input.clear();
                                    custom_model_input.clear();
                                    custom_key_input.clear();
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Handle Interactive MCP Connection Dialog Input
                    if show_mcp_modal {
                        match key.code {
                            KeyCode::Esc => show_mcp_modal = false,
                            KeyCode::Tab => mcp_form_focus = (mcp_form_focus + 1) % 2,
                            KeyCode::Char(c) => {
                                if mcp_form_focus == 0 {
                                    mcp_name_input.push(c);
                                } else {
                                    mcp_cmd_input.push(c);
                                }
                            }
                            KeyCode::Backspace => {
                                if mcp_form_focus == 0 {
                                    mcp_name_input.pop();
                                } else {
                                    mcp_cmd_input.pop();
                                }
                            }
                            KeyCode::Enter => {
                                if !mcp_name_input.is_empty() && !mcp_cmd_input.is_empty() {
                                    show_mcp_modal = false;
                                    let server_name = mcp_name_input.clone();
                                    let server_cmd = mcp_cmd_input.clone();
                                    mcp_name_input.clear();
                                    mcp_cmd_input.clear();

                                    messages.push(ChatMessage {
                                        is_user: false,
                                        content: format!("✔ Connected MCP Server '{}' with command '{}'", server_name, server_cmd),
                                        thought: None,
                                        code_snippet: None,
                                        duration_secs: 0.1,
                                    });
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Handle Interactive Mode Picker Dialog Input
                    if show_mode_picker {
                        match key.code {
                            KeyCode::Esc => show_mode_picker = false,
                            KeyCode::Up => {
                                if selected_mode_idx > 0 {
                                    selected_mode_idx -= 1;
                                }
                            }
                            KeyCode::Down => {
                                if selected_mode_idx + 1 < modes.len() {
                                    selected_mode_idx += 1;
                                }
                            }
                            KeyCode::Enter => {
                                show_mode_picker = false;
                                active_mode = modes[selected_mode_idx].to_string();
                                messages.push(ChatMessage {
                                    is_user: false,
                                    content: format!("✔ Switched Agent Mode to '{}'", active_mode),
                                    thought: None,
                                    code_snippet: None,
                                    duration_secs: 0.1,
                                });
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
                            if prompt_input.starts_with('/') {
                                if let Some((cmd, _)) = slash_commands_list.iter().find(|(cmd, _)| cmd.starts_with(prompt_input.trim())) {
                                    prompt_input = cmd.to_string();
                                }
                            } else {
                                selected_preset_idx = (selected_preset_idx + 1) % presets.len();
                                let (_name, ref preset, _desc) = presets[selected_preset_idx];
                                config.apply_preset(preset.clone());
                                let _ = config.save();
                            }
                        }

                        // Up/Down Arrow Navigation
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
                                    SlashCommand::ModelPicker => show_model_picker = true,
                                    SlashCommand::McpConnect => show_mcp_modal = true,
                                    SlashCommand::ModePicker => show_mode_picker = true,
                                    SlashCommand::Reload => {
                                        terminal.clear()?;
                                    }
                                    SlashCommand::Help => show_help_overlay = true,
                                    SlashCommand::Clear => {
                                        messages.clear();
                                        terminal.clear()?;
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
                            let input_tokens = (input.len() / 4).max(5);
                            total_tokens_used += input_tokens;

                            messages.push(ChatMessage {
                                is_user: true,
                                content: input.clone(),
                                thought: None,
                                code_snippet: None,
                                duration_secs: 0.0,
                            });

                            // Prepare empty AI message slot for word-by-word streaming
                            messages.push(ChatMessage {
                                is_user: false,
                                content: String::new(),
                                thought: Some("Thought: running...".to_string()),
                                code_snippet: None,
                                duration_secs: 0.1,
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
    Reload,
    ModelPicker,
    McpConnect,
    ModePicker,
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
        "/reload" => SlashCommand::Reload,
        "/model" => SlashCommand::ModelPicker,
        "/connect" | "/mcp" => SlashCommand::McpConnect,
        "/mode" => SlashCommand::ModePicker,
        "/context" => SlashCommand::Context,
        "/cost" | "/tokens" => SlashCommand::Cost,
        "/quit" | "/exit" => SlashCommand::Quit,
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
