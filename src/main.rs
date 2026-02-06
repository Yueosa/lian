mod config;
mod deepseek;
mod package_manager;
mod prompt;
mod report;
mod tui;

use anyhow::Result;
use std::env;
use std::process::Command;

/// 预先验证 sudo 权限，确保 TUI 运行时不需要交互输入密码
fn validate_sudo() -> Result<()> {
    println!("🔐 验证 sudo 权限...");
    println!("   (paru/yay/pacman 更新需要 sudo 权限)");
    println!();
    
    // 运行 sudo -v 来验证/刷新 sudo 凭据
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

    // 检查是否为测试模式
    let args: Vec<String> = env::args().collect();
    let test_mode = args.iter().any(|a| a == "--test" || a == "-t");

    // 检查 API key
    let api_key = env::var("DEEPSEEK_API_KEY").unwrap_or_else(|_| {
        eprintln!("错误: 未设置 DEEPSEEK_API_KEY 环境变量");
        eprintln!("请运行: export DEEPSEEK_API_KEY='your-api-key'");
        std::process::exit(1);
    });

    // 加载配置
    let config = config::Config::load_or_default()?;

    if test_mode {
        println!("🧪 测试模式：将模拟更新输出");
        println!();
    } else {
        // 预先验证 sudo 权限
        validate_sudo()?;
    }

    // 启动 TUI
    tui::run(api_key, config, test_mode).await?;

    Ok(())
}
