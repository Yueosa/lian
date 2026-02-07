use crate::package_manager::{PackageDetail, PackageInfo, PackageManager, UpdateOutput};
use crate::sysinfo::SystemInfo;

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Dashboard,
    Update,    // Shift+U: -Syu
    Install,   // Shift+S: -S
    Remove,    // Shift+R: -Rns
    Query,     // Shift+Q: -Qs/-Ss/-Qi/-Ql
    Settings,  // Shift+C: 设置
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    PackageManagerCheck,
    PreUpdate,
    PreviewingUpdates,
    Updating,
    UpdateComplete,
    Analyzing,
    AnalysisComplete,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    UpdateLog,
    AIAnalysis,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QueryPanel {
    Local,
    Remote,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QueryView {
    List,
    Detail,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileListMode {
    Files,
    Directories,
}

#[derive(Debug)]
pub enum AppEvent {
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
    InstalledCount(usize),
    QueryLocalResults(Vec<PackageInfo>),
    QueryRemoteResults(Vec<PackageInfo>),
    QueryDetailLoaded {
        detail: PackageDetail,
        files: Vec<String>,
        dirs: Vec<String>,
    },
    UpdatePreviewReady(Vec<String>),
}

pub struct App {
    pub mode: AppMode,
    pub state: AppState,
    pub view_mode: ViewMode,
    pub package_manager: Option<PackageManager>,
    pub system_info: Option<SystemInfo>,
    pub update_output: Option<UpdateOutput>,
    pub update_lines: Vec<String>,
    pub update_progress: String,
    pub packages_before: Option<String>,
    pub packages_after: Option<String>,
    pub analysis_result: Option<String>,
    pub error_message: Option<String>,
    pub scroll_offset: usize,
    pub should_quit: bool,
    pub saved_report_path: Option<String>,
    pub installed_count: Option<usize>,
    // 查询相关状态
    pub query_input: String,
    pub query_cursor: usize,
    pub query_panel: QueryPanel,
    pub query_view: QueryView,
    pub query_local_results: Vec<PackageInfo>,
    pub query_remote_results: Vec<PackageInfo>,
    pub query_local_selected: usize,
    pub query_remote_selected: usize,
    pub query_detail: Option<PackageDetail>,
    pub query_files: Vec<String>,
    pub query_dirs: Vec<String>,
    pub query_file_mode: FileListMode,
    pub query_detail_scroll: usize,
    pub query_searching: bool,
    // 更新预览
    pub update_preview: Vec<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: AppMode::Dashboard,
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
            installed_count: None,
            // 查询
            query_input: String::new(),
            query_cursor: 0,
            query_panel: QueryPanel::Local,
            query_view: QueryView::List,
            query_local_results: Vec::new(),
            query_remote_results: Vec::new(),
            query_local_selected: 0,
            query_remote_selected: 0,
            query_detail: None,
            query_files: Vec::new(),
            query_dirs: Vec::new(),
            query_file_mode: FileListMode::Files,
            query_detail_scroll: 0,
            query_searching: false,
            update_preview: Vec::new(),
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, max_lines: usize, visible_height: usize) {
        let max_scroll = max_lines.saturating_sub(visible_height);
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    pub fn scroll_page_up(&mut self, page_size: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(page_size);
    }

    pub fn scroll_page_down(&mut self, page_size: usize, max_lines: usize, visible_height: usize) {
        let max_scroll = max_lines.saturating_sub(visible_height);
        self.scroll_offset = (self.scroll_offset + page_size).min(max_scroll);
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn clamp_scroll(&mut self, max_lines: usize, visible_height: usize) {
        let max_scroll = max_lines.saturating_sub(visible_height);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
    }

    pub fn get_current_content(&self) -> Vec<String> {
        match self.view_mode {
            ViewMode::UpdateLog => {
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

    pub fn add_update_line(&mut self, line: String) {
        self.parse_progress(&line);
        self.update_lines.push(line);
        if self.update_lines.len() > 1 {
            self.scroll_offset = self.update_lines.len().saturating_sub(1);
        }
    }

    /// 从输出行中解析进度信息
    pub fn parse_progress(&mut self, line: &str) {
        let trimmed = line.trim();
        // 解析 "( 3/12) upgrading xxx" 或 "(3/12) checking xxx" 等格式
        if trimmed.starts_with('(') {
            if let Some(end) = trimmed.find(')') {
                let inner = &trimmed[1..end].trim();
                if inner.contains('/') {
                    let rest = trimmed[end + 1..].trim();
                    let action = rest.split_whitespace().next().unwrap_or("");
                    self.update_progress = format!("[{action}] {inner}");
                    return;
                }
            }
        }
        // 解析网速信息: "xxx MiB/s" 或 "xxx KiB/s"
        if let Some(speed_pos) = trimmed.find("iB/s") {
            let before = &trimmed[..speed_pos + 4];
            if let Some(last_space) = before.rfind([' ', '\t']) {
                let speed = before[last_space..].trim();
                if !speed.is_empty() {
                    self.update_progress = format!("下载中... {speed}");
                }
            }
        }
    }

    /// 重置更新相关状态，用于从其他模式进入更新模式时
    pub fn reset_update_state(&mut self) {
        self.state = AppState::PackageManagerCheck;
        self.view_mode = ViewMode::UpdateLog;
        self.update_output = None;
        self.update_lines.clear();
        self.update_progress.clear();
        self.packages_before = None;
        self.packages_after = None;
        self.analysis_result = None;
        self.error_message = None;
        self.scroll_offset = 0;
        self.saved_report_path = None;
        self.update_preview.clear();
    }

    /// 重置查询相关状态
    pub fn reset_query_state(&mut self) {
        self.query_input.clear();
        self.query_cursor = 0;
        self.query_panel = QueryPanel::Local;
        self.query_view = QueryView::List;
        self.query_local_results.clear();
        self.query_remote_results.clear();
        self.query_local_selected = 0;
        self.query_remote_selected = 0;
        self.query_detail = None;
        self.query_files.clear();
        self.query_dirs.clear();
        self.query_file_mode = FileListMode::Files;
        self.query_detail_scroll = 0;
        self.query_searching = false;
    }
}
