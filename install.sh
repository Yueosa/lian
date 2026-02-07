#!/bin/bash
# Lian-Pacman 快速安装脚本

set -e

echo "🚀 Lian-Pacman 安装脚本"
echo "========================"
echo ""

# 检查 Rust 是否已安装
if ! command -v cargo &> /dev/null; then
    echo "❌ 未检测到 Rust 工具链"
    echo "请先安装 Rust:"
    echo "  1. 使用包管理器: paru -S rust"
    echo "  2. 使用 rustup: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

echo "✅ 检测到 Rust 工具链: $(rustc --version)"
echo ""

# 编译项目
echo "📦 编译项目 (release 模式)..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "❌ 编译失败,请检查错误信息"
    exit 1
fi

echo "✅ 编译完成"
echo ""

# 安装到系统
echo "📥 安装到系统..."
INSTALL_PATH="/usr/local/bin/lian-pacman"

if [ -f "$INSTALL_PATH" ]; then
    echo "⚠️  检测到已存在的安装: $INSTALL_PATH"
    read -p "是否覆盖? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "❌ 安装已取消"
        exit 1
    fi
fi

sudo cp target/release/lian-pacman "$INSTALL_PATH"
sudo chmod +x "$INSTALL_PATH"

echo "✅ 已安装到: $INSTALL_PATH"
echo ""

# 检查 API Key
if [ -z "$LIAN_PACMAN_AI_KEY" ]; then
    echo "⚠️  未检测到 LIAN_PACMAN_AI_KEY 环境变量"
    echo ""
    echo "请设置你的 AI API Key:"
    echo "  export LIAN_PACMAN_AI_KEY='your-api-key-here'"
    echo ""
    echo "建议添加到 ~/.zshrc 或 ~/.bashrc:"
    echo "  echo 'export LIAN_PACMAN_AI_KEY=\"your-api-key\"' >> ~/.zshrc"
    echo ""
    echo "也可以在配置文件中设置 api_key 字段 (优先级更高)"
else
    echo "✅ 检测到 LIAN_PACMAN_AI_KEY"
fi

# 创建配置目录
CONFIG_DIR="$HOME/.config/lian-pacman"
if [ ! -d "$CONFIG_DIR" ]; then
    mkdir -p "$CONFIG_DIR"
    echo "✅ 已创建配置目录: $CONFIG_DIR"
fi

# 创建示例配置文件
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    cat > "$CONFIG_DIR/config.toml" << 'EOF'
# Lian-Pacman 配置文件

# AI API 地址 (可选，默认为 DeepSeek)
# 兼容 OpenAI 格式的 API 均可使用
# api_url = "https://api.deepseek.com/chat/completions"

# AI API Key (可选，优先级高于环境变量 LIAN_PACMAN_AI_KEY)
# api_key = "sk-your-key-here"

# HTTP 代理 (可选，支持 http/https/socks5)
# proxy = "http://127.0.0.1:7890"

# 使用的 AI 模型
# 可选值: "deepseek-chat" (快速) 或 "deepseek-reasoner" (深度思考,推荐)
model = "deepseek-reasoner"

# Temperature 设置
# 0.0: 代码/数学计算 (确定性强)
# 0.8: 数据分析 (推荐)
# 1.0: 默认值
# 1.3: 通用对话
temperature = 0.8

# 报告保存目录
# 使用绝对路径,或使用 $HOME 变量
report_dir = "$HOME/.lian/pacman/"
EOF
    
    # 替换 $HOME 为实际路径
    sed -i "s|\$HOME|$HOME|g" "$CONFIG_DIR/config.toml"
    
    echo "✅ 已创建示例配置文件: $CONFIG_DIR/config.toml"
fi

echo ""
echo "========================"
echo "🎉 安装完成!"
echo ""
echo "使用方法:"
echo "  1. 设置 API Key (如果还没设置):"
echo "     export LIAN_PACMAN_AI_KEY='your-api-key'"
echo "     或在配置文件中设置 api_key 字段"
echo ""
echo "  2. 运行程序:"
echo "     lian-pacman"
echo ""
echo "  3. 测试模式 (不执行真实更新):"
echo "     lian-pacman --test"
echo ""
echo "  4. 查看帮助:"
echo "     lian-pacman --help"
echo ""
echo "配置文件位置: $CONFIG_DIR/config.toml"
echo "报告保存位置: $HOME/.lian/pacman/"
echo ""
