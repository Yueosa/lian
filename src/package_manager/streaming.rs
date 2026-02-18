//! 流式命令执行（update / install / remove）

use super::parser::clean_terminal_output;
use super::types::UpdateOutput;
use super::PackageManager;
use anyhow::Result;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use tokio::sync::mpsc;

/// 尝试删除 pacman db.lck（仅在确认没有 pacman 进程在运行时调用）
fn try_remove_db_lock() {
    let lock_paths = [
        "/var/lib/pacman/db.lck",
    ];
    for path in &lock_paths {
        if std::path::Path::new(path).exists() {
            // 检查是否有其他 pacman 进程在运行
            let any_pacman = std::process::Command::new("pgrep")
                .args(["-x", "pacman"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !any_pacman {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// 全局变量用于存储当前运行的子进程 PID
static CHILD_PID: AtomicU32 = AtomicU32::new(0);
static SHOULD_CANCEL: AtomicBool = AtomicBool::new(false);

/// 设置取消标志
pub fn cancel_update() {
    SHOULD_CANCEL.store(true, Ordering::SeqCst);
    let pid = CHILD_PID.load(Ordering::SeqCst);
    if pid != 0 {
        unsafe {
            // 先发 SIGTERM 到整个进程组，让 pacman 有机会清理锁文件
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
        // 在后台线程中等待进程退出，超时后 SIGKILL，以免阻塞 TUI
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let still_running = unsafe { libc::kill(-(pid as i32), 0) == 0 };
                if !still_running {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    break;
                }
            }
            unsafe { libc::waitpid(-(pid as i32), std::ptr::null_mut(), libc::WNOHANG); }
            CHILD_PID.store(0, Ordering::SeqCst);
            // 进程退出后再尝试清理残留的锁文件
            try_remove_db_lock();
        });
    }
}

/// 重置取消标志
pub fn reset_cancel() {
    SHOULD_CANCEL.store(false, Ordering::SeqCst);
    CHILD_PID.store(0, Ordering::SeqCst);
}

/// 清理残留子进程（退出应用时调用）
pub fn cleanup_child_processes() {
    let pid = CHILD_PID.load(Ordering::SeqCst);
    if pid != 0 {
        unsafe {
            // 先发 SIGTERM 让 pacman/paru 正常退出并自行清理锁文件
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
        // 等待进程自然退出（最多 3 秒），期间每 100ms 检查一次
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let still_running = unsafe {
                libc::kill(-(pid as i32), 0) == 0
            };
            if !still_running {
                break;
            }
            if std::time::Instant::now() >= deadline {
                // 超时：强制杀死整个进程组
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
                break;
            }
        }
        // 回收僵尸进程
        unsafe {
            libc::waitpid(-(pid as i32), std::ptr::null_mut(), libc::WNOHANG);
        }
        CHILD_PID.store(0, Ordering::SeqCst);
        SHOULD_CANCEL.store(false, Ordering::SeqCst);
    }
    // 如果进程已不存在但锁文件还在，安全地尝试删除
    try_remove_db_lock();
}

/// 检查是否应该取消
fn should_cancel() -> bool {
    SHOULD_CANCEL.load(Ordering::SeqCst)
}

/// 从流中读取行并发送到 channel（通用辅助函数）
/// `\n` 行正常发送；`\r` 行（pacman 下载进度条）以 "PROGRESS:" 前缀发送，
/// 接收方可用来刷新进度显示而非追加到日志。
fn read_stream_lines(
    stream: Option<impl Read>,
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
                        // pacman 下载进度条通过 \r 就地刷新，以特殊前缀发送供 TUI 显示进度
                        let cleaned = clean_terminal_output(&line_buffer);
                        if !cleaned.trim().is_empty() {
                            let _ = tx.send(format!("PROGRESS:{}", cleaned));
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

/// 通用的流式命令执行框架
fn run_streaming_command(
    pm: &PackageManager,
    pacman_args: &[&str],
    aur_args: &[&str],
    extra_packages: &[String],
    output_tx: mpsc::UnboundedSender<String>,
    cancel_label: &str,
) -> Result<UpdateOutput> {
    reset_cancel();

    use std::os::unix::process::CommandExt;

    let mut child = if pm.command == "pacman" {
        let mut cmd = Command::new("sudo");
        let mut args: Vec<&str> = vec!["pacman"];
        args.extend_from_slice(pacman_args);
        let pkg_refs: Vec<&str> = extra_packages.iter().map(|s| s.as_str()).collect();
        args.extend(pkg_refs);
        cmd.args(&args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        unsafe {
            cmd.pre_exec(|| {
                // 创建独立进程组，方便统一杀死 sudo + pacman 整棵进程树
                libc::setpgid(0, 0);
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        cmd.spawn()?
    } else {
        let mut cmd = Command::new(&pm.command);
        let mut args: Vec<String> = aur_args.iter().map(|s| s.to_string()).collect();
        args.extend(extra_packages.iter().cloned());
        cmd.args(&args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        unsafe {
            cmd.pre_exec(|| {
                libc::setpgid(0, 0);
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

    let stderr_handle =
        std::thread::spawn(move || read_stream_lines(stderr, &output_tx, true));

    let all_stdout = stdout_handle.join().unwrap_or_default();
    let all_stderr = stderr_handle.join().unwrap_or_default();

    let cancelled = should_cancel();
    if cancelled {
        // 等待子进程自然退出（最多 3 秒），确保 pacman 有机会清理锁文件
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break, // 进程已退出
                _ => {}
            }
            if std::time::Instant::now() >= deadline {
                let pid = CHILD_PID.load(Ordering::SeqCst);
                if pid != 0 {
                    unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
                }
                let _ = child.wait();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        CHILD_PID.store(0, Ordering::SeqCst);
        try_remove_db_lock();
        return Ok(UpdateOutput {
            stdout: all_stdout,
            stderr: format!("{}已取消", cancel_label),
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

impl PackageManager {
    /// 执行系统更新命令（流式输出）
    pub fn update_streaming(
        &self,
        output_tx: mpsc::UnboundedSender<String>,
    ) -> Result<UpdateOutput> {
        run_streaming_command(
            self,
            &["-Syu", "--noconfirm"],
            &["-Syu", "--noconfirm"],
            &[],
            output_tx,
            "更新",
        )
    }

    /// 执行安装命令（流式输出）
    pub fn install_streaming(
        &self,
        packages: &[String],
        output_tx: mpsc::UnboundedSender<String>,
    ) -> Result<UpdateOutput> {
        run_streaming_command(
            self,
            &["-S", "--noconfirm"],
            &["-S", "--noconfirm"],
            packages,
            output_tx,
            "安装",
        )
    }

    /// 执行卸载命令（流式输出）
    pub fn remove_streaming(
        &self,
        packages: &[String],
        output_tx: mpsc::UnboundedSender<String>,
    ) -> Result<UpdateOutput> {
        run_streaming_command(
            self,
            &["-Rns", "--noconfirm"],
            &["-Rns", "--noconfirm"],
            packages,
            output_tx,
            "卸载",
        )
    }
}

/// 执行任意自定义 shell 命令（流式输出）
/// cmd_parts: 命令及参数列表，例如 ["sudo", "pacman", "-Scc"]
pub fn run_custom_command_streaming(
    cmd_parts: Vec<String>,
    output_tx: mpsc::UnboundedSender<String>,
) -> Result<UpdateOutput> {
    if cmd_parts.is_empty() {
        anyhow::bail!("命令不能为空");
    }

    reset_cancel();

    let program = &cmd_parts[0];
    let args = &cmd_parts[1..];

    use std::os::unix::process::CommandExt;

    let mut child = Command::new(program);
    child.args(args);
    child.stdout(Stdio::piped());
    child.stderr(Stdio::piped());
    unsafe {
        child.pre_exec(|| {
            libc::setpgid(0, 0);
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let mut child = child.spawn()?;

    CHILD_PID.store(child.id(), Ordering::SeqCst);

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let output_tx_clone = output_tx.clone();
    let stdout_handle =
        std::thread::spawn(move || read_stream_lines(stdout, &output_tx_clone, false));
    let stderr_handle =
        std::thread::spawn(move || read_stream_lines(stderr, &output_tx, true));

    let all_stdout = stdout_handle.join().unwrap_or_default();
    let all_stderr = stderr_handle.join().unwrap_or_default();

    let cancelled = should_cancel();
    if cancelled {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                _ => {}
            }
            if std::time::Instant::now() >= deadline {
                let pid = CHILD_PID.load(Ordering::SeqCst);
                if pid != 0 {
                    unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
                }
                let _ = child.wait();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        CHILD_PID.store(0, Ordering::SeqCst);
        try_remove_db_lock();
        return Ok(UpdateOutput {
            stdout: all_stdout,
            stderr: "命令已取消".to_string(),
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
