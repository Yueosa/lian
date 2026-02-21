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
///
/// 锁文件归 root 所有，普通用户无写权限，必须通过 `sudo -n rm -f` 删除。
/// `-n`（non-interactive）保证若 sudo 凭证已过期时静默失败，不阻塞 TUI。
fn try_remove_db_lock() {
    let lock_path = "/var/lib/pacman/db.lck";
    if !std::path::Path::new(lock_path).exists() {
        return;
    }
    // 确认没有其他 pacman 进程持有锁
    let any_pacman = std::process::Command::new("pgrep")
        .args(["-x", "pacman"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !any_pacman {
        // 锁文件属于 root，必须借助 sudo 删除；-n 保证非交互静默失败
        let status = std::process::Command::new("sudo")
            .args(["-n", "rm", "-f", lock_path])
            .status();
        if let Ok(s) = status {
            if !s.success() {
                log::warn!("try_remove_db_lock: sudo rm -f {} 失败（exit={:?}）", lock_path, s.code());
            }
        }
    }
}

/// 全局变量用于存储当前运行的子进程 PID
static CHILD_PID: AtomicU32 = AtomicU32::new(0);
static SHOULD_CANCEL: AtomicBool = AtomicBool::new(false);

/// 请求取消当前正在运行的包管理器操作。
///
/// 信号阶梯（均针对整个进程组 -pgid）：
///   1. SIGINT  — 模拟用户 Ctrl+C，pacman 的信号处理器会在退出前自动清理 db.lck
///   2. SIGTERM — 若 5 秒内进程未退出则发送，强制终止
///   3. SIGKILL — 若再等 1 秒仍未退出则发送，最后手段
///
/// 当 CHILD_PID 尚未被工作线程写入（进程刚在启动窗口期）时，
/// 最多等待 500 ms（10×50ms）让 PID 就绪；若超时仍为 0 则仅
/// 依靠 SHOULD_CANCEL 标志让工作线程在读取流时自行退出。
pub fn cancel_update() {
    SHOULD_CANCEL.store(true, Ordering::SeqCst);

    // 处理竞争窗口：进程可能正在启动，PID 尚未存入
    let mut pid = CHILD_PID.load(Ordering::SeqCst);
    if pid == 0 {
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            pid = CHILD_PID.load(Ordering::SeqCst);
            if pid != 0 {
                break;
            }
        }
    }

    if pid != 0 {
        unsafe {
            // 第一步：SIGINT — 让 pacman 执行正常的信号处理退出流程（会自行删除锁文件）
            libc::kill(-(pid as i32), libc::SIGINT);
        }
        // 在后台线程中等待，超时后逐步升级信号，以免阻塞 TUI
        std::thread::spawn(move || {
            // 等待 pacman 响应 SIGINT（最多 5 秒）
            let sigterm_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let still_running = unsafe { libc::kill(-(pid as i32), 0) == 0 };
                if !still_running {
                    break;
                }
                if std::time::Instant::now() >= sigterm_deadline {
                    // 第二步：SIGTERM
                    unsafe { libc::kill(-(pid as i32), libc::SIGTERM); }
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                    // 第三步：SIGKILL（最后手段）
                    if unsafe { libc::kill(-(pid as i32), 0) == 0 } {
                        unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                    break;
                }
            }
            unsafe { libc::waitpid(-(pid as i32), std::ptr::null_mut(), libc::WNOHANG); }
            CHILD_PID.store(0, Ordering::SeqCst);
            // 进程退出后尝试清理可能残留的锁文件（SIGKILL 时 pacman 无法自清）
            try_remove_db_lock();
        });
    }
}

/// 重置取消标志
pub fn reset_cancel() {
    SHOULD_CANCEL.store(false, Ordering::SeqCst);
    CHILD_PID.store(0, Ordering::SeqCst);
}

/// 清理残留子进程（退出应用时调用）。
///
/// 与 cancel_update 相同的信号阶梯：SIGINT → SIGTERM → SIGKILL。
/// 此函数在主线程同步等待进程退出（阻塞 TUI 退出流程可接受）。
pub fn cleanup_child_processes() {
    let pid = CHILD_PID.load(Ordering::SeqCst);
    if pid != 0 {
        unsafe {
            // 首先发 SIGINT，让 pacman 有机会自行清理锁文件
            libc::kill(-(pid as i32), libc::SIGINT);
        }
        // 等待进程响应 SIGINT（最多 5 秒）
        let sigterm_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let still_running = unsafe { libc::kill(-(pid as i32), 0) == 0 };
            if !still_running {
                break;
            }
            if std::time::Instant::now() >= sigterm_deadline {
                // 升级为 SIGTERM
                unsafe { libc::kill(-(pid as i32), libc::SIGTERM); }
                std::thread::sleep(std::time::Duration::from_millis(1000));
                // 最后手段：SIGKILL
                if unsafe { libc::kill(-(pid as i32), 0) == 0 } {
                    unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
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
    // 无论进程是否存在，检查并清理残留锁文件
    try_remove_db_lock();
}

/// 检查是否应该取消
fn should_cancel() -> bool {
    SHOULD_CANCEL.load(Ordering::SeqCst)
}

/// 从流中读取行并发送到 channel（通用辅助函数）
/// `\n` 行正常发送，stderr 加 `⚠ ` 前缀；
/// `\r` 行（pacman/paru 下载进度条）统一以 "PROGRESS:" 前缀发送，无论来自 stdout 还是 stderr。
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
                        // pacman/paru/yay 的下载进度条和 AUR 编译进度通过 \r 就地刷新
                        // 无论来自 stdout 还是 stderr，都作为进度行发送，不追加到日志
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
///
/// 注意：**不再在此处调用 `reset_cancel()`**。
/// 调用方（`spawn_update_task` / `spawn_install_task` / `spawn_remove_task`）
/// 必须在启动工作线程之前于 UI 线程中调用 `reset_cancel()`，
/// 以避免工作线程内部的重置覆盖用户在启动窗口期已经设置的取消标志。
fn run_streaming_command(
    pm: &PackageManager,
    pacman_args: &[&str],
    aur_args: &[&str],
    extra_packages: &[String],
    output_tx: mpsc::UnboundedSender<String>,
    cancel_label: &str,
) -> Result<UpdateOutput> {
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

    let child_pid = child.id();
    CHILD_PID.store(child_pid, Ordering::SeqCst);

    // 处理竞争窗口：用户可能在 PID 存入前就已调用 cancel_update() 设置了取消标志，
    // 此时需要补发 SIGINT 让 pacman 有机会自行清理锁文件。
    if should_cancel() {
        unsafe { libc::kill(-(child_pid as i32), libc::SIGINT); }
    }

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
        // 等待子进程退出（最多 6 秒），此时 SIGINT/SIGTERM 已在 cancel_update 中发出；
        // 超时才升级为 SIGKILL（最后手段，不触发 pacman 清理）
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
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
        // SIGKILL 情况下 pacman 无法自清，兜底删除锁文件
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

    // 注意：reset_cancel() 由调用方（UI 线程）在 spawn 前调用，此处不重置。

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

    let child_pid = child.id();
    CHILD_PID.store(child_pid, Ordering::SeqCst);

    // 补发 SIGINT 处理竞争窗口
    if should_cancel() {
        unsafe { libc::kill(-(child_pid as i32), libc::SIGINT); }
    }

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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
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
