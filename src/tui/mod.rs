mod dashboard;
pub mod input;
mod install;
mod layout;
mod query;
mod remove;
mod settings;
pub mod state;
mod shell;
mod theme;
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
use state::{App, AppEvent, AppMode, UpdatePhase};
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
            let content = app.update.get_content();
            let term_size = terminal.size()?;
            let visible_height = layout::visible_content_height(term_size.height);
            app.update.clamp_scroll(content.len(), visible_height);
        }
        // 查询详情视图 clamp scroll
        if app.mode == AppMode::Query && app.query.view == state::QueryView::Detail {
            let term_size = terminal.size()?;
            let total = query::detail_total_lines(&app);
            let visible = term_size.height.saturating_sub(8) as usize;
            let max_scroll = total.saturating_sub(visible);
            app.query.detail_scroll = app.query.detail_scroll.min(max_scroll);
        }
        // 安装模式 clamp scroll（输出阶段）
        if app.mode == AppMode::Install {
            match app.install.phase {
                state::InstallPhase::Installing
                | state::InstallPhase::InstallComplete
                | state::InstallPhase::Analyzing
                | state::InstallPhase::AnalysisComplete
                | state::InstallPhase::Error => {
                    let content = app.install.get_content();
                    let term_size = terminal.size()?;
                    let visible = layout::visible_content_height(term_size.height);
                    let max_scroll = content.len().saturating_sub(visible);
                    app.install.scroll = app.install.scroll.min(max_scroll);
                }
                state::InstallPhase::PreviewingInstall => {
                    let term_size = terminal.size()?;
                    let visible = layout::visible_content_height(term_size.height);
                    let max_scroll = app.install.preview.len().saturating_sub(visible);
                    app.install.scroll = app.install.scroll.min(max_scroll);
                }
                _ => {}
            }
        }
        // 卸载模式 clamp scroll（输出阶段）
        if app.mode == AppMode::Remove {
            match app.remove.phase {
                state::RemovePhase::Removing
                | state::RemovePhase::RemoveComplete
                | state::RemovePhase::Analyzing
                | state::RemovePhase::AnalysisComplete
                | state::RemovePhase::Error => {
                    let content = app.remove.get_content();
                    let term_size = terminal.size()?;
                    let visible = layout::visible_content_height(term_size.height);
                    let max_scroll = content.len().saturating_sub(visible);
                    app.remove.scroll = app.remove.scroll.min(max_scroll);
                }
                state::RemovePhase::PreviewingRemove => {
                    let term_size = terminal.size()?;
                    let visible = layout::visible_content_height(term_size.height);
                    let max_scroll = app.remove.preview.len().saturating_sub(visible);
                    app.remove.scroll = app.remove.scroll.min(max_scroll);
                }
                _ => {}
            }
        }
        // Shell 模式 clamp scroll（Running/Done/Error 阶段）
        if app.mode == AppMode::Shell {
            match app.shell.phase {
                state::ShellPhase::Running
                | state::ShellPhase::Done
                | state::ShellPhase::Error => {
                    let content = app.shell.get_content();
                    let term_size = terminal.size()?;
                    let visible = layout::visible_content_height(term_size.height);
                    let max_scroll = content.len().saturating_sub(visible);
                    app.shell.scroll = app.shell.scroll.min(max_scroll);
                }
                _ => {}
            }
        }

        // 防抖: 延迟执行搜索，避免每次按键都触发
        {
            const DEBOUNCE_MS: u128 = 250;
            if let Some(scheduled) = app.query.search_scheduled {
                if scheduled.elapsed().as_millis() >= DEBOUNCE_MS {
                    app.query.search_scheduled = None;
                    query::execute_pending_search(&mut app, &tx);
                }
            }
            if let Some(scheduled) = app.install.search_scheduled {
                if scheduled.elapsed().as_millis() >= DEBOUNCE_MS {
                    app.install.search_scheduled = None;
                    install::execute_pending_search(&mut app, &tx);
                }
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
                            || app.mode == AppMode::Shell
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
                                app.update.reset_scroll();
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
                            AppMode::Shell => {
                                shell::handle_shell_key(
                                    crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                                    &mut app,
                                    &tx,
                                    term_size.height,
                                );
                            }
                        }
                    }
                    // 模式切换快捷键 (Shift + 字母)
                    // 当处于文本输入状态时（Shell Input、Install 搜索、Remove 浏览、Query）不触发
                    KeyCode::Char('U' | 'S' | 'R' | 'Q' | 'C' | 'X')
                        if matches!(
                            app.mode,
                            AppMode::Shell
                                | AppMode::Install
                                | AppMode::Remove
                                | AppMode::Query
                        ) =>
                    {
                        // 转发给当前模式处理（作为普通字符输入）
                        match app.mode {
                            AppMode::Shell => {
                                shell::handle_shell_key(key, &mut app, &tx, term_size.height);
                            }
                            AppMode::Install => {
                                install::handle_install_key(key, &mut app, &tx, term_size.height);
                            }
                            AppMode::Remove => {
                                remove::handle_remove_key(key, &mut app, &tx, term_size.height);
                            }
                            AppMode::Query => {
                                query::handle_query_key(key, &mut app, &tx, term_size.height);
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Char('U') => {
                        if app.mode != AppMode::Update {
                            app.mode = AppMode::Update;
                            app.reset_update_state();
                            // 如果 PM 已检测到，直接检查可用更新
                            if let Some(pm) = app.package_manager.clone() {
                                app.update.lines.push("正在检查可用更新...".to_string());
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
                                app.remove.loading = true;
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
                    KeyCode::Char('X') => {
                        if app.mode != AppMode::Shell {
                            app.mode = AppMode::Shell;
                            app.reset_shell_state();
                        }
                    }
                    // 委托给当前模式处理
                    _ => {
                        match app.mode {
                            AppMode::Update => {
                                if key.code == KeyCode::Enter && app.update.phase == UpdatePhase::PreviewingUpdates {
                                    // Enter：sudo 鉴权 + 开始更新
                                    if !app.update.preview.is_empty() {
                                        match validate_sudo_tui(&mut terminal) {
                                            Ok(true) => {
                                                update::spawn_update_task(&mut app, &tx);
                                            }
                                            Ok(false) => {
                                                app.error_message = Some("sudo 验证失败，请确保你有 sudo 权限".to_string());
                                                app.update.phase = UpdatePhase::Error;
                                            }
                                            Err(e) => {
                                                app.error_message = Some(format!("sudo 验证出错: {}", e));
                                                app.update.phase = UpdatePhase::Error;
                                            }
                                        }
                                    }
                                } else {
                                    update::handle_update_key(key, &mut app, term_size.height);
                                }
                            }
                            AppMode::Install => {
                                if key.code == KeyCode::Enter
                                    && app.install.phase == state::InstallPhase::PreviewingInstall
                                    && app.install.preview.len() > 1
                                {
                                    // Enter in preview: sudo → install
                                    match validate_sudo_tui(&mut terminal) {
                                        Ok(true) => {
                                            install::spawn_install_task(&mut app, &tx);
                                        }
                                        Ok(false) => {
                                            app.error_message = Some("sudo 验证失败，请确保你有 sudo 权限".to_string());
                                            app.install.phase = state::InstallPhase::Error;
                                        }
                                        Err(e) => {
                                            app.error_message = Some(format!("sudo 验证出错: {}", e));
                                            app.install.phase = state::InstallPhase::Error;
                                        }
                                    }
                                } else {
                                    install::handle_install_key(key, &mut app, &tx, term_size.height);
                                }
                            }
                            AppMode::Remove => {
                                if key.code == KeyCode::Enter
                                    && app.remove.phase == state::RemovePhase::PreviewingRemove
                                    && app.remove.preview.len() > 1
                                {
                                    // Enter in preview: sudo → remove
                                    match validate_sudo_tui(&mut terminal) {
                                        Ok(true) => {
                                            remove::spawn_remove_task(&mut app, &tx);
                                        }
                                        Ok(false) => {
                                            app.error_message = Some("sudo 验证失败，请确保你有 sudo 权限".to_string());
                                            app.remove.phase = state::RemovePhase::Error;
                                        }
                                        Err(e) => {
                                            app.error_message = Some(format!("sudo 验证出错: {}", e));
                                            app.remove.phase = state::RemovePhase::Error;
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
                            AppMode::Shell => {
                                shell::handle_shell_key(key, &mut app, &tx, term_size.height);
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
                        && app.update.phase == UpdatePhase::PackageManagerCheck
                    {
                        if let Some(pm) = app.package_manager.clone() {
                            app.update.lines.push("正在检查可用更新...".to_string());
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
                    app.update.add_line(line);
                }
                AppEvent::UpdateComplete {
                    output,
                    packages_before,
                    packages_after,
                } => {
                    app.update.output = Some(output);
                    app.update.packages_before = packages_before;
                    app.update.packages_after = packages_after;
                    app.update.phase = UpdatePhase::UpdateComplete;
                    app.update.add_line("--- 更新完成 ---".to_string());

                    // 启动 AI 分析
                    update::handle_update_complete(&mut app, &tx, &api_key);
                }
                AppEvent::AnalysisComplete(analysis) => {
                    update::handle_analysis_complete(&mut app, analysis, &tx);
                }
                AppEvent::ReportSaved(path) => {
                    // 根据当前模式分配报告路径
                    match app.mode {
                        AppMode::Install => { app.install.report_path = Some(path); }
                        AppMode::Remove => { app.remove.report_path = Some(path); }
                        _ => { app.update.report_path = Some(path); }
                    }
                }
                AppEvent::Error(msg) => {
                    app.error_message = Some(msg.clone());
                    // 根据当前模式设置对应错误状态
                    match app.mode {
                        AppMode::Install => { app.install.phase = state::InstallPhase::Error; }
                        AppMode::Remove => { app.remove.phase = state::RemovePhase::Error; }
                        _ => { app.update.phase = UpdatePhase::Error; }
                    }
                }
                AppEvent::QueryLocalResults { results, seq } => {
                    if seq == app.query.search_seq {
                        app.query.local_results = results;
                        app.query.local_selected = 0;
                        if app.query.search_scheduled.is_none() {
                            app.query.searching = false;
                        }
                    }
                }
                AppEvent::QueryRemoteResults { results, seq } => {
                    if seq == app.query.search_seq {
                        app.query.remote_results = results;
                        app.query.remote_selected = 0;
                        if app.query.search_scheduled.is_none() {
                            app.query.searching = false;
                        }
                    }
                }
                AppEvent::QueryDetailLoaded { detail, files, dirs } => {
                    app.query.detail = Some(detail);
                    app.query.files = files;
                    app.query.dirs = dirs;
                    app.query.file_mode = state::FileListMode::Files;
                    app.query.detail_scroll = 0;
                    app.query.view = state::QueryView::Detail;
                }
                AppEvent::UpdatePreviewReady(updates) => {
                    app.update.preview = updates;
                    app.update.lines.clear();
                    if app.update.preview.is_empty() {
                        app.update.lines.push("系统已是最新，没有可用更新。".to_string());
                    } else {
                        app.update.lines.push(format!("找到 {} 个可用更新：", app.update.preview.len()));
                        app.update.lines.push(String::new());
                        for pkg in &app.update.preview {
                            app.update.lines.push(format!("  {}", pkg));
                        }
                    }
                    app.update.phase = UpdatePhase::PreviewingUpdates;
                    app.update.reset_scroll();
                }
                // ===== Install 事件 =====
                AppEvent::InstallSearchResults { results, seq } => {
                    if seq == app.install.search_seq {
                        app.install.results = results;
                        app.install.selected = 0;
                        app.install.marked.clear();
                        if app.install.search_scheduled.is_none() {
                            app.install.searching = false;
                        }
                    }
                }
                AppEvent::InstallPreviewReady(preview) => {
                    app.install.preview = preview;
                    app.install.scroll = 0;
                }
                AppEvent::InstallLine(line) => {
                    app.install.add_line(line);
                }
                AppEvent::InstallComplete { output } => {
                    app.install.output = Some(output);
                    app.install.phase = state::InstallPhase::InstallComplete;
                    app.install.add_line("--- 安装完成 ---".to_string());
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
                    app.remove.packages = packages;
                    app.remove.loading = false;
                    app.remove.apply_filter();
                }
                AppEvent::RemovePreviewReady(preview) => {
                    app.remove.preview = preview;
                    app.remove.scroll = 0;
                }
                AppEvent::RemoveLine(line) => {
                    app.remove.add_line(line);
                }
                AppEvent::RemoveComplete { output } => {
                    app.remove.output = Some(output);
                    app.remove.phase = state::RemovePhase::RemoveComplete;
                    app.remove.add_line("--- 卸载完成 ---".to_string());
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
                AppEvent::ShellLine(line) => {
                    app.shell.add_line(line);
                }
                AppEvent::ShellComplete { output } => {
                    let success = output.success;
                    // 把 stderr 中有内容的行追加到 lines（stdout 已经通过 ShellLine 流式写入）
                    let stderr = output.stderr.clone();
                    app.shell.output = Some(output);
                    if !stderr.trim().is_empty() {
                        for line in stderr.lines() {
                            if !line.trim().is_empty() {
                                app.shell.lines.push(format!("⚠ {}", line));
                            }
                        }
                    }
                    // 追加完成标志行
                    app.shell.lines.push(if success {
                        "─── 命令完成 ───".to_string()
                    } else {
                        "─── 命令失败 ───".to_string()
                    });
                    app.shell.phase = state::ShellPhase::Done;
                    // scroll 已由 add_line 自动推进，这里确保它指向最后一行
                    app.shell.scroll = app.shell.lines.len().saturating_sub(1);
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // 清理残留的 pacman/paru 子进程，确保释放 db.lck
    crate::package_manager::cleanup_child_processes();

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
        AppMode::Shell => shell::render_shell(f, app),
    }
}
