use std::process::Command;

/// 系统环境信息，用于注入 AI 提示词
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub kernel: String,
    pub distro: String,
    pub gpu: String,
    pub desktop: String,
    pub display_protocol: String,
    pub cpu: String,
    pub memory: String,
}

impl SystemInfo {
    /// 自动检测系统环境信息
    pub fn detect() -> Self {
        Self {
            kernel: Self::get_kernel(),
            distro: Self::get_distro(),
            gpu: Self::get_gpu(),
            desktop: Self::get_desktop(),
            display_protocol: Self::get_display_protocol(),
            cpu: Self::get_cpu(),
            memory: Self::get_memory(),
        }
    }

    /// 格式化为提示词片段
    pub fn to_prompt_section(&self) -> String {
        format!(
            "## 系统环境信息\n\
                - 发行版: {}\n\
                - 内核: {}\n\
                - CPU: {}\n\
                - 内存: {}\n\
                - 显卡: {}\n\
                - 桌面环境: {}\n\
                - 显示协议: {}",
            self.distro,
            self.kernel,
            self.cpu,
            self.memory,
            self.gpu,
            self.desktop,
            self.display_protocol,
        )
    }

    fn run_cmd(cmd: &str, args: &[&str]) -> String {
        Command::new(cmd)
            .args(args)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "未知".to_string())
    }

    fn run_shell(cmd: &str) -> String {
        Command::new("sh")
            .args(["-c", cmd])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() { None } else { Some(s) }
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "未知".to_string())
    }

    fn get_kernel() -> String {
        Self::run_cmd("uname", &["-r"])
    }

    fn get_distro() -> String {
        // 解析 PRETTY_NAME="Arch Linux"
        let raw = Self::run_shell("grep PRETTY_NAME /etc/os-release");
        raw.split('=')
            .nth(1)
            .map(|s| s.trim_matches('"').to_string())
            .unwrap_or(raw)
    }

    fn get_gpu() -> String {
        // lspci 输出形如: 00:02.0 VGA compatible controller: Intel Corporation ...
        let raw = Self::run_shell("lspci | grep -i vga");
        // 提取冒号后面的实际设备名
        raw.lines()
            .filter_map(|line| {
                line.split(": ").nth(1).map(|s| s.trim().to_string())
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn get_desktop() -> String {
        // 优先 XDG_CURRENT_DESKTOP，备选 DESKTOP_SESSION
        std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("DESKTOP_SESSION"))
            .unwrap_or_else(|_| "未知".to_string())
    }

    fn get_display_protocol() -> String {
        std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_else(|_| "未知".to_string())
    }

    fn get_cpu() -> String {
        // 兼容中英文 locale: "Model name" 或 "型号名称"
        let raw = Self::run_shell(
            "lscpu | grep -iE 'Model name|型号名称' | head -1"
        );
        // 提取冒号后的值
        raw.split(':')
            .nth(1)
            .map(|s| s.trim().to_string())
            .unwrap_or(raw)
    }

    fn get_memory() -> String {
        // 兼容中英文 locale: "Mem:" 或 "内存："
        let raw = Self::run_shell(
            "free -h | grep -iE '^Mem|^内存'"
        );
        // 解析 free 输出获取总量和可用量
        let parts: Vec<&str> = raw.split_whitespace().collect();
        if parts.len() >= 3 {
            // parts[1] = 总量, parts[2] = 已用, parts[6] (或最后) = 可用
            let total = parts[1];
            let used = parts[2];
            format!("{} (已用 {})", total, used)
        } else {
            raw
        }
    }
}
