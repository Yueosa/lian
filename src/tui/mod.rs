mod dashboard;
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

    let mut app = App::new();

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

        terminal.draw(|f| ui(f, &app))?;

        // 处理事件
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let term_size = terminal.size()?;

                // 全局按键
                match key.code {
                    KeyCode::Char('q') => {
                        if app.mode == AppMode::Update {
                            crate::package_manager::cancel_update();
                        }
                        app.should_quit = true;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if app.mode == AppMode::Update {
                            crate::package_manager::cancel_update();
                        }
                        app.should_quit = true;
                    }
                    KeyCode::Esc => {
                        if app.mode != AppMode::Dashboard {
                            if app.mode == AppMode::Update {
                                crate::package_manager::cancel_update();
                            }
                            app.mode = AppMode::Dashboard;
                            app.reset_scroll();
                        }
                    }
                    // 模式切换快捷键 (Shift + 字母)
                    KeyCode::Char('U') => {
                        if app.mode != AppMode::Update {
                            app.mode = AppMode::Update;
                            app.reset_update_state();
                            // 如果 PM 已检测到，直接进入 PreUpdate
                            if app.package_manager.is_some() {
                                app.state = AppState::PreUpdate;
                            }
                        }
                    }
                    KeyCode::Char('S') => {
                        app.mode = AppMode::Install;
                        app.reset_scroll();
                    }
                    KeyCode::Char('R') => {
                        app.mode = AppMode::Remove;
                        app.reset_scroll();
                    }
                    KeyCode::Char('Q') => {
                        app.mode = AppMode::Query;
                        app.reset_scroll();
                    }
                    KeyCode::Char('C') => {
                        app.mode = AppMode::Settings;
                        app.reset_scroll();
                    }
                    // 委托给当前模式处理
                    _ => {
                        match app.mode {
                            AppMode::Update => {
                                update::handle_update_key(key, &mut app, &tx, term_size.height);
                            }
                            AppMode::Install => {
                                install::handle_install_key(key, &mut app);
                            }
                            AppMode::Remove => {
                                remove::handle_remove_key(key, &mut app);
                            }
                            AppMode::Query => {
                                query::handle_query_key(key, &mut app);
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
                    // 如果当前在更新模式且还在检测状态，推进到 PreUpdate
                    if app.mode == AppMode::Update
                        && app.state == AppState::PackageManagerCheck
                    {
                        app.state = AppState::PreUpdate;
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
                    update::handle_update_complete(&mut app, &tx, &api_key, &config);
                }
                AppEvent::AnalysisComplete(analysis) => {
                    update::handle_analysis_complete(&mut app, analysis, &tx, &config);
                }
                AppEvent::ReportSaved(path) => {
                    app.saved_report_path = Some(path);
                }
                AppEvent::Error(msg) => {
                    app.error_message = Some(msg);
                    app.state = AppState::Error;
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
