use super::layout;
use super::state::{App, AppEvent, UpdatePhase, ViewMode};
use crate::deepseek::AiClient;
use crate::prompt;
use crate::report::ReportSaver;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    Frame,
};
use tokio::sync::mpsc;

/// 处理更新模式的按键事件，返回 true 表示已消费该按键
pub fn handle_update_key(
    key: KeyEvent,
    app: &mut App,
    term_height: u16,
) -> bool {
    match key.code {
        KeyCode::Tab => {
            if app.update.phase == UpdatePhase::AnalysisComplete {
                app.update.view_mode = match app.update.view_mode {
                    ViewMode::UpdateLog => ViewMode::AIAnalysis,
                    ViewMode::AIAnalysis => ViewMode::UpdateLog,
                };
                app.update.reset_scroll();
            }
            true
        }
        KeyCode::Up => {
            app.update.scroll_up();
            true
        }
        KeyCode::Down => {
            let content = app.update.get_content();
            let visible = layout::visible_content_height(term_height);
            app.update.scroll_down(content.len(), visible);
            true
        }
        KeyCode::PageUp => {
            app.update.scroll_page_up(10);
            true
        }
        KeyCode::PageDown => {
            let content = app.update.get_content();
            let visible = layout::visible_content_height(term_height);
            app.update.scroll_page_down(10, content.len(), visible);
            true
        }
        _ => false,
    }
}

/// 启动更新异步任务
pub fn spawn_update_task(app: &mut App, tx: &mpsc::Sender<AppEvent>) {
    let pm = match app.package_manager.clone() {
        Some(pm) => pm,
        None => return,
    };
    let tx_clone = tx.clone();
    app.update.phase = UpdatePhase::Updating;
    app.update.lines.clear();
    app.update.lines.push("正在执行更新...".to_string());

    std::thread::spawn(move || {
        let packages_before = pm.get_explicit_packages().ok();

        let (output_tx, mut output_rx) = tokio::sync::mpsc::unbounded_channel();

        let tx_for_lines = tx_clone.clone();
        std::thread::spawn(move || {
            while let Some(line) = output_rx.blocking_recv() {
                let _ = tx_for_lines.blocking_send(AppEvent::UpdateLine(line));
            }
        });

        let result = pm.update_streaming(output_tx);

        match result {
            Ok(output) => {
                let packages_after = pm.get_explicit_packages().ok();
                let _ = tx_clone.blocking_send(AppEvent::UpdateComplete {
                    output,
                    packages_before,
                    packages_after,
                });
            }
            Err(e) => {
                let _ =
                    tx_clone.blocking_send(AppEvent::Error(format!("更新失败: {}", e)));
            }
        }
    });
}

/// 处理更新完成事件，启动 AI 分析
pub fn handle_update_complete(
    app: &mut App,
    tx: &mpsc::Sender<AppEvent>,
    api_key: &str,
) {
    if let Some(output) = &app.update.output {
        if output.success && app.config.ai_enabled_for("update") {
            app.update.phase = UpdatePhase::Analyzing;

            let pm_name = app.package_manager.as_ref().unwrap().name().to_string();
            let update_log = output.combined_output();
            let pkg_before = app.update.packages_before.as_deref();
            let pkg_after = app.update.packages_after.as_deref();
            let sys_info = app.system_info.clone();

            let prompt_text = prompt::generate_analysis_prompt(
                &pm_name,
                &update_log,
                pkg_before,
                pkg_after,
                sys_info.as_ref(),
            );

            let client = AiClient::new(
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
                        let _ = tx_clone.send(AppEvent::AnalysisComplete(analysis)).await;
                    }
                    Err(e) => {
                        let _ = tx_clone
                            .send(AppEvent::Error(format!("AI 分析失败: {}", e)))
                            .await;
                    }
                }
            });
        } else if output.success {
            // AI 分析已关闭，在 stdout 中追加提示
        }
    }
    // 如果 AI 未启用但更新成功，追加提示到输出
    if let Some(ref output) = app.update.output {
        if output.success && !app.config.ai_enabled_for("update") {
            let mut new_output = output.clone();
            new_output.stdout.push_str("\n\n[AI 分析已关闭，可在设置中开启]");
            app.update.output = Some(new_output);
        }
    }
}

/// 处理分析完成事件，保存报告
pub fn handle_analysis_complete(
    app: &mut App,
    analysis: String,
    tx: &mpsc::Sender<AppEvent>,
) {
    app.update.analysis = Some(analysis.clone());
    app.update.phase = UpdatePhase::AnalysisComplete;
    app.update.view_mode = ViewMode::AIAnalysis;
    app.update.reset_scroll();

    let report_dir = app.config.report_dir.clone();
    let distro_name = app.system_info.as_ref()
        .map(|info| info.distro.clone())
        .unwrap_or_else(|| "Linux".to_string());
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let saver = ReportSaver::new(report_dir);
        match saver.save(&analysis, &distro_name, "Syu") {
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

/// 渲染更新视图
pub fn render_update(f: &mut Frame, app: &App) {
    let chunks = layout::main_layout(f.area());

    render_update_header(f, app, chunks[0]);
    render_update_content(f, app, chunks[1]);
    render_update_footer(f, app, chunks[2]);
}

fn render_update_header(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let title = match app.update.phase {
        UpdatePhase::PackageManagerCheck => "🔍 检测包管理器...",
        UpdatePhase::PreviewingUpdates => "📝 可用更新列表",
        UpdatePhase::Updating => "⚙️  正在更新系统...",
        UpdatePhase::UpdateComplete => "✅ 更新完成",
        UpdatePhase::Analyzing => "🤖 AI 分析中...",
        UpdatePhase::AnalysisComplete => "✨ 分析完成",
        UpdatePhase::Error => "❌ 错误",
    };

    let pm_info = if let Some(pm) = &app.package_manager {
        format!(" | 包管理器: {}", pm.name())
    } else {
        String::new()
    };

    let header = ratatui::widgets::Paragraph::new(format!("{}{}", title, pm_info))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::ALL))
        .alignment(Alignment::Center);

    f.render_widget(header, area);
}

fn render_update_content(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let title = match app.update.view_mode {
        ViewMode::UpdateLog => "更新日志 [Tab 切换到 AI 分析]",
        ViewMode::AIAnalysis => "AI 分析报告 [Tab 切换到更新日志]",
    };

    let content = app.update.get_content();
    layout::render_scrollable_content(f, title, &content, app.update.scroll, area);
}

fn render_update_footer(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let owned_text: String;
    let footer_text = match app.update.phase {
        UpdatePhase::PackageManagerCheck => "正在检测包管理器...",
        UpdatePhase::PreviewingUpdates => {
            if app.update.preview.is_empty() {
                "Esc 返回 | q 退出"
            } else {
                "按 Enter 开始更新 | Esc 返回 | ↑↓ 滚动"
            }
        }
        UpdatePhase::Updating => {
            if app.update.progress.is_empty() {
                "更新进行中..."
            } else {
                owned_text = format!("更新进行中 | {}", app.update.progress);
                &owned_text
            }
        }
        UpdatePhase::UpdateComplete => "更新完成 | ↑↓ 滚动 | Esc 返回主页",
        UpdatePhase::Analyzing => "AI 正在分析更新内容...",
        UpdatePhase::AnalysisComplete => {
            if let Some(path) = &app.update.report_path {
                owned_text = format!(
                    "报告已保存: {} | Tab 切换视图 | ↑↓ 滚动 | Esc 返回主页 | q 退出",
                    path
                );
                &owned_text
            } else {
                "Tab 切换视图 | ↑↓ 滚动 | Esc 返回主页 | q 退出"
            }
        }
        UpdatePhase::Error => {
            if let Some(msg) = &app.error_message {
                msg
            } else {
                "发生错误 | Esc 返回主页 | q 退出"
            }
        }
    };

    layout::render_footer(f, footer_text, area);
}
