use resvg::tiny_skia::Point;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::PathBuf;
use svg::node::element::{Definitions, Rectangle};

use syntect::easy::HighlightFile;

use rustybuzz::ttf_parser::Rect;
use rustybuzz::Face;
use rustybuzz::GlyphBuffer;

use crate::font::{FontConfig, FontStyle};
use crate::highlight::{HighlightColor, HighlightFontStyle, HighlightSetting};
use crate::svg::{GlyphCache, GlyphDefs, ShardCache, TextBuilder};
use crate::utils::open_file_by_lines;
use crate::utils::open_file_by_lines_pixel_width;
use crate::utils::open_file_by_lines_width;
use crate::utils::wrap_text_by_pixel_width;
use crate::utils::{wrap_text_knuth_plass, JustifiedLine};

use svg::node::element::{Group, Style};
use svg::Document;
use syntect::highlighting::Style as TokenStyle;

/// Text alignment for multi-line rendering.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "lower"))]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseTextAlignErr;

impl std::fmt::Display for ParseTextAlignErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid text alignment (expected: left, center, right, justify)"
        )
    }
}

impl std::str::FromStr for TextAlign {
    type Err = ParseTextAlignErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "left" => Ok(TextAlign::Left),
            "center" => Ok(TextAlign::Center),
            "right" => Ok(TextAlign::Right),
            "justify" => Ok(TextAlign::Justify),
            _ => Err(ParseTextAlignErr),
        }
    }
}

impl std::fmt::Display for TextAlign {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextAlign::Left => write!(f, "left"),
            TextAlign::Center => write!(f, "center"),
            TextAlign::Right => write!(f, "right"),
            TextAlign::Justify => write!(f, "justify"),
        }
    }
}

/// Animation preset applied to the generated SVG.
///
/// - `None`: static output.
/// - `Draw`: legacy per-line stroke "draw" effect (stroke-dashoffset).
/// - `Settle`: glyph shards fly in from their outline vector and assemble once.
/// - `Scatter`: glyph shards continuously scatter outward and reassemble (loop).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "lower"))]
pub enum Animation {
    #[default]
    None,
    Draw,
    Settle,
    Scatter,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseAnimationErr;

impl std::fmt::Display for ParseAnimationErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid animation (expected: none, draw, settle, scatter)"
        )
    }
}

impl std::str::FromStr for Animation {
    type Err = ParseAnimationErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(Animation::None),
            "draw" => Ok(Animation::Draw),
            "settle" => Ok(Animation::Settle),
            "scatter" => Ok(Animation::Scatter),
            _ => Err(ParseAnimationErr),
        }
    }
}

impl std::fmt::Display for Animation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Animation::None => write!(f, "none"),
            Animation::Draw => write!(f, "draw"),
            Animation::Settle => write!(f, "settle"),
            Animation::Scatter => write!(f, "scatter"),
        }
    }
}

// render config for non-highlight mode
pub struct RenderConfig {
    animation: Animation,
    hollow_hover: bool,
    explode: Option<f32>,
    font_style: FontStyle,
    max_width: Option<usize>,
    max_pixel_width: Option<f32>,
    align: TextAlign,
    justify: bool,
    hyphenate: bool,
}

impl RenderConfig {
    pub fn new(animation: Animation, style: FontStyle) -> Self {
        Self {
            animation,
            hollow_hover: false,
            explode: None,
            font_style: style,
            max_width: None,
            max_pixel_width: None,
            align: TextAlign::Left,
            justify: false,
            // Hyphenation helps justified paragraphs; on by default and only
            // takes effect on the justify path.
            hyphenate: true,
        }
    }

    pub fn set_max_width(&mut self, width: Option<usize>) -> &mut Self {
        self.max_width = width;
        self
    }

    pub fn set_max_pixel_width(&mut self, pixel_width: Option<f32>) -> &mut Self {
        self.max_pixel_width = pixel_width;
        self
    }

    pub fn set_align(&mut self, align: TextAlign) -> &mut Self {
        self.align = align;
        // Justified alignment requires Knuth-Plass optimal breaking.
        if align == TextAlign::Justify {
            self.justify = true;
        }
        self
    }

    /// Enable Knuth-Plass optimal line breaking (off by default to preserve
    /// existing greedy output).
    pub fn set_justify(&mut self, justify: bool) -> &mut Self {
        self.justify = justify;
        self
    }

    pub fn get_justify(&self) -> bool {
        self.justify
    }

    /// Enable/disable dictionary hyphenation (only effective on the justify path).
    pub fn set_hyphenate(&mut self, hyphenate: bool) -> &mut Self {
        self.hyphenate = hyphenate;
        self
    }

    pub fn get_hyphenate(&self) -> bool {
        self.hyphenate
    }

    pub fn get_font_style(&self) -> &FontStyle {
        &self.font_style
    }

    pub fn get_animation(&self) -> Animation {
        self.animation
    }

    /// Whether the active animation renders per-glyph shards (settle/scatter).
    pub fn emit_shards(&self) -> bool {
        matches!(self.animation, Animation::Settle | Animation::Scatter) || self.explode.is_some()
    }

    /// Whether the legacy per-line stroke "draw" effect is active.
    pub fn is_draw(&self) -> bool {
        self.animation == Animation::Draw
    }

    /// Whether shards travel over time (settle/scatter) rather than sitting in a
    /// static exploded pose.
    fn is_animated_shards(&self) -> bool {
        matches!(self.animation, Animation::Settle | Animation::Scatter)
    }

    /// Set the shard spread amplitude in pixels.
    pub fn set_explode(&mut self, explode: Option<f32>) -> &mut Self {
        self.explode = explode;
        self
    }

    pub fn get_explode(&self) -> Option<f32> {
        self.explode
    }

    /// Resolve the shard spread into `(animation_travel_px, static_offset_px)`.
    /// Only one is ever non-zero: settle/scatter animate the travel distance,
    /// while an explode without animation bakes a static outward offset.
    fn shard_spread(&self, size: u32) -> (f32, f32) {
        let amplitude = self.explode.unwrap_or(size as f32 * 1.5);
        if self.is_animated_shards() {
            (amplitude, 0.0)
        } else {
            (0.0, self.explode.unwrap_or(0.0))
        }
    }

    /// Enable the hollow-outline hover effect (solid fill turns to a translucent
    /// fill plus a visible stroke on hover).
    pub fn set_hollow_hover(&mut self, hollow_hover: bool) -> &mut Self {
        self.hollow_hover = hollow_hover;
        self
    }

    pub fn get_hollow_hover(&self) -> bool {
        self.hollow_hover
    }

    pub fn get_align(&self) -> TextAlign {
        self.align
    }
}

/// Render a highlighted file and return the SVG Document.
pub fn render_file_highlight_to_doc(
    file: &PathBuf,
    font_config: &mut FontConfig,
    highlight_setting: &HighlightSetting,
) -> Option<Document> {
    let mut max_width: u32 = 0;
    let mut current_height: u32 = 0;
    let line_height = font_config.get_size();

    let syntax_set = &highlight_setting.syntax_set;
    let theme_set = &highlight_setting.theme_set;

    let mut doc = Document::new();
    let mut glyph_cache: GlyphCache = HashMap::new();
    let mut glyph_defs: GlyphDefs = HashMap::new();
    // Highlight mode does not use shard animations; kept only to satisfy the
    // shared TextBuilder::build signature.
    let mut shard_cache: ShardCache = HashMap::new();
    let mut main_content = Group::new();

    let theme = theme_set.themes.get(&highlight_setting.theme)?;
    let mut highlighter = HighlightFile::new(file, syntax_set, theme).unwrap();

    let background_color = HighlightColor::new(theme.settings.background.unwrap()).to_string();

    for l in highlighter.reader.lines() {
        let line = l.unwrap();
        let mut line_group =
            Group::new().set("transform", format!("translate(0, {})", current_height));
        let mut current_x: f32 = 0.0;
        let mut line_max_x: f32 = 0.0;

        if !line.is_empty() {
            let regions = highlighter
                .highlight_lines
                .highlight_line(line.as_str(), syntax_set)
                .unwrap();

            for region in regions.iter() {
                let style = region.0;
                let token = region.1;
                if let Some((token_group, token_bbox)) = render_token(
                    current_x,
                    0.0,
                    token,
                    font_config,
                    style,
                    &mut glyph_cache,
                    &mut glyph_defs,
                    &mut shard_cache,
                ) {
                    let foreground_color = HighlightColor::new(style.foreground).to_string();
                    let styled_token_group = token_group
                        .set("fill", foreground_color.clone())
                        .set("stroke", foreground_color);

                    line_group = line_group.add(styled_token_group);
                    current_x += token_bbox.width() as f32;
                    line_max_x = current_x;
                }
            }
        }
        main_content = main_content.add(line_group);
        max_width = max_width.max(line_max_x.ceil() as u32);
        current_height += line_height;
    }

    let background_rect = Rectangle::new()
        .set("width", max_width)
        .set("height", current_height)
        .set("fill", background_color);

    let mut defs = Definitions::new();
    for node_box in glyph_defs.values() {
        defs = defs.add(node_box.clone());
    }

    doc = doc.add(defs);
    doc = doc.add(background_rect);
    doc = doc.add(main_content);

    doc = doc
        .set("height", current_height)
        .set("width", max_width)
        .set("viewBox", format!("0 0 {} {}", max_width, current_height));

    Some(doc)
}

pub fn render_file_highlight(
    file: &PathBuf,
    font_config: &mut FontConfig,
    highlight_setting: &HighlightSetting,
    output: PathBuf,
) {
    if let Some(doc) = render_file_highlight_to_doc(file, font_config, highlight_setting) {
        svg::save(output, &doc).unwrap();
    }
}

// Renders a token (part of a highlighted line)
#[allow(clippy::too_many_arguments)]
pub fn render_token(
    x: f32,
    y: f32,
    token: &str,
    font_config: &mut FontConfig,
    style: TokenStyle,
    glyph_cache: &mut GlyphCache,
    glyph_defs: &mut GlyphDefs,
    shard_cache: &mut ShardCache,
) -> Option<(Group, Rect)> {
    let font_style = HighlightFontStyle::new(style.font_style).get_style();

    if font_config.get_debug() {
        eprintln!("token: '{}', font style: {:?}", token, font_style);
    }

    if let Some(glyph_buffer) = text_shape(token, font_config, &font_style) {
        let mut svg_builder = TextBuilder::new();
        svg_builder.set_origin(Point { x, y });

        return Some(svg_builder.build(
            font_config,
            &font_style,
            &glyph_buffer,
            glyph_cache,
            glyph_defs,
            shard_cache,
        ));
    }
    None
}

// Renders a plain text line
#[allow(clippy::too_many_arguments)]
pub fn render_text_line(
    x: f32,
    y: f32,
    line: &str,
    font_config: &mut FontConfig,
    render_config: &RenderConfig,
    glyph_cache: &mut GlyphCache,
    glyph_defs: &mut GlyphDefs,
    shard_cache: &mut ShardCache,
) -> Option<(Group, Rect)> {
    let style = render_config.get_font_style();

    if let Some(glyph_buffer) = text_shape(line, font_config, style) {
        if font_config.get_debug() {
            eprintln!("shape line: {:?}", line);
        }
        let mut svg_builder = TextBuilder::new();
        svg_builder.set_origin(Point { x, y });
        if render_config.emit_shards() {
            let (travel, offset) = render_config.shard_spread(font_config.get_size());
            svg_builder.set_shards(true, travel);
            svg_builder.set_explode_offset(offset);
        }

        return Some(svg_builder.build(
            font_config,
            style,
            &glyph_buffer,
            glyph_cache,
            glyph_defs,
            shard_cache,
        ));
    }

    if font_config.get_debug() {
        eprintln!("failed to shape with harfbuzz:\n{:?}", line);
    }
    None
}

/// Render a single line (anchored at the local origin) with inter-word spaces
/// expanded by `ratio` (justify).
fn render_text_line_justified(
    line: &str,
    ratio: f32,
    font_config: &mut FontConfig,
    render_config: &RenderConfig,
    glyph_cache: &mut GlyphCache,
    glyph_defs: &mut GlyphDefs,
    shard_cache: &mut ShardCache,
) -> Option<(Group, Rect)> {
    let style = render_config.get_font_style();

    if let Some(glyph_buffer) = text_shape(line, font_config, style) {
        let mut svg_builder = TextBuilder::new();
        svg_builder.set_origin(Point { x: 0.0, y: 0.0 });
        svg_builder.set_justify_ratio(ratio);
        svg_builder.set_justify_text(line);
        if render_config.emit_shards() {
            let (travel, offset) = render_config.shard_spread(font_config.get_size());
            svg_builder.set_shards(true, travel);
            svg_builder.set_explode_offset(offset);
        }
        return Some(svg_builder.build(
            font_config,
            style,
            &glyph_buffer,
            glyph_cache,
            glyph_defs,
            shard_cache,
        ));
    }
    None
}

/// Build an SVG Document from Knuth-Plass justified lines.
///
/// Each non-final line's inter-word spaces are stretched by its ratio so the
/// line fills the target width; the last line of the paragraph stays ragged.
fn build_multiline_doc_justified(
    lines: &[JustifiedLine],
    font_config: &mut FontConfig,
    render_config: &RenderConfig,
) -> Option<Document> {
    let mut max_width: u32 = 0;
    let mut current_height: u32 = 0;
    let line_height = font_config.get_size();

    let mut doc = Document::new();
    let mut glyph_cache: GlyphCache = HashMap::new();
    let mut glyph_defs: GlyphDefs = HashMap::new();
    let mut shard_cache: ShardCache = HashMap::new();
    let ink = font_config.get_fill_color().to_string();
    let mut main_group = Group::new()
        .set("fill", font_config.get_fill_color().as_str())
        .set("stroke", font_config.get_color().as_str());

    let last_index = lines.len().saturating_sub(1);
    let mut rendered_lines: Vec<(Group, u32)> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        if line.text.is_empty() {
            rendered_lines.push((Group::new(), 0));
            continue;
        }
        // Only stretch when the optimizer asked to stretch (ratio > 0) and this
        // is not the final ragged line.
        let ratio = if idx == last_index || line.ratio <= 0.0 {
            0.0
        } else {
            line.ratio
        };

        if let Some((line_group, line_bbox)) = render_text_line_justified(
            &line.text,
            ratio,
            font_config,
            render_config,
            &mut glyph_cache,
            &mut glyph_defs,
            &mut shard_cache,
        ) {
            let line_w = line_bbox.width() as u32;
            max_width = max_width.max(line_w);
            rendered_lines.push((line_group, line_w));
        } else {
            rendered_lines.push((Group::new(), 0));
        }
    }

    for (line_index, (line_content_group, _line_w)) in rendered_lines.into_iter().enumerate() {
        let line_group_transform = format!("translate({}, {})", 0.0, current_height);
        let mut positioned_line_group = Group::new()
            .set("transform", line_group_transform)
            .add(line_content_group);

        if render_config.is_draw() {
            let animation_delay = line_index as f32 * 0.8;
            positioned_line_group = positioned_line_group
                .set("class", "text-line")
                .set("style", format!("animation-delay: {}s", animation_delay));
        }

        main_group = main_group.add(positioned_line_group);
        current_height += line_height;
    }

    main_group = decorate_main_group(main_group, render_config, &ink);

    let mut defs = Definitions::new();
    for node_box in glyph_defs.values() {
        defs = defs.add(node_box.clone());
    }
    doc = doc.add(defs);
    doc = doc.add(main_group);

    if let Some(style) = build_style(render_config) {
        doc = doc.add(style);
    }

    doc = doc
        .set("height", current_height)
        .set("width", max_width)
        // overflow:visible so animated/exploded shards are not clipped when the
        // SVG is embedded in HTML.
        .set("overflow", "visible")
        .set("viewBox", format!("0 0 {} {}", max_width, current_height));

    Some(doc)
}

/// Legacy per-line stroke "draw" effect.
const DRAW_CSS: &str = r#"
  @keyframes draw { to { stroke-dashoffset: 0; } }
  .text-line {
    stroke-dasharray: 1000 1000;
    stroke-dashoffset: 1000;
    animation: draw 1.5s ease forwards;
  }
"#;

// The settle/scatter glyph-shard animations below are adapted from yuragi
// (https://github.com/lawvs/yuragi, MIT (c) lawvs and CircuitCoder).

/// Shards fade in from their outline vector and assemble once.
const SETTLE_CSS: &str = r#"
  .shard { transform-box: fill-box; transform-origin: center; animation-fill-mode: both; }
  @keyframes shard-settle {
    from { opacity: 0; transform: translate(var(--dx, 0), var(--dy, 0)) scale(1.05); }
    60%  { opacity: 1; }
    to   { opacity: 1; transform: translate(0, 0) scale(1); }
  }
  .text-anim-settle .shard {
    animation-name: shard-settle;
    animation-duration: 0.7s;
    animation-timing-function: cubic-bezier(0, 0, 0, 1);
  }
  @media (prefers-reduced-motion: reduce) { .shard { animation: none; opacity: 1; } }
"#;

/// Shards continuously scatter outward and fade out, then fade back in and
/// reassemble (a looping fade-out / fade-in sway).
const SCATTER_CSS: &str = r#"
  .shard { transform-box: fill-box; transform-origin: center; animation-fill-mode: both; }
  @keyframes shard-scatter {
    0%   { opacity: 1; transform: translate(0, 0) scale(1); }
    45%  { opacity: 0; transform: translate(var(--dx, 0), var(--dy, 0)) scale(0.9); }
    55%  { opacity: 0; transform: translate(var(--dx, 0), var(--dy, 0)) scale(0.9); }
    100% { opacity: 1; transform: translate(0, 0) scale(1); }
  }
  .text-anim-scatter .shard {
    animation-name: shard-scatter;
    animation-duration: 2.6s;
    animation-iteration-count: infinite;
    animation-timing-function: cubic-bezier(0.22, 1, 0.36, 1);
  }
  @media (prefers-reduced-motion: reduce) { .shard { animation: none; opacity: 1; } }
"#;

/// Hollow-outline hover: the solid fill fades to a translucent tint and a
/// stroke appears, so the glyphs read as outlines while hovered.
const HOLLOW_HOVER_CSS: &str = r#"
  .hollow-target { transition: fill 0.25s ease, stroke 0.25s ease; }
  svg:hover .hollow-target {
    fill: color-mix(in srgb, var(--ink, #000) 22%, transparent);
    stroke: var(--ink, #000);
  }
"#;

/// Build the combined `<style>` block for the active animation and hover
/// effect, or `None` when the output is fully static.
fn build_style(render_config: &RenderConfig) -> Option<Style> {
    let mut css = String::new();
    match render_config.get_animation() {
        Animation::Draw => css.push_str(DRAW_CSS),
        Animation::Settle => css.push_str(SETTLE_CSS),
        Animation::Scatter => css.push_str(SCATTER_CSS),
        Animation::None => {}
    }
    if render_config.get_hollow_hover() {
        css.push_str(HOLLOW_HOVER_CSS);
    }
    if css.is_empty() {
        None
    } else {
        Some(Style::new(css))
    }
}

/// Apply animation/hover classes (and the hover ink variable) to the top-level
/// text group.
fn decorate_main_group(mut group: Group, render_config: &RenderConfig, ink: &str) -> Group {
    let mut classes: Vec<&str> = Vec::new();
    match render_config.get_animation() {
        Animation::Settle => classes.push("text-anim-settle"),
        Animation::Scatter => classes.push("text-anim-scatter"),
        _ => {}
    }
    if render_config.get_hollow_hover() {
        classes.push("hollow-target");
        group = group.set("style", format!("--ink:{}", ink));
    }
    if !classes.is_empty() {
        group = group.set("class", classes.join(" "));
    }
    group
}

/// Build an SVG Document from multiple text lines (shared logic for file and text rendering).
fn build_multiline_doc(
    lines: &[String],
    font_config: &mut FontConfig,
    render_config: &RenderConfig,
) -> Option<Document> {
    let mut max_width: u32 = 0;
    let mut current_height: u32 = 0;
    let line_height = font_config.get_size();

    let mut doc = Document::new();
    let mut glyph_cache: GlyphCache = HashMap::new();
    let mut glyph_defs: GlyphDefs = HashMap::new();
    let mut shard_cache: ShardCache = HashMap::new();
    let ink = font_config.get_fill_color().to_string();
    let mut main_group = Group::new();
    main_group = main_group
        .set("fill", font_config.get_fill_color().as_str())
        .set("stroke", font_config.get_color().as_str());

    let mut rendered_lines: Vec<(Group, u32)> = Vec::new();

    for line in lines.iter() {
        if line.is_empty() {
            rendered_lines.push((Group::new(), 0));
        } else if let Some((line_content_group, line_bbox)) = render_text_line(
            0.0,
            0.0,
            line,
            font_config,
            render_config,
            &mut glyph_cache,
            &mut glyph_defs,
            &mut shard_cache,
        ) {
            let line_w = line_bbox.width() as u32;
            max_width = max_width.max(line_w);
            rendered_lines.push((line_content_group, line_w));
        } else {
            rendered_lines.push((Group::new(), 0));
        }
    }

    let align = render_config.get_align();
    for (line_index, (line_content_group, line_w)) in rendered_lines.into_iter().enumerate() {
        let x_offset = match align {
            // Justify is rendered by expanding intra-line glue (task #3), so the
            // line group itself stays left-anchored.
            TextAlign::Left | TextAlign::Justify => 0.0,
            TextAlign::Center => (max_width as f32 - line_w as f32) / 2.0,
            TextAlign::Right => max_width as f32 - line_w as f32,
        };
        let line_group_transform = format!("translate({}, {})", x_offset, current_height);
        let mut positioned_line_group = Group::new()
            .set("transform", line_group_transform)
            .add(line_content_group);

        if render_config.is_draw() {
            let animation_delay = line_index as f32 * 0.8;
            positioned_line_group = positioned_line_group
                .set("class", "text-line")
                .set("style", format!("animation-delay: {}s", animation_delay));
        }

        main_group = main_group.add(positioned_line_group);
        current_height += line_height;
    }

    main_group = decorate_main_group(main_group, render_config, &ink);

    let mut defs = Definitions::new();
    for node_box in glyph_defs.values() {
        defs = defs.add(node_box.clone());
    }
    doc = doc.add(defs);
    doc = doc.add(main_group);

    if let Some(style) = build_style(render_config) {
        doc = doc.add(style);
    }

    if let Some(style) = build_style(render_config) {
        doc = doc.add(style);
    }

    doc = doc
        .set("height", current_height)
        .set("width", max_width)
        // overflow:visible so animated/exploded shards are not clipped when the
        // SVG is embedded in HTML.
        .set("overflow", "visible")
        .set("viewBox", format!("0 0 {} {}", max_width, current_height));

    Some(doc)
}

/// Render a text file to an SVG Document.
pub fn render_text_file_to_svg_doc(
    file: &PathBuf,
    font_config: &mut FontConfig,
    render_config: &RenderConfig,
) -> Option<Document> {
    let file_lines = if let Some(pixel_width) = render_config.max_pixel_width {
        open_file_by_lines_pixel_width(
            file,
            pixel_width,
            font_config,
            render_config.get_font_style(),
        )
    } else if let Some(char_width) = render_config.max_width {
        open_file_by_lines_width(file, char_width)
    } else {
        open_file_by_lines(file)
    };

    if font_config.get_debug() {
        eprintln!("file lines : {:?}", file_lines);
    }

    if let Ok(lines) = file_lines {
        build_multiline_doc(&lines, font_config, render_config)
    } else {
        None
    }
}

pub fn render_text_file_to_svg(
    file: &PathBuf,
    font_config: &mut FontConfig,
    render_config: &RenderConfig,
    output: PathBuf,
) {
    if let Some(doc) = render_text_file_to_svg_doc(file, font_config, render_config) {
        svg::save(output, &doc).unwrap();
    }
}

/// Render text to an SVG Document.
pub fn render_text_to_svg(
    text: &str,
    font_config: &mut FontConfig,
    render_config: &RenderConfig,
) -> Option<Document> {
    let mut glyph_cache: GlyphCache = HashMap::new();
    let mut glyph_defs: GlyphDefs = HashMap::new();
    let mut shard_cache: ShardCache = HashMap::new();

    // Justified rendering takes a dedicated path: Knuth-Plass breaking plus
    // per-line glue expansion at render time.
    if render_config.get_justify() {
        if let Some(pixel_width) = render_config.max_pixel_width {
            let justified = wrap_text_knuth_plass(
                text,
                pixel_width,
                font_config,
                render_config.get_font_style(),
                render_config.get_hyphenate(),
            );
            return build_multiline_doc_justified(&justified, font_config, render_config);
        }
    }

    // Handle text wrapping if pixel width is specified
    let text_lines = if let Some(pixel_width) = render_config.max_pixel_width {
        wrap_text_by_pixel_width(
            text,
            pixel_width,
            font_config,
            render_config.get_font_style(),
        )
    } else {
        vec![text.to_string()]
    };

    // If we have multiple lines, use the multiline builder
    if text_lines.len() > 1 {
        return build_multiline_doc(&text_lines, font_config, render_config);
    }

    // Single line rendering
    let text_to_render = &text_lines[0];

    if let Some((text_content_group, text_bbox)) = render_text_line(
        0.0,
        0.0,
        text_to_render,
        font_config,
        render_config,
        &mut glyph_cache,
        &mut glyph_defs,
        &mut shard_cache,
    ) {
        let height = text_bbox.height() as u32;
        let width = text_bbox.width() as u32;
        let view_box = format!(
            "{} {} {} {}",
            text_bbox.x_min,
            text_bbox.y_min,
            text_bbox.width(),
            text_bbox.height()
        );

        let ink = font_config.get_fill_color().to_string();
        let mut styled_group = text_content_group
            .set("fill", font_config.get_fill_color().as_str())
            .set("stroke", font_config.get_color().as_str());

        // Single line: the styled group is itself the shard/draw container, so
        // classes go directly on it (no per-line wrapper exists here).
        let mut classes: Vec<&str> = Vec::new();
        match render_config.get_animation() {
            Animation::Draw => classes.push("text-line"),
            Animation::Settle => classes.push("text-anim-settle"),
            Animation::Scatter => classes.push("text-anim-scatter"),
            Animation::None => {}
        }
        if render_config.get_hollow_hover() {
            classes.push("hollow-target");
            styled_group = styled_group.set("style", format!("--ink:{}", ink));
        }
        if !classes.is_empty() {
            styled_group = styled_group.set("class", classes.join(" "));
        }

        let mut defs = Definitions::new();
        for node_box in glyph_defs.values() {
            defs = defs.add(node_box.clone());
        }
        let mut doc = Document::new();
        doc = doc.add(defs);
        doc = doc.add(styled_group);

        if let Some(style) = build_style(render_config) {
            doc = doc.add(style);
        }

        doc = doc
            .set("height", height)
            .set("width", width)
            .set("overflow", "visible")
            .set("viewBox", view_box);

        Some(doc)
    } else {
        eprintln!("Failed to render text to SVG.");
        None
    }
}

pub fn render_text_to_svg_file(
    text: &str,
    font_config: &mut FontConfig,
    render_config: &RenderConfig,
    output: PathBuf,
) {
    if let Some(doc) = render_text_to_svg(text, font_config, render_config) {
        svg::save(output, &doc).unwrap();
    }
}

/// Shape text with font default size (units_per_em)
/// Therefore we need to scale these glyphs later according to the size
fn text_shape(
    text: &str,
    font_config: &mut FontConfig,
    font_style: &FontStyle,
) -> Option<GlyphBuffer> {
    let ft_face = font_config
        .get_font_by_style(font_style)
        .or_else(|| {
            if font_config.get_debug() && *font_style != FontStyle::Regular {
                eprintln!(
                    "Warning: Font style {:?} not found, falling back to Regular.",
                    font_style
                );
            }
            font_config.get_font_by_style(&FontStyle::Regular)
        })
        .or_else(|| {
            eprintln!(
                "Error: Regular font style not found either for font '{}'.",
                font_config.get_font_name()
            );
            None
        });

    if let Some(ft_face) = ft_face {
        if let Some(font_data) = ft_face.copy_font_data() {
            if let Some(mut hb_face) = Face::from_slice(&font_data, 0) {
                hb_face.set_variations(font_config.get_variations());
                let mut buffer = rustybuzz::UnicodeBuffer::new();
                buffer.push_str(text);

                let glyph_buffer = rustybuzz::shape(&hb_face, font_config.get_features(), buffer);

                if font_config.get_debug() {
                    let format_flags = rustybuzz::SerializeFlags::default();
                    eprintln!(
                        "rustybuzz shape output:\n {:?}",
                        glyph_buffer.serialize(&hb_face, format_flags)
                    );
                }

                return Some(glyph_buffer);
            } else {
                eprintln!(
                    "Failed to create rustybuzz::Face from font data for font '{}', style {:?}.",
                    font_config.get_font_name(),
                    font_style
                );
            }
        } else {
            eprintln!(
                "Failed to copy font data for font '{}', style {:?}.",
                font_config.get_font_name(),
                font_style
            );
        }
    }

    None
}
