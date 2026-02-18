use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

/// 标准三段式布局：Header(3) + Content(弹性) + Footer(3)
pub fn main_layout(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area)
        .to_vec()
}

/// 渲染通用 header
pub fn render_header(f: &mut Frame, title: &str, area: Rect) {
    let header = Paragraph::new(title)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    f.render_widget(header, area);
}

/// 渲染通用 footer
pub fn render_footer(f: &mut Frame, text: &str, area: Rect) {
    let footer = Paragraph::new(format!(" {}", text))
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Left);
    f.render_widget(footer, area);
}

/// 渲染带滚动条的内容区域
pub fn render_scrollable_content(
    f: &mut Frame,
    title: &str,
    lines: &[String],
    scroll_offset: usize,
    area: Rect,
) {
    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // 内部水平边距
    let padded = inner.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });

    let total_lines = lines.len();
    let visible_height = padded.height as usize;
    let max_scroll = total_lines.saturating_sub(visible_height);
    let actual_scroll = scroll_offset.min(max_scroll);

    let visible_content: Vec<Line> = lines
        .iter()
        .skip(actual_scroll)
        .take(visible_height)
        .map(|line| Line::from(line.clone()))
        .collect();

    let paragraph = Paragraph::new(visible_content)
        .wrap(ratatui::widgets::Wrap { trim: false });

    f.render_widget(paragraph, padded);

    // 滚动条
    if total_lines > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state = ScrollbarState::new(total_lines).position(actual_scroll);

        f.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                horizontal: 0,
                vertical: 1,
            }),
            &mut scrollbar_state,
        );
    }
}

/// 估算内容区域可见行数（总高度减去 header/footer/borders）
pub fn visible_content_height(term_height: u16) -> usize {
    term_height.saturating_sub(8) as usize
}

/// 将文本复制到系统剪贴板。
/// 优先尝试 wl-copy（Wayland），然后 xclip，最后 xsel。
/// 返回 true 表示成功，false 表示找不到可用工具。
pub fn copy_to_clipboard(text: &str) -> bool {
    let candidates: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];
    for (cmd, args) in candidates {
        if let Ok(mut child) = std::process::Command::new(cmd)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            if child.wait().map(|s| s.success()).unwrap_or(false) {
                return true;
            }
        }
    }
    false
}
