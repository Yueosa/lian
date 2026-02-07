use super::input::{self, InputBox};
use super::layout;
use super::state::{App, AppEvent, FileListMode, QueryPanel, QueryView};
use super::theme::{BLUE, BRIGHT_WHITE, DESC_DIM, DIM, PINK, SEL_BG};
use crate::package_manager::PackageInfo;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};
use tokio::sync::mpsc;
use unicode_width::UnicodeWidthStr;

/// 从 App 状态构建 InputBox 用于渲染
fn input_box_from_app(app: &App) -> InputBox {
    let mut ib = InputBox::new();
    for c in app.query_input.chars() {
        ib.insert(c);
    }
    ib.move_home();
    for _ in 0..app.query_cursor {
        ib.move_right();
    }
    ib
}

/// 计算详情视图总行数（用于滚动边界）
pub fn detail_total_lines(app: &App) -> usize {
    let field_count = app.query_detail.as_ref().map(|d| d.fields.len()).unwrap_or(0);
    let list_items = match app.query_file_mode {
        FileListMode::Files => &app.query_files,
        FileListMode::Directories => &app.query_dirs,
    };
    let file_lines = if !list_items.is_empty() {
        2 + list_items.len()
    } else if app.query_detail.is_some() {
        2
    } else {
        0
    };
    field_count + file_lines
}

/// 处理查询模式按键
pub fn handle_query_key(
    key: KeyEvent,
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
    term_height: u16,
) {
    match app.query_view {
        QueryView::List => handle_list_key(key, app, tx),
        QueryView::Detail => handle_detail_key(key, app, term_height),
    }
}

/// 列表视图按键处理
fn handle_list_key(
    key: KeyEvent,
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
) {
    match key.code {
        // Esc 返回 Dashboard
        KeyCode::Esc => {
            app.mode = super::state::AppMode::Dashboard;
            app.reset_query_state();
        }
        // Tab 切换面板
        KeyCode::Tab => {
            app.query_panel = match app.query_panel {
                QueryPanel::Local => QueryPanel::Remote,
                QueryPanel::Remote => QueryPanel::Local,
            };
        }
        // 上下选择
        KeyCode::Up => {
            match app.query_panel {
                QueryPanel::Local => {
                    app.query_local_selected = app.query_local_selected.saturating_sub(1);
                }
                QueryPanel::Remote => {
                    app.query_remote_selected = app.query_remote_selected.saturating_sub(1);
                }
            }
        }
        KeyCode::Down => {
            match app.query_panel {
                QueryPanel::Local => {
                    let max = app.query_local_results.len().saturating_sub(1);
                    if app.query_local_selected < max {
                        app.query_local_selected += 1;
                    }
                }
                QueryPanel::Remote => {
                    let max = app.query_remote_results.len().saturating_sub(1);
                    if app.query_remote_selected < max {
                        app.query_remote_selected += 1;
                    }
                }
            }
        }
        // Enter 查看详情
        KeyCode::Enter => {
            let selected_pkg = match app.query_panel {
                QueryPanel::Local => app.query_local_results.get(app.query_local_selected).cloned(),
                QueryPanel::Remote => app.query_remote_results.get(app.query_remote_selected).cloned(),
            };
            if let Some(pkg) = selected_pkg {
                load_package_detail(app, &pkg, tx);
            }
        }
        // 文本输入
        KeyCode::Char(c) => {
            // 忽略带 Ctrl/Alt 修饰的字符
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT)
            {
                return;
            }
            insert_char(app, c);
            trigger_search(app, tx);
        }
        KeyCode::Backspace => {
            delete_back(app);
            trigger_search(app, tx);
        }
        KeyCode::Delete => {
            delete_forward(app);
            trigger_search(app, tx);
        }
        KeyCode::Left => {
            if app.query_cursor > 0 {
                app.query_cursor -= 1;
            }
        }
        KeyCode::Right => {
            let char_count = app.query_input.chars().count();
            if app.query_cursor < char_count {
                app.query_cursor += 1;
            }
        }
        KeyCode::Home => {
            app.query_cursor = 0;
        }
        KeyCode::End => {
            app.query_cursor = app.query_input.chars().count();
        }
        _ => {}
    }
}

/// 在光标位置插入字符
fn insert_char(app: &mut App, c: char) {
    let byte_pos = char_to_byte(&app.query_input, app.query_cursor);
    app.query_input.insert(byte_pos, c);
    app.query_cursor += 1;
}

/// Backspace
fn delete_back(app: &mut App) {
    if app.query_cursor > 0 {
        app.query_cursor -= 1;
        let byte_pos = char_to_byte(&app.query_input, app.query_cursor);
        let next_byte_pos = char_to_byte(&app.query_input, app.query_cursor + 1);
        app.query_input.drain(byte_pos..next_byte_pos);
    }
}

/// Delete
fn delete_forward(app: &mut App) {
    let char_count = app.query_input.chars().count();
    if app.query_cursor < char_count {
        let byte_pos = char_to_byte(&app.query_input, app.query_cursor);
        let next_byte_pos = char_to_byte(&app.query_input, app.query_cursor + 1);
        app.query_input.drain(byte_pos..next_byte_pos);
    }
}

fn char_to_byte(s: &str, char_pos: usize) -> usize {
    s.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// 触发异步搜索
fn trigger_search(app: &mut App, tx: &mpsc::Sender<AppEvent>) {
    let keyword = app.query_input.clone();
    if keyword.trim().is_empty() {
        app.query_local_results.clear();
        app.query_remote_results.clear();
        app.query_local_selected = 0;
        app.query_remote_selected = 0;
        app.query_searching = false;
        return;
    }

    app.query_searching = true;

    let pm = match app.package_manager.clone() {
        Some(pm) => pm,
        None => return,
    };

    // 本地搜索
    let tx_local = tx.clone();
    let pm_local = pm.clone();
    let kw_local = keyword.clone();
    tokio::spawn(async move {
        let results = tokio::task::spawn_blocking(move || pm_local.search_local(&kw_local))
            .await
            .unwrap_or_default();
        let _ = tx_local.send(AppEvent::QueryLocalResults(results)).await;
    });

    // 远程搜索
    let tx_remote = tx.clone();
    let kw_remote = keyword;
    tokio::spawn(async move {
        let results = tokio::task::spawn_blocking(move || pm.search_remote(&kw_remote))
            .await
            .unwrap_or_default();
        let _ = tx_remote.send(AppEvent::QueryRemoteResults(results)).await;
    });
}

/// 加载包详情
fn load_package_detail(app: &App, pkg: &PackageInfo, tx: &mpsc::Sender<AppEvent>) {
    let pm = match app.package_manager.clone() {
        Some(pm) => pm,
        None => return,
    };
    let name = pkg.name.clone();
    let is_installed = pkg.installed;
    let tx_clone = tx.clone();

    tokio::spawn(async move {
        let detail_result = tokio::task::spawn_blocking({
            let pm = pm.clone();
            let name = name.clone();
            move || {
                if is_installed {
                    pm.package_info_local(&name)
                } else {
                    pm.package_info_remote(&name)
                }
            }
        })
        .await;

        let files = if is_installed {
            let pm_f = pm.clone();
            let name_f = name.clone();
            tokio::task::spawn_blocking(move || pm_f.package_files(&name_f))
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let dirs = if is_installed {
            let pm_d = pm.clone();
            let name_d = name.clone();
            tokio::task::spawn_blocking(move || pm_d.package_dirs(&name_d))
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        match detail_result {
            Ok(Ok(detail)) => {
                let _ = tx_clone
                    .send(AppEvent::QueryDetailLoaded { detail, files, dirs })
                    .await;
            }
            Ok(Err(e)) => {
                let _ = tx_clone
                    .send(AppEvent::Error(format!("获取包信息失败: {}", e)))
                    .await;
            }
            Err(e) => {
                let _ = tx_clone
                    .send(AppEvent::Error(format!("任务执行失败: {}", e)))
                    .await;
            }
        }
    });
}

/// 详情视图按键处理
fn handle_detail_key(key: KeyEvent, app: &mut App, term_height: u16) {
    let total = detail_total_lines(app);
    let visible = term_height.saturating_sub(8) as usize;
    let max_scroll = total.saturating_sub(visible);

    match key.code {
        KeyCode::Esc => {
            app.query_view = QueryView::List;
            app.query_detail = None;
            app.query_files.clear();
            app.query_dirs.clear();
            app.query_file_mode = FileListMode::Files;
            app.query_detail_scroll = 0;
        }
        KeyCode::Tab => {
            // 切换文件/目录视图
            app.query_file_mode = match app.query_file_mode {
                FileListMode::Files => FileListMode::Directories,
                FileListMode::Directories => FileListMode::Files,
            };
            app.query_detail_scroll = 0;
        }
        KeyCode::Up => {
            app.query_detail_scroll = app.query_detail_scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            if app.query_detail_scroll < max_scroll {
                app.query_detail_scroll += 1;
            }
        }
        KeyCode::PageUp => {
            app.query_detail_scroll = app.query_detail_scroll.saturating_sub(10);
        }
        KeyCode::PageDown => {
            app.query_detail_scroll = (app.query_detail_scroll + 10).min(max_scroll);
        }
        _ => {}
    }
}

// ===== 渲染 =====

/// 渲染查询视图
pub fn render_query(f: &mut Frame, app: &App) {
    match app.query_view {
        QueryView::List => render_list_view(f, app),
        QueryView::Detail => render_detail_view(f, app),
    }
}

/// 渲染列表视图（左右分栏）
fn render_list_view(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(3), // 输入框
            Constraint::Min(0),   // 结果列表
            Constraint::Length(3), // footer
        ])
        .split(area);

    // Header
    layout::render_header(f, "🔍 查询软件包 (Shift+Q)", chunks[0]);

    // 输入框
    let input = input_box_from_app(app);
    input::render_input_box(f, &input, ">", true, chunks[1]);

    // 左右分栏
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[2]);

    // 本地面板
    render_result_panel(
        f,
        "本地已安装 (Qs)",
        &app.query_local_results,
        app.query_local_selected,
        app.query_panel == QueryPanel::Local,
        panels[0],
    );

    // 远程面板
    render_result_panel(
        f,
        "远程仓库 (Ss)",
        &app.query_remote_results,
        app.query_remote_selected,
        app.query_panel == QueryPanel::Remote,
        panels[1],
    );

    // Footer
    let footer_text = if app.query_searching {
        "搜索中... | Tab 切换面板 | ↑↓ 选择 | Enter 查看详情 | Esc 返回"
    } else {
        "输入关键词搜索 | Tab 切换面板 | ↑↓ 选择 | Enter 查看详情 | Esc 返回"
    };
    layout::render_footer(f, footer_text, chunks[3]);
}

/// 渲染搜索结果面板
fn render_result_panel(
    f: &mut Frame,
    title: &str,
    results: &[PackageInfo],
    selected: usize,
    focused: bool,
    area: Rect,
) {
    let border_color = if focused { Color::Yellow } else { Color::DarkGray };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // 内部水平边距
    let padded = inner.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });

    if results.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "无结果",
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(empty, padded);
        return;
    }

    // 每个包占 2 行（名称 + 描述）
    let item_height = 2usize;
    let visible_items = (padded.height as usize) / item_height;
    // 计算滚动偏移以确保选中项可见
    let scroll = if selected >= visible_items {
        selected - visible_items + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::new();
    for (i, pkg) in results.iter().enumerate().skip(scroll).take(visible_items) {
        let is_selected = i == selected && focused;
        let marker = if is_selected { "► " } else { "  " };
        let installed_mark = if pkg.installed { " [已安装]" } else { "" };

        // 第一行：包名 + 版本
        if is_selected {
            let bg = Style::default().bg(SEL_BG);
            lines.push(Line::from(vec![
                Span::styled(marker.to_string(), bg.fg(BRIGHT_WHITE).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{}/", pkg.repo), bg.fg(PINK).add_modifier(Modifier::BOLD)),
                Span::styled(pkg.name.clone(), bg.fg(BRIGHT_WHITE).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" {}", pkg.version), bg.fg(BLUE)),
                Span::styled(installed_mark.to_string(), bg.fg(DIM)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(marker.to_string(), Style::default().fg(Color::White)),
                Span::styled(format!("{}/", pkg.repo), Style::default().fg(PINK)),
                Span::styled(pkg.name.clone(), Style::default().fg(BLUE)),
                Span::styled(format!(" {}", pkg.version), Style::default().fg(Color::White)),
                Span::styled(installed_mark.to_string(), Style::default().fg(DIM)),
            ]));
        }

        // 第二行：描述（缩进） 
        let desc = if pkg.description.is_empty() {
            "(无描述)".to_string()
        } else {
            pkg.description.clone()
        };
        if is_selected {
            lines.push(Line::from(Span::styled(
                format!("    {}", desc),
                Style::default().bg(SEL_BG).fg(DESC_DIM),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                format!("    {}", desc),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, padded);

    // 滚动条
    if results.len() > visible_items {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut scrollbar_state = ScrollbarState::new(results.len()).position(scroll);
        f.render_stateful_widget(
            scrollbar,
            area.inner(ratatui::layout::Margin {
                horizontal: 0,
                vertical: 1,
            }),
            &mut scrollbar_state,
        );
    }
}

/// 渲染详情视图
fn render_detail_view(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),   // 详情内容
            Constraint::Length(3), // footer
        ])
        .split(area);

    // Header - 使用详情的第一个字段作为包名（兼容所有语言环境）
    let pkg_name = app
        .query_detail
        .as_ref()
        .and_then(|d| d.fields.first().map(|(_, v)| v.as_str()))
        .unwrap_or("未知");
    layout::render_header(f, &format!("📦 包信息 - {}", pkg_name), chunks[0]);

    // 详情内容
    render_detail_content(f, app, chunks[1]);

    // Footer
    let footer_text = if app.query_files.is_empty() && app.query_dirs.is_empty() {
        "↑↓ 滚动 | PgUp/PgDn 翻页 | Esc 返回列表"
    } else {
        match app.query_file_mode {
            FileListMode::Files => "↑↓ 滚动 | PgUp/PgDn 翻页 | Tab 切换目录视图 | Esc 返回列表",
            FileListMode::Directories => "↑↓ 滚动 | PgUp/PgDn 翻页 | Tab 切换文件视图 | Esc 返回列表",
        }
    };
    layout::render_footer(f, footer_text, chunks[2]);
}

/// 渲染详情内容区域
fn render_detail_content(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // 内部水平边距
    let padded = inner.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });

    let mut all_lines: Vec<Line> = Vec::new();

    // 包信息字段（CJK 对齐）
    if let Some(detail) = &app.query_detail {
        for (key, value) in &detail.fields {
            let key_width = UnicodeWidthStr::width(key.as_str());
            let target_width: usize = 18;
            let pad = target_width.saturating_sub(key_width);
            let padded_key = format!("{}{} ", key, " ".repeat(pad));
            all_lines.push(Line::from(vec![
                Span::styled(
                    padded_key,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(value.clone(), Style::default().fg(Color::White)),
            ]));
        }
    }

    // 文件/目录列表
    let (list_items, list_title, empty_msg) = match app.query_file_mode {
        FileListMode::Files => (
            &app.query_files,
            "──── 文件列表 ────",
            "  (远程包，无文件列表)",
        ),
        FileListMode::Directories => (
            &app.query_dirs,
            "──── 目录列表 ────",
            "  (远程包，无目录列表)",
        ),
    };

    if !list_items.is_empty() {
        all_lines.push(Line::from(""));
        all_lines.push(Line::from(Span::styled(
            list_title,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        for item in list_items {
            all_lines.push(Line::from(Span::styled(
                format!("  {}", item),
                Style::default().fg(Color::White),
            )));
        }
    } else if app.query_detail.is_some() {
        all_lines.push(Line::from(""));
        all_lines.push(Line::from(Span::styled(
            empty_msg,
            Style::default().fg(Color::DarkGray),
        )));
    }

    let total_lines = all_lines.len();
    let visible_height = padded.height as usize;
    let max_scroll = total_lines.saturating_sub(visible_height);
    let actual_scroll = app.query_detail_scroll.min(max_scroll);

    let visible: Vec<Line> = all_lines
        .into_iter()
        .skip(actual_scroll)
        .take(visible_height)
        .collect();

    let paragraph = Paragraph::new(visible)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, padded);

    // 滚动条
    if total_lines > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut scrollbar_state = ScrollbarState::new(total_lines).position(actual_scroll);
        f.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                horizontal: 0,
                vertical: 1,
            }),
            &mut scrollbar_state,
        );
    }
}
