<div align="center">

# lian 🤖📦

基于 Rust 的智能 TUI 包管理器，支持 Arch Linux 及其衍生发行版，集成 AI 分析。

</div>

> 💡 **项目说明**
>
> 本项目是对 Claude AI 能力的探索实验. 这是作者第一次使用 Claude.
> - **项目框架** - 由 Claude Sonnet 4.5 生成
> - **Bug 修复与最终发布** - 由 Claude Opus 4.6 完成
>
> 感谢 Claude 在每个环节的支持！

## ✨ 特性

- 🎯 **智能检测** - 自动检测包管理器 (paru → yay → pacman)
- 🖥️ **精美 TUI** - 基于 ratatui 的终端界面
- 🤖 **AI 分析** - AI 深度分析更新内容（默认 DeepSeek，支持自定义 API）
- 🔍 **系统感知** - 自动检测系统环境，AI 分析更有针对性
- 📊 **分类整理** - 按类型分类（内核、系统、驱动、应用等）
- ⚠️ **风险提示** - 针对关键组件的更新警告
- 💾 **自动存档** - 报告保存到 `~/.lian/pacman/YYYY/MM/DD/`

## 🚀 安装

### 前置要求

- Arch Linux (或衍生发行版，如 Manjaro、EndeavourOS、CachyOS 等)
- [DeepSeek API Key](https://platform.deepseek.com/api_keys) 或其他兼容 OpenAI 格式的 API

### 方法一：下载预编译版本

```bash
# 从 GitHub Releases 下载
# https://github.com/Yueosa/lian-pacman/releases

chmod +x lian_*_linux_x86_64
sudo mv lian_*_linux_x86_64 /usr/local/bin/lian
```

### 方法二：从源码编译

```bash
# 安装 Rust (如果没有)
paru -S rust
# 或: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 编译安装
cd lian-pacman
cargo build --release
sudo cp target/release/lian /usr/local/bin/
```

## ⚙️ 配置

### 设置 API Key

两种方式任选其一（配置文件优先级更高）：

**方式一：环境变量**

```bash
echo 'export LIAN_AI_KEY="sk-your-key-here"' >> ~/.zshrc
source ~/.zshrc
```

**方式二：配置文件**

在 `~/.config/lian/config.toml` 中设置 `api_key` 字段（见下方）。

### 配置文件 (可选)

创建 `~/.config/lian/config.toml`：

```toml
# AI API 地址 (可选，默认 DeepSeek，兼容 OpenAI 格式)
# api_url = "https://api.deepseek.com/chat/completions"

# AI API Key (可选，优先级高于环境变量)
# api_key = "sk-your-key-here"

# HTTP 代理 (可选，支持 http/https/socks5)
# proxy = "http://127.0.0.1:7890"

# AI 模型: "deepseek-chat" (快速) 或 "deepseek-reasoner" (深度分析)
model = "deepseek-reasoner"

# Temperature: 0.0-1.5，推荐 0.8
temperature = 0.8

# 报告保存目录
report_dir = "/home/your-username/.lian/pacman"
```

## 📖 使用

```bash
# 运行程序
lian
```

### 快捷键

| 按键 | 功能 |
|------|------|
| `Enter` | 开始更新 |
| `Tab` | 切换视图 (更新日志 ↔ AI 分析) |
| `↑` / `k` | 向上滚动 |
| `↓` / `j` | 向下滚动 |
| `PgUp/PgDn` | 翻页 |
| `q` / `Esc` | 退出 |

### 查看历史报告

```bash
# 查看最新报告
ls -t ~/.lian/pacman/*/*/*/*.md | head -1

# 查看今天的报告
ls ~/.lian/pacman/$(date +%Y/%m/%d)/
```

## � 故障排除

### API 请求失败
1. 检查 API Key: `echo $LIAN_AI_KEY`
2. 检查网络连接
3. 确认 API 服务状态

### 找不到包管理器
```bash
# 安装 paru
sudo pacman -S paru
```

### 编译失败
```bash
rustup update stable
cargo clean && cargo build --release
```

## 📜 许可证

MIT License

## 🔗 链接

- **项目主页**: https://github.com/Yueosa/lian-pacman
- **问题反馈**: https://github.com/Yueosa/lian-pacman/issues
- **DeepSeek**: https://www.deepseek.com/
