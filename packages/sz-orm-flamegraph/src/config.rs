//! 火焰图渲染配置：颜色方案、布局参数、输出选项。
//!
//! - [`RenderConfig`] — 渲染配置
//! - [`ColorScheme`] — 颜色方案
//! - [`ColorPalette`] — 颜色调色板
//! - [`LayoutConfig`] — 布局参数
//! - [`OutputFormat`] — 输出格式

use serde::{Deserialize, Serialize};

// ============================================================================
// ColorScheme — 颜色方案
// ============================================================================

/// 颜色方案
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorScheme {
    /// 默认（暖色：红/橙/黄）
    #[default]
    Default,
    /// 冷色（蓝/青/绿）
    Cool,
    /// 热点色（红越深越热）
    Hot,
    /// 差异色（红=回归，绿=改善）
    Diff,
    /// 灰度
    Grayscale,
    /// 随机色
    Random,
}

impl ColorScheme {
    /// 方案名
    pub fn name(&self) -> &'static str {
        match self {
            ColorScheme::Default => "default",
            ColorScheme::Cool => "cool",
            ColorScheme::Hot => "hot",
            ColorScheme::Diff => "diff",
            ColorScheme::Grayscale => "grayscale",
            ColorScheme::Random => "random",
        }
    }

    /// 生成调色板
    pub fn palette(&self) -> ColorPalette {
        match self {
            ColorScheme::Default => ColorPalette::default_warm(),
            ColorScheme::Cool => ColorPalette::cool(),
            ColorScheme::Hot => ColorPalette::hot(),
            ColorScheme::Diff => ColorPalette::diff(),
            ColorScheme::Grayscale => ColorPalette::grayscale(),
            ColorScheme::Random => ColorPalette::random(),
        }
    }
}

// ============================================================================
// ColorPalette — 颜色调色板
// ============================================================================

/// 颜色调色板
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorPalette {
    primary: String,
    secondary: String,
    accent: String,
    background: String,
    text: String,
    border: String,
    highlight: String,
    regression: String,
    improvement: String,
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self::default_warm()
    }
}

impl ColorPalette {
    /// 暖色调色板
    pub fn default_warm() -> Self {
        Self {
            primary: "#d95f02".to_string(),
            secondary: "#fdb863".to_string(),
            accent: "#e7298a".to_string(),
            background: "#ffffff".to_string(),
            text: "#333333".to_string(),
            border: "#cccccc".to_string(),
            highlight: "#f0f0f0".to_string(),
            regression: "#d73027".to_string(),
            improvement: "#1a9850".to_string(),
        }
    }

    /// 冷色调色板
    pub fn cool() -> Self {
        Self {
            primary: "#1b9e77".to_string(),
            secondary: "#80cdc1".to_string(),
            accent: "#018571".to_string(),
            background: "#ffffff".to_string(),
            text: "#333333".to_string(),
            border: "#cccccc".to_string(),
            highlight: "#e0f3f8".to_string(),
            regression: "#d73027".to_string(),
            improvement: "#1a9850".to_string(),
        }
    }

    /// 热点色调色板
    pub fn hot() -> Self {
        Self {
            primary: "#d73027".to_string(),
            secondary: "#fc8d59".to_string(),
            accent: "#fee090".to_string(),
            background: "#ffffff".to_string(),
            text: "#333333".to_string(),
            border: "#cccccc".to_string(),
            highlight: "#ffffbf".to_string(),
            regression: "#d73027".to_string(),
            improvement: "#1a9850".to_string(),
        }
    }

    /// 差异色调色板
    pub fn diff() -> Self {
        Self {
            primary: "#1a9850".to_string(),
            secondary: "#91cf60".to_string(),
            accent: "#d73027".to_string(),
            background: "#ffffff".to_string(),
            text: "#333333".to_string(),
            border: "#cccccc".to_string(),
            highlight: "#fee090".to_string(),
            regression: "#d73027".to_string(),
            improvement: "#1a9850".to_string(),
        }
    }

    /// 灰度调色板
    pub fn grayscale() -> Self {
        Self {
            primary: "#666666".to_string(),
            secondary: "#999999".to_string(),
            accent: "#333333".to_string(),
            background: "#ffffff".to_string(),
            text: "#333333".to_string(),
            border: "#cccccc".to_string(),
            highlight: "#eeeeee".to_string(),
            regression: "#444444".to_string(),
            improvement: "#aaaaaa".to_string(),
        }
    }

    /// 随机色调色板
    pub fn random() -> Self {
        Self {
            primary: "#a6cee3".to_string(),
            secondary: "#b2df8a".to_string(),
            accent: "#fb9a99".to_string(),
            background: "#ffffff".to_string(),
            text: "#333333".to_string(),
            border: "#cccccc".to_string(),
            highlight: "#fdbf6f".to_string(),
            regression: "#e31a1c".to_string(),
            improvement: "#33a02c".to_string(),
        }
    }

    /// 主色
    pub fn primary(&self) -> &str {
        &self.primary
    }

    /// 次色
    pub fn secondary(&self) -> &str {
        &self.secondary
    }

    /// 强调色
    pub fn accent(&self) -> &str {
        &self.accent
    }

    /// 背景色
    pub fn background(&self) -> &str {
        &self.background
    }

    /// 文字色
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 边框色
    pub fn border(&self) -> &str {
        &self.border
    }

    /// 高亮色
    pub fn highlight(&self) -> &str {
        &self.highlight
    }

    /// 回归色
    pub fn regression(&self) -> &str {
        &self.regression
    }

    /// 改善色
    pub fn improvement(&self) -> &str {
        &self.improvement
    }

    /// 按值比例（0.0~1.0）选择颜色
    pub fn color_for_ratio(&self, ratio: f64) -> &str {
        let clamped = ratio.clamp(0.0, 1.0);
        if clamped < 0.33 {
            &self.secondary
        } else if clamped < 0.66 {
            &self.primary
        } else {
            &self.accent
        }
    }
}

// ============================================================================
// LayoutConfig — 布局参数
// ============================================================================

/// 布局参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    width: u64,
    height: u64,
    row_height: u64,
    margin: u64,
    padding: u64,
    font_size: u64,
    font_family: String,
    min_width: u64,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            row_height: 16,
            margin: 4,
            padding: 2,
            font_size: 11,
            font_family: "monospace".to_string(),
            min_width: 1,
        }
    }
}

impl LayoutConfig {
    /// 创建默认布局
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置宽度（链式）
    pub fn width(mut self, w: u64) -> Self {
        self.width = w;
        self
    }

    /// 设置高度（链式）
    pub fn height(mut self, h: u64) -> Self {
        self.height = h;
        self
    }

    /// 设置行高（链式）
    pub fn row_height(mut self, h: u64) -> Self {
        self.row_height = h;
        self
    }

    /// 设置边距（链式）
    pub fn margin(mut self, m: u64) -> Self {
        self.margin = m;
        self
    }

    /// 设置内边距（链式）
    pub fn padding(mut self, p: u64) -> Self {
        self.padding = p;
        self
    }

    /// 设置字体大小（链式）
    pub fn font_size(mut self, s: u64) -> Self {
        self.font_size = s;
        self
    }

    /// 设置字体族（链式）
    pub fn font_family(mut self, family: &str) -> Self {
        self.font_family = family.to_string();
        self
    }

    /// 设置最小宽度（链式）
    pub fn min_width(mut self, w: u64) -> Self {
        self.min_width = w;
        self
    }

    /// 宽度
    pub fn width_value(&self) -> u64 {
        self.width
    }

    /// 高度
    pub fn height_value(&self) -> u64 {
        self.height
    }

    /// 行高
    pub fn row_height_value(&self) -> u64 {
        self.row_height
    }

    /// 边距
    pub fn margin_value(&self) -> u64 {
        self.margin
    }

    /// 内边距
    pub fn padding_value(&self) -> u64 {
        self.padding
    }

    /// 字体大小
    pub fn font_size_value(&self) -> u64 {
        self.font_size
    }

    /// 字体族
    pub fn font_family_value(&self) -> &str {
        &self.font_family
    }

    /// 最小宽度
    pub fn min_width_value(&self) -> u64 {
        self.min_width
    }

    /// 计算给定行数的总高度
    pub fn total_height(&self, rows: usize) -> u64 {
        self.margin * 2 + self.row_height * rows as u64
    }

    /// 计算给定值占比的块宽度
    pub fn block_width(&self, ratio: f64) -> u64 {
        let w = (ratio * self.width as f64) as u64;
        w.max(self.min_width)
    }
}

// ============================================================================
// OutputFormat — 输出格式
// ============================================================================

/// 输出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutputFormat {
    /// SVG
    #[default]
    Svg,
    /// Brendan Gregg 折叠栈
    Folded,
    /// JSON
    Json,
    /// HTML
    Html,
    /// 文本
    Text,
}

impl OutputFormat {
    /// 格式名
    pub fn name(&self) -> &'static str {
        match self {
            OutputFormat::Svg => "svg",
            OutputFormat::Folded => "folded",
            OutputFormat::Json => "json",
            OutputFormat::Html => "html",
            OutputFormat::Text => "text",
        }
    }

    /// 文件扩展名
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Svg => "svg",
            OutputFormat::Folded => "folded",
            OutputFormat::Json => "json",
            OutputFormat::Html => "html",
            OutputFormat::Text => "txt",
        }
    }

    /// MIME 类型
    pub fn mime_type(&self) -> &'static str {
        match self {
            OutputFormat::Svg => "image/svg+xml",
            OutputFormat::Folded => "text/plain",
            OutputFormat::Json => "application/json",
            OutputFormat::Html => "text/html",
            OutputFormat::Text => "text/plain",
        }
    }
}

// ============================================================================
// RenderConfig — 渲染配置
// ============================================================================

/// 渲染配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderConfig {
    color_scheme: ColorScheme,
    palette: ColorPalette,
    layout: LayoutConfig,
    format: OutputFormat,
    title: String,
    show_legend: bool,
    show_labels: bool,
    show_values: bool,
    sort_by_value: bool,
    reverse: bool,
    max_frames: Option<usize>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            color_scheme: ColorScheme::default(),
            palette: ColorPalette::default(),
            layout: LayoutConfig::default(),
            format: OutputFormat::default(),
            title: "Flame Graph".to_string(),
            show_legend: true,
            show_labels: true,
            show_values: true,
            sort_by_value: true,
            reverse: false,
            max_frames: None,
        }
    }
}

impl RenderConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置颜色方案（链式，自动更新调色板）
    pub fn color_scheme(mut self, scheme: ColorScheme) -> Self {
        self.palette = scheme.palette();
        self.color_scheme = scheme;
        self
    }

    /// 设置调色板（链式）
    pub fn palette(mut self, palette: ColorPalette) -> Self {
        self.palette = palette;
        self
    }

    /// 设置布局（链式）
    pub fn layout(mut self, layout: LayoutConfig) -> Self {
        self.layout = layout;
        self
    }

    /// 设置输出格式（链式）
    pub fn format(mut self, format: OutputFormat) -> Self {
        self.format = format;
        self
    }

    /// 设置标题（链式）
    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    /// 显示图例（链式）
    pub fn show_legend(mut self) -> Self {
        self.show_legend = true;
        self
    }

    /// 隐藏图例（链式）
    pub fn hide_legend(mut self) -> Self {
        self.show_legend = false;
        self
    }

    /// 显示标签（链式）
    pub fn show_labels(mut self) -> Self {
        self.show_labels = true;
        self
    }

    /// 隐藏标签（链式）
    pub fn hide_labels(mut self) -> Self {
        self.show_labels = false;
        self
    }

    /// 显示值（链式）
    pub fn show_values(mut self) -> Self {
        self.show_values = true;
        self
    }

    /// 隐藏值（链式）
    pub fn hide_values(mut self) -> Self {
        self.show_values = false;
        self
    }

    /// 按值排序（链式）
    pub fn sort_by_value(mut self) -> Self {
        self.sort_by_value = true;
        self
    }

    /// 反转顺序（链式）
    pub fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    /// 设置最大帧数（链式）
    pub fn max_frames(mut self, n: usize) -> Self {
        self.max_frames = Some(n);
        self
    }

    /// 颜色方案
    pub fn color_scheme_value(&self) -> ColorScheme {
        self.color_scheme
    }

    /// 调色板引用
    pub fn palette_value(&self) -> &ColorPalette {
        &self.palette
    }

    /// 布局引用
    pub fn layout_value(&self) -> &LayoutConfig {
        &self.layout
    }

    /// 输出格式
    pub fn format_value(&self) -> OutputFormat {
        self.format
    }

    /// 标题
    pub fn title_value(&self) -> &str {
        &self.title
    }

    /// 是否显示图例
    pub fn is_show_legend(&self) -> bool {
        self.show_legend
    }

    /// 是否显示标签
    pub fn is_show_labels(&self) -> bool {
        self.show_labels
    }

    /// 是否显示值
    pub fn is_show_values(&self) -> bool {
        self.show_values
    }

    /// 是否按值排序
    pub fn is_sort_by_value(&self) -> bool {
        self.sort_by_value
    }

    /// 是否反转
    pub fn is_reverse(&self) -> bool {
        self.reverse
    }

    /// 最大帧数
    pub fn max_frames_value(&self) -> Option<usize> {
        self.max_frames
    }

    /// 转换为 JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

// ============================================================================
// RenderOptions — 快速渲染选项
// ============================================================================

/// 快速渲染选项（简化配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderOptions {
    width: u64,
    height: u64,
    color: String,
    title: String,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            color: "default".to_string(),
            title: "Flame Graph".to_string(),
        }
    }
}

impl RenderOptions {
    /// 创建默认选项
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 RenderConfig 创建
    pub fn from_config(config: &RenderConfig) -> Self {
        Self {
            width: config.layout.width_value(),
            height: config.layout.height_value(),
            color: config.color_scheme.name().to_string(),
            title: config.title.clone(),
        }
    }

    /// 设置宽度（链式）
    pub fn width(mut self, w: u64) -> Self {
        self.width = w;
        self
    }

    /// 设置高度（链式）
    pub fn height(mut self, h: u64) -> Self {
        self.height = h;
        self
    }

    /// 设置颜色方案名（链式）
    pub fn color(mut self, name: &str) -> Self {
        self.color = name.to_string();
        self
    }

    /// 设置标题（链式）
    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    /// 宽度
    pub fn width_value(&self) -> u64 {
        self.width
    }

    /// 高度
    pub fn height_value(&self) -> u64 {
        self.height
    }

    /// 颜色方案名
    pub fn color_value(&self) -> &str {
        &self.color
    }

    /// 标题
    pub fn title_value(&self) -> &str {
        &self.title
    }

    /// 转换为完整 RenderConfig
    pub fn to_config(&self) -> RenderConfig {
        let scheme = match self.color.as_str() {
            "cool" => ColorScheme::Cool,
            "hot" => ColorScheme::Hot,
            "diff" => ColorScheme::Diff,
            "grayscale" => ColorScheme::Grayscale,
            "random" => ColorScheme::Random,
            _ => ColorScheme::Default,
        };
        RenderConfig::new()
            .color_scheme(scheme)
            .title(&self.title)
            .layout(LayoutConfig::new().width(self.width).height(self.height))
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- ColorScheme -----

    #[test]
    fn color_scheme_name() {
        assert_eq!(ColorScheme::Default.name(), "default");
        assert_eq!(ColorScheme::Cool.name(), "cool");
        assert_eq!(ColorScheme::Hot.name(), "hot");
        assert_eq!(ColorScheme::Diff.name(), "diff");
        assert_eq!(ColorScheme::Grayscale.name(), "grayscale");
        assert_eq!(ColorScheme::Random.name(), "random");
    }

    #[test]
    fn color_scheme_palette() {
        let p = ColorScheme::Default.palette();
        assert!(!p.primary().is_empty());
        let p2 = ColorScheme::Cool.palette();
        assert_ne!(p.primary(), p2.primary());
    }

    #[test]
    fn color_scheme_default() {
        assert_eq!(ColorScheme::default(), ColorScheme::Default);
    }

    // ----- ColorPalette -----

    #[test]
    fn color_palette_default_warm() {
        let p = ColorPalette::default_warm();
        assert!(p.primary().starts_with('#'));
    }

    #[test]
    fn color_palette_cool() {
        let p = ColorPalette::cool();
        assert!(p.primary().starts_with('#'));
    }

    #[test]
    fn color_palette_hot() {
        let p = ColorPalette::hot();
        assert!(p.primary().starts_with('#'));
    }

    #[test]
    fn color_palette_diff() {
        let p = ColorPalette::diff();
        assert!(p.primary().starts_with('#'));
    }

    #[test]
    fn color_palette_grayscale() {
        let p = ColorPalette::grayscale();
        assert!(p.primary().starts_with('#'));
    }

    #[test]
    fn color_palette_random() {
        let p = ColorPalette::random();
        assert!(p.primary().starts_with('#'));
    }

    #[test]
    fn color_palette_color_for_ratio() {
        let p = ColorPalette::default_warm();
        let c1 = p.color_for_ratio(0.1);
        let c2 = p.color_for_ratio(0.5);
        let c3 = p.color_for_ratio(0.9);
        assert!(c1.starts_with('#'));
        assert!(c2.starts_with('#'));
        assert!(c3.starts_with('#'));
    }

    #[test]
    fn color_palette_color_for_ratio_clamped() {
        let p = ColorPalette::default_warm();
        let c1 = p.color_for_ratio(-1.0);
        let c2 = p.color_for_ratio(2.0);
        assert!(c1.starts_with('#'));
        assert!(c2.starts_with('#'));
    }

    #[test]
    fn color_palette_getters() {
        let p = ColorPalette::default_warm();
        assert!(!p.primary().is_empty());
        assert!(!p.secondary().is_empty());
        assert!(!p.accent().is_empty());
        assert!(!p.background().is_empty());
        assert!(!p.text().is_empty());
        assert!(!p.border().is_empty());
        assert!(!p.highlight().is_empty());
        assert!(!p.regression().is_empty());
        assert!(!p.improvement().is_empty());
    }

    // ----- LayoutConfig -----

    #[test]
    fn layout_config_default() {
        let l = LayoutConfig::new();
        assert_eq!(l.width_value(), 800);
        assert_eq!(l.height_value(), 600);
        assert_eq!(l.row_height_value(), 16);
    }

    #[test]
    fn layout_config_builder() {
        let l = LayoutConfig::new()
            .width(1200)
            .height(800)
            .row_height(20)
            .margin(10)
            .padding(5)
            .font_size(14)
            .font_family("sans-serif")
            .min_width(2);
        assert_eq!(l.width_value(), 1200);
        assert_eq!(l.height_value(), 800);
        assert_eq!(l.row_height_value(), 20);
        assert_eq!(l.margin_value(), 10);
        assert_eq!(l.padding_value(), 5);
        assert_eq!(l.font_size_value(), 14);
        assert_eq!(l.font_family_value(), "sans-serif");
        assert_eq!(l.min_width_value(), 2);
    }

    #[test]
    fn layout_config_total_height() {
        let l = LayoutConfig::new().margin(10).row_height(20);
        assert_eq!(l.total_height(5), 10 * 2 + 20 * 5);
    }

    #[test]
    fn layout_config_block_width() {
        let l = LayoutConfig::new().width(1000).min_width(2);
        assert_eq!(l.block_width(0.5), 500);
        assert_eq!(l.block_width(0.0), 2);
    }

    // ----- OutputFormat -----

    #[test]
    fn output_format_name() {
        assert_eq!(OutputFormat::Svg.name(), "svg");
        assert_eq!(OutputFormat::Folded.name(), "folded");
        assert_eq!(OutputFormat::Json.name(), "json");
        assert_eq!(OutputFormat::Html.name(), "html");
        assert_eq!(OutputFormat::Text.name(), "text");
    }

    #[test]
    fn output_format_extension() {
        assert_eq!(OutputFormat::Svg.extension(), "svg");
        assert_eq!(OutputFormat::Folded.extension(), "folded");
        assert_eq!(OutputFormat::Json.extension(), "json");
        assert_eq!(OutputFormat::Html.extension(), "html");
        assert_eq!(OutputFormat::Text.extension(), "txt");
    }

    #[test]
    fn output_format_mime_type() {
        assert_eq!(OutputFormat::Svg.mime_type(), "image/svg+xml");
        assert_eq!(OutputFormat::Json.mime_type(), "application/json");
        assert_eq!(OutputFormat::Html.mime_type(), "text/html");
    }

    #[test]
    fn output_format_default() {
        assert_eq!(OutputFormat::default(), OutputFormat::Svg);
    }

    // ----- RenderConfig -----

    #[test]
    fn render_config_default() {
        let c = RenderConfig::new();
        assert_eq!(c.color_scheme_value(), ColorScheme::Default);
        assert!(c.is_show_legend());
        assert!(c.is_show_labels());
        assert!(c.is_show_values());
        assert!(c.is_sort_by_value());
        assert!(!c.is_reverse());
        assert_eq!(c.max_frames_value(), None);
    }

    #[test]
    fn render_config_color_scheme() {
        let c = RenderConfig::new().color_scheme(ColorScheme::Cool);
        assert_eq!(c.color_scheme_value(), ColorScheme::Cool);
    }

    #[test]
    fn render_config_title() {
        let c = RenderConfig::new().title("My Flame Graph");
        assert_eq!(c.title_value(), "My Flame Graph");
    }

    #[test]
    fn render_config_hide_legend() {
        let c = RenderConfig::new().hide_legend();
        assert!(!c.is_show_legend());
    }

    #[test]
    fn render_config_hide_labels() {
        let c = RenderConfig::new().hide_labels();
        assert!(!c.is_show_labels());
    }

    #[test]
    fn render_config_hide_values() {
        let c = RenderConfig::new().hide_values();
        assert!(!c.is_show_values());
    }

    #[test]
    fn render_config_reverse() {
        let c = RenderConfig::new().reverse();
        assert!(c.is_reverse());
    }

    #[test]
    fn render_config_max_frames() {
        let c = RenderConfig::new().max_frames(100);
        assert_eq!(c.max_frames_value(), Some(100));
    }

    #[test]
    fn render_config_format() {
        let c = RenderConfig::new().format(OutputFormat::Json);
        assert_eq!(c.format_value(), OutputFormat::Json);
    }

    #[test]
    fn render_config_layout() {
        let c = RenderConfig::new().layout(LayoutConfig::new().width(1200));
        assert_eq!(c.layout_value().width_value(), 1200);
    }

    #[test]
    fn render_config_palette() {
        let c = RenderConfig::new().palette(ColorPalette::cool());
        assert!(c.palette_value().primary().starts_with('#'));
    }

    #[test]
    fn render_config_to_json() {
        let c = RenderConfig::new();
        let json = c.to_json();
        assert!(json.contains("color_scheme"));
    }

    // ----- RenderOptions -----

    #[test]
    fn render_options_default() {
        let o = RenderOptions::new();
        assert_eq!(o.width_value(), 800);
        assert_eq!(o.height_value(), 600);
        assert_eq!(o.color_value(), "default");
        assert_eq!(o.title_value(), "Flame Graph");
    }

    #[test]
    fn render_options_builder() {
        let o = RenderOptions::new()
            .width(1200)
            .height(800)
            .color("cool")
            .title("Test");
        assert_eq!(o.width_value(), 1200);
        assert_eq!(o.height_value(), 800);
        assert_eq!(o.color_value(), "cool");
        assert_eq!(o.title_value(), "Test");
    }

    #[test]
    fn render_options_from_config() {
        let c = RenderConfig::new()
            .title("Custom")
            .layout(LayoutConfig::new().width(1000).height(500));
        let o = RenderOptions::from_config(&c);
        assert_eq!(o.width_value(), 1000);
        assert_eq!(o.height_value(), 500);
        assert_eq!(o.title_value(), "Custom");
    }

    #[test]
    fn render_options_to_config() {
        let o = RenderOptions::new().color("cool").title("Test");
        let c = o.to_config();
        assert_eq!(c.color_scheme_value(), ColorScheme::Cool);
        assert_eq!(c.title_value(), "Test");
    }

    #[test]
    fn render_options_to_config_default() {
        let o = RenderOptions::new();
        let c = o.to_config();
        assert_eq!(c.color_scheme_value(), ColorScheme::Default);
    }

    #[test]
    fn render_options_to_config_hot() {
        let o = RenderOptions::new().color("hot");
        let c = o.to_config();
        assert_eq!(c.color_scheme_value(), ColorScheme::Hot);
    }

    #[test]
    fn render_options_to_config_grayscale() {
        let o = RenderOptions::new().color("grayscale");
        let c = o.to_config();
        assert_eq!(c.color_scheme_value(), ColorScheme::Grayscale);
    }

    #[test]
    fn render_options_to_config_random() {
        let o = RenderOptions::new().color("random");
        let c = o.to_config();
        assert_eq!(c.color_scheme_value(), ColorScheme::Random);
    }
}
