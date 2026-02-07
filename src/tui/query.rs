use super::layout;
use super::state::App;
use ratatui::Frame;

/// 处理查询模式按键（占位）
pub fn handle_query_key(
    _key: crossterm::event::KeyEvent,
    _app: &mut App,
) -> bool {
    false
}

/// 渲染查询视图（占位）
pub fn render_query(f: &mut Frame, _app: &App) {
    layout::render_placeholder(f, "查询软件包", f.area());
}
