use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
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

/// 渲染 "开发中" 占位页面
pub fn render_placeholder(f: &mut Frame, mode_name: &str, area: Rect) {
    let chunks = main_layout(area);

    render_header(f, &format!("📦 {mode_name}"), chunks[0]);

    let placeholder_lines = vec![
        Line::from(""),
        Line::from(""),
        Line::from(""),
        Line::from(vec![Span::styled(
            "🚧 开发中...",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("{mode_name}功能将在后续版本中实现"),
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    let content = Paragraph::new(placeholder_lines)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow)))
        .alignment(Alignment::Center);
    f.render_widget(content, chunks[1]);

    render_footer(f, "Esc 返回主页 | q 退出", chunks[2]);
}

/// 估算内容区域可见行数（总高度减去 header/footer/borders）
pub fn visible_content_height(term_height: u16) -> usize {
    term_height.saturating_sub(8) as usize
}
