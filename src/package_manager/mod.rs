//! 包管理器模块 — 对 pacman / paru / yay 的封装

pub mod parser;
pub mod streaming;
pub mod types;

// 重新导出常用类型和函数
pub use streaming::cancel_update;
pub use streaming::cleanup_child_processes;
pub use streaming::run_custom_command_streaming;
pub use types::{InstalledPackage, PackageDetail, PackageInfo, UpdateOutput};

use anyhow::{anyhow, Result};
use parser::{parse_installed_packages, parse_package_detail, parse_search_output};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct PackageManager {
    pub command: String,
}

impl PackageManager {
    pub fn detect() -> Result<Self> {
        for pm in &["paru", "yay", "pacman"] {
            if Command::new("which")
                .arg(pm)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return Ok(PackageManager {
                    command: pm.to_string(),
                });
            }
        }
        Err(anyhow!("未找到包管理器 (paru/yay/pacman)"))
    }

    pub fn name(&self) -> &str {
        &self.command
    }

    /// 获取已安装包数量
    pub fn count_installed(&self) -> usize {
        Command::new("pacman")
            .args(["-Q"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
            .unwrap_or(0)
    }

    /// 检查可用更新（不实际执行更新）
    pub fn check_updates(&self) -> Vec<String> {
        let output = Command::new("checkupdates").output();
        if let Ok(o) = &output {
            if o.status.success() {
                return String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|s| s.to_string())
                    .collect();
            }
        }
        let output = if self.command == "pacman" {
            Command::new("pacman").args(["-Qu"]).output()
        } else {
            Command::new(&self.command).args(["-Qu"]).output()
        };
        match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// 获取当前已安装的显式安装包列表
    pub fn get_explicit_packages(&self) -> Result<String> {
        let output = Command::new("pacman").args(["-Qe"]).output()?;
        if !output.status.success() {
            anyhow::bail!("pacman -Qe 执行失败");
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// 获取显式安装的包列表（含大小和描述）
    pub fn get_installed_packages_with_size(&self) -> Vec<InstalledPackage> {
        let output = Command::new("pacman").args(["-Qei"]).output();
        match output {
            Ok(o) if o.status.success() => {
                parse_installed_packages(&String::from_utf8_lossy(&o.stdout))
            }
            _ => Vec::new(),
        }
    }

    // ===== 查询 =====

    /// 搜索本地已安装包 (pacman -Qs)
    pub fn search_local(&self, keyword: &str) -> Vec<PackageInfo> {
        if keyword.trim().is_empty() {
            return Vec::new();
        }
        let output = Command::new("pacman").args(["-Qs", keyword]).output();
        match output {
            Ok(o) if o.status.success() => {
                parse_search_output(&String::from_utf8_lossy(&o.stdout), true)
            }
            _ => Vec::new(),
        }
    }

    /// 搜索远程仓库包 (paru/yay/pacman -Ss)
    pub fn search_remote(&self, keyword: &str) -> Vec<PackageInfo> {
        if keyword.trim().is_empty() {
            return Vec::new();
        }
        let output = Command::new(&self.command).args(["-Ss", keyword]).output();
        match output {
            Ok(o) if o.status.success() => {
                parse_search_output(&String::from_utf8_lossy(&o.stdout), false)
            }
            _ => Vec::new(),
        }
    }

    /// 获取本地包详情 (pacman -Qi)
    pub fn package_info_local(&self, name: &str) -> Result<PackageDetail> {
        let output = Command::new("pacman").args(["-Qi", name]).output()?;
        if !output.status.success() {
            anyhow::bail!("pacman -Qi {} 执行失败", name);
        }
        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(parse_package_detail(&raw))
    }

    /// 获取远程包详情 (pacman -Si)
    pub fn package_info_remote(&self, name: &str) -> Result<PackageDetail> {
        let output = Command::new("pacman").args(["-Si", name]).output()?;
        if !output.status.success() {
            anyhow::bail!("pacman -Si {} 执行失败", name);
        }
        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(parse_package_detail(&raw))
    }

    /// 获取已安装包的文件列表 (pacman -Ql)
    pub fn package_files(&self, name: &str) -> Vec<String> {
        let output = Command::new("pacman").args(["-Ql", name]).output();
        match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|line| {
                    if let Some(pos) = line.find(' ') {
                        let path = &line[pos + 1..];
                        if path.ends_with('/') {
                            None
                        } else {
                            Some(path.to_string())
                        }
                    } else {
                        None
                    }
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// 获取已安装包的目录列表 (pacman -Ql)
    pub fn package_dirs(&self, name: &str) -> Vec<String> {
        let output = Command::new("pacman").args(["-Ql", name]).output();
        match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|line| {
                    if let Some(pos) = line.find(' ') {
                        let path = &line[pos + 1..];
                        if path.ends_with('/') {
                            Some(path.to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    // ===== 预览 =====

    /// 预览安装操作（显示将安装的包和依赖）
    pub fn preview_install(&self, packages: &[String]) -> Vec<String> {
        let mut lines = Vec::new();

        for pkg in packages {
            let output = Command::new("pacman").args(["-Si", pkg]).output();

            if let Ok(o) = output {
                if o.status.success() {
                    let info = String::from_utf8_lossy(&o.stdout);
                    let mut name = String::new();
                    let mut version = String::new();
                    let mut size = String::new();
                    let mut depends = String::new();

                    for line in info.lines() {
                        if let Some(colon) = line.find(':') {
                            let key = line[..colon].trim();
                            let val = line[colon + 1..].trim();
                            match key {
                                "Name" | "名称" | "名字" => name = val.to_string(),
                                "Version" | "版本" => version = val.to_string(),
                                "Installed Size" | "Download Size" | "安装大小" | "安装后大小"
                                | "下载大小" => {
                                    if size.is_empty() {
                                        size = val.to_string();
                                    }
                                }
                                "Depends On" | "依赖于" => depends = val.to_string(),
                                _ => {}
                            }
                        }
                    }

                    lines.push(format!("  {} {}", name, version));
                    if !size.is_empty() {
                        lines.push(format!("    大小: {}", size));
                    }
                    if !depends.is_empty() && depends != "None" {
                        lines.push(format!("    依赖: {}", depends));
                    }
                    lines.push(String::new());
                } else {
                    lines.push(format!("  {} (未找到包信息)", pkg));
                    lines.push(String::new());
                }
            }
        }

        lines
    }

    /// 预览卸载操作（显示将被移除的包）
    pub fn preview_remove(&self, packages: &[String]) -> Vec<String> {
        let mut lines = Vec::new();

        for pkg in packages {
            let output = Command::new("pacman").args(["-Qi", pkg]).output();

            if let Ok(o) = output {
                if o.status.success() {
                    let info = String::from_utf8_lossy(&o.stdout);
                    let mut name = String::new();
                    let mut version = String::new();
                    let mut size = String::new();
                    let mut required_by = String::new();

                    for line in info.lines() {
                        if let Some(colon) = line.find(':') {
                            let key = line[..colon].trim();
                            let val = line[colon + 1..].trim();
                            match key {
                                "Name" | "名称" | "名字" => name = val.to_string(),
                                "Version" | "版本" => version = val.to_string(),
                                "Installed Size" | "安装大小" | "安装后大小" => {
                                    size = val.to_string()
                                }
                                "Required By" | "依赖它" => required_by = val.to_string(),
                                _ => {}
                            }
                        }
                    }

                    lines.push(format!("  {} {}", name, version));
                    if !size.is_empty() {
                        lines.push(format!("    大小: {}", size));
                    }
                    if !required_by.is_empty() && required_by != "None" && required_by != "无" {
                        lines.push(format!("    ⚠ 被依赖: {}", required_by));
                    }
                    lines.push(String::new());
                } else {
                    lines.push(format!("  {} (未找到包信息)", pkg));
                    lines.push(String::new());
                }
            }
        }

        // 用 pacman -Rns --print 获取完整移除列表（含依赖）
        let mut args = vec!["-Rns".to_string(), "--print".to_string()];
        args.extend(packages.iter().cloned());

        let output = Command::new("pacman").args(&args).output();

        if let Ok(o) = output {
            if o.status.success() {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let remove_list: Vec<&str> = stdout.lines().collect();
                if !remove_list.is_empty() {
                    lines.push(format!(
                        "将移除以下 {} 个包（含孤立依赖）:",
                        remove_list.len()
                    ));
                    for l in &remove_list {
                        lines.push(format!("  {}", l));
                    }
                }
            } else {
                let mut args2 = vec!["-Rn".to_string(), "--print".to_string()];
                args2.extend(packages.iter().cloned());
                if let Ok(o2) = Command::new("pacman").args(&args2).output() {
                    if o2.status.success() {
                        let stdout = String::from_utf8_lossy(&o2.stdout);
                        let remove_list: Vec<&str> = stdout.lines().collect();
                        if !remove_list.is_empty() {
                            lines.push(format!("将移除以下 {} 个包:", remove_list.len()));
                            for l in &remove_list {
                                lines.push(format!("  {}", l));
                            }
                        }
                    }
                }
            }
        }

        lines
    }
}
