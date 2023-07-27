use anyhow::Result;
use resvg::tiny_skia::Point;
use std::path::PathBuf;

use rustybuzz::Face;
use rustybuzz::GlyphBuffer;

use crate::font::{FontConfig, FontStyle};
use crate::svg::Text;
use crate::utils::open_file_by_lines;

use svg::node::element::{Group, Style, Path};
use svg::Document;

pub fn render_line_to_path(x: f32, y: f32, line: &str, font_config: &FontConfig) -> Option<Text> {
    // shape with harfbuzz algorithm
    if let Some(glyph_buffer) = text_shape(line, font_config) {
        let mut svg_builder = Text::builder();
        svg_builder
            .set_origin(Point {
                x,
                y
            })
            .set_color(&font_config.color)
            .set_fill_color(&font_config.fill_color);

        return Some(svg_builder.build(font_config, &glyph_buffer));
    }
    return None;
}

pub fn render_text_file_to_svg(file: &PathBuf, font_config:&FontConfig, output: PathBuf) {
    let mut width: u32 = 0;
    let mut height: u32 = 0;

    if let Ok(lines) = open_file_by_lines(file) {
        let mut group = Group::new().set("class", "text");
        for line in lines.iter() {
            if line.is_empty() {
                height += font_config.size;
            }else if let Some(path_line) = render_line_to_path(0.0, height as f32,line,font_config) {
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

pub fn render_text_to_svg_file(text: &str, font_config: &FontConfig, output: PathBuf) {
    // shape with harfbuzz algorithm
    if let Some(text_path) = render_line_to_path(0.0, 0.0, text, font_config) {

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
fn text_shape(text: &str, font_config: &FontConfig) -> Option<GlyphBuffer> {
    if let Some(ft_face) = font_config.faces.get(&FontStyle::REGULAR) {
        if let Some(font_data) = ft_face.copy_font_data() {
            if let Some(hb_face) = Face::from_slice(&font_data, 0) {
                let mut buffer = rustybuzz::UnicodeBuffer::new();
                buffer.push_str(text);

                let glyph_buffer = rustybuzz::shape(&hb_face, &font_config.features, buffer);

                if font_config.debug {
                    let format_flags = rustybuzz::SerializeFlags::default();
                    println!("{:?}", glyph_buffer.serialize(&hb_face, format_flags));
                }

                return Some(glyph_buffer);
            }
        }
    }
    None
}
