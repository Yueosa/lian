//! 自定义命令模式（Shell 模式）
//! 用户可以自由输入任意命令并查看流式输出，支持历史记录。

use super::input::{str_delete_back, str_delete_forward, str_insert_char};
use super::layout;
use super::state::{App, AppEvent, AppMode, ShellPhase};
use super::theme::{BRIGHT_WHITE, DIM, PINK};
use crate::tui::input::InputBox;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use tokio::sync::mpsc;

fn input_box_from_app(app: &App) -> InputBox {
    let mut ib = InputBox::new();
    for c in app.shell.input.chars() {
        ib.insert(c);
    }
    ib.move_home();
    for _ in 0..app.shell.cursor {
        ib.move_right();
    }
    ib
}

/// 处理 Shell 模式按键，返回 true 表示已消费该按键
pub fn handle_shell_key(
    key: KeyEvent,
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
    term_height: u16,
) -> bool {
    match app.shell.phase {
        ShellPhase::Input => handle_input_key(key, app, tx),
        ShellPhase::Running => handle_running_key(key, app),
        ShellPhase::Done | ShellPhase::Error => handle_done_key(key, app, tx, term_height),
    }
}

fn handle_input_key(key: KeyEvent, app: &mut App, tx: &mpsc::Sender<AppEvent>) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Dashboard;
            app.reset_shell_state();
            true
        }
        KeyCode::Enter => {
            let cmd = app.shell.input.trim().to_string();
            if cmd.is_empty() {
                return true;
            }
            // 保存到历史
            if app.shell.history.last().map(|s| s.as_str()) != Some(&cmd) {
                app.shell.history.push(cmd.clone());
            }
            app.shell.history_idx = None;
            spawn_shell_task(app, tx, cmd);
            true
        }
        KeyCode::Up => {
            // 历史记录向前浏览
            if app.shell.history.is_empty() {
                return true;
            }
            let new_idx = match app.shell.history_idx {
                None => app.shell.history.len() - 1,
                Some(0) => 0,
                Some(i) => i - 1,
            };
            app.shell.history_idx = Some(new_idx);
            if let Some(hist_cmd) = app.shell.history.get(new_idx) {
                app.shell.input = hist_cmd.clone();
                app.shell.cursor = app.shell.input.chars().count();
            }
            true
        }
        KeyCode::Down => {
            // 历史记录向后浏览
            match app.shell.history_idx {
                None => {}
                Some(i) if i + 1 >= app.shell.history.len() => {
                    app.shell.history_idx = None;
                    app.shell.input.clear();
                    app.shell.cursor = 0;
                }
                Some(i) => {
                    let new_idx = i + 1;
                    app.shell.history_idx = Some(new_idx);
                    if let Some(hist_cmd) = app.shell.history.get(new_idx) {
                        app.shell.input = hist_cmd.clone();
                        app.shell.cursor = app.shell.input.chars().count();
                    }
                }
            }
            true
        }
        KeyCode::Backspace => {
            str_delete_back(&mut app.shell.input, &mut app.shell.cursor);
            true
        }
        KeyCode::Delete => {
            str_delete_forward(&mut app.shell.input, &mut app.shell.cursor);
            true
        }
        KeyCode::Left => {
            if app.shell.cursor > 0 {
                app.shell.cursor -= 1;
            }
            true
        }
        KeyCode::Right => {
            let max = app.shell.input.chars().count();
            if app.shell.cursor < max {
                app.shell.cursor += 1;
            }
            true
        }
        KeyCode::Home => {
            app.shell.cursor = 0;
            true
        }
        KeyCode::End => {
            app.shell.cursor = app.shell.input.chars().count();
            true
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                return false;
            }
            str_insert_char(&mut app.shell.input, &mut app.shell.cursor, c);
            true
        }
        _ => false,
    }
}

fn handle_running_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        KeyCode::Esc => {
            // 取消正在运行的命令
            crate::package_manager::cancel_update();
            true
        }
        KeyCode::Up => {
            app.shell.scroll = app.shell.scroll.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            app.shell.scroll += 1;
            true
        }
        _ => false,
    }
}

fn handle_done_key(
    key: KeyEvent,
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
    term_height: u16,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            // 返回输入模式，准备下一条命令
            app.shell.phase = ShellPhase::Input;
            app.shell.input.clear();
            app.shell.cursor = 0;
            app.shell.lines.clear();
            app.shell.output = None;
            app.shell.progress.clear();
            app.shell.scroll = 0;
            true
        }
        KeyCode::Enter => {
            // 快速再次执行同一条命令（如果历史非空）
            if let Some(last) = app.shell.history.last().cloned() {
                app.shell.phase = ShellPhase::Input;
                app.shell.input = last.clone();
                app.shell.cursor = app.shell.input.chars().count();
                app.shell.lines.clear();
                app.shell.output = None;
                app.shell.progress.clear();
                app.shell.scroll = 0;
                spawn_shell_task(app, tx, last);
            }
            true
        }
        KeyCode::Up => {
            app.shell.scroll = app.shell.scroll.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            let content = app.shell.get_content();
            let visible = layout::visible_content_height(term_height);
            let max_scroll = content.len().saturating_sub(visible);
            if app.shell.scroll < max_scroll {
                app.shell.scroll += 1;
            }
            true
        }
        KeyCode::PageUp => {
            app.shell.scroll = app.shell.scroll.saturating_sub(10);
            true
        }
        KeyCode::PageDown => {
            let content = app.shell.get_content();
            let visible = layout::visible_content_height(term_height);
            let max_scroll = content.len().saturating_sub(visible);
            app.shell.scroll = (app.shell.scroll + 10).min(max_scroll);
            true
        }
        _ => false,
    }
}

/// 解析命令字符串为参数列表
fn parse_command(cmd: &str) -> Vec<String> {
    // 简单按空格拆分，支持单引号/双引号包裹的参数
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for c in cmd.chars() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// 启动命令执行异步任务
fn spawn_shell_task(app: &mut App, tx: &mpsc::Sender<AppEvent>, cmd: String) {
    let cmd_parts = parse_command(&cmd);
    if cmd_parts.is_empty() {
        return;
    }

    app.shell.phase = ShellPhase::Running;
    app.shell.lines.clear();
    app.shell.output = None;
    app.shell.progress.clear();
    app.shell.scroll = 0;
    app.shell.lines.push(format!("$ {}", cmd));
    app.shell.lines.push(String::new());

    let tx_clone = tx.clone();
    std::thread::spawn(move || {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::unbounded_channel();

        let tx_for_lines = tx_clone.clone();
        std::thread::spawn(move || {
            while let Some(line) = output_rx.blocking_recv() {
                let _ = tx_for_lines.blocking_send(AppEvent::ShellLine(line));
            }
        });

        match crate::package_manager::run_custom_command_streaming(cmd_parts, output_tx) {
            Ok(output) => {
                let _ = tx_clone.blocking_send(AppEvent::ShellComplete { output });
            }
            Err(e) => {
                let _ = tx_clone.blocking_send(AppEvent::Error(format!("命令执行失败: {}", e)));
            }
        }
    });
}

// ===== 渲染 =====

pub fn render_shell(f: &mut Frame, app: &App) {
    let chunks = layout::main_layout(f.area());

    render_shell_header(f, app, chunks[0]);

    match app.shell.phase {
        ShellPhase::Input => render_input_view(f, app, chunks[1], chunks[2]),
        ShellPhase::Running => render_output_view(f, app, chunks[1], chunks[2]),
        ShellPhase::Done | ShellPhase::Error => render_output_view(f, app, chunks[1], chunks[2]),
    }
}

fn render_shell_header(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let title = match app.shell.phase {
        ShellPhase::Input => "💻 自定义命令",
        ShellPhase::Running => "⚙️  命令执行中...",
        ShellPhase::Done => "✅ 命令完成",
        ShellPhase::Error => "❌ 命令错误",
    };
    let header = Paragraph::new(title)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    f.render_widget(header, area);
}

fn render_input_view(
    f: &mut Frame,
    app: &App,
    content_area: ratatui::layout::Rect,
    footer_area: ratatui::layout::Rect,
) {
    let block = Block::default()
        .title("输入命令")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(content_area);
    f.render_widget(block, content_area);

    let padded = inner.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    // 输入框
    let ib = input_box_from_app(app);
    let prompt = Line::from(vec![
        Span::styled("$ ", Style::default().fg(PINK).add_modifier(Modifier::BOLD)),
        Span::styled(ib.content().to_string(), Style::default().fg(BRIGHT_WHITE)),
        Span::styled("_", Style::default().fg(Color::White).add_modifier(Modifier::RAPID_BLINK)),
    ]);
    f.render_widget(Paragraph::new(prompt), padded);

    // 历史提示
    if !app.shell.history.is_empty() {
        let hint_area = ratatui::layout::Rect {
            y: padded.y + 2,
            height: padded.height.saturating_sub(2),
            ..padded
        };
        if hint_area.height > 0 {
            let hist_start = app.shell.history.len().saturating_sub(hint_area.height as usize);
            let lines: Vec<Line> = app.shell.history[hist_start..]
                .iter()
                .rev()
                .enumerate()
                .map(|(i, cmd)| {
                    let idx = app.shell.history.len() - 1 - (hist_start + i);
                    Line::from(vec![
                        Span::styled(
                            format!("  {}: ", idx + 1),
                            Style::default().fg(DIM),
                        ),
                        Span::styled(cmd.clone(), Style::default().fg(Color::DarkGray)),
                    ])
                })
                .collect();
            let para = Paragraph::new(lines);
            f.render_widget(para, hint_area);
        }
    }

    // 页脚
    let footer = if app.shell.history.is_empty() {
        "输入命令后 Enter 执行 | ↑↓ 历史 | Esc 返回"
    } else {
        "Enter 执行 | ↑↓ 历史记录 | Esc 返回主页"
    };
    layout::render_footer(f, footer, footer_area);
}

fn render_output_view(
    f: &mut Frame,
    app: &App,
    content_area: ratatui::layout::Rect,
    footer_area: ratatui::layout::Rect,
) {
    let content = app.shell.get_content();
    let total_lines = content.len();
    let visible = content_area.height.saturating_sub(2) as usize;
    let scroll = app.shell.scroll.min(total_lines.saturating_sub(visible));

    let lines: Vec<Line> = content
        .iter()
        .skip(scroll)
        .take(visible)
        .map(|line| {
            if line.starts_with("$ ") {
                Line::from(Span::styled(
                    line.clone(),
                    Style::default().fg(PINK).add_modifier(Modifier::BOLD),
                ))
            } else if line.starts_with("⚠ ") {
                Line::from(Span::styled(
                    line.clone(),
                    Style::default().fg(Color::Yellow),
                ))
            } else {
                Line::from(Span::styled(line.clone(), Style::default().fg(BRIGHT_WHITE)))
            }
        })
        .collect();

    let block_title = match app.shell.phase {
        ShellPhase::Running => "输出 (Esc 取消)",
        ShellPhase::Done => "输出",
        ShellPhase::Error => "输出 (错误)",
        ShellPhase::Input => "输出",
    };

    let block = Block::default()
        .title(block_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, content_area);

    // 滚动条
    if total_lines > visible {
        let mut scrollbar_state = ScrollbarState::new(total_lines.saturating_sub(visible))
            .position(scroll);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            content_area,
            &mut scrollbar_state,
        );
    }

    // 页脚
    let owned_footer: String;
    let footer = match app.shell.phase {
        ShellPhase::Running => {
            if app.shell.progress.is_empty() {
                "执行中... | Esc 取消 | ↑↓ 滚动"
            } else {
                owned_footer = format!("{} | Esc 取消 | ↑↓ 滚动", app.shell.progress);
                &owned_footer
            }
        }
        ShellPhase::Done => {
            if let Some(output) = &app.shell.output {
                if output.success {
                    "✓ 命令成功 | Enter 重新执行 | Esc 新命令 | ↑↓ 滚动"
                } else {
                    "✗ 命令失败 | Enter 重新执行 | Esc 新命令 | ↑↓ 滚动"
                }
            } else {
                "Esc 返回输入 | Enter 重新执行 | ↑↓ 滚动"
            }
        }
        ShellPhase::Error => "❌ 执行出错 | Esc 新命令 | ↑↓ 滚动",
        ShellPhase::Input => "",
    };
    layout::render_footer(f, footer, footer_area);
}
