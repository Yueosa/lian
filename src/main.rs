mod config;
mod deepseek;
mod package_manager;
mod prompt;
mod report;
mod sysinfo;
mod tui;

use anyhow::Result;

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

    tui::run(api_key, config).await?;

    Ok(())
}
