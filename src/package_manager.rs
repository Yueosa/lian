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

    /// 模拟更新输出（测试模式）
    pub fn mock_update(&self, output_tx: mpsc::UnboundedSender<String>) -> Result<UpdateOutput> {
        use std::thread::sleep;
        use std::time::Duration;

        let mock_output = vec![
            // 同步数据库
            (":: Synchronizing package databases...", 80),
            (" core                                    132.8 KiB  1524 KiB/s 00:00", 60),
            (" extra                                     8.5 MiB  4.12 MiB/s 00:02", 120),
            (" multilib                                142.9 KiB  1842 KiB/s 00:00", 60),
            (" archlinuxcn                             1024.3 KiB  2.1 MiB/s 00:00", 80),
            ("", 30),
            
            // AUR 检查
            (":: Searching AUR for updates...", 200),
            (" -> Found 3 AUR package updates", 100),
            ("", 30),
            
            // 依赖解析
            (":: Starting full system upgrade...", 150),
            (":: Resolving dependencies...", 200),
            (":: Looking for conflicting packages...", 150),
            ("", 30),
            
            // 官方仓库包列表
            ("Repo (12) alsa-lib-1.2.12-1  firefox-134.0.1-1  hyprland-0.45.0-1", 50),
            ("         hyprutils-0.3.0-1  kitty-0.38.1-1  lib32-mesa-24.3.2-1", 50),
            ("         linux-6.12.4.arch1-1  linux-headers-6.12.4.arch1-1", 50),
            ("         mesa-24.3.2-1  nvidia-dkms-565.77.01-2  nvidia-utils-565.77.01-2", 50),
            ("         python-3.12.8-1", 50),
            ("", 30),
            
            // AUR 包列表
            ("Aur (3)  visual-studio-code-bin-1.96.2-1  wechat-universal-bwrap-1.0.0.238-1", 50),
            ("         yay-bin-12.4.3-1", 50),
            ("", 30),
            
            // 下载大小
            ("Total Download Size:    412.85 MiB", 80),
            ("Total Installed Size:  1285.42 MiB", 50),
            ("Net Upgrade Size:        45.67 MiB", 50),
            ("", 50),
            
            // 确认
            (":: Proceed with installation? [Y/n]", 80),
            ("", 30),
            
            // 下载官方包
            (":: Retrieving packages...", 150),
            (" linux-6.12.4.arch1-1-x86_64 downloading...", 80),
            (" linux-6.12.4.arch1-1-x86_64             142.3 MiB  5.2 MiB/s 00:27", 150),
            (" linux-headers-6.12.4.arch1-1-x86_64      38.7 MiB  4.8 MiB/s 00:08", 100),
            (" nvidia-dkms-565.77.01-2-x86_64           42.1 MiB  5.1 MiB/s 00:08", 100),
            (" nvidia-utils-565.77.01-2-x86_64          35.8 MiB  4.9 MiB/s 00:07", 100),
            (" hyprland-0.45.0-1-x86_64                 12.4 MiB  5.3 MiB/s 00:02", 80),
            (" mesa-24.3.2-1-x86_64                     28.9 MiB  5.0 MiB/s 00:06", 90),
            (" lib32-mesa-24.3.2-1-x86_64               18.2 MiB  4.7 MiB/s 00:04", 80),
            (" firefox-134.0.1-1-x86_64                 78.5 MiB  5.4 MiB/s 00:15", 120),
            (" python-3.12.8-1-x86_64                   18.3 MiB  5.1 MiB/s 00:04", 80),
            (" kitty-0.38.1-1-x86_64                     5.2 MiB  4.2 MiB/s 00:01", 60),
            (" hyprutils-0.3.0-1-x86_64                  0.1 MiB  2.1 MiB/s 00:00", 50),
            (" alsa-lib-1.2.12-1-x86_64                  0.5 MiB  3.5 MiB/s 00:00", 50),
            ("", 30),
            
            // 校验
            ("(12/12) checking keys in keyring                    [######################] 100%", 80),
            ("(12/12) checking package integrity                  [######################] 100%", 80),
            ("(12/12) loading package files                       [######################] 100%", 80),
            ("(12/12) checking for file conflicts                 [######################] 100%", 100),
            ("(12/12) checking available disk space               [######################] 100%", 80),
            ("", 30),
            
            // 安装官方包
            (":: Processing package changes...", 150),
            ("( 1/12) upgrading alsa-lib                          [######################] 100%", 80),
            ("( 2/12) upgrading linux                             [######################] 100%", 150),
            ("( 3/12) upgrading linux-headers                     [######################] 100%", 120),
            ("( 4/12) upgrading nvidia-dkms                       [######################] 100%", 100),
            ("", 30),
            
            // DKMS 构建
            ("==> dkms install --no-depmod nvidia/565.77.01 -k 6.12.4-arch1-1", 150),
            ("==> Running DKMS build for nvidia 565.77.01...", 100),
            ("  -> Building module nvidia...", 300),
            ("  -> Building module nvidia-modeset...", 200),
            ("  -> Building module nvidia-drm...", 200),
            ("  -> Building module nvidia-uvm...", 200),
            ("  -> Building module nvidia-peermem...", 150),
            ("==> DKMS install completed.", 100),
            ("", 30),
            
            ("( 5/12) upgrading nvidia-utils                      [######################] 100%", 100),
            ("( 6/12) upgrading mesa                              [######################] 100%", 100),
            ("( 7/12) upgrading lib32-mesa                        [######################] 100%", 80),
            ("( 8/12) upgrading hyprutils                         [######################] 100%", 60),
            ("( 9/12) upgrading hyprland                          [######################] 100%", 100),
            ("(10/12) upgrading python                            [######################] 100%", 100),
            ("(11/12) upgrading firefox                           [######################] 100%", 120),
            ("(12/12) upgrading kitty                             [######################] 100%", 80),
            ("", 30),
            
            // Post-transaction hooks
            (":: Running post-transaction hooks...", 150),
            ("(1/8) Arming ConditionNeedsUpdate...", 60),
            ("(2/8) Updating module dependencies...", 100),
            ("(3/8) Updating linux initcpios...", 200),
            ("==> Building image from preset: /etc/mkinitcpio.d/linux.preset: 'default'", 100),
            ("==> Using configuration file: '/etc/mkinitcpio.conf'", 50),
            ("  -> -k /boot/vmlinuz-linux -g /boot/initramfs-linux.img", 150),
            ("==> Building image from preset: /etc/mkinitcpio.d/linux.preset: 'fallback'", 80),
            ("  -> -k /boot/vmlinuz-linux -g /boot/initramfs-linux-fallback.img -S autodetect", 200),
            ("(4/8) Updating icon theme caches...", 80),
            ("(5/8) Updating the MIME type database...", 60),
            ("(6/8) Updating desktop file MIME type cache...", 60),
            ("(7/8) Updating the info directory file...", 50),
            ("(8/8) Reloading system bus configuration...", 60),
            ("", 50),
            
            // 开始构建 AUR 包
            (":: Building AUR packages...", 200),
            ("", 30),
            
            // 构建 visual-studio-code-bin
            ("==> Making package: visual-studio-code-bin 1.96.2-1", 100),
            ("==> Retrieving sources...", 80),
            ("  -> Downloading code-stable-x64-1.96.2.tar.gz...", 100),
            ("  % Total    % Received % Xferd  Average Speed   Time    Time     Time  Current", 50),
            ("                                 Dload  Upload   Total   Spent    Left  Speed", 50),
            ("100  142M  100  142M    0     0  8.5M/s  0:00:16  0:00:16 --:--:-- 9.1M/s", 300),
            ("==> Validating source files with sha256sums...", 80),
            ("    code-stable-x64-1.96.2.tar.gz ... Passed", 60),
            ("==> Extracting sources...", 150),
            ("  -> Extracting code-stable-x64-1.96.2.tar.gz...", 200),
            ("==> Entering fakeroot environment...", 80),
            ("==> Starting package()...", 100),
            ("==> Tidying install...", 80),
            ("  -> Removing libtool files...", 30),
            ("  -> Purging unwanted files...", 30),
            ("  -> Compressing man and info pages...", 50),
            ("==> Checking for packaging issues...", 80),
            ("==> Creating package \"visual-studio-code-bin\"...", 150),
            ("  -> Generating .PKGINFO file...", 50),
            ("  -> Generating .BUILDINFO file...", 50),
            ("  -> Generating .MTREE file...", 80),
            ("  -> Compressing package...", 200),
            ("==> Leaving fakeroot environment.", 50),
            ("==> Finished making: visual-studio-code-bin 1.96.2-1", 80),
            ("", 30),
            
            // 构建 wechat-universal-bwrap
            ("==> Making package: wechat-universal-bwrap 1.0.0.238-1", 100),
            ("==> Retrieving sources...", 80),
            ("  -> Found wechat-universal_1.0.0.238_amd64.deb in cache", 60),
            ("==> Validating source files with sha256sums...", 80),
            ("    wechat-universal_1.0.0.238_amd64.deb ... Passed", 60),
            ("==> Extracting sources...", 100),
            ("==> Entering fakeroot environment...", 80),
            ("==> Starting package()...", 100),
            ("==> Tidying install...", 80),
            ("==> Checking for packaging issues...", 80),
            ("==> Creating package \"wechat-universal-bwrap\"...", 150),
            ("  -> Compressing package...", 150),
            ("==> Leaving fakeroot environment.", 50),
            ("==> Finished making: wechat-universal-bwrap 1.0.0.238-1", 80),
            ("", 30),
            
            // 构建 yay-bin
            ("==> Making package: yay-bin 12.4.3-1", 100),
            ("==> Retrieving sources...", 80),
            ("  -> Downloading yay_12.4.3_x86_64.tar.gz...", 80),
            ("  % Total    % Received % Xferd  Average Speed   Time    Time     Time  Current", 50),
            ("100 5.2M  100 5.2M    0     0  12.1M/s  0:00:00  0:00:00 --:--:-- 12.1M/s", 100),
            ("==> Validating source files with sha256sums...", 60),
            ("    yay_12.4.3_x86_64.tar.gz ... Passed", 50),
            ("==> Extracting sources...", 80),
            ("==> Entering fakeroot environment...", 60),
            ("==> Starting package()...", 80),
            ("==> Tidying install...", 60),
            ("==> Checking for packaging issues...", 60),
            ("==> Creating package \"yay-bin\"...", 100),
            ("  -> Compressing package...", 80),
            ("==> Leaving fakeroot environment.", 40),
            ("==> Finished making: yay-bin 12.4.3-1", 60),
            ("", 30),
            
            // 安装 AUR 包
            (":: Installing AUR packages...", 150),
            ("(1/3) installing visual-studio-code-bin             [######################] 100%", 100),
            ("(2/3) installing wechat-universal-bwrap             [######################] 100%", 100),
            ("(3/3) installing yay-bin                            [######################] 100%", 80),
            ("", 30),
            
            // 完成
            (":: Cleaning up...", 100),
            ("", 30),
            (" -> Transaction completed successfully.", 80),
            ("", 30),
            ("==> WARNING: 内核已更新 (linux 6.12.4.arch1-1)，请重启系统！", 100),
            ("==> WARNING: NVIDIA 驱动已更新，请重新登录以应用显卡更改。", 100),
            ("==> WARNING: Hyprland 已更新，建议重新登录。", 80),
        ];

        let mut all_output = String::new();

        for (line, delay_ms) in mock_output {
            if should_cancel() {
                return Ok(UpdateOutput {
                    stdout: all_output,
                    stderr: "测试已取消".to_string(),
                    success: false,
                });
            }
            
            let _ = output_tx.send(line.to_string());
            all_output.push_str(line);
            all_output.push('\n');
            sleep(Duration::from_millis(delay_ms));
        }

        Ok(UpdateOutput {
            stdout: all_output,
            stderr: String::new(),
            success: true,
        })
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
