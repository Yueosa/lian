use anyhow::{anyhow, Result};
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

    /// 获取当前已安装的显式安装包列表
    pub fn get_explicit_packages(&self) -> Result<String> {
        let output = Command::new("pacman")
            .args(["-Qe"])
            .output()?;
        if !output.status.success() {
            anyhow::bail!("pacman -Qe 执行失败");
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// 执行系统更新命令（流式输出）
    pub fn update_streaming(&self, output_tx: mpsc::UnboundedSender<String>) -> Result<UpdateOutput> {
        reset_cancel();  // 重置取消标志
        
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
                
                use std::io::Read;
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
                
                use std::io::Read;
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
        CHILD_PID.store(0, Ordering::SeqCst);  // 清除 PID

        Ok(UpdateOutput {
            stdout: all_stdout,
            stderr: all_stderr,
            success: status.success(),
        })
    }
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
