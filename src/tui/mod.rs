mod dashboard;
pub mod input;
mod install;
mod layout;
mod query;
mod remove;
mod settings;
pub mod state;
mod update;

use crate::config::Config;
use crate::package_manager::PackageManager;
use crate::sysinfo::SystemInfo;
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};
use state::{App, AppEvent, AppMode, AppState};
use std::io;
use tokio::sync::mpsc;

pub async fn run(api_key: String, config: Config) -> Result<()> {
    // 终端初始化
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config);

    let (tx, mut rx) = mpsc::channel(32);

    // 检测包管理器
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        match PackageManager::detect() {
            Ok(pm) => {
                let _ = tx_clone.send(AppEvent::PackageManagerDetected(pm)).await;
            }
            Err(e) => {
                let _ = tx_clone
                    .send(AppEvent::Error(format!("检测包管理器失败: {}", e)))
                    .await;
            }
        }
    });

    // 异步获取系统信息
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let info = tokio::task::spawn_blocking(SystemInfo::detect)
            .await
            .unwrap_or_else(|_| SystemInfo::detect());
        let _ = tx_clone.send(AppEvent::SystemInfoDetected(info)).await;
    });

    // 主循环
    loop {
        // 更新模式下 clamp scroll
        if app.mode == AppMode::Update {
            let content = app.get_current_content();
            let term_size = terminal.size()?;
            let visible_height = layout::visible_content_height(term_size.height);
            app.clamp_scroll(content.len(), visible_height);
        }
        // 查询详情视图 clamp scroll
        if app.mode == AppMode::Query && app.query_view == state::QueryView::Detail {
            let term_size = terminal.size()?;
            let total = query::detail_total_lines(&app);
            let visible = term_size.height.saturating_sub(8) as usize;
            let max_scroll = total.saturating_sub(visible);
            app.query_detail_scroll = app.query_detail_scroll.min(max_scroll);
        }
        // 安装模式 clamp scroll（输出阶段）
        if app.mode == AppMode::Install {
            match app.install_state {
                state::InstallState::Installing
                | state::InstallState::InstallComplete
                | state::InstallState::Analyzing
                | state::InstallState::AnalysisComplete
                | state::InstallState::Error => {
                    let content = app.get_install_content();
                    let term_size = terminal.size()?;
                    let visible = layout::visible_content_height(term_size.height);
                    let max_scroll = content.len().saturating_sub(visible);
                    app.install_scroll = app.install_scroll.min(max_scroll);
                }
                state::InstallState::PreviewingInstall => {
                    let term_size = terminal.size()?;
                    let visible = layout::visible_content_height(term_size.height);
                    let max_scroll = app.install_preview.len().saturating_sub(visible);
                    app.install_scroll = app.install_scroll.min(max_scroll);
                }
                _ => {}
            }
        }
        // 卸载模式 clamp scroll（输出阶段）
        if app.mode == AppMode::Remove {
            match app.remove_state {
                state::RemoveState::Removing
                | state::RemoveState::RemoveComplete
                | state::RemoveState::Analyzing
                | state::RemoveState::AnalysisComplete
                | state::RemoveState::Error => {
                    let content = app.get_remove_content();
                    let term_size = terminal.size()?;
                    let visible = layout::visible_content_height(term_size.height);
                    let max_scroll = content.len().saturating_sub(visible);
                    app.remove_scroll = app.remove_scroll.min(max_scroll);
                }
                state::RemoveState::PreviewingRemove => {
                    let term_size = terminal.size()?;
                    let visible = layout::visible_content_height(term_size.height);
                    let max_scroll = app.remove_preview.len().saturating_sub(visible);
                    app.remove_scroll = app.remove_scroll.min(max_scroll);
                }
                _ => {}
            }
        }

        terminal.draw(|f| ui(f, &app))?;

        // 处理事件
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let term_size = terminal.size()?;

                // 全局按键
                match key.code {
                    // q 仅在 Dashboard 退出
                    KeyCode::Char('q') if app.mode == AppMode::Dashboard => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if app.mode == AppMode::Update
                            || app.mode == AppMode::Install
                            || app.mode == AppMode::Remove
                        {
                            crate::package_manager::cancel_update();
                        }
                        app.should_quit = true;
                    }
                    KeyCode::Esc => {
                        match app.mode {
                            AppMode::Dashboard => {}
                            AppMode::Update => {
                                crate::package_manager::cancel_update();
                                app.mode = AppMode::Dashboard;
                                app.reset_scroll();
                            }
                            AppMode::Query => {
                                query::handle_query_key(
                                    crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                                    &mut app,
                                    &tx,
                                    term_size.height,
                                );
                            }
                            AppMode::Install => {
                                install::handle_install_key(
                                    crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                                    &mut app,
                                    &tx,
                                    term_size.height,
                                );
                            }
                            AppMode::Remove => {
                                remove::handle_remove_key(
                                    crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                                    &mut app,
                                    &tx,
                                    term_size.height,
                                );
                            }
                            AppMode::Settings => {
                                settings::handle_settings_key(
                                    crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                                    &mut app,
                                );
                            }
                            _ => {
                                app.mode = AppMode::Dashboard;
                                app.reset_scroll();
                            }
                        }
                    }
                    // 模式切换快捷键 (Shift + 字母)
                    KeyCode::Char('U') => {
                        if app.mode != AppMode::Update {
                            app.mode = AppMode::Update;
                            app.reset_update_state();
                            // 如果 PM 已检测到，直接检查可用更新
                            if let Some(pm) = app.package_manager.clone() {
                                app.update_lines.push("正在检查可用更新...".to_string());
                                let tx_clone = tx.clone();
                                tokio::spawn(async move {
                                    let updates = tokio::task::spawn_blocking(move || pm.check_updates())
                                        .await
                                        .unwrap_or_default();
                                    let _ = tx_clone.send(AppEvent::UpdatePreviewReady(updates)).await;
                                });
                            }
                        }
                    }
                    KeyCode::Char('S') => {
                        if app.mode != AppMode::Install {
                            app.mode = AppMode::Install;
                            app.reset_install_state();
                        }
                    }
                    KeyCode::Char('R') => {
                        if app.mode != AppMode::Remove {
                            app.mode = AppMode::Remove;
                            app.reset_remove_state();
                            // 自动加载已安装包列表
                            if let Some(pm) = app.package_manager.clone() {
                                app.remove_loading = true;
                                let tx_clone = tx.clone();
                                tokio::spawn(async move {
                                    let packages = tokio::task::spawn_blocking(move || {
                                        pm.get_installed_packages_with_size()
                                    })
                                    .await
                                    .unwrap_or_default();
                                    let _ = tx_clone.send(AppEvent::RemovePackagesLoaded(packages)).await;
                                });
                            }
                        }
                    }
                    KeyCode::Char('Q') => {
                        if app.mode != AppMode::Query {
                            app.mode = AppMode::Query;
                            app.reset_query_state();
                        }
                    }
                    KeyCode::Char('C') => {
                        app.mode = AppMode::Settings;
                        app.build_settings_items();
                    }
                    // 委托给当前模式处理
                    _ => {
                        match app.mode {
                            AppMode::Update => {
                                if key.code == KeyCode::Enter && app.state == AppState::PreviewingUpdates {
                                    // Enter：sudo 鉴权 + 开始更新
                                    if !app.update_preview.is_empty() {
                                        match validate_sudo_tui(&mut terminal) {
                                            Ok(true) => {
                                                update::spawn_update_task(&mut app, &tx);
                                            }
                                            Ok(false) => {
                                                app.error_message = Some("sudo 验证失败，请确保你有 sudo 权限".to_string());
                                                app.state = AppState::Error;
                                            }
                                            Err(e) => {
                                                app.error_message = Some(format!("sudo 验证出错: {}", e));
                                                app.state = AppState::Error;
                                            }
                                        }
                                    }
                                } else {
                                    update::handle_update_key(key, &mut app, term_size.height);
                                }
                            }
                            AppMode::Install => {
                                if key.code == KeyCode::Enter
                                    && app.install_state == state::InstallState::PreviewingInstall
                                    && app.install_preview.len() > 1
                                {
                                    // Enter in preview: sudo → install
                                    match validate_sudo_tui(&mut terminal) {
                                        Ok(true) => {
                                            install::spawn_install_task(&mut app, &tx);
                                        }
                                        Ok(false) => {
                                            app.error_message = Some("sudo 验证失败，请确保你有 sudo 权限".to_string());
                                            app.install_state = state::InstallState::Error;
                                        }
                                        Err(e) => {
                                            app.error_message = Some(format!("sudo 验证出错: {}", e));
                                            app.install_state = state::InstallState::Error;
                                        }
                                    }
                                } else {
                                    install::handle_install_key(key, &mut app, &tx, term_size.height);
                                }
                            }
                            AppMode::Remove => {
                                if key.code == KeyCode::Enter
                                    && app.remove_state == state::RemoveState::PreviewingRemove
                                    && app.remove_preview.len() > 1
                                {
                                    // Enter in preview: sudo → remove
                                    match validate_sudo_tui(&mut terminal) {
                                        Ok(true) => {
                                            remove::spawn_remove_task(&mut app, &tx);
                                        }
                                        Ok(false) => {
                                            app.error_message = Some("sudo 验证失败，请确保你有 sudo 权限".to_string());
                                            app.remove_state = state::RemoveState::Error;
                                        }
                                        Err(e) => {
                                            app.error_message = Some(format!("sudo 验证出错: {}", e));
                                            app.remove_state = state::RemoveState::Error;
                                        }
                                    }
                                } else {
                                    remove::handle_remove_key(key, &mut app, &tx, term_size.height);
                                }
                            }
                            AppMode::Query => {
                                query::handle_query_key(key, &mut app, &tx, term_size.height);
                            }
                            AppMode::Settings => {
                                settings::handle_settings_key(key, &mut app);
                            }
                            AppMode::Dashboard => {}
                        }
                    }
                }
            }
        }

        // 处理异步事件
        while let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::PackageManagerDetected(pm) => {
                    app.package_manager = Some(pm);
                    // 如果当前在更新模式且还在检测状态，自动检查更新
                    if app.mode == AppMode::Update
                        && app.state == AppState::PackageManagerCheck
                    {
                        if let Some(pm) = app.package_manager.clone() {
                            app.update_lines.push("正在检查可用更新...".to_string());
                            let tx_clone = tx.clone();
                            tokio::spawn(async move {
                                let updates = tokio::task::spawn_blocking(move || pm.check_updates())
                                    .await
                                    .unwrap_or_default();
                                let _ = tx_clone.send(AppEvent::UpdatePreviewReady(updates)).await;
                            });
                        }
                    }
                    // 检测到 PM 后，获取已安装包数量
                    if let Some(pm) = &app.package_manager {
                        let count = pm.count_installed();
                        let tx_clone = tx.clone();
                        tokio::spawn(async move {
                            let _ = tx_clone.send(AppEvent::InstalledCount(count)).await;
                        });
                    }
                }
                AppEvent::SystemInfoDetected(info) => {
                    app.system_info = Some(info);
                }
                AppEvent::InstalledCount(count) => {
                    app.installed_count = Some(count);
                }
                AppEvent::UpdateLine(line) => {
                    app.add_update_line(line);
                }
                AppEvent::UpdateComplete {
                    output,
                    packages_before,
                    packages_after,
                } => {
                    app.update_output = Some(output);
                    app.packages_before = packages_before;
                    app.packages_after = packages_after;
                    app.state = AppState::UpdateComplete;
                    app.add_update_line("--- 更新完成 ---".to_string());

                    // 启动 AI 分析
                    update::handle_update_complete(&mut app, &tx, &api_key);
                }
                AppEvent::AnalysisComplete(analysis) => {
                    update::handle_analysis_complete(&mut app, analysis, &tx);
                }
                AppEvent::ReportSaved(path) => {
                    // 根据当前模式分配报告路径
                    match app.mode {
                        AppMode::Install => { app.install_saved_report = Some(path); }
                        AppMode::Remove => { app.remove_saved_report = Some(path); }
                        _ => { app.saved_report_path = Some(path); }
                    }
                }
                AppEvent::Error(msg) => {
                    app.error_message = Some(msg.clone());
                    // 根据当前模式设置对应错误状态
                    match app.mode {
                        AppMode::Install => { app.install_state = state::InstallState::Error; }
                        AppMode::Remove => { app.remove_state = state::RemoveState::Error; }
                        _ => { app.state = AppState::Error; }
                    }
                }
                AppEvent::QueryLocalResults(results) => {
                    app.query_local_results = results;
                    app.query_local_selected = 0;
                    app.query_searching = false;
                }
                AppEvent::QueryRemoteResults(results) => {
                    app.query_remote_results = results;
                    app.query_remote_selected = 0;
                    app.query_searching = false;
                }
                AppEvent::QueryDetailLoaded { detail, files, dirs } => {
                    app.query_detail = Some(detail);
                    app.query_files = files;
                    app.query_dirs = dirs;
                    app.query_file_mode = state::FileListMode::Files;
                    app.query_detail_scroll = 0;
                    app.query_view = state::QueryView::Detail;
                }
                AppEvent::UpdatePreviewReady(updates) => {
                    app.update_preview = updates;
                    app.update_lines.clear();
                    if app.update_preview.is_empty() {
                        app.update_lines.push("系统已是最新，没有可用更新。".to_string());
                    } else {
                        app.update_lines.push(format!("找到 {} 个可用更新：", app.update_preview.len()));
                        app.update_lines.push(String::new());
                        for pkg in &app.update_preview {
                            app.update_lines.push(format!("  {}", pkg));
                        }
                    }
                    app.state = AppState::PreviewingUpdates;
                    app.reset_scroll();
                }
                // ===== Install 事件 =====
                AppEvent::InstallSearchResults(results) => {
                    app.install_results = results;
                    app.install_selected = 0;
                    app.install_marked.clear();
                    app.install_searching = false;
                }
                AppEvent::InstallPreviewReady(preview) => {
                    app.install_preview = preview;
                    app.install_scroll = 0;
                }
                AppEvent::InstallLine(line) => {
                    app.add_install_line(line);
                }
                AppEvent::InstallComplete { output } => {
                    app.install_output = Some(output);
                    app.install_state = state::InstallState::InstallComplete;
                    app.add_install_line("--- 安装完成 ---".to_string());
                    install::handle_install_complete(&mut app, &tx, &api_key);
                    // 刷新已安装包数量
                    if let Some(pm) = &app.package_manager {
                        let count = pm.count_installed();
                        app.installed_count = Some(count);
                    }
                }
                AppEvent::InstallAnalysisComplete(analysis) => {
                    install::handle_install_analysis_complete(&mut app, analysis, &tx);
                }
                // ===== Remove 事件 =====
                AppEvent::RemovePackagesLoaded(packages) => {
                    app.remove_packages = packages;
                    app.remove_loading = false;
                    app.apply_remove_filter();
                }
                AppEvent::RemovePreviewReady(preview) => {
                    app.remove_preview = preview;
                    app.remove_scroll = 0;
                }
                AppEvent::RemoveLine(line) => {
                    app.add_remove_line(line);
                }
                AppEvent::RemoveComplete { output } => {
                    app.remove_output = Some(output);
                    app.remove_state = state::RemoveState::RemoveComplete;
                    app.add_remove_line("--- 卸载完成 ---".to_string());
                    remove::handle_remove_complete(&mut app, &tx, &api_key);
                    // 刷新已安装包数量
                    if let Some(pm) = &app.package_manager {
                        let count = pm.count_installed();
                        app.installed_count = Some(count);
                    }
                }
                AppEvent::RemoveAnalysisComplete(analysis) => {
                    remove::handle_remove_analysis_complete(&mut app, analysis, &tx);
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // 恢复终端
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// 临时退出 TUI 执行 sudo 鉴权，成功后恢复 TUI
fn validate_sudo_tui(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<bool> {
    // 退出 TUI
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // 提示并执行 sudo -v
    println!("🔐 需要 sudo 权限来执行此操作");
    println!();

    let status = std::process::Command::new("sudo")
        .arg("-v")
        .status()?;

    let success = status.success();

    if success {
        println!();
        println!("✅ sudo 验证成功！");
    } else {
        println!();
        println!("❌ sudo 验证失败");
    }

    std::thread::sleep(std::time::Duration::from_millis(500));

    // 恢复 TUI
    enable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture
    )?;
    terminal.hide_cursor()?;
    terminal.clear()?;

    Ok(success)
}

fn ui(f: &mut Frame, app: &App) {
    match app.mode {
        AppMode::Dashboard => dashboard::render_dashboard(f, app),
        AppMode::Update => update::render_update(f, app),
        AppMode::Install => install::render_install(f, app),
        AppMode::Remove => remove::render_remove(f, app),
        AppMode::Query => query::render_query(f, app),
        AppMode::Settings => settings::render_settings(f, app),
    }
}
