mod config;
mod deepseek;
mod package_manager;
mod prompt;
mod report;
mod sysinfo;
mod tui;

use anyhow::Result;
use std::process::Command;

/// 预先验证 sudo 权限，确保 TUI 运行时不需要交互输入密码
fn validate_sudo() -> Result<()> {
    println!("🔐 验证 sudo 权限...");
    
    let status = Command::new("sudo")
        .arg("-v")
        .status()?;
    
    if !status.success() {
        anyhow::bail!("sudo 验证失败，请确保你有 sudo 权限");
    }
    
    println!("✅ sudo 验证成功！");
    println!();
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // 加载配置
    let config = config::Config::load_or_default()?;

    // API Key 优先级：配置文件 > 环境变量
    let api_key = config.api_key.clone()
        .or_else(|| std::env::var("LIAN_AI_KEY").ok())
        .unwrap_or_else(|| {
            eprintln!("错误: 未设置 AI API Key");
            eprintln!("请在配置文件 ~/.config/lian/config.toml 中设置 api_key");
            eprintln!("或设置环境变量: export LIAN_AI_KEY='your-api-key'");
            std::process::exit(1);
        });

    validate_sudo()?;

    tui::run(api_key, config).await?;

    Ok(())
}
