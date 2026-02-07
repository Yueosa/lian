use crate::package_manager::{InstalledPackage, PackageDetail, PackageInfo, PackageManager, UpdateOutput};
use crate::sysinfo::SystemInfo;
use std::collections::HashSet;

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

#[derive(Debug, Clone, PartialEq)]
pub enum InstallState {
    Searching,
    PreviewingInstall,
    Installing,
    InstallComplete,
    Analyzing,
    AnalysisComplete,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemoveState {
    Browsing,
    PreviewingRemove,
    Removing,
    RemoveComplete,
    Analyzing,
    AnalysisComplete,
    Error,
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
    // Install 事件
    InstallSearchResults(Vec<PackageInfo>),
    InstallPreviewReady(Vec<String>),
    InstallLine(String),
    InstallComplete { output: UpdateOutput },
    InstallAnalysisComplete(String),
    // Remove 事件
    RemovePackagesLoaded(Vec<InstalledPackage>),
    RemovePreviewReady(Vec<String>),
    RemoveLine(String),
    RemoveComplete { output: UpdateOutput },
    RemoveAnalysisComplete(String),
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
    // 安装相关状态
    pub install_state: InstallState,
    pub install_input: String,
    pub install_cursor: usize,
    pub install_results: Vec<PackageInfo>,
    pub install_selected: usize,
    pub install_marked: HashSet<usize>,
    pub install_preview: Vec<String>,
    pub install_lines: Vec<String>,
    pub install_output: Option<UpdateOutput>,
    pub install_progress: String,
    pub install_analysis: Option<String>,
    pub install_scroll: usize,
    pub install_searching: bool,
    pub install_view_mode: ViewMode,
    pub install_saved_report: Option<String>,
    // 卸载相关状态
    pub remove_state: RemoveState,
    pub remove_input: String,
    pub remove_cursor: usize,
    pub remove_packages: Vec<InstalledPackage>,
    pub remove_filtered: Vec<usize>,
    pub remove_selected: usize,
    pub remove_marked: HashSet<usize>,
    pub remove_preview: Vec<String>,
    pub remove_lines: Vec<String>,
    pub remove_output: Option<UpdateOutput>,
    pub remove_progress: String,
    pub remove_analysis: Option<String>,
    pub remove_scroll: usize,
    pub remove_loading: bool,
    pub remove_view_mode: ViewMode,
    pub remove_saved_report: Option<String>,
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
            // 安装
            install_state: InstallState::Searching,
            install_input: String::new(),
            install_cursor: 0,
            install_results: Vec::new(),
            install_selected: 0,
            install_marked: HashSet::new(),
            install_preview: Vec::new(),
            install_lines: Vec::new(),
            install_output: None,
            install_progress: String::new(),
            install_analysis: None,
            install_scroll: 0,
            install_searching: false,
            install_view_mode: ViewMode::UpdateLog,
            install_saved_report: None,
            // 卸载
            remove_state: RemoveState::Browsing,
            remove_input: String::new(),
            remove_cursor: 0,
            remove_packages: Vec::new(),
            remove_filtered: Vec::new(),
            remove_selected: 0,
            remove_marked: HashSet::new(),
            remove_preview: Vec::new(),
            remove_lines: Vec::new(),
            remove_output: None,
            remove_progress: String::new(),
            remove_analysis: None,
            remove_scroll: 0,
            remove_loading: false,
            remove_view_mode: ViewMode::UpdateLog,
            remove_saved_report: None,
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

    /// 重置安装相关状态
    pub fn reset_install_state(&mut self) {
        self.install_state = InstallState::Searching;
        self.install_input.clear();
        self.install_cursor = 0;
        self.install_results.clear();
        self.install_selected = 0;
        self.install_marked.clear();
        self.install_preview.clear();
        self.install_lines.clear();
        self.install_output = None;
        self.install_progress.clear();
        self.install_analysis = None;
        self.install_scroll = 0;
        self.install_searching = false;
        self.install_view_mode = ViewMode::UpdateLog;
        self.install_saved_report = None;
        self.error_message = None;
    }

    /// 重置卸载相关状态
    pub fn reset_remove_state(&mut self) {
        self.remove_state = RemoveState::Browsing;
        self.remove_input.clear();
        self.remove_cursor = 0;
        self.remove_packages.clear();
        self.remove_filtered.clear();
        self.remove_selected = 0;
        self.remove_marked.clear();
        self.remove_preview.clear();
        self.remove_lines.clear();
        self.remove_output = None;
        self.remove_progress.clear();
        self.remove_analysis = None;
        self.remove_scroll = 0;
        self.remove_loading = false;
        self.remove_view_mode = ViewMode::UpdateLog;
        self.remove_saved_report = None;
        self.error_message = None;
    }

    /// 添加安装输出行
    pub fn add_install_line(&mut self, line: String) {
        self.parse_install_progress(&line);
        self.install_lines.push(line);
        if self.install_lines.len() > 1 {
            self.install_scroll = self.install_lines.len().saturating_sub(1);
        }
    }

    /// 添加卸载输出行
    pub fn add_remove_line(&mut self, line: String) {
        self.parse_remove_progress(&line);
        self.remove_lines.push(line);
        if self.remove_lines.len() > 1 {
            self.remove_scroll = self.remove_lines.len().saturating_sub(1);
        }
    }

    /// 解析安装进度信息
    fn parse_install_progress(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.starts_with('(') {
            if let Some(end) = trimmed.find(')') {
                let inner = &trimmed[1..end].trim();
                if inner.contains('/') {
                    let rest = trimmed[end + 1..].trim();
                    let action = rest.split_whitespace().next().unwrap_or("");
                    self.install_progress = format!("[{action}] {inner}");
                    return;
                }
            }
        }
        if let Some(speed_pos) = trimmed.find("iB/s") {
            let before = &trimmed[..speed_pos + 4];
            if let Some(last_space) = before.rfind([' ', '\t']) {
                let speed = before[last_space..].trim();
                if !speed.is_empty() {
                    self.install_progress = format!("下载中... {speed}");
                }
            }
        }
    }

    /// 解析卸载进度信息
    fn parse_remove_progress(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.starts_with('(') {
            if let Some(end) = trimmed.find(')') {
                let inner = &trimmed[1..end].trim();
                if inner.contains('/') {
                    let rest = trimmed[end + 1..].trim();
                    let action = rest.split_whitespace().next().unwrap_or("");
                    self.remove_progress = format!("[{action}] {inner}");
                }
            }
        }
    }

    /// 获取安装视图当前内容
    pub fn get_install_content(&self) -> Vec<String> {
        match self.install_view_mode {
            ViewMode::UpdateLog => {
                if let Some(output) = &self.install_output {
                    output.combined_output().lines().map(|s| s.to_string()).collect()
                } else if !self.install_lines.is_empty() {
                    self.install_lines.clone()
                } else {
                    vec!["等待安装...".to_string()]
                }
            }
            ViewMode::AIAnalysis => {
                if let Some(analysis) = &self.install_analysis {
                    analysis.lines().map(|s| s.to_string()).collect()
                } else {
                    vec!["AI 分析中...".to_string()]
                }
            }
        }
    }

    /// 获取卸载视图当前内容
    pub fn get_remove_content(&self) -> Vec<String> {
        match self.remove_view_mode {
            ViewMode::UpdateLog => {
                if let Some(output) = &self.remove_output {
                    output.combined_output().lines().map(|s| s.to_string()).collect()
                } else if !self.remove_lines.is_empty() {
                    self.remove_lines.clone()
                } else {
                    vec!["等待卸载...".to_string()]
                }
            }
            ViewMode::AIAnalysis => {
                if let Some(analysis) = &self.remove_analysis {
                    analysis.lines().map(|s| s.to_string()).collect()
                } else {
                    vec!["AI 分析中...".to_string()]
                }
            }
        }
    }

    /// 对卸载的包列表应用筛选
    pub fn apply_remove_filter(&mut self) {
        let keyword = self.remove_input.to_lowercase();
        if keyword.is_empty() {
            self.remove_filtered = (0..self.remove_packages.len()).collect();
        } else {
            self.remove_filtered = self.remove_packages
                .iter()
                .enumerate()
                .filter(|(_, pkg)| {
                    pkg.name.to_lowercase().contains(&keyword)
                        || pkg.description.to_lowercase().contains(&keyword)
                })
                .map(|(i, _)| i)
                .collect();
        }
        self.remove_selected = 0;
        // 清除不在筛选结果中的标记
        self.remove_marked.retain(|idx| self.remove_filtered.contains(idx));
    }
}
