use super::layout;
use super::state::{App, AppMode, SettingsItem};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

// MTF flag colors
const PINK: Color = Color::Rgb(245, 169, 184);
const BLUE: Color = Color::Rgb(91, 206, 250);
const SEL_BG: Color = Color::Rgb(45, 35, 55);
const BRIGHT_WHITE: Color = Color::Rgb(255, 255, 255);
const DIM: Color = Color::Rgb(130, 130, 140);

/// 处理设置模式按键
pub fn handle_settings_key(key: KeyEvent, app: &mut App) -> bool {
    if app.settings_editing {
        handle_editing_key(key, app)
    } else {
        handle_browsing_key(key, app)
    }
}

/// 浏览模式按键处理
fn handle_browsing_key(key: KeyEvent, app: &mut App) -> bool {
    let total = app.settings_focusable_count();
    if total == 0 {
        if key.code == KeyCode::Esc {
            app.mode = AppMode::Dashboard;
        }
        return true;
    }

    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Dashboard;
            true
        }
        KeyCode::Up => {
            app.settings_selected = app.settings_selected.saturating_sub(1);
            app.settings_message = None;
            true
        }
        KeyCode::Down => {
            if app.settings_selected + 1 < total {
                app.settings_selected += 1;
            }
            app.settings_message = None;
            true
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            // 获取当前选中项的实际索引
            let focusable: Vec<usize> = app.settings_items.iter().enumerate()
                .filter(|(_, item)| !matches!(item, SettingsItem::Section(_)))
                .map(|(i, _)| i)
                .collect();

            if let Some(&real_idx) = focusable.get(app.settings_selected) {
                match &app.settings_items[real_idx] {
                    SettingsItem::Toggle { .. } => {
                        app.toggle_settings_item();
                    }
                    SettingsItem::TextEdit { .. } => {
                        app.start_settings_edit();
                    }
                    _ => {}
                }
            }
            app.settings_message = None;
            true
        }
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.save_settings();
            true
        }
        _ => false,
    }
}

/// 编辑模式按键处理
fn handle_editing_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        KeyCode::Esc => {
            // 取消编辑
            app.settings_editing = false;
            true
        }
        KeyCode::Enter => {
            // 确认编辑
            app.confirm_settings_edit();
            true
        }
        KeyCode::Backspace => {
            if app.settings_edit_cursor > 0 {
                // UTF-8 安全删除
                let byte_pos = app.settings_edit_buffer
                    .char_indices()
                    .nth(app.settings_edit_cursor - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let next_byte = app.settings_edit_buffer
                    .char_indices()
                    .nth(app.settings_edit_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(app.settings_edit_buffer.len());
                app.settings_edit_buffer = format!(
                    "{}{}",
                    &app.settings_edit_buffer[..byte_pos],
                    &app.settings_edit_buffer[next_byte..]
                );
                app.settings_edit_cursor -= 1;
            }
            true
        }
        KeyCode::Delete => {
            let char_count = app.settings_edit_buffer.chars().count();
            if app.settings_edit_cursor < char_count {
                let byte_pos = app.settings_edit_buffer
                    .char_indices()
                    .nth(app.settings_edit_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(app.settings_edit_buffer.len());
                let next_byte = app.settings_edit_buffer
                    .char_indices()
                    .nth(app.settings_edit_cursor + 1)
                    .map(|(i, _)| i)
                    .unwrap_or(app.settings_edit_buffer.len());
                app.settings_edit_buffer = format!(
                    "{}{}",
                    &app.settings_edit_buffer[..byte_pos],
                    &app.settings_edit_buffer[next_byte..]
                );
            }
            true
        }
        KeyCode::Left => {
            app.settings_edit_cursor = app.settings_edit_cursor.saturating_sub(1);
            true
        }
        KeyCode::Right => {
            let char_count = app.settings_edit_buffer.chars().count();
            if app.settings_edit_cursor < char_count {
                app.settings_edit_cursor += 1;
            }
            true
        }
        KeyCode::Home => {
            app.settings_edit_cursor = 0;
            true
        }
        KeyCode::End => {
            app.settings_edit_cursor = app.settings_edit_buffer.chars().count();
            true
        }
        KeyCode::Char(c) => {
            // 在光标位置插入字符
            let byte_pos = app.settings_edit_buffer
                .char_indices()
                .nth(app.settings_edit_cursor)
                .map(|(i, _)| i)
                .unwrap_or(app.settings_edit_buffer.len());
            app.settings_edit_buffer.insert(byte_pos, c);
            app.settings_edit_cursor += 1;
            true
        }
        _ => false,
    }
}

/// 渲染设置视图
pub fn render_settings(f: &mut Frame, app: &App) {
    let chunks = layout::main_layout(f.area());

    // Header
    layout::render_header(f, "⚙  设置", chunks[0]);

    // Content
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let content_inner = content_block.inner(chunks[1]);
    f.render_widget(content_block, chunks[1]);

    let padded = content_inner.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });

    if padded.height < 3 {
        return;
    }

    render_items(f, app, padded);

    // Footer
    let footer_text = if app.settings_editing {
        "输入新值 | Enter 确认 | Esc 取消"
    } else {
        "↑↓ 选择 | Enter/Space 切换/编辑 | Ctrl+S 保存 | Esc 返回"
    };

    // 如果有消息，显示在 footer
    if let Some(msg) = &app.settings_message {
        let msg_color = if msg.starts_with('✓') {
            Color::Green
        } else {
            Color::Red
        };
        let footer_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let footer_inner = footer_block.inner(chunks[2]);
        f.render_widget(footer_block, chunks[2]);
        let footer_padded = footer_inner.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        let para = Paragraph::new(Line::from(vec![
            Span::styled(format!("{} | ", msg), Style::default().fg(msg_color)),
            Span::styled(footer_text.to_string(), Style::default().fg(Color::DarkGray)),
        ]));
        f.render_widget(para, footer_padded);
    } else {
        layout::render_footer(f, footer_text, chunks[2]);
    }
}

/// 渲染设置项列表
fn render_items(f: &mut Frame, app: &App, area: Rect) {
    if app.settings_items.is_empty() {
        let hint = Paragraph::new("正在加载设置...")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(hint, area);
        return;
    }

    // 计算 label 最大宽度用于对齐
    let max_label_width = app.settings_items.iter()
        .filter_map(|item| match item {
            SettingsItem::TextEdit { label, .. } => Some(UnicodeWidthStr::width(label.as_str())),
            _ => None,
        })
        .max()
        .unwrap_or(10);

    let visible_height = area.height as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut focusable_idx = 0;

    for (i, item) in app.settings_items.iter().enumerate() {
        match item {
            SettingsItem::Section(title) => {
                // 分组前空一行（非首项）
                if i > 0 {
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    format!("── {} ──", title),
                    Style::default().fg(PINK).add_modifier(Modifier::BOLD),
                )));
            }
            SettingsItem::Toggle { label, value, .. } => {
                let is_selected = focusable_idx == app.settings_selected;
                let checkbox = if *value { "[✓]" } else { "[ ]" };
                let checkbox_color = if *value { BLUE } else { DIM };

                if is_selected {
                    let bg = Style::default().bg(SEL_BG);
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!(" {} ", checkbox),
                            bg.fg(checkbox_color).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" {}", label),
                            bg.fg(BRIGHT_WHITE).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!(" {} ", checkbox),
                            Style::default().fg(checkbox_color),
                        ),
                        Span::styled(
                            format!(" {}", label),
                            Style::default().fg(Color::White),
                        ),
                    ]));
                }
                focusable_idx += 1;
            }
            SettingsItem::TextEdit { label, value, masked, .. } => {
                let is_selected = focusable_idx == app.settings_selected;
                let is_editing = is_selected && app.settings_editing;

                let label_width = UnicodeWidthStr::width(label.as_str());
                let padding = max_label_width.saturating_sub(label_width);
                let label_padded = format!(" {}:{} ", label, " ".repeat(padding));

                if is_editing {
                    // 编辑中：显示 buffer 和光标
                    let buf = &app.settings_edit_buffer;
                    let cursor_pos = app.settings_edit_cursor;
                    let before: String = buf.chars().take(cursor_pos).collect();
                    let cursor_char: String = buf.chars().skip(cursor_pos).take(1).collect();
                    let after: String = buf.chars().skip(cursor_pos + 1).collect();
                    let cursor_display = if cursor_char.is_empty() {
                        " ".to_string()
                    } else {
                        cursor_char
                    };

                    let bg = Style::default().bg(SEL_BG);
                    lines.push(Line::from(vec![
                        Span::styled(label_padded, bg.fg(BLUE).add_modifier(Modifier::BOLD)),
                        Span::styled(before, bg.fg(BRIGHT_WHITE)),
                        Span::styled(cursor_display, Style::default().fg(Color::Black).bg(Color::Yellow)),
                        Span::styled(after, bg.fg(BRIGHT_WHITE)),
                    ]));
                } else {
                    let display_value = if *masked && !value.is_empty() {
                        mask_value(value)
                    } else if value.is_empty() {
                        "(未设置)".to_string()
                    } else {
                        value.clone()
                    };

                    if is_selected {
                        let bg = Style::default().bg(SEL_BG);
                        lines.push(Line::from(vec![
                            Span::styled(label_padded, bg.fg(BLUE).add_modifier(Modifier::BOLD)),
                            Span::styled(
                                display_value,
                                bg.fg(BRIGHT_WHITE).add_modifier(Modifier::BOLD),
                            ),
                        ]));
                    } else {
                        let value_color = if value.is_empty() { DIM } else { Color::White };
                        lines.push(Line::from(vec![
                            Span::styled(label_padded, Style::default().fg(BLUE)),
                            Span::styled(display_value, Style::default().fg(value_color)),
                        ]));
                    }
                }
                focusable_idx += 1;
            }
        }
    }

    // 滚动处理
    let total_lines = lines.len();
    let scroll = if total_lines > visible_height {
        let selected_line = find_selected_line(&app.settings_items, app.settings_selected);
        if selected_line >= visible_height {
            selected_line.saturating_sub(visible_height / 2)
        } else {
            0
        }
    } else {
        0
    };

    let visible_lines: Vec<Line> = lines.into_iter()
        .skip(scroll)
        .take(visible_height)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    f.render_widget(paragraph, area);

    // 滚动条
    if total_lines > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut state = ScrollbarState::new(total_lines).position(scroll);
        f.render_stateful_widget(
            scrollbar,
            area.inner(Margin { horizontal: 0, vertical: 0 }),
            &mut state,
        );
    }
}

/// 找到选中项在渲染行中的行号
fn find_selected_line(items: &[SettingsItem], selected: usize) -> usize {
    let mut line = 0;
    let mut focusable_idx = 0;

    for (i, item) in items.iter().enumerate() {
        match item {
            SettingsItem::Section(_) => {
                if i > 0 {
                    line += 1;
                }
                line += 1;
            }
            _ => {
                if focusable_idx == selected {
                    return line;
                }
                line += 1;
                focusable_idx += 1;
            }
        }
    }
    line
}

/// API Key 脱敏：sk-abc...xyz → sk-***...***
fn mask_value(value: &str) -> String {
    let len = value.len();
    if len <= 8 {
        "*".repeat(len)
    } else {
        let prefix: String = value.chars().take(4).collect();
        let suffix: String = value.chars().skip(len - 4).collect();
        format!("{}****{}", prefix, suffix)
    }
}
