use crate::config::Config;
use crate::package_manager::{InstalledPackage, PackageDetail, PackageInfo, PackageManager, UpdateOutput};
use crate::sysinfo::SystemInfo;
use std::collections::HashSet;

// ========== 枚举 ==========

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Dashboard,
    Update,   // Shift+U: -Syu
    Install,  // Shift+S: -S
    Remove,   // Shift+R: -Rns
    Query,    // Shift+Q: -Qs/-Ss/-Qi/-Ql
    Settings, // Shift+C: 设置
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdatePhase {
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
pub enum InstallPhase {
    Searching,
    PreviewingInstall,
    Installing,
    InstallComplete,
    Analyzing,
    AnalysisComplete,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemovePhase {
    Browsing,
    PreviewingRemove,
    Removing,
    RemoveComplete,
    Analyzing,
    AnalysisComplete,
    Error,
}

/// 设置页面项目类型
#[derive(Debug, Clone)]
pub enum SettingsItem {
    /// 分组标题（不可选中）
    Section(String),
    /// 复选框开关项
    Toggle {
        label: String,
        key: String,
        value: bool,
    },
    /// 文本编辑项
    TextEdit {
        label: String,
        key: String,
        value: String,
        masked: bool,
    },
}

// ========== 事件 ==========

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
    // Install
    InstallSearchResults(Vec<PackageInfo>),
    InstallPreviewReady(Vec<String>),
    InstallLine(String),
    InstallComplete { output: UpdateOutput },
    InstallAnalysisComplete(String),
    // Remove
    RemovePackagesLoaded(Vec<InstalledPackage>),
    RemovePreviewReady(Vec<String>),
    RemoveLine(String),
    RemoveComplete { output: UpdateOutput },
    RemoveAnalysisComplete(String),
}

// ========== 子状态结构体 ==========

pub struct UpdateModeState {
    pub phase: UpdatePhase,
    pub view_mode: ViewMode,
    pub output: Option<UpdateOutput>,
    pub lines: Vec<String>,
    pub progress: String,
    pub packages_before: Option<String>,
    pub packages_after: Option<String>,
    pub analysis: Option<String>,
    pub scroll: usize,
    pub report_path: Option<String>,
    pub preview: Vec<String>,
}

pub struct QueryModeState {
    pub input: String,
    pub cursor: usize,
    pub panel: QueryPanel,
    pub view: QueryView,
    pub local_results: Vec<PackageInfo>,
    pub remote_results: Vec<PackageInfo>,
    pub local_selected: usize,
    pub remote_selected: usize,
    pub detail: Option<PackageDetail>,
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    pub file_mode: FileListMode,
    pub detail_scroll: usize,
    pub searching: bool,
}

pub struct InstallModeState {
    pub phase: InstallPhase,
    pub input: String,
    pub cursor: usize,
    pub results: Vec<PackageInfo>,
    pub selected: usize,
    pub marked: HashSet<usize>,
    pub preview: Vec<String>,
    pub lines: Vec<String>,
    pub output: Option<UpdateOutput>,
    pub progress: String,
    pub analysis: Option<String>,
    pub scroll: usize,
    pub searching: bool,
    pub view_mode: ViewMode,
    pub report_path: Option<String>,
}

pub struct RemoveModeState {
    pub phase: RemovePhase,
    pub input: String,
    pub cursor: usize,
    pub packages: Vec<InstalledPackage>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub marked: HashSet<usize>,
    pub preview: Vec<String>,
    pub lines: Vec<String>,
    pub output: Option<UpdateOutput>,
    pub progress: String,
    pub analysis: Option<String>,
    pub scroll: usize,
    pub loading: bool,
    pub view_mode: ViewMode,
    pub report_path: Option<String>,
}

pub struct SettingsModeState {
    pub items: Vec<SettingsItem>,
    pub selected: usize,
    pub editing: bool,
    pub edit_buffer: String,
    pub edit_cursor: usize,
    pub message: Option<String>,
    pub scroll: usize,
}

// ========== 子状态 impl ==========

/// 从输出行中解析进度信息（共用）
fn parse_progress_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with('(') {
        if let Some(end) = trimmed.find(')') {
            let inner = trimmed[1..end].trim();
            if inner.contains('/') {
                let rest = trimmed[end + 1..].trim();
                let action = rest.split_whitespace().next().unwrap_or("");
                return Some(format!("[{action}] {inner}"));
            }
        }
    }
    if let Some(speed_pos) = trimmed.find("iB/s") {
        let before = &trimmed[..speed_pos + 4];
        if let Some(last_space) = before.rfind([' ', '\t']) {
            let speed = before[last_space..].trim();
            if !speed.is_empty() {
                return Some(format!("下载中... {speed}"));
            }
        }
    }
    None
}

/// 获取模式内容（共用）
fn get_mode_content(
    view_mode: &ViewMode,
    output: &Option<UpdateOutput>,
    lines: &[String],
    analysis: &Option<String>,
    waiting_msg: &str,
) -> Vec<String> {
    match view_mode {
        ViewMode::UpdateLog => {
            if let Some(output) = output {
                output.combined_output().lines().map(|s| s.to_string()).collect()
            } else if !lines.is_empty() {
                lines.to_vec()
            } else {
                vec![waiting_msg.to_string()]
            }
        }
        ViewMode::AIAnalysis => {
            if let Some(analysis) = analysis {
                analysis.lines().map(|s| s.to_string()).collect()
            } else {
                vec!["AI 分析中...".to_string()]
            }
        }
    }
}

impl UpdateModeState {
    pub fn new() -> Self {
        Self {
            phase: UpdatePhase::PackageManagerCheck,
            view_mode: ViewMode::UpdateLog,
            output: None,
            lines: Vec::new(),
            progress: String::new(),
            packages_before: None,
            packages_after: None,
            analysis: None,
            scroll: 0,
            report_path: None,
            preview: Vec::new(),
        }
    }

    pub fn get_content(&self) -> Vec<String> {
        get_mode_content(&self.view_mode, &self.output, &self.lines, &self.analysis, "等待更新...")
    }

    pub fn add_line(&mut self, line: String) {
        if let Some(p) = parse_progress_line(&line) {
            self.progress = p;
        }
        self.lines.push(line);
        if self.lines.len() > 1 {
            self.scroll = self.lines.len().saturating_sub(1);
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, max_lines: usize, visible_height: usize) {
        let max_scroll = max_lines.saturating_sub(visible_height);
        if self.scroll < max_scroll {
            self.scroll += 1;
        }
    }

    pub fn scroll_page_up(&mut self, page_size: usize) {
        self.scroll = self.scroll.saturating_sub(page_size);
    }

    pub fn scroll_page_down(&mut self, page_size: usize, max_lines: usize, visible_height: usize) {
        let max_scroll = max_lines.saturating_sub(visible_height);
        self.scroll = (self.scroll + page_size).min(max_scroll);
    }

    pub fn reset_scroll(&mut self) {
        self.scroll = 0;
    }

    pub fn clamp_scroll(&mut self, max_lines: usize, visible_height: usize) {
        let max_scroll = max_lines.saturating_sub(visible_height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }
}

impl QueryModeState {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor: 0,
            panel: QueryPanel::Local,
            view: QueryView::List,
            local_results: Vec::new(),
            remote_results: Vec::new(),
            local_selected: 0,
            remote_selected: 0,
            detail: None,
            files: Vec::new(),
            dirs: Vec::new(),
            file_mode: FileListMode::Files,
            detail_scroll: 0,
            searching: false,
        }
    }
}

impl InstallModeState {
    pub fn new() -> Self {
        Self {
            phase: InstallPhase::Searching,
            input: String::new(),
            cursor: 0,
            results: Vec::new(),
            selected: 0,
            marked: HashSet::new(),
            preview: Vec::new(),
            lines: Vec::new(),
            output: None,
            progress: String::new(),
            analysis: None,
            scroll: 0,
            searching: false,
            view_mode: ViewMode::UpdateLog,
            report_path: None,
        }
    }

    pub fn get_content(&self) -> Vec<String> {
        get_mode_content(&self.view_mode, &self.output, &self.lines, &self.analysis, "等待安装...")
    }

    pub fn add_line(&mut self, line: String) {
        if let Some(p) = parse_progress_line(&line) {
            self.progress = p;
        }
        self.lines.push(line);
        if self.lines.len() > 1 {
            self.scroll = self.lines.len().saturating_sub(1);
        }
    }
}

impl RemoveModeState {
    pub fn new() -> Self {
        Self {
            phase: RemovePhase::Browsing,
            input: String::new(),
            cursor: 0,
            packages: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            marked: HashSet::new(),
            preview: Vec::new(),
            lines: Vec::new(),
            output: None,
            progress: String::new(),
            analysis: None,
            scroll: 0,
            loading: false,
            view_mode: ViewMode::UpdateLog,
            report_path: None,
        }
    }

    pub fn get_content(&self) -> Vec<String> {
        get_mode_content(&self.view_mode, &self.output, &self.lines, &self.analysis, "等待卸载...")
    }

    pub fn add_line(&mut self, line: String) {
        if let Some(p) = parse_progress_line(&line) {
            self.progress = p;
        }
        self.lines.push(line);
        if self.lines.len() > 1 {
            self.scroll = self.lines.len().saturating_sub(1);
        }
    }

    /// 对卸载的包列表应用筛选
    pub fn apply_filter(&mut self) {
        let keyword = self.input.to_lowercase();
        if keyword.is_empty() {
            self.filtered = (0..self.packages.len()).collect();
        } else {
            self.filtered = self.packages
                .iter()
                .enumerate()
                .filter(|(_, pkg)| {
                    pkg.name.to_lowercase().contains(&keyword)
                        || pkg.description.to_lowercase().contains(&keyword)
                })
                .map(|(i, _)| i)
                .collect();
        }
        self.selected = 0;
        self.marked.retain(|idx| self.filtered.contains(idx));
    }
}

impl SettingsModeState {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            editing: false,
            edit_buffer: String::new(),
            edit_cursor: 0,
            message: None,
            scroll: 0,
        }
    }
}

// ========== App ==========

pub struct App {
    pub mode: AppMode,
    pub config: Config,
    pub package_manager: Option<PackageManager>,
    pub system_info: Option<SystemInfo>,
    pub error_message: Option<String>,
    pub should_quit: bool,
    pub installed_count: Option<usize>,
    // 子状态
    pub update: UpdateModeState,
    pub query: QueryModeState,
    pub install: InstallModeState,
    pub remove: RemoveModeState,
    pub settings: SettingsModeState,
}

impl App {
    pub fn new(config: Config) -> Self {
        Self {
            mode: AppMode::Dashboard,
            config,
            package_manager: None,
            system_info: None,
            error_message: None,
            should_quit: false,
            installed_count: None,
            update: UpdateModeState::new(),
            query: QueryModeState::new(),
            install: InstallModeState::new(),
            remove: RemoveModeState::new(),
            settings: SettingsModeState::new(),
        }
    }

    /// 重置更新相关状态
    pub fn reset_update_state(&mut self) {
        self.update = UpdateModeState::new();
        self.error_message = None;
    }

    /// 重置查询相关状态
    pub fn reset_query_state(&mut self) {
        self.query = QueryModeState::new();
    }

    /// 重置安装相关状态
    pub fn reset_install_state(&mut self) {
        self.install = InstallModeState::new();
        self.error_message = None;
    }

    /// 重置卸载相关状态
    pub fn reset_remove_state(&mut self) {
        self.remove = RemoveModeState::new();
        self.error_message = None;
    }

    /// 从当前 config 构建设置项列表
    pub fn build_settings_items(&mut self) {
        self.settings.items = vec![
            SettingsItem::Section("AI 分析".to_string()),
            SettingsItem::Toggle {
                label: "系统更新后自动分析".to_string(),
                key: "ai.update".to_string(),
                value: self.config.ai.update,
            },
            SettingsItem::Toggle {
                label: "安装软件包后分析".to_string(),
                key: "ai.install".to_string(),
                value: self.config.ai.install,
            },
            SettingsItem::Toggle {
                label: "卸载软件包后分析".to_string(),
                key: "ai.remove".to_string(),
                value: self.config.ai.remove,
            },
            SettingsItem::Section("AI 配置".to_string()),
            SettingsItem::TextEdit {
                label: "模型".to_string(),
                key: "model".to_string(),
                value: self.config.model.clone(),
                masked: false,
            },
            SettingsItem::TextEdit {
                label: "温度".to_string(),
                key: "temperature".to_string(),
                value: format!("{}", self.config.temperature),
                masked: false,
            },
            SettingsItem::TextEdit {
                label: "API 地址".to_string(),
                key: "api_url".to_string(),
                value: self.config.api_url.clone().unwrap_or_default(),
                masked: false,
            },
            SettingsItem::TextEdit {
                label: "API Key".to_string(),
                key: "api_key".to_string(),
                value: self.config.api_key.clone().unwrap_or_default(),
                masked: true,
            },
            SettingsItem::TextEdit {
                label: "代理".to_string(),
                key: "proxy".to_string(),
                value: self.config.proxy.clone().unwrap_or_default(),
                masked: false,
            },
            SettingsItem::Section("报告".to_string()),
            SettingsItem::TextEdit {
                label: "保存目录".to_string(),
                key: "report_dir".to_string(),
                value: self.config.report_dir.display().to_string(),
                masked: false,
            },
        ];
        self.settings.selected = 0;
        self.settings.editing = false;
        self.settings.message = None;
        self.settings.scroll = 0;
    }

    /// 切换 Toggle 项的值并同步到 config
    pub fn toggle_settings_item(&mut self) {
        let focusable: Vec<usize> = self.settings.items.iter().enumerate()
            .filter(|(_, item)| !matches!(item, SettingsItem::Section(_)))
            .map(|(i, _)| i)
            .collect();

        if let Some(&real_idx) = focusable.get(self.settings.selected) {
            if let SettingsItem::Toggle { key, value, .. } = &self.settings.items[real_idx] {
                let new_val = !*value;
                let key = key.clone();
                if let SettingsItem::Toggle { value: v, .. } = &mut self.settings.items[real_idx] {
                    *v = new_val;
                }
                match key.as_str() {
                    "ai.update" => self.config.ai.update = new_val,
                    "ai.install" => self.config.ai.install = new_val,
                    "ai.remove" => self.config.ai.remove = new_val,
                    _ => {}
                }
            }
        }
    }

    /// 开始编辑 TextEdit 项
    pub fn start_settings_edit(&mut self) {
        let focusable: Vec<usize> = self.settings.items.iter().enumerate()
            .filter(|(_, item)| !matches!(item, SettingsItem::Section(_)))
            .map(|(i, _)| i)
            .collect();

        if let Some(&real_idx) = focusable.get(self.settings.selected) {
            if let SettingsItem::TextEdit { value, .. } = &self.settings.items[real_idx] {
                self.settings.edit_buffer = value.clone();
                self.settings.edit_cursor = self.settings.edit_buffer.len();
                self.settings.editing = true;
            }
        }
    }

    /// 确认编辑并写回 config
    pub fn confirm_settings_edit(&mut self) {
        let focusable: Vec<usize> = self.settings.items.iter().enumerate()
            .filter(|(_, item)| !matches!(item, SettingsItem::Section(_)))
            .map(|(i, _)| i)
            .collect();

        if let Some(&real_idx) = focusable.get(self.settings.selected) {
            let buf = self.settings.edit_buffer.clone();
            if let SettingsItem::TextEdit { key, value, .. } = &mut self.settings.items[real_idx] {
                *value = buf.clone();
                match key.as_str() {
                    "model" => self.config.model = buf,
                    "temperature" => {
                        if let Ok(t) = buf.parse::<f32>() {
                            self.config.temperature = t;
                        }
                    }
                    "api_url" => {
                        self.config.api_url = if buf.is_empty() { None } else { Some(buf) };
                    }
                    "api_key" => {
                        self.config.api_key = if buf.is_empty() { None } else { Some(buf) };
                    }
                    "proxy" => {
                        self.config.proxy = if buf.is_empty() { None } else { Some(buf) };
                    }
                    "report_dir" => {
                        self.config.report_dir = std::path::PathBuf::from(buf);
                    }
                    _ => {}
                }
            }
        }
        self.settings.editing = false;
    }

    /// 保存配置到磁盘
    pub fn save_settings(&mut self) {
        match self.config.save() {
            Ok(()) => {
                self.settings.message = Some("✓ 已保存到 ~/.config/lian/config.toml".to_string());
            }
            Err(e) => {
                self.settings.message = Some(format!("✗ 保存失败: {}", e));
            }
        }
    }

    /// 获取可聚焦项数量
    pub fn settings_focusable_count(&self) -> usize {
        self.settings.items.iter()
            .filter(|item| !matches!(item, SettingsItem::Section(_)))
            .count()
    }
}
