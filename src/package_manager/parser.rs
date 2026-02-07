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
