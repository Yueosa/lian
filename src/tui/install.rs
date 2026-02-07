use super::input::InputBox;
use super::layout;
use super::state::{App, AppEvent, AppMode, InstallState, ViewMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};
use tokio::sync::mpsc;

/// 从 App 状态构建 InputBox 用于渲染
 fn input_box_from_app(app: &App) -> InputBox {
    let mut ib = InputBox::new();
    for c in app.install_input.chars() {
        ib.insert(c);
    }
    ib.move_home();
    for _ in 0..app.install_cursor {
        ib.move_right();
    }
    ib
}

/// UTF-8 安全的字符位置转字节位置
fn char_to_byte(s: &str, char_pos: usize) -> usize {
    s.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

fn insert_char(app: &mut App, c: char) {
    let byte_pos = char_to_byte(&app.install_input, app.install_cursor);
    app.install_input.insert(byte_pos, c);
    app.install_cursor += 1;
}

fn delete_back(app: &mut App) {
    if app.install_cursor > 0 {
        app.install_cursor -= 1;
        let byte_pos = char_to_byte(&app.install_input, app.install_cursor);
        let next_byte_pos = char_to_byte(&app.install_input, app.install_cursor + 1);
        app.install_input.drain(byte_pos..next_byte_pos);
    }
}

fn delete_forward(app: &mut App) {
    let char_count = app.install_input.chars().count();
    if app.install_cursor < char_count {
        let byte_pos = char_to_byte(&app.install_input, app.install_cursor);
        let next_byte_pos = char_to_byte(&app.install_input, app.install_cursor + 1);
        app.install_input.drain(byte_pos..next_byte_pos);
    }
}

/// 处理安装模式按键
pub fn handle_install_key(
    key: KeyEvent,
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
    term_height: u16,
) -> bool {
    match app.install_state {
        InstallState::Searching => handle_searching_key(key, app, tx),
        InstallState::PreviewingInstall => handle_preview_key(key, app),
        InstallState::Installing => handle_output_key(key, app, term_height),
        InstallState::InstallComplete => handle_output_key(key, app, term_height),
        InstallState::Analyzing => handle_output_key(key, app, term_height),
        InstallState::AnalysisComplete => handle_complete_key(key, app, term_height),
        InstallState::Error => handle_output_key(key, app, term_height),
    }
}

/// 搜索状态按键处理
fn handle_searching_key(
    key: KeyEvent,
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Dashboard;
            app.reset_install_state();
            true
        }
        KeyCode::Up => {
            app.install_selected = app.install_selected.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            let max = app.install_results.len().saturating_sub(1);
            if app.install_selected < max {
                app.install_selected += 1;
            }
            true
        }
        KeyCode::Char(' ') => {
            // 多选切换
            if !app.install_results.is_empty() {
                if app.install_marked.contains(&app.install_selected) {
                    app.install_marked.remove(&app.install_selected);
                } else {
                    app.install_marked.insert(app.install_selected);
                }
                // 选中后自动下移
                let max = app.install_results.len().saturating_sub(1);
                if app.install_selected < max {
                    app.install_selected += 1;
                }
            }
            true
        }
        KeyCode::Enter => {
            // 收集选中的包，准备安装
            if !app.install_results.is_empty() {
                let packages = collect_selected_packages(app);
                if !packages.is_empty() {
                    // 获取安装预览
                    if let Some(pm) = app.package_manager.clone() {
                        let tx_clone = tx.clone();
                        let pkgs = packages.clone();
                        tokio::spawn(async move {
                            let preview = tokio::task::spawn_blocking(move || {
                                pm.preview_install(&pkgs)
                            })
                            .await
                            .unwrap_or_default();
                            let _ = tx_clone.send(AppEvent::InstallPreviewReady(preview)).await;
                        });
                        app.install_state = InstallState::PreviewingInstall;
                        app.install_preview = vec!["正在获取安装预览...".to_string()];
                        app.install_scroll = 0;
                    }
                }
            }
            true
        }
        KeyCode::Backspace => {
            delete_back(app);
            trigger_search(app, tx);
            true
        }
        KeyCode::Delete => {
            delete_forward(app);
            trigger_search(app, tx);
            true
        }
        KeyCode::Left => {
            if app.install_cursor > 0 {
                app.install_cursor -= 1;
            }
            true
        }
        KeyCode::Right => {
            let max = app.install_input.chars().count();
            if app.install_cursor < max {
                app.install_cursor += 1;
            }
            true
        }
        KeyCode::Home => {
            app.install_cursor = 0;
            true
        }
        KeyCode::End => {
            app.install_cursor = app.install_input.chars().count();
            true
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                return false;
            }
            insert_char(app, c);
            trigger_search(app, tx);
            true
        }
        _ => false,
    }
}

/// 预览状态按键处理
fn handle_preview_key(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.install_state = InstallState::Searching;
            app.install_preview.clear();
            app.install_scroll = 0;
            true
        }
        KeyCode::Up => {
            app.install_scroll = app.install_scroll.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            app.install_scroll += 1;
            true
        }
        // Enter 在 mod.rs 中处理（需要 sudo）
        _ => false,
    }
}

/// 输出状态按键处理（Installing/Complete/Analyzing/Error）
fn handle_output_key(key: KeyEvent, app: &mut App, term_height: u16) -> bool {
    match key.code {
        KeyCode::Up => {
            app.install_scroll = app.install_scroll.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            let content = app.get_install_content();
            let visible = layout::visible_content_height(term_height);
            let max_scroll = content.len().saturating_sub(visible);
            if app.install_scroll < max_scroll {
                app.install_scroll += 1;
            }
            true
        }
        KeyCode::PageUp => {
            app.install_scroll = app.install_scroll.saturating_sub(10);
            true
        }
        KeyCode::PageDown => {
            let content = app.get_install_content();
            let visible = layout::visible_content_height(term_height);
            let max_scroll = content.len().saturating_sub(visible);
            app.install_scroll = (app.install_scroll + 10).min(max_scroll);
            true
        }
        _ => false,
    }
}

/// 完成状态按键处理（可切换 Tab）
fn handle_complete_key(key: KeyEvent, app: &mut App, term_height: u16) -> bool {
    match key.code {
        KeyCode::Tab => {
            app.install_view_mode = match app.install_view_mode {
                ViewMode::UpdateLog => ViewMode::AIAnalysis,
                ViewMode::AIAnalysis => ViewMode::UpdateLog,
            };
            app.install_scroll = 0;
            true
        }
        _ => handle_output_key(key, app, term_height),
    }
}

/// 触发异步搜索
fn trigger_search(app: &mut App, tx: &mpsc::Sender<AppEvent>) {
    let keyword = app.install_input.clone();
    if keyword.trim().is_empty() {
        app.install_results.clear();
        app.install_selected = 0;
        app.install_marked.clear();
        app.install_searching = false;
        return;
    }

    if let Some(pm) = app.package_manager.clone() {
        app.install_searching = true;
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let results = tokio::task::spawn_blocking(move || pm.search_remote(&keyword))
                .await
                .unwrap_or_default();
            let _ = tx_clone.send(AppEvent::InstallSearchResults(results)).await;
        });
    }
}

/// 收集选中的包名列表
fn collect_selected_packages(app: &App) -> Vec<String> {
    if app.install_marked.is_empty() {
        if let Some(pkg) = app.install_results.get(app.install_selected) {
            vec![pkg.name.clone()]
        } else {
            Vec::new()
        }
    } else {
        app.install_marked
            .iter()
            .filter_map(|&idx| app.install_results.get(idx))
            .map(|pkg| pkg.name.clone())
            .collect()
    }
}

/// 启动安装异步任务
pub fn spawn_install_task(app: &mut App, tx: &mpsc::Sender<AppEvent>) {
    let pm = match app.package_manager.clone() {
        Some(pm) => pm,
        None => return,
    };

    let packages = collect_selected_packages(app);
    if packages.is_empty() {
        return;
    }

    let tx_clone = tx.clone();
    app.install_state = InstallState::Installing;
    app.install_lines.clear();
    app.install_lines.push(format!(
        "正在安装: {} ...",
        packages.join(", ")
    ));
    app.install_scroll = 0;

    std::thread::spawn(move || {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::unbounded_channel();

        let tx_for_lines = tx_clone.clone();
        std::thread::spawn(move || {
            while let Some(line) = output_rx.blocking_recv() {
                let _ = tx_for_lines.blocking_send(AppEvent::InstallLine(line));
            }
        });

        let result = pm.install_streaming(&packages, output_tx);

        match result {
            Ok(output) => {
                let _ = tx_clone.blocking_send(AppEvent::InstallComplete { output });
            }
            Err(e) => {
                let _ = tx_clone.blocking_send(AppEvent::Error(format!("安装失败: {}", e)));
            }
        }
    });
}

/// 处理安装完成事件
pub fn handle_install_complete(
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
    api_key: &str,
    config: &crate::config::Config,
) {
    if let Some(output) = &app.install_output {
        if output.success && config.ai_enabled_for("install") {
            app.install_state = InstallState::Analyzing;

            let pm_name = app.package_manager.as_ref().unwrap().name().to_string();
            let install_log = output.combined_output();
            let sys_info = app.system_info.clone();
            let packages = collect_selected_packages(app);

            let prompt_text = format!(
                "以下是在 {} 系统上使用 {} 安装软件包的日志。\n\
                 安装的包: {}\n\n\
                 安装日志:\n{}\n\n\
                 请简要分析安装结果，说明是否成功，安装了哪些包及其依赖，是否有需要注意的问题。",
                sys_info.as_ref().map(|i| i.distro.as_str()).unwrap_or("Linux"),
                pm_name,
                packages.join(", "),
                install_log
            );

            let client = crate::deepseek::AiClient::new(
                api_key.to_string(),
                config.get_api_url().to_string(),
                config.proxy.as_deref(),
            );
            let model = config.model.clone();
            let temperature = config.temperature;
            let tx_clone = tx.clone();

            tokio::spawn(async move {
                match client.analyze_update(&prompt_text, &model, temperature).await {
                    Ok(analysis) => {
                        let _ = tx_clone.send(AppEvent::InstallAnalysisComplete(analysis)).await;
                    }
                    Err(e) => {
                        let _ = tx_clone
                            .send(AppEvent::Error(format!("AI 分析失败: {}", e)))
                            .await;
                    }
                }
            });
        } else if output.success && !config.ai_enabled_for("install") {
            let mut new_output = output.clone();
            new_output.stdout.push_str("\n\n[AI 分析已关闭，可在设置中开启]");
            app.install_output = Some(new_output);
        }
    }
}

/// 处理安装 AI 分析完成事件
pub fn handle_install_analysis_complete(
    app: &mut App,
    analysis: String,
    tx: &mpsc::Sender<AppEvent>,
    config: &crate::config::Config,
) {
    app.install_analysis = Some(analysis.clone());
    app.install_state = InstallState::AnalysisComplete;
    app.install_view_mode = ViewMode::AIAnalysis;
    app.install_scroll = 0;

    let report_dir = config.report_dir.clone();
    let distro_name = app.system_info.as_ref()
        .map(|info| info.distro.clone())
        .unwrap_or_else(|| "Linux".to_string());
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let saver = crate::report::ReportSaver::new(report_dir);
        match saver.save(&analysis, &distro_name) {
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

/// 渲染安装视图
pub fn render_install(f: &mut Frame, app: &App) {
    match app.install_state {
        InstallState::Searching => render_search_view(f, app),
        InstallState::PreviewingInstall => render_preview_view(f, app),
        _ => render_output_view(f, app),
    }
}

/// 渲染搜索视图
fn render_search_view(f: &mut Frame, app: &App) {
    let chunks = layout::main_layout(f.area());

    // Header
    layout::render_header(f, "📦 安装软件包 (-S)", chunks[0]);

    // Content: 搜索框 + 结果列表
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

    // 分割：搜索框(1行) + 间隔(1行) + 结果列表
    let inner_chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(1), // 搜索框
            ratatui::layout::Constraint::Length(1), // 间隔
            ratatui::layout::Constraint::Min(0),    // 结果列表
        ])
        .split(padded);

    // 搜索框
    let ib = input_box_from_app(app);
    let search_text = if app.install_searching {
        format!("> 搜索: {}_ (搜索中...)", ib.content())
    } else {
        format!("> 搜索: {}_", ib.content())
    };
    let search_line = Paragraph::new(search_text)
        .style(Style::default().fg(Color::White));
    f.render_widget(search_line, inner_chunks[0]);

    // 结果列表
    render_result_list(f, app, inner_chunks[2]);

    // Footer
    let footer = if app.install_results.is_empty() {
        "输入关键词搜索远程仓库包 | Esc 返回"
    } else if app.install_marked.is_empty() {
        "↑↓ 选择 | Space 多选 | Enter 安装选中 | Esc 返回"
    } else {
        "↑↓ 选择 | Space 多选/取消 | Enter 安装标记项 | Esc 返回"
    };
    layout::render_footer(f, footer, chunks[2]);
}

/// 渲染搜索结果列表
fn render_result_list(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.install_results.is_empty() {
        if !app.install_input.is_empty() && !app.install_searching {
            let hint = Paragraph::new("  未找到匹配的包")
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(hint, area);
        }
        return;
    }

    let visible_height = area.height as usize;
    let total = app.install_results.len();

    // 确保选中项在可见范围内
    let scroll = if app.install_selected >= visible_height {
        app.install_selected.saturating_sub(visible_height - 1)
    } else {
        0
    };

    let lines: Vec<Line> = app.install_results
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(idx, pkg)| {
            let is_selected = idx == app.install_selected;
            let is_marked = app.install_marked.contains(&idx);

            let marker = if is_marked { "[✓] " } else { "    " };
            let cursor = if is_selected { ">" } else { " " };
            let installed_tag = if pkg.installed { " [已安装]" } else { "" };

            let text = format!(
                "{}{}{}/{} {}{} - {}",
                cursor, marker, pkg.repo, pkg.name, pkg.version, installed_tag, pkg.description
            );

            let style = if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if is_marked {
                Style::default().fg(Color::Green)
            } else if pkg.installed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            Line::from(Span::styled(text, style))
        })
        .collect();

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);

    // 滚动条
    if total > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut state = ScrollbarState::new(total).position(scroll);
        f.render_stateful_widget(
            scrollbar,
            area.inner(Margin { horizontal: 0, vertical: 0 }),
            &mut state,
        );
    }
}

/// 渲染安装预览视图
fn render_preview_view(f: &mut Frame, app: &App) {
    let chunks = layout::main_layout(f.area());

    let packages = collect_selected_packages(app);
    let header_text = format!(
        "📦 安装预览 - {} 个包: {}",
        packages.len(),
        packages.join(", ")
    );
    layout::render_header(f, &header_text, chunks[0]);

    layout::render_scrollable_content(
        f,
        "将安装以下软件包",
        &app.install_preview,
        app.install_scroll,
        chunks[1],
    );

    let footer = if app.install_preview.len() == 1
        && app.install_preview[0].contains("正在获取")
    {
        "正在获取安装预览..."
    } else {
        "按 Enter 确认安装 | Esc 返回搜索 | ↑↓ 滚动"
    };
    layout::render_footer(f, footer, chunks[2]);
}

/// 渲染输出视图（安装中/完成/分析中/分析完成/错误）
fn render_output_view(f: &mut Frame, app: &App) {
    let chunks = layout::main_layout(f.area());

    // Header
    let title = match app.install_state {
        InstallState::Installing => "⚙️  正在安装...",
        InstallState::InstallComplete => "✅ 安装完成",
        InstallState::Analyzing => "🤖 AI 分析中...",
        InstallState::AnalysisComplete => "✨ 分析完成",
        InstallState::Error => "❌ 错误",
        _ => "📦 安装",
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

    // Content
    let content_title = if app.install_state == InstallState::AnalysisComplete {
        match app.install_view_mode {
            ViewMode::UpdateLog => "安装日志 [Tab 切换到 AI 分析]",
            ViewMode::AIAnalysis => "AI 分析报告 [Tab 切换到安装日志]",
        }
    } else {
        "安装日志"
    };

    let content = app.get_install_content();
    layout::render_scrollable_content(f, content_title, &content, app.install_scroll, chunks[1]);

    // Footer
    let owned_text: String;
    let footer_text = match app.install_state {
        InstallState::Installing => {
            if app.install_progress.is_empty() {
                "安装进行中..."
            } else {
                owned_text = format!("安装进行中 | {}", app.install_progress);
                &owned_text
            }
        }
        InstallState::InstallComplete => "安装完成 | ↑↓ 滚动 | Esc 返回主页",
        InstallState::Analyzing => "AI 正在分析安装内容...",
        InstallState::AnalysisComplete => {
            if let Some(path) = &app.install_saved_report {
                owned_text = format!("报告已保存: {} | Tab 切换视图 | Esc 返回主页", path);
                &owned_text
            } else {
                "Tab 切换视图 | ↑↓ 滚动 | Esc 返回主页"
            }
        }
        InstallState::Error => {
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
