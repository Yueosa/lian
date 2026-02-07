//! MTF flag 主题色定义，全局统一使用

use ratatui::style::Color;

/// 粉色 (MTF flag)
pub const PINK: Color = Color::Rgb(245, 169, 184);
/// 蓝色 (MTF flag)
pub const BLUE: Color = Color::Rgb(91, 206, 250);
/// 选中行背景色
pub const SEL_BG: Color = Color::Rgb(45, 35, 55);
/// 亮白色
pub const BRIGHT_WHITE: Color = Color::Rgb(255, 255, 255);
/// 暗灰色（次要信息）
pub const DIM: Color = Color::Rgb(130, 130, 140);
/// 描述文字灰色（选中行内）
pub const DESC_DIM: Color = Color::Rgb(180, 180, 190);
