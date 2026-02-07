#!/bin/bash
# lian 构建 + 测试脚本

set -e

PROJECT_NAME="lian-pacman"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
TEST_DIR="test"
BINARY="target/release/$PROJECT_NAME"

echo "=== lian 构建脚本 ==="
echo "版本: $VERSION"
echo ""

# 编译
echo "[1/3] 编译 release..."
cargo build --release 2>&1

if [ $? -ne 0 ]; then
    echo "❌ 编译失败"
    exit 1
fi
echo "✅ 编译完成"

# clippy 检查
echo "[2/3] clippy 检查..."
cargo clippy --release 2>&1
echo "✅ clippy 通过"

# 复制到 test 目录
echo "[3/3] 复制到 $TEST_DIR/..."
mkdir -p "$TEST_DIR"
cp "$BINARY" "$TEST_DIR/$PROJECT_NAME"
chmod +x "$TEST_DIR/$PROJECT_NAME"

echo ""
echo "=== 构建完成 ==="
echo "产物: $TEST_DIR/$PROJECT_NAME"
echo "大小: $(du -h "$TEST_DIR/$PROJECT_NAME" | cut -f1)"
echo ""
echo "运行测试:"
echo "  ./$TEST_DIR/$PROJECT_NAME"
echo "  ./$TEST_DIR/$PROJECT_NAME --help"
