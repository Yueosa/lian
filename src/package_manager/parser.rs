//! 输出解析函数

use super::types::{InstalledPackage, PackageDetail, PackageInfo};

/// 清理终端输出中的 ANSI 转义序列和特殊字符
pub fn clean_terminal_output(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\x1b' => {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            }
            '\r' => {
                if chars.peek() != Some(&'\n') && !result.ends_with('\n') {
                    result.push('\n');
                }
            }
            c if c.is_control() && c != '\n' && c != '\t' => {}
            _ => result.push(c),
        }
    }

    let lines: Vec<&str> = result.lines().collect();
    let mut cleaned_lines = Vec::new();
    let mut prev_empty = false;

    for line in lines {
        let is_empty = line.trim().is_empty();
        if is_empty && prev_empty {
            continue;
        }
        cleaned_lines.push(line);
        prev_empty = is_empty;
    }

    cleaned_lines.join("\n")
}

// ========== 进度信息解析 ==========

/// 结构化进度信息，从 pacman/paru 的 \r 进度行解析
#[derive(Debug, Clone, Default)]
pub struct ProgressInfo {
    /// 原始文本（渲染到主面板的显示文本）
    pub raw: String,
    /// 包名 / 操作标签（如 "wget-2.1-1" 或 "-> Building ..."）
    pub label: String,
    /// 总大小，如 "4.50 MiB"
    pub total_size: String,
    /// 下载速度，如 "2.10 MiB/s"
    pub speed: String,
    /// 剩余时间，如 "00:23"
    pub eta: String,
}

impl ProgressInfo {
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// 格式化为状态栏文本：label  ⬇ speed  / total  剩余 eta
    pub fn footer_text(&self) -> String {
        if self.raw.is_empty() {
            return String::new();
        }
        let mut parts: Vec<String> = Vec::new();
        if !self.label.is_empty() {
            parts.push(self.label.clone());
        }
        if !self.speed.is_empty() {
            parts.push(format!("⬇ {}", self.speed));
        }
        if !self.total_size.is_empty() {
            parts.push(format!("/ {}", self.total_size));
        }
        if !self.eta.is_empty() {
            parts.push(format!("剩余 {}", self.eta));
        }
        if parts.is_empty() {
            // fallback: 截断 raw 文本
            let s = &self.raw;
            if s.len() > 60 { format!("{}…", &s[..58]) } else { s.clone() }
        } else {
            parts.join("  ")
        }
    }
}

/// 从 pacman/paru \r 进度行解析结构化信息。
///
/// 典型 pacman 下载行（管道模式）：
///   `wget-2.1-1  200.0 KiB  1.23 MiB/s 00:00 [####################] 100%`
/// 典型 paru AUR 行：
///   `  -> Building ...`  或  `:: Downloading sources...`
pub fn parse_progress_info(raw: &str) -> ProgressInfo {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ProgressInfo::default();
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let n = tokens.len();
    let mut label = String::new();
    let mut total_size = String::new();
    let mut speed = String::new();
    let mut eta = String::new();

    let mut i = 0;
    while i < n {
        let t = tokens[i];

        // 跳过进度条 [####] 类似内容
        if t.starts_with('[') {
            i += 1;
            continue;
        }
        // 跳过百分比 "100%"
        if t.ends_with('%') && t[..t.len() - 1].chars().all(|c| c.is_ascii_digit()) {
            i += 1;
            continue;
        }
        // ETA：匹配 "N:NN" 或 "NN:NN"
        if is_time_token(t) {
            if eta.is_empty() {
                eta = t.to_string();
            }
            i += 1;
            continue;
        }
        // 速度：两个 token "1.23 MiB/s"
        if i + 1 < n && contains_speed_unit(tokens[i + 1]) {
            if is_number_token(t) {
                speed = format!("{} {}", t, tokens[i + 1]);
                i += 2;
                continue;
            }
        }
        // 速度：单个 token "1.23MiB/s"
        if contains_speed_unit(t) {
            if speed.is_empty() {
                speed = t.to_string();
            }
            i += 1;
            continue;
        }
        // 大小：两个 token "200.0 KiB"（不跟 /s）
        if i + 1 < n && contains_size_unit(tokens[i + 1]) && !contains_speed_unit(tokens[i + 1]) {
            if is_number_token(t) {
                if total_size.is_empty() {
                    total_size = format!("{} {}", t, tokens[i + 1]);
                }
                i += 2;
                continue;
            }
        }
        // 大小：单个 token "200.0KiB"
        if contains_size_unit(t) && !contains_speed_unit(t) {
            if total_size.is_empty() {
                total_size = t.to_string();
            }
            i += 1;
            continue;
        }
        // 标签：第一个非数字、非特殊 token
        if label.is_empty() && !is_number_token(t) {
            label = t.to_string();
        }
        i += 1;
    }

    // 对于 paru 风格的纯文字行（无数字字段），直接用裁剪后的 raw 作为标签
    if label.is_empty() && speed.is_empty() && total_size.is_empty() {
        label = if trimmed.len() > 45 {
            format!("{}…", &trimmed[..44])
        } else {
            trimmed.to_string()
        };
    }

    ProgressInfo { raw: trimmed.to_string(), label, total_size, speed, eta }
}

fn is_number_token(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn is_time_token(s: &str) -> bool {
    let mut parts = s.splitn(2, ':');
    match (parts.next(), parts.next()) {
        (Some(h), Some(m)) => {
            !h.is_empty()
                && !m.is_empty()
                && h.chars().all(|c| c.is_ascii_digit())
                && m.chars().all(|c| c.is_ascii_digit())
                && m.len() == 2
        }
        _ => false,
    }
}

fn contains_speed_unit(s: &str) -> bool {
    s.contains("iB/s") || (s.contains("iB/") && s.ends_with('s'))
}

fn contains_size_unit(s: &str) -> bool {
    s.ends_with("iB") || s == "B"
}

/// 解析 pacman -Qs / -Ss 的搜索输出
pub fn parse_search_output(output: &str, is_local: bool) -> Vec<PackageInfo> {
    let mut results = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if !line.starts_with(' ') && !line.starts_with('\t') {
            let cleaned = clean_terminal_output(line);
            let trimmed = cleaned.trim();

            if let Some(slash_pos) = trimmed.find('/') {
                let repo = &trimmed[..slash_pos];
                let rest = &trimmed[slash_pos + 1..];
                let parts: Vec<&str> = rest.split_whitespace().collect();

                if let Some(&name) = parts.first() {
                    let version = parts.get(1).unwrap_or(&"").to_string();
                    let installed =
                        is_local || rest.contains("[installed") || rest.contains("[已安装");

                    let description = if i + 1 < lines.len() {
                        let desc_line = lines[i + 1];
                        if desc_line.starts_with(' ') || desc_line.starts_with('\t') {
                            i += 1;
                            clean_terminal_output(desc_line.trim())
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    };

                    results.push(PackageInfo {
                        repo: repo.to_string(),
                        name: name.to_string(),
                        version,
                        description,
                        installed,
                    });
                }
            }
        }
        i += 1;
    }

    results
}

/// 解析 pacman -Qi / -Si 的详情输出
pub fn parse_package_detail(output: &str) -> PackageDetail {
    let mut fields: Vec<(String, String)> = Vec::new();

    for line in output.lines() {
        if let Some(colon_pos) = line.find(':') {
            let key_part = &line[..colon_pos];
            let value_part = line[colon_pos + 1..].trim();

            if !key_part.starts_with(' ') || key_part.trim().is_empty() {
                let key = key_part.trim();
                if !key.is_empty() {
                    fields.push((key.to_string(), value_part.to_string()));
                    continue;
                }
            }
        }
        if (line.starts_with(' ') || line.starts_with('\t')) && !fields.is_empty() {
            let last = fields.last_mut().unwrap();
            last.1.push(' ');
            last.1.push_str(line.trim());
        }
    }

    PackageDetail { fields }
}

/// 解析 pacman -Qei 输出为 InstalledPackage 列表
pub fn parse_installed_packages(output: &str) -> Vec<InstalledPackage> {
    let mut packages = Vec::new();
    let mut name = String::new();
    let mut version = String::new();
    let mut size = String::new();
    let mut description = String::new();

    for line in output.lines() {
        if line.is_empty() || line.trim().is_empty() {
            if !name.is_empty() {
                packages.push(InstalledPackage {
                    name: name.clone(),
                    version: version.clone(),
                    size: size.clone(),
                    description: description.clone(),
                });
                name.clear();
                version.clear();
                size.clear();
                description.clear();
            }
            continue;
        }

        if let Some(colon) = line.find(':') {
            let key = line[..colon].trim();
            let val = line[colon + 1..].trim();
            match key {
                "Name" | "名称" | "名字" => name = val.to_string(),
                "Version" | "版本" => version = val.to_string(),
                "Installed Size" | "安装大小" | "安装后大小" => size = val.to_string(),
                "Description" | "描述" => description = val.to_string(),
                _ => {}
            }
        }
    }

    if !name.is_empty() {
        packages.push(InstalledPackage {
            name,
            version,
            size,
            description,
        });
    }

    packages
}
