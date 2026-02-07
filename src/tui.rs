use crate::config::Config;
use crate::deepseek::AiClient;
use crate::package_manager::{PackageManager, UpdateOutput};
use crate::prompt;
use crate::report::ReportSaver;
use crate::sysinfo::SystemInfo;
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame, Terminal,
};
use std::io;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
enum AppState {
    PackageManagerCheck,
    PreUpdate,
    Updating,
    UpdateComplete,
    Analyzing,
    AnalysisComplete,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
enum ViewMode {
    UpdateLog,
    AIAnalysis,
}

struct App {
    state: AppState,
    view_mode: ViewMode,
    package_manager: Option<PackageManager>,
    system_info: Option<SystemInfo>,
    update_output: Option<UpdateOutput>,
    update_lines: Vec<String>,
    update_progress: String,
    packages_before: Option<String>,
    packages_after: Option<String>,
    analysis_result: Option<String>,
    error_message: Option<String>,
    scroll_offset: usize,
    should_quit: bool,
    saved_report_path: Option<String>,
}

impl App {
    fn new() -> Self {
        Self {
            state: AppState::PackageManagerCheck,
            view_mode: ViewMode::UpdateLog,
            package_manager: None,
            system_info: None,
            update_output: None,
            update_lines: Vec::new(),
            update_progress: String::new(),
            packages_before: None,
            packages_after: None,
            analysis_result: None,
            error_message: None,
            scroll_offset: 0,
            should_quit: false,
            saved_report_path: None,
        }
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    fn scroll_down(&mut self, max_lines: usize, visible_height: usize) {
        let max_scroll = max_lines.saturating_sub(visible_height);
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    fn scroll_page_up(&mut self, page_size: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(page_size);
    }

    fn scroll_page_down(&mut self, page_size: usize, max_lines: usize, visible_height: usize) {
        let max_scroll = max_lines.saturating_sub(visible_height);
        self.scroll_offset = (self.scroll_offset + page_size).min(max_scroll);
    }

    fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    fn clamp_scroll(&mut self, max_lines: usize, visible_height: usize) {
        let max_scroll = max_lines.saturating_sub(visible_height);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
    }

    fn get_current_content(&self) -> Vec<String> {
        match self.view_mode {
            ViewMode::UpdateLog => {
                // 如果有完整的输出，使用它；否则使用实时输出行
                if let Some(output) = &self.update_output {
                    output.combined_output().lines().map(|s| s.to_string()).collect()
                } else if !self.update_lines.is_empty() {
                    self.update_lines.clone()
                } else {
                    vec!["等待更新...".to_string()]
                }
            }
            ViewMode::AIAnalysis => {
                if let Some(analysis) = &self.analysis_result {
                    analysis.lines().map(|s| s.to_string()).collect()
                } else {
                    vec!["AI 分析中...".to_string()]
                }
            }
        }
    }

    fn add_update_line(&mut self, line: String) {
        // 解析进度信息
        self.parse_progress(&line);
        self.update_lines.push(line);
        // 自动滚动到底部
        if self.update_lines.len() > 1 {
            self.scroll_offset = self.update_lines.len().saturating_sub(1);
        }
    }

    /// 从输出行中解析进度信息
    fn parse_progress(&mut self, line: &str) {
        let trimmed = line.trim();
        // 解析 "( 3/12) upgrading xxx" 或 "(3/12) checking xxx" 等格式
        if trimmed.starts_with('(') {
            if let Some(end) = trimmed.find(')') {
                let inner = &trimmed[1..end].trim();
                if inner.contains('/') {
                    // 提取操作名称
                    let rest = trimmed[end+1..].trim();
                    let action = rest.split_whitespace().next().unwrap_or("");
                    self.update_progress = format!("[{action}] {inner}");
                    return;
                }
            }
        }
        // 解析网速信息: "xxx MiB/s" 或 "xxx KiB/s"
        if let Some(speed_pos) = trimmed.find("iB/s") {
            // 向前找速度值
            let before = &trimmed[..speed_pos + 4];
            if let Some(last_space) = before.rfind([' ', '\t']) {
                let speed = before[last_space..].trim();
                if !speed.is_empty() {
                    self.update_progress = format!("下载中... {speed}");
                }
            }
        }
        // :: 这种行是状态描述
        if trimmed.starts_with("::") {
            self.update_progress = trimmed.trim_start_matches(':').trim().to_string();
        }
    }
}

pub async fn run(api_key: String, config: Config) -> Result<()> {
    // 设置终端
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 创建应用状态
    let mut app = App::new();

    // 创建通道用于异步任务通信
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
        // 在绘制前确保 scroll 在有效范围内
        let content = app.get_current_content();
        let term_size = terminal.size()?;
        // 估算内容区域高度：总高度 - header(3) - footer(3) - borders(2)
        let visible_height = term_size.height.saturating_sub(8) as usize;
        app.clamp_scroll(content.len(), visible_height);
        
        terminal.draw(|f| ui(f, &app))?;

        // 处理事件
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        // 取消正在进行的更新
                        crate::package_manager::cancel_update();
                        app.should_quit = true;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // 取消正在进行的更新
                        crate::package_manager::cancel_update();
                        app.should_quit = true;
                    }
                    KeyCode::Tab => {
                        if app.state == AppState::AnalysisComplete {
                            app.view_mode = match app.view_mode {
                                ViewMode::UpdateLog => ViewMode::AIAnalysis,
                                ViewMode::AIAnalysis => ViewMode::UpdateLog,
                            };
                            app.reset_scroll();
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.scroll_up();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let content = app.get_current_content();
                        let visible = term_size.height.saturating_sub(8) as usize;
                        app.scroll_down(content.len(), visible);
                    }
                    KeyCode::PageUp => {
                        app.scroll_page_up(10);
                    }
                    KeyCode::PageDown => {
                        let content = app.get_current_content();
                        let visible = term_size.height.saturating_sub(8) as usize;
                        app.scroll_page_down(10, content.len(), visible);
                    }
                    KeyCode::Enter => {
                        if app.state == AppState::PreUpdate {
                            // 开始更新
                            let pm = app.package_manager.clone().unwrap();
                            let tx_clone = tx.clone();
                            app.state = AppState::Updating;
                            app.update_lines.clear();
                            app.update_lines.push("正在执行更新...".to_string());

                            // 使用 std thread 运行阻塞的更新操作
                            std::thread::spawn(move || {
                                let packages_before = pm.get_explicit_packages().ok();

                                let (output_tx, mut output_rx) = tokio::sync::mpsc::unbounded_channel();

                                // 转发输出行到主事件通道
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
                                        let _ = tx_clone.blocking_send(
                                            AppEvent::Error(format!("更新失败: {}", e))
                                        );
                                    }
                                }
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        // 处理异步事件
        while let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::PackageManagerDetected(pm) => {
                    app.package_manager = Some(pm);
                    app.state = AppState::PreUpdate;
                }
                AppEvent::SystemInfoDetected(info) => {
                    app.system_info = Some(info);
                }
                AppEvent::UpdateLine(line) => {
                    // 实时添加输出行
                    app.add_update_line(line);
                }
                AppEvent::UpdateComplete {
                    output,
                    packages_before,
                    packages_after,
                } => {
                    app.update_output = Some(output.clone());
                    app.packages_before = packages_before.clone();
                    app.packages_after = packages_after.clone();
                    app.state = AppState::UpdateComplete;
                    app.add_update_line("--- 更新完成 ---".to_string());

                    // 如果更新成功,启动 AI 分析
                    if output.success {
                        app.state = AppState::Analyzing;

                        let pm_name = app.package_manager.as_ref().unwrap().name().to_string();
                        let update_log = output.combined_output();
                        let pkg_before = packages_before.as_deref();
                        let pkg_after = packages_after.as_deref();
                        let sys_info = app.system_info.clone();

                        let prompt_text =
                            prompt::generate_analysis_prompt(&pm_name, &update_log, pkg_before, pkg_after, sys_info.as_ref());

                        let client = AiClient::new(api_key.clone(), config.get_api_url().to_string(), config.proxy.as_deref());
                        let model = config.model.clone();
                        let temperature = config.temperature;
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
                    }
                }
                AppEvent::AnalysisComplete(analysis) => {
                    app.analysis_result = Some(analysis.clone());
                    app.state = AppState::AnalysisComplete;
                    app.view_mode = ViewMode::AIAnalysis;
                    app.reset_scroll();  // 重置滚动位置

                    // 保存报告
                    let report_dir = config.report_dir.clone();
                    let tx_clone = tx.clone();
                    tokio::spawn(async move {
                        let saver = ReportSaver::new(report_dir);
                        match saver.save(&analysis) {
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

#[derive(Debug)]
enum AppEvent {
    PackageManagerDetected(PackageManager),
    SystemInfoDetected(SystemInfo),
    UpdateLine(String),
    UpdateComplete {
        output: UpdateOutput,
        packages_before: Option<String>,
        packages_after: Option<String>,
    },
    AnalysisComplete(String),
    ReportSaved(String),
    Error(String),
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // 顶部标题栏
    render_header(f, app, chunks[0]);

    // 主内容区
    render_content(f, app, chunks[1]);

    // 底部状态栏
    render_footer(f, app, chunks[2]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let title = match app.state {
        AppState::PackageManagerCheck => "🔍 检测包管理器...",
        AppState::PreUpdate => "📦 准备更新系统",
        AppState::Updating => "⚙️  正在更新系统...",
        AppState::UpdateComplete => "✅ 更新完成",
        AppState::Analyzing => "🤖 AI 分析中...",
        AppState::AnalysisComplete => "✨ 分析完成",
        AppState::Error => "❌ 错误",
    };

    let pm_info = if let Some(pm) = &app.package_manager {
        format!(" | 包管理器: {}", pm.name())
    } else {
        String::new()
    };

    let header = Paragraph::new(format!("{}{}", title, pm_info))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    f.render_widget(header, area);
}

fn render_content(f: &mut Frame, app: &App, area: Rect) {
    let title = match app.view_mode {
        ViewMode::UpdateLog => "更新日志 [Tab 切换到 AI 分析]",
        ViewMode::AIAnalysis => "AI 分析报告 [Tab 切换到更新日志]",
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    // 获取内容
    let content = app.get_current_content();
    let total_lines = content.len();

    let visible_height = area.height.saturating_sub(2) as usize;
    let max_scroll = total_lines.saturating_sub(visible_height);
    let actual_scroll = app.scroll_offset.min(max_scroll);

    let visible_content: Vec<Line> = content
        .iter()
        .skip(actual_scroll)
        .take(visible_height)
        .map(|line| Line::from(line.clone()))
        .collect();

    let paragraph = Paragraph::new(visible_content)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);

    // 渲染滚动条
    if total_lines > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state = ScrollbarState::new(total_lines)
            .position(actual_scroll);

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

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let owned_text: String;
    let footer_text = match app.state {
        AppState::PackageManagerCheck => "请稍候...",
        AppState::PreUpdate => "按 Enter 开始更新 | q 退出",
        AppState::Updating => {
            if app.update_progress.is_empty() {
                "更新进行中..."
            } else {
                owned_text = format!("更新进行中 | {}", app.update_progress);
                &owned_text
            }
        }
        AppState::UpdateComplete => "更新完成,等待 AI 分析...",
        AppState::Analyzing => "AI 正在分析更新内容...",
        AppState::AnalysisComplete => {
            if let Some(path) = &app.saved_report_path {
                owned_text = format!("报告已保存: {} | Tab 切换视图 | ↑↓ 滚动 | q 退出", path);
                &owned_text
            } else {
                "Tab 切换视图 | ↑↓ 滚动 | q 退出"
            }
        }
        AppState::Error => {
            if let Some(msg) = &app.error_message {
                msg
            } else {
                "发生错误 | q 退出"
            }
        }
    };

    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Left);

    f.render_widget(footer, area);
}
