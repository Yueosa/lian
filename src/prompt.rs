/// 生成用于 DeepSeek AI 的系统更新分析提示词
pub fn generate_analysis_prompt(
    package_manager: &str,
    update_log: &str,
    packages_before: Option<&str>,
    packages_after: Option<&str>,
) -> String {
    let mut prompt = String::from(
        r#"# Arch Linux 系统更新分析任务

你是一个专业的 Arch Linux 系统管理员和软件包分析专家。请仔细分析以下系统更新日志,并生成一份结构化的更新报告。

## 系统环境信息
- 发行版: Arch Linux
- 桌面环境: Hyprland (Wayland)
- 显卡: NVIDIA
- 包管理器: "#,
    );

    prompt.push_str(package_manager);
    prompt.push_str("\n\n## 输出格式要求\n\n");
    prompt.push_str(
        r#"【重要】请使用纯文本格式输出，适合终端 TUI 显示！

格式规范：
1. 标题使用 [===] 包围，如：[=== 标题 ===]
2. 小节标题使用 [---]，如：[--- 小节 ---]
3. 列表使用简单符号：* 或 - 
4. 版本变更用箭头：包名: 旧版本 -> 新版本
5. 重要程度用文字标注：[警告] [注意] [正常]
6. 不要使用表格！不要使用 Unicode 方框字符！
7. 不要使用 Markdown 语法

## 报告结构模板

[=== 更新概览 ===]

更新状态: 成功/失败
更新包总数: X 个
  - 官方仓库: X 个
  - AUR: X 个
警告数量: X 条

[=== 包分类详情 ===]

[--- 内核及驱动 ---]
* linux: 6.12.1 -> 6.12.4
* nvidia-dkms: 565.57 -> 565.77

[--- 系统核心 ---]
* systemd: ...
* glibc: ...

[--- 桌面环境 ---]
* hyprland: ...
* wayland: ...

[--- 应用程序 ---]
* firefox: ...
* ...

[--- 开发工具 ---]
* ...

[--- 其他 ---]
* ...

[=== 重点关注 ===]

[注意] 内核已更新
  - linux: 6.12.1 -> 6.12.4
  - 需要重启系统生效

[注意] NVIDIA 驱动已更新  
  - nvidia-dkms: 565.57 -> 565.77
  - DKMS 已重新编译，需要重启

[警告] Hyprland 更新
  - 建议重新登录 Wayland 会话

[=== 版本变更清单 ===]

包名                    旧版本          新版本
----                    ------          ------
linux                   6.12.1          6.12.4
nvidia-dkms             565.57          565.77
hyprland                0.44.0          0.45.0
...

[=== 建议操作 ===]

[已完成] 更新已成功安装
[待办] 重启系统（内核已更新）
[待办] 重新登录（显卡驱动已更新）
[可选] 检查 .pacnew 配置文件

[=== 报告结束 ===]

---

## 更新日志
```
"#,
    );

    prompt.push_str(update_log);
    prompt.push_str("\n```\n\n");

    // 如果提供了更新前后的包列表
    if let (Some(before), Some(after)) = (packages_before, packages_after) {
        prompt.push_str("## 更新前已安装包列表 (pacman -Qe)\n```\n");
        prompt.push_str(before);
        prompt.push_str("\n```\n\n");
        prompt.push_str("## 更新后已安装包列表 (pacman -Qe)\n```\n");
        prompt.push_str(after);
        prompt.push_str("\n```\n\n");
    }

    prompt.push_str(
        r#"
请根据以上信息生成纯文本格式分析报告：
1. 严格按照模板格式输出
2. 禁止使用 Markdown 语法（#, **, ```, | 等）
3. 禁止使用 Unicode 方框字符（─│┌┐└┘├┤┬┴┼等）
4. 版本变更清单用空格对齐即可，不要画表格
5. 突出重点关注项，说明原因和建议
6. 保持简洁清晰
"#,
    );

    prompt
}
