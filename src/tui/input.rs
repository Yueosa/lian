use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// 通用文本输入框组件，支持 UTF-8 (中英文)
#[derive(Debug, Clone)]
pub struct InputBox {
    /// 输入内容
    content: String,
    /// 光标位置（按字符计数，非字节）
    cursor: usize,
}

impl InputBox {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor: 0,
        }
    }

    /// 在光标处插入字符
    pub fn insert(&mut self, c: char) {
        let byte_pos = self.char_to_byte_pos(self.cursor);
        self.content.insert(byte_pos, c);
        self.cursor += 1;
    }

    /// 光标右移
    pub fn move_right(&mut self) {
        let char_count = self.content.chars().count();
        if self.cursor < char_count {
            self.cursor += 1;
        }
    }

    /// 光标移到行首
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// 获取内容
    pub fn content(&self) -> &str {
        &self.content
    }

    /// 获取光标位置
    pub fn cursor_pos(&self) -> usize {
        self.cursor
    }

    /// 字符位置 → 字节位置
    fn char_to_byte_pos(&self, char_pos: usize) -> usize {
        self.content
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.content.len())
    }
}

/// 渲染输入框
pub fn render_input_box(
    f: &mut Frame,
    input: &InputBox,
    label: &str,
    focused: bool,
    area: Rect,
) {
    let border_color = if focused { Color::Yellow } else { Color::DarkGray };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    // 构建显示内容：label + 输入文本 + 光标
    let content = input.content();
    let cursor_pos = input.cursor_pos();
    let chars: Vec<char> = content.chars().collect();
    let before: String = chars[..cursor_pos].iter().collect();
    let cursor_char = if cursor_pos < chars.len() {
        chars[cursor_pos].to_string()
    } else {
        " ".to_string()
    };
    let after: String = if cursor_pos < chars.len() {
        chars[cursor_pos + 1..].iter().collect()
    } else {
        String::new()
    };

    let mut spans = vec![
        Span::styled(
            format!("{label} "),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(before, Style::default().fg(Color::White)),
    ];

    if focused {
        spans.push(Span::styled(
            cursor_char,
            Style::default()
                .fg(Color::Black)
                .bg(Color::White),
        ));
    } else {
        spans.push(Span::styled(cursor_char, Style::default().fg(Color::White)));
    }

    spans.push(Span::styled(after, Style::default().fg(Color::White)));

    let paragraph = Paragraph::new(Line::from(spans)).block(block);
    f.render_widget(paragraph, area);
}

// ===== 共享文本编辑工具函数 =====

/// UTF-8 安全的字符位置转字节位置
pub fn char_to_byte(s: &str, char_pos: usize) -> usize {
    s.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// 在字符串的指定光标位置插入字符，返回新的光标位置
pub fn str_insert_char(s: &mut String, cursor: &mut usize, c: char) {
    let byte_pos = char_to_byte(s, *cursor);
    s.insert(byte_pos, c);
    *cursor += 1;
}

/// Backspace: 删除光标前的字符
pub fn str_delete_back(s: &mut String, cursor: &mut usize) {
    if *cursor > 0 {
        *cursor -= 1;
        let byte_pos = char_to_byte(s, *cursor);
        let next_byte_pos = char_to_byte(s, *cursor + 1);
        s.drain(byte_pos..next_byte_pos);
    }
}

/// Delete: 删除光标后的字符
pub fn str_delete_forward(s: &mut String, cursor: &mut usize) {
    let char_count = s.chars().count();
    if *cursor < char_count {
        let byte_pos = char_to_byte(s, *cursor);
        let next_byte_pos = char_to_byte(s, *cursor + 1);
        s.drain(byte_pos..next_byte_pos);
    }
}
