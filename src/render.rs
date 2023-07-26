use std::path::PathBuf;

use rustybuzz::Face;
use rustybuzz::GlyphBuffer;

use crate::font::{FontConfig, FontStyle};
use crate::svg::Text;
use svg::node::element::{Group, Style};
use svg::Document;

pub fn render_text_to_svg_file(text: &str, font_config: &FontConfig, output: PathBuf) {
    // shape with harfbuzz algorithm
    if let Some(glyph_buffer) = text_shape(text, font_config) {
        let mut svg_builder = Text::builder();
        svg_builder
            .set_color(&font_config.color)
            .set_fill_color(&font_config.fill_color);

        let text_path = svg_builder.build(font_config, &glyph_buffer);

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
