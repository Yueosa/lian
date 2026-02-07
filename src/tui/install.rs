use super::layout;
use super::state::App;
use ratatui::Frame;

/// 处理安装模式按键（占位）
pub fn handle_install_key(
    _key: crossterm::event::KeyEvent,
    _app: &mut App,
) -> bool {
    false
}

/// 渲染安装视图（占位）
pub fn render_install(f: &mut Frame, _app: &App) {
    layout::render_placeholder(f, "安装软件包", f.area());
}
