use super::state::App;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

const ASCII_LOGO: &str = r#"
██       ██                   
░██      ░░                   
░██       ██  ██████  ███████ 
░██      ░██ ░░░░░░██░░██░░░██
░██      ░██  ███████ ░██  ░██
░██      ░██ ██░░░░██ ░██  ░██
░████████░██░░████████░██  ░██
░░░░░░░░ ░░  ░░░░░░░░ ░░   ░░"#;

pub fn render_dashboard(f: &mut Frame, app: &App) {
    let area = f.area();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    f.render_widget(block, area);

    // 构建所有行
    let mut lines: Vec<Line> = Vec::new();

    // 空行填充（顶部留白）
    lines.push(Line::from(""));

    // ASCII Logo
    for logo_line in ASCII_LOGO.lines() {
        lines.push(Line::from(vec![Span::styled(
            logo_line.to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // 系统信息标题
    lines.push(Line::from(vec![Span::styled(
        "── 系统信息 ──",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // 系统信息内容
    if let Some(info) = &app.system_info {
        lines.push(info_line("发行版  ", &info.distro));
        lines.push(info_line("内核  ", &info.kernel));
    } else {
        lines.push(info_line("发行版  ", "检测中..."));
        lines.push(info_line("内核  ", "检测中..."));
    }

    if let Some(pm) = &app.package_manager {
        lines.push(info_line("包管理器  ", pm.name()));
    } else {
        lines.push(info_line("包管理器  ", "检测中..."));
    }

    if let Some(count) = app.installed_count {
        let count_str = format!("{count} 个");
        lines.push(info_line("已安装包  ", &count_str));
    } else {
        lines.push(info_line("已安装包  ", "统计中..."));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // 快捷键标题
    lines.push(Line::from(vec![Span::styled(
        "── 快捷键 ──",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // 快捷键列表
    lines.push(shortcut_line("U", " 系统更新 (Syu)  "));
    lines.push(shortcut_line("S", " 安装软件包       "));
    lines.push(shortcut_line("R", " 卸载软件包       "));
    lines.push(shortcut_line("Q", " 查询软件包       "));
    lines.push(shortcut_line("C", " 设置             "));
    lines.push(shortcut_line("q", " 退出             "));

    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // 版本号
    lines.push(Line::from(vec![Span::styled(
        format!("lian v{}  ", env!("CARGO_PKG_VERSION")),
        Style::default().fg(Color::DarkGray),
    )]));

    let lines_count = lines.len();
    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);

    // 垂直居中：计算内容高度，用 Layout 居中
    let content_height = lines_count as u16;
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(content_height),
            Constraint::Min(0),
        ])
        .split(inner);

    f.render_widget(paragraph, vertical[1]);
}

/// 系统信息行: "  标签: 值"
fn info_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(Color::White)),
    ])
}

/// 快捷键行: "  X  描述"
fn shortcut_line<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("  {key}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(desc.to_string(), Style::default().fg(Color::White)),
    ])
}
