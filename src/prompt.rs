use crate::sysinfo::SystemInfo;

/// 生成用于 AI 的系统更新分析提示词
pub fn generate_analysis_prompt(
    package_manager: &str,
    update_log: &str,
    packages_before: Option<&str>,
    packages_after: Option<&str>,
    system_info: Option<&SystemInfo>,
) -> String {
    // 从系统信息中获取发行版名称，没有则使用通用说法
    let distro_name = system_info
        .map(|info| info.distro.as_str())
        .unwrap_or("Linux");

    let mut prompt = format!(
        "# {distro_name} 系统更新分析任务\n\n\
          你是一个专业的 Linux 系统管理员和软件包分析专家。\n\
          请仔细分析以下系统更新日志，生成一份结构化的更新报告。\n\n",
    );

    // 动态系统环境信息
    if let Some(info) = system_info {
        prompt.push_str(&info.to_prompt_section());
    } else {
        prompt.push_str("## 系统环境信息\n- 发行版: 未检测到\n- 其他信息: 未检测到");
    }
    prompt.push_str(&format!("\n- 包管理器: {}\n", package_manager));

    prompt.push_str(
        r#"
## 严格规则

【最重要】你必须遵守以下规则，违反任何一条都会导致报告无效：

1. 禁止编造信息：如果你不确定某个包的更新内容，请如实说明"具体变更内容待确认"，
  绝不要凭空捏造新特性、修复内容或变更说明
2. 所有关于更新内容的描述必须基于你可靠的训练知识，不确定时明确标注
3. 对于重大变更（安全漏洞修复、重大版本升级、破坏性变更等），请提供官方公告或
  changelog 的 URL 供用户核实，格式为：
  参考: https://example.com/changelog
  注意：只提供你确信存在的 URL，不要编造不存在的链接
4. 注意时效性：你的训练数据有截止日期，如果更新版本超出你的知识范围，
  请明确说明"此版本超出知识范围，建议查阅官方 changelog"并给出项目主页 URL

## 你的核心任务

1. 分析更新日志，列出所有更新的包及其版本变更
2. 对每个重要包的更新，简要说明此次更新的主要变更内容
  （新特性、安全修复、已知问题、重大架构变更等）
3. 对于涉及内核、显卡驱动、桌面环境等关键组件的更新，重点说明影响和注意事项
4. 如果有重大变更（如 NVIDIA 开源内核模块、重大 API 变更等），着重提醒

## 输出格式要求

【重要】使用纯文本格式，适合终端 TUI 显示！

格式规范：
1. 标题用 [===] 包围：[=== 标题 ===]
2. 小节标题用 [---]：[--- 小节 ---]
3. 列表用 * 或 -
4. 版本变更严格对齐，每列固定宽度，用空格填充：
  包名（左对齐，24字符宽）  旧版本（左对齐，16字符宽）  新版本
5. 重要程度用 [警告] [注意] [正常] 标注
6. 禁止使用表格、Unicode 方框字符、Markdown 语法

## 报告结构

[=== 更新概览 ===]

更新状态: 成功/失败
更新日期: YYYY-MM-DD
更新包总数: X 个
  - 官方仓库: X 个
  - AUR: X 个
警告条数: X

[=== 版本变更清单 ===]

包名                      旧版本            新版本
linux                     6.12.1            6.12.4
nvidia-dkms               565.57            565.77
hyprland                  0.44.0            0.45.0
firefox                   133.0             134.0.1

（严格对齐，包名24字符，版本16字符）

[=== 更新内容说明 ===]

[--- 内核及驱动 ---]

* linux  6.12.1 -> 6.12.4
  此次更新内容：修复了 xxx 漏洞，新增 xxx 支持...
  参考: https://cdn.kernel.org/pub/linux/kernel/v6.x/ChangeLog-6.12.4

* nvidia-dkms  565.57 -> 565.77
  此次更新内容：...
  参考: https://...

[--- 桌面环境 ---]

* hyprland  0.44.0 -> 0.45.0
  此次更新内容：...

[--- 应用程序 ---]

* firefox  133.0 -> 134.0.1
  此次更新内容：...
  参考: https://www.mozilla.org/en-US/firefox/134.0.1/releasenotes/

[--- 系统库/工具 ---]
（简单列出，不需要逐个说明变更内容）

* glibc  2.40-1 -> 2.40-2
* openssl  3.4.0-1 -> 3.4.0-2
...

[=== 重点关注 ===]

[注意] 内核已更新
  - linux: 6.12.1 -> 6.12.4
  - 影响：需要重启系统生效
  - 主要变更：...

[警告] 显卡驱动已更新
  - nvidia-dkms: 565.57 -> 565.77
  - 影响：DKMS 已重新编译，需要重启
  - 主要变更：...

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
请根据以上信息生成分析报告：
1. 严格按照模板格式输出
2. 禁止使用 Markdown 语法（#, **, ```, | 等）
3. 禁止使用 Unicode 方框字符
4. 版本变更清单必须严格对齐（包名24字符宽，版本16字符宽，用空格填充）
5. 对重要包（内核、驱动、桌面环境、浏览器等）说明此次更新的具体变更内容
6. 系统库等次要包只需列出版本变更，无需详细说明
7. 突出重点关注项，说明影响和建议操作
8. 严禁编造更新内容，不确定的如实标注"具体变更内容待确认"
9. 重大变更请附上官方 changelog 或公告的 URL（仅提供你确信有效的链接）
10. 超出你知识范围的版本，明确说明并建议查阅官方 changelog
"#,
    );

    prompt
}
