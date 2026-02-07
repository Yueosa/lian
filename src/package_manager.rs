use anyhow::{anyhow, Result};
use std::io::Read;
use std::process::Command;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use tokio::sync::mpsc;

/// 全局变量用于存储当前运行的子进程 PID
static CHILD_PID: AtomicU32 = AtomicU32::new(0);
static SHOULD_CANCEL: AtomicBool = AtomicBool::new(false);

/// 设置取消标志
pub fn cancel_update() {
    SHOULD_CANCEL.store(true, Ordering::SeqCst);
    let pid = CHILD_PID.load(Ordering::SeqCst);
    if pid != 0 {
        // 尝试终止子进程
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
}

/// 重置取消标志
pub fn reset_cancel() {
    SHOULD_CANCEL.store(false, Ordering::SeqCst);
    CHILD_PID.store(0, Ordering::SeqCst);
}

/// 检查是否应该取消
fn should_cancel() -> bool {
    SHOULD_CANCEL.load(Ordering::SeqCst)
}

#[derive(Debug, Clone)]
pub struct PackageManager {
    pub command: String,
}

impl PackageManager {
    pub fn detect() -> Result<Self> {
        // 按优先级检测: paru -> yay -> pacman
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
        // 优先尝试 checkupdates（pacman-contrib 提供）
        let output = Command::new("checkupdates").output();
        if let Ok(o) = &output {
            if o.status.success() {
                return String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|s| s.to_string())
                    .collect();
            }
        }
        // 回退到 pacman -Qu / paru -Qu / yay -Qu
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

    /// 执行系统更新命令（流式输出）
    pub fn update_streaming(
        &self,
        output_tx: mpsc::UnboundedSender<String>,
    ) -> Result<UpdateOutput> {
        reset_cancel(); // 重置取消标志

        use std::os::unix::process::CommandExt;

        let mut child = if self.command == "pacman" {
            let mut cmd = Command::new("sudo");
            cmd.args(["pacman", "-Syu", "--noconfirm"]);
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            // 父进程死亡时，子进程收到 SIGTERM
            unsafe {
                cmd.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            cmd.spawn()?
        } else {
            let mut cmd = Command::new(&self.command);
            cmd.args(["-Syu", "--noconfirm"]);
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            unsafe {
                cmd.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            cmd.spawn()?
        };

        // 存储子进程 PID
        CHILD_PID.store(child.id(), Ordering::SeqCst);

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // 使用线程同时读取 stdout 和 stderr
        let output_tx_clone = output_tx.clone();
        let stdout_handle = std::thread::spawn(move || {
            let mut result = String::new();
            if let Some(mut stdout) = stdout {
                let mut buffer = [0u8; 1024];
                let mut line_buffer = String::new();

                while let Ok(n) = stdout.read(&mut buffer) {
                    if n == 0 || should_cancel() {
                        break;
                    }

                    let chunk = String::from_utf8_lossy(&buffer[..n]);
                    for c in chunk.chars() {
                        match c {
                            '\n' => {
                                // 完整的一行
                                let cleaned = clean_terminal_output(&line_buffer);
                                if !cleaned.trim().is_empty() {
                                    let _ = output_tx_clone.send(cleaned.clone());
                                    result.push_str(&cleaned);
                                    result.push('\n');
                                }
                                line_buffer.clear();
                            }
                            '\r' => {
                                // 进度条行，也发送出去
                                let cleaned = clean_terminal_output(&line_buffer);
                                if !cleaned.trim().is_empty() {
                                    let _ = output_tx_clone.send(cleaned.clone());
                                }
                                line_buffer.clear();
                            }
                            _ => {
                                line_buffer.push(c);
                            }
                        }
                    }
                }
                // 处理最后一行（没有换行符的情况）
                if !line_buffer.is_empty() {
                    let cleaned = clean_terminal_output(&line_buffer);
                    if !cleaned.trim().is_empty() {
                        let _ = output_tx_clone.send(cleaned.clone());
                        result.push_str(&cleaned);
                        result.push('\n');
                    }
                }
            }
            result
        });

        let stderr_handle = std::thread::spawn(move || {
            let mut result = String::new();
            if let Some(mut stderr) = stderr {
                let mut buffer = [0u8; 1024];
                let mut line_buffer = String::new();

                while let Ok(n) = stderr.read(&mut buffer) {
                    if n == 0 || should_cancel() {
                        break;
                    }

                    let chunk = String::from_utf8_lossy(&buffer[..n]);
                    for c in chunk.chars() {
                        match c {
                            '\n' | '\r' => {
                                let cleaned = clean_terminal_output(&line_buffer);
                                if !cleaned.trim().is_empty() {
                                    let _ = output_tx.send(format!("⚠ {}", cleaned));
                                    result.push_str(&cleaned);
                                    result.push('\n');
                                }
                                line_buffer.clear();
                            }
                            _ => {
                                line_buffer.push(c);
                            }
                        }
                    }
                }
                if !line_buffer.is_empty() {
                    let cleaned = clean_terminal_output(&line_buffer);
                    if !cleaned.trim().is_empty() {
                        let _ = output_tx.send(format!("⚠ {}", cleaned));
                        result.push_str(&cleaned);
                        result.push('\n');
                    }
                }
            }
            result
        });

        // 等待两个线程完成
        let all_stdout = stdout_handle.join().unwrap_or_default();
        let all_stderr = stderr_handle.join().unwrap_or_default();

        let cancelled = should_cancel();

        // 如果被取消，尝试终止子进程
        if cancelled {
            let _ = child.kill();
            let _ = child.wait();
            CHILD_PID.store(0, Ordering::SeqCst);
            return Ok(UpdateOutput {
                stdout: all_stdout,
                stderr: "更新已取消".to_string(),
                success: false,
            });
        }

        let status = child.wait()?;
        CHILD_PID.store(0, Ordering::SeqCst); // 清除 PID

        Ok(UpdateOutput {
            stdout: all_stdout,
            stderr: all_stderr,
            success: status.success(),
        })
    }
}

// ===== 安装 / 卸载 流式命令 =====

impl PackageManager {
    /// 执行安装命令（流式输出）
    pub fn install_streaming(
        &self,
        packages: &[String],
        output_tx: mpsc::UnboundedSender<String>,
    ) -> Result<UpdateOutput> {
        reset_cancel();

        use std::os::unix::process::CommandExt;

        let mut child = if self.command == "pacman" {
            let mut cmd = Command::new("sudo");
            let mut args = vec!["pacman", "-S", "--noconfirm"];
            let pkg_refs: Vec<&str> = packages.iter().map(|s| s.as_str()).collect();
            args.extend(pkg_refs);
            cmd.args(&args);
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            unsafe {
                cmd.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            cmd.spawn()?
        } else {
            let mut cmd = Command::new(&self.command);
            let mut args = vec!["-S".to_string(), "--noconfirm".to_string()];
            args.extend(packages.iter().cloned());
            cmd.args(&args);
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            unsafe {
                cmd.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            cmd.spawn()?
        };

        CHILD_PID.store(child.id(), Ordering::SeqCst);

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let output_tx_clone = output_tx.clone();
        let stdout_handle =
            std::thread::spawn(move || read_stream_lines(stdout, &output_tx_clone, false));

        let stderr_handle = std::thread::spawn(move || read_stream_lines(stderr, &output_tx, true));

        let all_stdout = stdout_handle.join().unwrap_or_default();
        let all_stderr = stderr_handle.join().unwrap_or_default();

        let cancelled = should_cancel();
        if cancelled {
            let _ = child.kill();
            let _ = child.wait();
            CHILD_PID.store(0, Ordering::SeqCst);
            return Ok(UpdateOutput {
                stdout: all_stdout,
                stderr: "安装已取消".to_string(),
                success: false,
            });
        }

        let status = child.wait()?;
        CHILD_PID.store(0, Ordering::SeqCst);

        Ok(UpdateOutput {
            stdout: all_stdout,
            stderr: all_stderr,
            success: status.success(),
        })
    }

    /// 执行卸载命令（流式输出）
    pub fn remove_streaming(
        &self,
        packages: &[String],
        output_tx: mpsc::UnboundedSender<String>,
    ) -> Result<UpdateOutput> {
        reset_cancel();

        use std::os::unix::process::CommandExt;

        let mut child = if self.command == "pacman" {
            let mut cmd = Command::new("sudo");
            let mut args = vec!["pacman", "-Rns", "--noconfirm"];
            let pkg_refs: Vec<&str> = packages.iter().map(|s| s.as_str()).collect();
            args.extend(pkg_refs);
            cmd.args(&args);
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            unsafe {
                cmd.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            cmd.spawn()?
        } else {
            let mut cmd = Command::new(&self.command);
            let mut args = vec!["-Rns".to_string(), "--noconfirm".to_string()];
            args.extend(packages.iter().cloned());
            cmd.args(&args);
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            unsafe {
                cmd.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            cmd.spawn()?
        };

        CHILD_PID.store(child.id(), Ordering::SeqCst);

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let output_tx_clone = output_tx.clone();
        let stdout_handle =
            std::thread::spawn(move || read_stream_lines(stdout, &output_tx_clone, false));

        let stderr_handle = std::thread::spawn(move || read_stream_lines(stderr, &output_tx, true));

        let all_stdout = stdout_handle.join().unwrap_or_default();
        let all_stderr = stderr_handle.join().unwrap_or_default();

        let cancelled = should_cancel();
        if cancelled {
            let _ = child.kill();
            let _ = child.wait();
            CHILD_PID.store(0, Ordering::SeqCst);
            return Ok(UpdateOutput {
                stdout: all_stdout,
                stderr: "卸载已取消".to_string(),
                success: false,
            });
        }

        let status = child.wait()?;
        CHILD_PID.store(0, Ordering::SeqCst);

        Ok(UpdateOutput {
            stdout: all_stdout,
            stderr: all_stderr,
            success: status.success(),
        })
    }

    /// 获取显式安装的包列表（含大小和描述）
    pub fn get_installed_packages_with_size(&self) -> Vec<InstalledPackage> {
        // 使用 pacman -Qe 获取显式安装的包，再用 pacman -Qi 获取详情
        let output = Command::new("pacman").args(["-Qei"]).output();

        match output {
            Ok(o) if o.status.success() => {
                parse_installed_packages(&String::from_utf8_lossy(&o.stdout))
            }
            _ => Vec::new(),
        }
    }

    /// 预览安装操作（显示将安装的包和依赖）
    pub fn preview_install(&self, packages: &[String]) -> Vec<String> {
        // 使用 pacman -Si 获取包信息
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

        // 用 pacman -Qi 获取每个包的详细信息
        for pkg in packages {
            let output = Command::new("pacman")
                .args(["-Qi", pkg])
                .output();

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
                                "Required By" | "依赖它" => {
                                    required_by = val.to_string()
                                }
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
                // --print 可能在某些场景下失败，尝试不带 -s
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

/// 已安装包信息
#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub size: String,
    pub description: String,
}

/// 解析 pacman -Qei 输出为 InstalledPackage 列表
fn parse_installed_packages(output: &str) -> Vec<InstalledPackage> {
    let mut packages = Vec::new();
    let mut name = String::new();
    let mut version = String::new();
    let mut size = String::new();
    let mut description = String::new();

    for line in output.lines() {
        if line.is_empty() || line.trim().is_empty() {
            // 空行分隔不同的包
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

    // 最后一个包
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

/// 从流中读取行并发送到 channel（通用辅助函数）
fn read_stream_lines(
    stream: Option<impl std::io::Read>,
    tx: &mpsc::UnboundedSender<String>,
    is_stderr: bool,
) -> String {
    let mut result = String::new();
    if let Some(mut reader) = stream {
        let mut buffer = [0u8; 1024];
        let mut line_buffer = String::new();

        while let Ok(n) = reader.read(&mut buffer) {
            if n == 0 || should_cancel() {
                break;
            }

            let chunk = String::from_utf8_lossy(&buffer[..n]);
            for c in chunk.chars() {
                match c {
                    '\n' => {
                        let cleaned = clean_terminal_output(&line_buffer);
                        if !cleaned.trim().is_empty() {
                            let msg = if is_stderr {
                                format!("⚠ {}", cleaned)
                            } else {
                                cleaned.clone()
                            };
                            let _ = tx.send(msg);
                            result.push_str(&cleaned);
                            result.push('\n');
                        }
                        line_buffer.clear();
                    }
                    '\r' => {
                        let cleaned = clean_terminal_output(&line_buffer);
                        if !cleaned.trim().is_empty() {
                            let msg = if is_stderr {
                                format!("⚠ {}", cleaned)
                            } else {
                                cleaned.clone()
                            };
                            let _ = tx.send(msg);
                        }
                        line_buffer.clear();
                    }
                    _ => {
                        line_buffer.push(c);
                    }
                }
            }
        }
        if !line_buffer.is_empty() {
            let cleaned = clean_terminal_output(&line_buffer);
            if !cleaned.trim().is_empty() {
                let msg = if is_stderr {
                    format!("⚠ {}", cleaned)
                } else {
                    cleaned.clone()
                };
                let _ = tx.send(msg);
                result.push_str(&cleaned);
                result.push('\n');
            }
        }
    }
    result
}

/// 清理终端输出中的 ANSI 转义序列和特殊字符
fn clean_terminal_output(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // 处理 ANSI 转义序列 (ESC [ ... m 等)
            '\x1b' => {
                // 跳过转义序列
                if chars.peek() == Some(&'[') {
                    chars.next(); // 消费 '['
                                  // 跳过直到遇到字母
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            }
            // 处理回车符 - 用于进度条，我们保留为换行
            '\r' => {
                // 如果下一个不是换行，添加换行
                if chars.peek() != Some(&'\n') && !result.ends_with('\n') {
                    result.push('\n');
                }
            }
            // 其他控制字符跳过（除了换行和制表符）
            c if c.is_control() && c != '\n' && c != '\t' => {}
            // 正常字符
            _ => result.push(c),
        }
    }

    // 清理连续的空行
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

#[derive(Debug, Clone)]
pub struct UpdateOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl UpdateOutput {
    pub fn combined_output(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

// ===== 查询相关结构体和方法 =====

/// 搜索结果条目
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub repo: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub installed: bool,
}

/// 包详情
#[derive(Debug, Clone)]
pub struct PackageDetail {
    pub fields: Vec<(String, String)>,
}

impl PackageManager {
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
            Ok(o) if o.status.success() => {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|line| {
                        // 格式: "pkgname /path/to/file"
                        if let Some(pos) = line.find(' ') {
                            let path = &line[pos + 1..];
                            // 过滤掉目录条目（以 / 结尾）
                            if path.ends_with('/') {
                                None
                            } else {
                                Some(path.to_string())
                            }
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// 获取已安装包的目录列表（最底层目录） (pacman -Ql)
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
}

/// 解析 pacman -Qs / -Ss 的搜索输出
/// 格式:
///   repo/name version [installed]
///       description
fn parse_search_output(output: &str, is_local: bool) -> Vec<PackageInfo> {
    let mut results = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        // 包头行: 不以空格开头
        if !line.starts_with(' ') && !line.starts_with('\t') {
            let cleaned = clean_terminal_output(line);
            let trimmed = cleaned.trim();

            // 解析 "repo/name version [installed]" 或 "local/name version"
            if let Some(slash_pos) = trimmed.find('/') {
                let repo = &trimmed[..slash_pos];
                let rest = &trimmed[slash_pos + 1..];
                let parts: Vec<&str> = rest.split_whitespace().collect();

                if let Some(&name) = parts.first() {
                    let version = parts.get(1).unwrap_or(&"").to_string();
                    let installed =
                        is_local || rest.contains("[installed") || rest.contains("[已安装");

                    // 下一行是描述
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
/// 格式:
///   Key            : Value
///   Key            : Value that
///                    spans multiple lines
fn parse_package_detail(output: &str) -> PackageDetail {
    let mut fields: Vec<(String, String)> = Vec::new();

    for line in output.lines() {
        if let Some(colon_pos) = line.find(':') {
            let key_part = &line[..colon_pos];
            let value_part = line[colon_pos + 1..].trim();

            // 如果 key 部分不以空格开头，且包含非空字符，则是新字段
            if !key_part.starts_with(' ') || key_part.trim().is_empty() {
                let key = key_part.trim();
                if !key.is_empty() {
                    fields.push((key.to_string(), value_part.to_string()));
                    continue;
                }
            }
        }
        // 续行（以空格开头，无冒号分隔的 key）
        if (line.starts_with(' ') || line.starts_with('\t')) && !fields.is_empty() {
            let last = fields.last_mut().unwrap();
            last.1.push(' ');
            last.1.push_str(line.trim());
        }
    }

    PackageDetail {
        fields,
    }
}
