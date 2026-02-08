//! 流式命令执行（update / install / remove）

use super::parser::clean_terminal_output;
use super::types::UpdateOutput;
use super::PackageManager;
use anyhow::Result;
use std::io::Read;
use std::process::{Command, Stdio};
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
        unsafe {
            // 发送 SIGTERM 到整个进程组（sudo + pacman + 所有子进程）
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
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
            // 先发 SIGTERM 让 pacman 正常清理锁文件
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
        // 给 pacman 时间清理锁文件
        std::thread::sleep(std::time::Duration::from_millis(800));
        unsafe {
            // 仍未退出则强制杀死
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
        // 回收僵尸进程
        unsafe {
            libc::waitpid(-(pid as i32), std::ptr::null_mut(), libc::WNOHANG);
        }
        CHILD_PID.store(0, Ordering::SeqCst);
        SHOULD_CANCEL.store(false, Ordering::SeqCst);
    }
}

/// 检查是否应该取消
fn should_cancel() -> bool {
    SHOULD_CANCEL.load(Ordering::SeqCst)
}

/// 从流中读取行并发送到 channel（通用辅助函数）
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
        // 给 pacman 时间处理 SIGTERM 并清理锁文件
        std::thread::sleep(std::time::Duration::from_millis(800));
        // 如果进程仍然存活，强制杀死整个进程组
        let pid = CHILD_PID.load(Ordering::SeqCst);
        if pid != 0 {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        let _ = child.wait();
        CHILD_PID.store(0, Ordering::SeqCst);
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
