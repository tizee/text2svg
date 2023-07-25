use std::path::PathBuf;

use rustybuzz::Face;
use rustybuzz::GlyphBuffer;

use crate::font::{FontConfig, FontStyle};
use crate::svg::Text;
use svg::Document;
use svg::node::element::Animate;

pub fn render_text_to_svg_file(text: &str, font_config: &FontConfig, output: PathBuf) {
    // shape with harfbuzz algorithm
    if let Some(glyph_buffer) = text_shape(text, font_config) {

        let svg_builder = Text::builder();
        let text_path = svg_builder.build(font_config, &glyph_buffer);

        let doc = Document::new()
            .set("height",text_path.height())
            .set("width",text_path.width())
            .set("viewBox",text_path.get_viewbox())
            .add(text_path.path);

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
