/// StrokeLineCap specifies the shape to be used at the end of open subpaths when stroked
use resvg::tiny_skia::LineCap as StrokeLineCap;
/// StrokeLineJoin specifies the shape to be used at the corners of paths when stroked
use resvg::tiny_skia::LineJoin as StrokeLineJoin;
use resvg::tiny_skia::Point;
use resvg::usvg::StrokeWidth;
use std::fmt::Write;

use crate::font::{FontConfig, FontStyle};
use rustybuzz::ttf_parser;
use rustybuzz::ttf_parser::{GlyphId, Rect};
use rustybuzz::Face;

use rustybuzz::GlyphBuffer;
use svg::node::element::Path;

/// path configuration for SVG1.1 https://www.w3.org/TR/SVG11/painting.html
pub struct PathConfig {
    pub stroke_width: StrokeWidth,
    pub stroke_linecap: StrokeLineCap,
    pub stroke_linejoin: StrokeLineJoin,
}

impl PathConfig {
    pub fn get_stroke_linejoin(&self) -> String {
        match self.stroke_linejoin {
            StrokeLineJoin::Round => {
                return "round".to_string();
            }
            StrokeLineJoin::Miter => {
                return "miter".to_string();
            }
            StrokeLineJoin::Bevel => {
                return "bevel".to_string();
            }
        }
    }

    pub fn get_stroke_linecap(&self) -> String {
        match self.stroke_linecap {
            StrokeLineCap::Round => {
                return "round".to_string();
            }
            StrokeLineCap::Butt => {
                return "butt".to_string();
            }
            StrokeLineCap::Square => {
                return "square".to_string();
            }
        }
    }
}

impl Default for PathConfig {
    fn default() -> Self {
        Self {
            stroke_width: StrokeWidth::new(1.0).unwrap(),
            stroke_linejoin: StrokeLineJoin::Round,
            stroke_linecap: StrokeLineCap::Round,
        }
    }
}

pub struct Text {
    pub path: Path,
    pub bounding_box: Rect,
}

impl Text {
    pub fn new(path: Path, bounding_box: Rect) -> Self {
        Self { path, bounding_box }
    }

    pub fn builder() -> TextBuilder<'static> {
        TextBuilder::default()
    }

    pub fn get_viewbox(&self) -> (u32, u32, u32, u32) {
        (
            self.bounding_box.x_min as u32,
            self.bounding_box.y_min as u32,
            self.bounding_box.width() as u32,
            self.bounding_box.height() as u32,
        )
    }

    pub fn width(&self) -> u32 {
        self.bounding_box.width() as u32
    }

    pub fn height(&self) -> u32 {
        self.bounding_box.height() as u32
    }
}

pub struct TextBuilder<'a> {
    pub origin: Point,
    pub color: &'a str,
    pub fill_color: &'a str,
    pub path_config: PathConfig,
}

impl Default for TextBuilder<'_> {
    fn default() -> Self {
        Self {
            origin: Point { x: 0.0, y: 0.0 },
            color: "#000",
            fill_color: "#000",
            path_config: PathConfig::default(),
        }
    }
}

impl<'a> TextBuilder<'a> {
    pub fn set_origin(&mut self, o: Point) -> &mut Self {
        self.origin = o;
        self
    }

    pub fn set_fill_color(&mut self, color: &'a str) -> &mut Self {
        self.fill_color = color;
        self
    }

    pub fn set_color(&mut self, color: &'a str) -> &mut Self {
        self.color = color;
        self
    }

    pub fn build(&self, font_config: &FontConfig, glyphs: &GlyphBuffer) -> Text {
        let ft_face = font_config.get_regular_font().unwrap();
        let metrics = ft_face.metrics();

        let origin_glyph_height = metrics.ascent - metrics.descent;
        // target size
        let glyph_height = font_config.size as f32;
        // factor used to convert origin size to given size
        let scale_factor = glyph_height / origin_glyph_height;

        if font_config.debug {
            println!(
                "origin height: {:?} scaled height: {:?} scale_factor:{:?} units_per_em:{:?}",
                origin_glyph_height, glyph_height, scale_factor, metrics.units_per_em
            );
        }

        let ft_face_data = &ft_face.copy_font_data().unwrap();
        let hb_face = Face::from_slice(ft_face_data, 0).unwrap();

        let glyph_num = glyphs.len();
        let glyph_positions = glyphs.glyph_positions();
        let glyph_infos = glyphs.glyph_infos();

        let mut x = self.origin.x;
        let mut d = String::new();

        let mut prev_space_glyph = true;
        let letter_space =
            scale_factor * font_config.letter_space * metrics.units_per_em as f32;
        let mut y_offset = i16::MAX;

        // convert glyph outlines to svg
        for i in 0..glyph_num {
            let glyph_id = glyph_infos[i].glyph_id;
            let glyph_pos = glyph_positions[i];

            if font_config.debug {
                println!(
                    "{:?}/{:?} x:{:?} glyph id: {:?} {:?} ",
                    i + 1,
                    glyph_num,
                    x,
                    glyph_id,
                    glyph_positions[i]
                );
            }

            x += if !prev_space_glyph { letter_space } else { 0.0 };

            // uniform scale
            // Note that the scale_y should be negative by adding a minus symbol to flip vertically to render correctly
            let mut glyph_builder = GlyphPathBuilder::new(
                scale_factor,
                -scale_factor,
                x,
                self.origin.y + glyph_height,
                &mut d,
            );

            let x_offset = if let Some(hb_bbox) =
                hb_face.outline_glyph(GlyphId(glyph_id as u16), &mut glyph_builder)
            {
                prev_space_glyph = false;
                if font_config.debug {
                    println!("bbox for glyph: {:?}", hb_bbox);
                }
                if hb_bbox.y_min < y_offset {
                    y_offset = hb_bbox.y_min;
                }
                // TODO: non-monospace font
                glyph_pos.x_advance as f32 * scale_factor
            } else {
                prev_space_glyph = true;
                // For the space glyph, we use its advance as its width
                glyph_pos.x_advance as f32 * scale_factor
            };

            // next glyph
            x += x_offset;
        }

        let bbox = Rect {
            x_min: self.origin.x.ceil() as i16,
            y_min: self.origin.y.ceil() as i16,
            x_max: (x + letter_space).ceil() as i16,
            y_max: (self.origin.y + glyph_height + y_offset.abs() as f32 * scale_factor).ceil() as i16,
        };

        if font_config.debug {
            println!(
                "x_min:{:?} y_min:{:?} x_max:{:?} y_max:{:?}",
                bbox.x_min, bbox.y_min, bbox.x_max, bbox.y_max
            );
        }

        Text::new(
            Path::new()
                .set("fill", self.fill_color)
                .set("stroke", self.color)
                .set("stroke-width", self.path_config.stroke_width.get())
                .set("stroke-linejoin", self.path_config.get_stroke_linejoin())
                .set("stroke-linecap", self.path_config.get_stroke_linecap())
                .set("d", d),
                bbox
        )
    }
}

pub struct GlyphPathBuilder<'a> {
    pub scale_x: f32,
    pub scale_y: f32,
    pub x: f32,
    pub y: f32,
    pub d: &'a mut String,
}

impl<'a> GlyphPathBuilder<'a> {
    fn new(scale_x: f32, scale_y: f32, x: f32, y: f32, d: &'a mut String) -> Self {
        Self {
            scale_x,
            scale_y,
            x,
            y,
            d,
        }
    }
}

impl ttf_parser::OutlineBuilder for GlyphPathBuilder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        write!(
            self.d,
            "M {} {}",
            self.x + x * self.scale_x,
            self.y + y * self.scale_y
        )
        .unwrap();
    }

    fn line_to(&mut self, x: f32, y: f32) {
        write!(
            self.d,
            "L {} {}",
            self.x + x * self.scale_x,
            self.y + y * self.scale_y
        )
        .unwrap();
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        write!(
            self.d,
            "Q {} {} {} {}",
            x1 * self.scale_x + self.x,
            y1 * self.scale_y + self.y,
            x * self.scale_x + self.x,
            y * self.scale_y + self.y
        )
        .unwrap();
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        write!(
            self.d,
            "C {} {} {} {} {} {}",
            x1 * self.scale_x + self.x,
            y1 * self.scale_y + self.y,
            x2 * self.scale_x + self.x,
            y2 * self.scale_y + self.y,
            x * self.scale_x + self.x,
            y * self.scale_y + self.y
        )
        .unwrap();
    }

    fn close(&mut self) {
        write!(self.d, "Z ").unwrap();
    }
}
