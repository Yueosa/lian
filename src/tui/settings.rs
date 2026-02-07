use super::layout;
use super::state::App;
use ratatui::Frame;

/// 处理设置模式按键（占位）
pub fn handle_settings_key(
    _key: crossterm::event::KeyEvent,
    _app: &mut App,
) -> bool {
    false
}

/// 渲染设置视图（占位）
pub fn render_settings(f: &mut Frame, _app: &App) {
    layout::render_placeholder(f, "设置", f.area());
}
