# 架构设计

本文档面向想要贡献代码的开发者。

## 代码结构

```
src/
├── main.rs              # 入口，参数解析，sudo 验证
├── config.rs            # 配置加载 (~/.config/lian/config.toml)
├── package_manager.rs   # 包管理器检测和更新执行
├── prompt.rs            # AI 提示词生成
├── deepseek.rs          # AI API 客户端
├── sysinfo.rs           # 系统环境检测
├── report.rs            # 报告保存 (~/.lian/pacman/YYYY/MM/DD/)
└── tui.rs               # TUI 界面和事件循环
```

## 核心模块

### package_manager.rs

- 检测顺序：paru → yay → pacman
- `update_streaming()` - 实时输出的更新执行
- 使用 `prctl(PR_SET_PDEATHSIG)` 确保子进程随父进程退出

### tui.rs

**状态机**:
```
PackageManagerCheck → PreUpdate → Updating → UpdateComplete → Analyzing → AnalysisComplete
                                     ↘                                      ↗
                                           Error ←────────────────────────
```

**事件通道**: 使用 `tokio::mpsc` 在异步任务和 TUI 主循环间通信

### prompt.rs

生成纯文本格式的 AI 提示词，包含：
- 系统环境信息
- 输出格式模板
- 更新日志内容

### deepseek.rs

封装 DeepSeek API 调用，支持：
- `deepseek-chat` - 快速响应
- `deepseek-reasoner` - 深度推理

## 数据流

```
检测包管理器 → 用户确认 → 执行更新 → 捕获输出 → 生成提示词 → AI 分析 → 显示/保存报告
```

## 开发

```bash
# 调试运行
RUST_LOG=debug cargo run

# 编译发布版本
cargo build --release
```
