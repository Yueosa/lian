use super::input::InputBox;
use super::layout;
use super::state::{App, AppEvent, AppMode, RemovePhase, ViewMode};
use super::theme::{BLUE, BRIGHT_WHITE, DESC_DIM, DIM, PINK, SEL_BG};
use crate::tui::input::{str_insert_char, str_delete_back, str_delete_forward};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use tokio::sync::mpsc;
use unicode_width::UnicodeWidthStr;

/// 从 App 状态构建 InputBox 用于渲染
fn input_box_from_app(app: &App) -> InputBox {
    let mut ib = InputBox::new();
    for c in app.remove.input.chars() {
        ib.insert(c);
    }
    ib.move_home();
    for _ in 0..app.remove.cursor {
        ib.move_right();
    }
    ib
}

/// 处理卸载模式按键
pub fn handle_remove_key(
    key: KeyEvent,
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
    term_height: u16,
) -> bool {
    match app.remove.phase {
        RemovePhase::Browsing => handle_browsing_key(key, app, tx),
        RemovePhase::PreviewingRemove => handle_preview_key(key, app),
        RemovePhase::Removing => handle_output_key(key, app, term_height),
        RemovePhase::RemoveComplete => handle_output_key(key, app, term_height),
        RemovePhase::Analyzing => handle_output_key(key, app, term_height),
        RemovePhase::AnalysisComplete => handle_complete_key(key, app, term_height),
        RemovePhase::Error => handle_output_key(key, app, term_height),
    }
}

/// 浏览状态按键处理
fn handle_browsing_key(
    key: KeyEvent,
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Dashboard;
            app.reset_remove_state();
            true
        }
        KeyCode::Up => {
            app.remove.selected = app.remove.selected.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            let max = app.remove.filtered.len().saturating_sub(1);
            if app.remove.selected < max {
                app.remove.selected += 1;
            }
            true
        }
        KeyCode::Char(' ') => {
            // 多选切换（使用原始索引标记）
            if !app.remove.filtered.is_empty() {
                if let Some(&real_idx) = app.remove.filtered.get(app.remove.selected) {
                    if app.remove.marked.contains(&real_idx) {
                        app.remove.marked.remove(&real_idx);
                    } else {
                        app.remove.marked.insert(real_idx);
                    }
                    // 选中后自动下移
                    let max = app.remove.filtered.len().saturating_sub(1);
                    if app.remove.selected < max {
                        app.remove.selected += 1;
                    }
                }
            }
            true
        }
        KeyCode::Enter => {
            // 收集选中的包，获取卸载预览
            if !app.remove.filtered.is_empty() {
                let packages = collect_selected_packages(app);
                if !packages.is_empty() {
                    if let Some(pm) = app.package_manager.clone() {
                        let tx_clone = tx.clone();
                        let pkgs = packages.clone();
                        tokio::spawn(async move {
                            let preview = tokio::task::spawn_blocking(move || {
                                pm.preview_remove(&pkgs)
                            })
                            .await
                            .unwrap_or_default();
                            let _ = tx_clone.send(AppEvent::RemovePreviewReady(preview)).await;
                        });
                        app.remove.phase = RemovePhase::PreviewingRemove;
                        app.remove.preview = vec!["正在获取卸载预览...".to_string()];
                        app.remove.scroll = 0;
                    }
                }
            }
            true
        }
        KeyCode::Backspace => {
            str_delete_back(&mut app.remove.input, &mut app.remove.cursor);
            app.remove.apply_filter();
            true
        }
        KeyCode::Delete => {
            str_delete_forward(&mut app.remove.input, &mut app.remove.cursor);
            app.remove.apply_filter();
            true
        }
        KeyCode::Left => {
            if app.remove.cursor > 0 {
                app.remove.cursor -= 1;
            }
            true
        }
        KeyCode::Right => {
            let max = app.remove.input.chars().count();
            if app.remove.cursor < max {
                app.remove.cursor += 1;
            }
            true
        }
        KeyCode::Home => {
            app.remove.cursor = 0;
            true
        }
        KeyCode::End => {
            app.remove.cursor = app.remove.input.chars().count();
            true
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                return false;
            }
            str_insert_char(&mut app.remove.input, &mut app.remove.cursor, c);
            app.remove.apply_filter();
            true
        }
        _ => false,
    }
}

/// 预览状态按键处理
fn handle_preview_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.remove.phase = RemovePhase::Browsing;
            app.remove.preview.clear();
            app.remove.scroll = 0;
            true
        }
        KeyCode::Up => {
            app.remove.scroll = app.remove.scroll.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            app.remove.scroll += 1;
            true
        }
        // Enter 在 mod.rs 中处理（需要 sudo）
        _ => false,
    }
}

/// 输出状态按键处理
fn handle_output_key(key: KeyEvent, app: &mut App, term_height: u16) -> bool {
    match key.code {
        KeyCode::Esc => {
            match app.remove.phase {
                RemovePhase::Removing | RemovePhase::Analyzing => {
                    // 进行中：取消并返回浏览
                    crate::package_manager::cancel_update();
                    app.remove.phase = RemovePhase::Browsing;
                    app.remove.scroll = 0;
                }
                _ => {
                    // 完成/错误：返回主页
                    app.mode = AppMode::Dashboard;
                    app.reset_remove_state();
                }
            }
            true
        }
        KeyCode::Up => {
            app.remove.scroll = app.remove.scroll.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            let content = app.remove.get_content();
            let visible = layout::visible_content_height(term_height);
            let max_scroll = content.len().saturating_sub(visible);
            if app.remove.scroll < max_scroll {
                app.remove.scroll += 1;
            }
            true
        }
        KeyCode::PageUp => {
            app.remove.scroll = app.remove.scroll.saturating_sub(10);
            true
        }
        KeyCode::PageDown => {
            let content = app.remove.get_content();
            let visible = layout::visible_content_height(term_height);
            let max_scroll = content.len().saturating_sub(visible);
            app.remove.scroll = (app.remove.scroll + 10).min(max_scroll);
            true
        }
        _ => false,
    }
}

/// 完成状态按键处理
fn handle_complete_key(key: KeyEvent, app: &mut App, term_height: u16) -> bool {
    match key.code {
        KeyCode::Tab => {
            app.remove.view_mode = match app.remove.view_mode {
                ViewMode::UpdateLog => ViewMode::AIAnalysis,
                ViewMode::AIAnalysis => ViewMode::UpdateLog,
            };
            app.remove.scroll = 0;
            true
        }
        _ => handle_output_key(key, app, term_height),
    }
}

/// 收集选中的包名列表
fn collect_selected_packages(app: &App) -> Vec<String> {
    if app.remove.marked.is_empty() {
        // 没有多选标记，使用当前高亮项对应的原始索引
        if let Some(&real_idx) = app.remove.filtered.get(app.remove.selected) {
            if let Some(pkg) = app.remove.packages.get(real_idx) {
                vec![pkg.name.clone()]
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        app.remove.marked
            .iter()
            .filter_map(|&idx| app.remove.packages.get(idx))
            .map(|pkg| pkg.name.clone())
            .collect()
    }
}

/// 启动卸载异步任务
pub fn spawn_remove_task(app: &mut App, tx: &mpsc::Sender<AppEvent>) {
    let pm = match app.package_manager.clone() {
        Some(pm) => pm,
        None => return,
    };

    let packages = collect_selected_packages(app);
    if packages.is_empty() {
        return;
    }

    let tx_clone = tx.clone();
    app.remove.phase = RemovePhase::Removing;
    app.remove.lines.clear();
    app.remove.lines.push(format!(
        "正在卸载: {} ...",
        packages.join(", ")
    ));
    app.remove.scroll = 0;

    std::thread::spawn(move || {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::unbounded_channel();

        let tx_for_lines = tx_clone.clone();
        std::thread::spawn(move || {
            while let Some(line) = output_rx.blocking_recv() {
                let _ = tx_for_lines.blocking_send(AppEvent::RemoveLine(line));
            }
        });

        let result = pm.remove_streaming(&packages, output_tx);

        match result {
            Ok(output) => {
                let _ = tx_clone.blocking_send(AppEvent::RemoveComplete { output });
            }
            Err(e) => {
                let _ = tx_clone.blocking_send(AppEvent::Error(format!("卸载失败: {}", e)));
            }
        }
    });
}

/// 处理卸载完成事件
pub fn handle_remove_complete(
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
    api_key: &str,
) {
    if let Some(output) = &app.remove.output {
        if output.success && app.config.ai_enabled_for("remove") {
            app.remove.phase = RemovePhase::Analyzing;

            let pm_name = app.package_manager.as_ref().unwrap().name().to_string();
            let remove_log = output.combined_output();
            let sys_info = app.system_info.clone();
            let packages = collect_selected_packages(app);

            let prompt_text = format!(
                "以下是在 {} 系统上使用 {} -Rns 卸载软件包的日志。\n\
                 卸载的包: {}\n\n\
                 卸载日志:\n{}\n\n\
                 请简要分析卸载结果，说明是否成功，移除了哪些包及其依赖和配置，是否有需要注意的问题。",
                sys_info.as_ref().map(|i| i.distro.as_str()).unwrap_or("Linux"),
                pm_name,
                packages.join(", "),
                remove_log
            );

            let client = crate::deepseek::AiClient::new(
                api_key.to_string(),
                app.config.get_api_url().to_string(),
                app.config.proxy.as_deref(),
            );
            let model = app.config.model.clone();
            let temperature = app.config.temperature;
            let tx_clone = tx.clone();

            tokio::spawn(async move {
                match client.analyze_update(&prompt_text, &model, temperature).await {
                    Ok(analysis) => {
                        let _ = tx_clone.send(AppEvent::RemoveAnalysisComplete(analysis)).await;
                    }
                    Err(e) => {
                        let _ = tx_clone
                            .send(AppEvent::Error(format!("AI 分析失败: {}", e)))
                            .await;
                    }
                }
            });
        } else if output.success && !app.config.ai_enabled_for("remove") {
            let mut new_output = output.clone();
            new_output.stdout.push_str("\n\n[AI 分析已关闭，可在设置中开启]");
            app.remove.output = Some(new_output);
        }
    }
}

/// 处理卸载 AI 分析完成事件
pub fn handle_remove_analysis_complete(
    app: &mut App,
    analysis: String,
    tx: &mpsc::Sender<AppEvent>,
) {
    app.remove.analysis = Some(analysis.clone());
    app.remove.phase = RemovePhase::AnalysisComplete;
    app.remove.view_mode = ViewMode::AIAnalysis;
    app.remove.scroll = 0;

    let report_dir = app.config.report_dir.clone();
    let distro_name = app.system_info.as_ref()
        .map(|info| info.distro.clone())
        .unwrap_or_else(|| "Linux".to_string());
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let saver = crate::report::ReportSaver::new(report_dir);
        match saver.save(&analysis, &distro_name, "Rns") {
            Ok(path) => {
                let _ = tx_clone
                    .send(AppEvent::ReportSaved(path.display().to_string()))
                    .await;
            }
            Err(e) => {
                log::error!("保存报告失败: {}", e);
            }
        }
    });
}

// ===== 渲染 =====

/// 渲染卸载视图
pub fn render_remove(f: &mut Frame, app: &App) {
    match app.remove.phase {
        RemovePhase::Browsing => render_browse_view(f, app),
        RemovePhase::PreviewingRemove => render_preview_view(f, app),
        _ => render_output_view(f, app),
    }
}

/// 渲染浏览视图
fn render_browse_view(f: &mut Frame, app: &App) {
    let chunks = layout::main_layout(f.area());

    // Header
    layout::render_header(f, "🗑️  卸载软件包 (-Rns)", chunks[0]);

    // Content
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let content_inner = content_block.inner(chunks[1]);
    f.render_widget(content_block, chunks[1]);

    let padded = content_inner.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });

    if padded.height < 3 {
        return;
    }

    if app.remove.loading {
        let loading = Paragraph::new("正在加载已安装包列表...")
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(loading, padded);
        layout::render_footer(f, "加载中...", chunks[2]);
        return;
    }

    // 分割：搜索框 + 统计 + 列表
    let inner_chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(1), // 搜索框
            ratatui::layout::Constraint::Length(1), // 统计
            ratatui::layout::Constraint::Min(0),    // 包列表
        ])
        .split(padded);

    // 搜索框
    let ib = input_box_from_app(app);
    let search_text = format!("> 筛选: {}_", ib.content());
    let search_line = Paragraph::new(search_text)
        .style(Style::default().fg(Color::White));
    f.render_widget(search_line, inner_chunks[0]);

    // 统计行
    let stat_text = format!(
        "共 {} 个匹配 / 已安装 {} 个",
        app.remove.filtered.len(),
        app.remove.packages.len()
    );
    let stat_line = Paragraph::new(stat_text)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(stat_line, inner_chunks[1]);

    // 包列表
    render_package_list(f, app, inner_chunks[2]);

    // Footer
    let footer = if app.remove.filtered.is_empty() {
        "输入关键词筛选已安装包 | Esc 返回"
    } else if app.remove.marked.is_empty() {
        "↑↓ 选择 | Space 多选 | Enter 卸载选中 | Esc 返回"
    } else {
        "↑↓ 选择 | Space 多选/取消 | Enter 卸载标记项 | Esc 返回"
    };
    layout::render_footer(f, footer, chunks[2]);
}

/// 渲染已安装包列表
fn render_package_list(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.remove.filtered.is_empty() {
        if !app.remove.input.is_empty() {
            let hint = Paragraph::new("  未找到匹配的包")
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(hint, area);
        }
        return;
    }

    let visible_height = area.height as usize;
    let total = app.remove.filtered.len();

    let scroll = if app.remove.selected >= visible_height {
        app.remove.selected.saturating_sub(visible_height - 1)
    } else {
        0
    };

    // 计算大小列对齐宽度
    let max_name_width = app.remove.filtered
        .iter()
        .skip(scroll)
        .take(visible_height)
        .filter_map(|&idx| app.remove.packages.get(idx))
        .map(|pkg| {
            let display = format!("{} {}", pkg.name, pkg.version);
            UnicodeWidthStr::width(display.as_str())
        })
        .max()
        .unwrap_or(20);

    let lines: Vec<Line> = app.remove.filtered
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(display_idx, &real_idx)| {
            let pkg = &app.remove.packages[real_idx];
            let is_selected = display_idx == app.remove.selected;
            let is_marked = app.remove.marked.contains(&real_idx);

            let marker = if is_marked { "[✓] " } else { "    " };
            let cursor = if is_selected { ">" } else { " " };

            let name_width = UnicodeWidthStr::width(pkg.name.as_str());
            let ver_width = UnicodeWidthStr::width(pkg.version.as_str());
            let name_ver_width = name_width + 1 + ver_width; // +1 for space
            let padding = max_name_width.saturating_sub(name_ver_width) + 2;

            // MTF flag colors from theme

            if is_selected {
                // 选中行：深色背景 + 多色加粗
                let bg = Style::default().bg(SEL_BG);
                Line::from(vec![
                    Span::styled(format!("{}{}", cursor, marker), bg.fg(BRIGHT_WHITE).add_modifier(Modifier::BOLD)),
                    Span::styled(pkg.name.clone(), bg.fg(BRIGHT_WHITE).add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" {}", pkg.version), bg.fg(BLUE)),
                    Span::styled(format!("{}{}", " ".repeat(padding), pkg.size), bg.fg(DESC_DIM)),
                ])
            } else if is_marked {
                // 标记行：粉色标识
                Line::from(vec![
                    Span::styled(format!("{}{}", cursor, marker), Style::default().fg(PINK)),
                    Span::styled(pkg.name.clone(), Style::default().fg(PINK)),
                    Span::styled(format!(" {}", pkg.version), Style::default().fg(Color::White)),
                    Span::styled(format!("{}{}", " ".repeat(padding), pkg.size), Style::default().fg(DIM)),
                ])
            } else {
                // 正常行：名称蓝色，版本白色，大小灰色
                Line::from(vec![
                    Span::styled(format!("{}{}", cursor, marker), Style::default().fg(Color::White)),
                    Span::styled(pkg.name.clone(), Style::default().fg(BLUE)),
                    Span::styled(format!(" {}", pkg.version), Style::default().fg(Color::White)),
                    Span::styled(format!("{}{}", " ".repeat(padding), pkg.size), Style::default().fg(DIM)),
                ])
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, area);

    // 滚动条
    if total > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut state = ScrollbarState::new(total).position(app.remove.selected);
        f.render_stateful_widget(
            scrollbar,
            area.inner(Margin { horizontal: 0, vertical: 0 }),
            &mut state,
        );
    }
}

/// 渲染卸载预览视图
fn render_preview_view(f: &mut Frame, app: &App) {
    let chunks = layout::main_layout(f.area());

    let packages = collect_selected_packages(app);
    let header_text = format!(
        "🗑️  卸载预览 - {} 个包: {}",
        packages.len(),
        packages.join(", ")
    );
    layout::render_header(f, &header_text, chunks[0]);

    layout::render_scrollable_content(
        f,
        "将卸载以下软件包及其依赖",
        &app.remove.preview,
        app.remove.scroll,
        chunks[1],
    );

    let footer = if app.remove.preview.len() == 1
        && app.remove.preview[0].contains("正在获取")
    {
        "正在获取卸载预览..."
    } else {
        "按 Enter 确认卸载 | Esc 返回列表 | ↑↓ 滚动"
    };
    layout::render_footer(f, footer, chunks[2]);
}

/// 渲染输出视图
fn render_output_view(f: &mut Frame, app: &App) {
    let chunks = layout::main_layout(f.area());

    let title = match app.remove.phase {
        RemovePhase::Removing => "⚙️  正在卸载...",
        RemovePhase::RemoveComplete => "✅ 卸载完成",
        RemovePhase::Analyzing => "🤖 AI 分析中...",
        RemovePhase::AnalysisComplete => "✨ 分析完成",
        RemovePhase::Error => "❌ 错误",
        _ => "🗑️  卸载",
    };

    let pm_info = if let Some(pm) = &app.package_manager {
        format!(" | 包管理器: {}", pm.name())
    } else {
        String::new()
    };

    let header = Paragraph::new(format!("{}{}", title, pm_info))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    f.render_widget(header, chunks[0]);

    let content_title = if app.remove.phase == RemovePhase::AnalysisComplete {
        match app.remove.view_mode {
            ViewMode::UpdateLog => "卸载日志 [Tab 切换到 AI 分析]",
            ViewMode::AIAnalysis => "AI 分析报告 [Tab 切换到卸载日志]",
        }
    } else {
        "卸载日志"
    };

    let content = app.remove.get_content();
    layout::render_scrollable_content(f, content_title, &content, app.remove.scroll, chunks[1]);

    let owned_text: String;
    let footer_text = match app.remove.phase {
        RemovePhase::Removing => {
            if app.remove.progress.is_empty() {
                "卸载进行中..."
            } else {
                owned_text = format!("卸载进行中 | {}", app.remove.progress);
                &owned_text
            }
        }
        RemovePhase::RemoveComplete => "卸载完成 | ↑↓ 滚动 | Esc 返回主页",
        RemovePhase::Analyzing => "AI 正在分析卸载内容...",
        RemovePhase::AnalysisComplete => {
            if let Some(path) = &app.remove.report_path {
                owned_text = format!("报告已保存: {} | Tab 切换视图 | Esc 返回主页", path);
                &owned_text
            } else {
                "Tab 切换视图 | ↑↓ 滚动 | Esc 返回主页"
            }
        }
        RemovePhase::Error => {
            if let Some(msg) = &app.error_message {
                msg
            } else {
                "发生错误 | Esc 返回主页"
            }
        }
        _ => "Esc 返回",
    };

    layout::render_footer(f, footer_text, chunks[2]);
}
