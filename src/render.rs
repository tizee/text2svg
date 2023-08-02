use resvg::tiny_skia::Point;
use std::io::BufRead;
use std::path::PathBuf;
use svg::node::element::Rectangle;
use syntect::easy::HighlightFile;

use rustybuzz::Face;
use rustybuzz::GlyphBuffer;

use crate::font::{FontConfig, FontStyle};
use crate::highlight::{HighlightColor, HighlightFontStyle, HighlightSetting};
use crate::svg::Text;
use crate::utils::open_file_by_lines;

use svg::node::element::{Group, Style};
use svg::Document;
use syntect::highlighting::Style as TokenStyle;

pub fn render_file_highlight(
    file: &PathBuf,
    font_config: &mut FontConfig,
    highlight_setting: &HighlightSetting,
    output: PathBuf,
) {
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let syntax_set = &highlight_setting.syntax_set;
    let theme_set = &highlight_setting.theme_set;

    let mut doc = Document::new();

    if let Some(theme) = theme_set.themes.get(&highlight_setting.theme) {
        let mut highlighter = HighlightFile::new(file, syntax_set, theme).unwrap();
        for l in highlighter.reader.lines() {
            // render each line in a group tag
            let line = l.unwrap();

            if !line.is_empty() {
                let mut group = Group::new();
                let regions = highlighter
                    .highlight_lines
                    .highlight_line(line.as_str(), syntax_set)
                    .unwrap();
                let mut x: f32 = 0.0;
                for region in regions.iter() {
                    let style = region.0;
                    let token = region.1;
                    if let Some(text) =
                        render_token_to_path(x, height as f32, token, font_config, style)
                    {
                        x += text.width() as f32;
                        width = width.max(x as u32);
                        group = group.add(text.path);
                    }
                }
                doc = doc.add(group);
            }
            height += font_config.get_size();

        }

        let background_color = HighlightColor::new(theme.settings.background.unwrap());

        let background_rect = Rectangle::new()
            .set("width", width)
            .set("height", height)
            .set("fill", background_color.to_string());

        let children = doc.get_children_mut();
        children.insert(0, Box::new(background_rect));

        doc = doc
            .set("height", height)
            .set("width", width)
            .set("viewBox", format!("0 0 {} {}", width, height));

        svg::save(output, &doc).unwrap();
    }
}

pub fn render_token_to_path(
    x: f32,
    y: f32,
    token: &str,
    font_config: &mut FontConfig,
    style: TokenStyle,
) -> Option<Text> {
    let foreground_color = HighlightColor::new(style.foreground).to_string();
    let font_style = HighlightFontStyle::new(style.font_style).get_style();

    if font_config.get_debug() {
        println!("font style: {:?}",font_style);
    }

    // shape with harfbuzz algorithm
    if let Some(glyph_buffer) = text_shape(token, font_config, &font_style) {
        let mut svg_builder = Text::builder();
        svg_builder
            .set_origin(Point { x, y })
            .set_color(&foreground_color)
            .set_fill_color(&foreground_color);

        return Some(svg_builder.build(font_config,&font_style, &glyph_buffer));
    }
    None
}

pub fn render_text_to_path(x: f32, y: f32, line: &str, font_config: &mut FontConfig) -> Option<Text> {
    // shape with harfbuzz algorithm
    if let Some(glyph_buffer) = text_shape(line, font_config, &FontStyle::REGULAR) {
        let mut svg_builder = Text::builder();
        let color = font_config.get_color().as_str();
        let fill_color = font_config.get_fill_color().as_str();
        svg_builder
            .set_origin(Point { x, y })
            .set_color(color)
            .set_fill_color(fill_color);

        return Some(svg_builder.build(font_config, &FontStyle::REGULAR,&glyph_buffer));
    }
    None
}

pub fn render_text_file_to_svg(file: &PathBuf, font_config: &mut FontConfig, output: PathBuf) {
    let mut width: u32 = 0;
    let mut height: u32 = 0;

    if let Ok(lines) = open_file_by_lines(file) {
        let mut group = Group::new().set("class", "text");
        for line in lines.iter() {
            if line.is_empty() {
                height += font_config.get_size();
            } else if let Some(path_line) =
                render_text_to_path(0.0, height as f32, line, font_config)
            {
                width = width.max(path_line.width());
                height += path_line.height();
                group = group.add(path_line.path);
            }
        }

        let doc = Document::new()
            .set("height", height)
            .set("width", width)
            .set("viewBox", format!("0 0 {} {}", width, height))
            .add(group);

        svg::save(output, &doc).unwrap();
    }
}

pub fn render_text_to_svg_file(text: &str, font_config: &mut FontConfig, output: PathBuf) {
    // shape with harfbuzz algorithm
    if let Some(text_path) = render_text_to_path(0.0, 0.0, text, font_config) {
        let style = Style::new(
            "
  @keyframes draw {
    to {
      stroke-dashoffset: 0;
    }
  }

  .text {
    stroke-dasharray: 450 450;
    stroke-dashoffset: 450;
    animation: draw 2.3s ease forwards infinite;
  }
                               ",
        );
        let height = text_path.height();
        let width = text_path.width();
        let view_box = text_path.get_viewbox();

        let group = Group::new().set("class", "text").add(text_path.path);

        let doc = Document::new()
            .set("height", height)
            .set("width", width)
            .set("viewBox", view_box)
            .add(style)
            .add(group);

        svg::save(output, &doc).unwrap();
    }
}

/// Shape text with font default size (units_per_em)
/// Therefore we need to scale these glyphs later according to the size
fn text_shape(text: &str, font_config: &mut FontConfig, font_style: &FontStyle) -> Option<GlyphBuffer> {
    match font_style {
        FontStyle::ITALIC => {
            if !font_config.has_feature("ital") {
                font_config.add_feature("ital");
            }
        }
        _ => {
            if font_config.has_feature("ital") {
                font_config.remove_feature("ital");
            }
        }
    }
    if let Some(ft_face) = font_config.get_font_by_style(font_style) {
        if let Some(font_data) = ft_face.copy_font_data() {
            if let Some(hb_face) = Face::from_slice(&font_data, 0) {
                let mut buffer = rustybuzz::UnicodeBuffer::new();
                buffer.push_str(text);

                let glyph_buffer = rustybuzz::shape(&hb_face, font_config.get_features(), buffer);

                if font_config.get_debug() {
                    let format_flags = rustybuzz::SerializeFlags::default();
                    println!("{:?}", glyph_buffer.serialize(&hb_face, format_flags));
                }

                return Some(glyph_buffer);
            }
        }
    }
    None
}
