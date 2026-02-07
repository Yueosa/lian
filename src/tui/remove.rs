use super::layout;
use super::state::App;
use ratatui::Frame;

/// 处理卸载模式按键（占位）
pub fn handle_remove_key(
    _key: crossterm::event::KeyEvent,
    _app: &mut App,
) -> bool {
    false
}

/// 渲染卸载视图（占位）
pub fn render_remove(f: &mut Frame, _app: &App) {
    layout::render_placeholder(f, "卸载软件包", f.area());
}
