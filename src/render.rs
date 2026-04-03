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
use crate::svg::{GlyphCache, GlyphDefs, TextBuilder};
use crate::utils::open_file_by_lines;
use crate::utils::open_file_by_lines_pixel_width;
use crate::utils::open_file_by_lines_width;
use crate::utils::wrap_text_by_pixel_width;

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
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseTextAlignErr;

impl std::fmt::Display for ParseTextAlignErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid text alignment (expected: left, center, right)")
    }
}

impl std::str::FromStr for TextAlign {
    type Err = ParseTextAlignErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "left" => Ok(TextAlign::Left),
            "center" => Ok(TextAlign::Center),
            "right" => Ok(TextAlign::Right),
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
        }
    }
}

// render config for non-highlight mode
pub struct RenderConfig {
    animate: bool,
    font_style: FontStyle,
    max_width: Option<usize>,
    max_pixel_width: Option<f32>,
    align: TextAlign,
}

impl RenderConfig {
    pub fn new(animate: bool, style: FontStyle) -> Self {
        Self {
            animate,
            font_style: style,
            max_width: None,
            max_pixel_width: None,
            align: TextAlign::Left,
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
        self
    }

    pub fn get_font_style(&self) -> &FontStyle {
        &self.font_style
    }

    pub fn get_animate(&self) -> bool {
        self.animate
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
    for (_id, node_box) in glyph_defs.iter() {
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
pub fn render_token(
    x: f32,
    y: f32,
    token: &str,
    font_config: &mut FontConfig,
    style: TokenStyle,
    glyph_cache: &mut GlyphCache,
    glyph_defs: &mut GlyphDefs,
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
        ));
    }
    None
}

// Renders a plain text line
pub fn render_text_line(
    x: f32,
    y: f32,
    line: &str,
    font_config: &mut FontConfig,
    render_config: &RenderConfig,
    glyph_cache: &mut GlyphCache,
    glyph_defs: &mut GlyphDefs,
) -> Option<(Group, Rect)> {
    let style = render_config.get_font_style();

    if let Some(glyph_buffer) = text_shape(line, font_config, style) {
        if font_config.get_debug() {
            eprintln!("shape line: {:?}", line);
        }
        let mut svg_builder = TextBuilder::new();
        svg_builder.set_origin(Point { x, y });

        return Some(svg_builder.build(font_config, style, &glyph_buffer, glyph_cache, glyph_defs));
    }

    if font_config.get_debug() {
        eprintln!("failed to shape with harfbuzz:\n{:?}", line);
    }
    None
}

fn get_animation_style() -> Style {
    Style::new(
        "
  @keyframes draw {
    to {
      stroke-dashoffset: 0;
    }
  }

  .text-line {
    /* Adjust stroke-dasharray based on expected max path length if needed */
    stroke-dasharray: 1000 1000;
    stroke-dashoffset: 1000;
    animation: draw 1.5s ease forwards;
  }",
    )
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
            TextAlign::Left => 0.0,
            TextAlign::Center => (max_width as f32 - line_w as f32) / 2.0,
            TextAlign::Right => max_width as f32 - line_w as f32,
        };
        let line_group_transform = format!("translate({}, {})", x_offset, current_height);
        let mut positioned_line_group = Group::new()
            .set("transform", line_group_transform)
            .add(line_content_group);

        if render_config.get_animate() {
            let animation_delay = line_index as f32 * 0.8;
            positioned_line_group = positioned_line_group
                .set("class", "text-line")
                .set("style", format!("animation-delay: {}s", animation_delay));
        }

        main_group = main_group.add(positioned_line_group);
        current_height += line_height;
    }

    let mut defs = Definitions::new();
    for (_id, node_box) in glyph_defs.iter() {
        defs = defs.add(node_box.clone());
    }
    doc = doc.add(defs);
    doc = doc.add(main_group);

    if render_config.get_animate() {
        doc = doc.add(get_animation_style());
    }

    doc = doc
        .set("height", current_height)
        .set("width", max_width)
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

        let mut styled_group = text_content_group
            .set("fill", font_config.get_fill_color().as_str())
            .set("stroke", font_config.get_color().as_str());
        if render_config.get_animate() {
            styled_group = styled_group.set("class", "text-line");
        }

        let mut defs = Definitions::new();
        for (_id, node_box) in glyph_defs.iter() {
            defs = defs.add(node_box.clone());
        }
        let mut doc = Document::new();
        doc = doc.add(defs);
        doc = doc.add(styled_group);

        if render_config.get_animate() {
            doc = doc.add(get_animation_style());
        }

        doc = doc
            .set("height", height)
            .set("width", width)
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
            if let Some(hb_face) = Face::from_slice(&font_data, 0) {
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
