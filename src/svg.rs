use std::collections::HashMap;
/// StrokeLineCap specifies the shape to be used at the end of open subpaths when stroked
use resvg::tiny_skia::LineCap as StrokeLineCap;
/// StrokeLineJoin specifies the shape to be used at the corners of paths when stroked
use resvg::tiny_skia::LineJoin as StrokeLineJoin;
use resvg::tiny_skia::Point;
// use resvg::usvg::StrokeWidth; // Removed unused import
use std::fmt::Write;

use crate::font::{FontConfig, FontStyle};
use rustybuzz::ttf_parser;
use rustybuzz::ttf_parser::{GlyphId, Rect};
use rustybuzz::Face;

use rustybuzz::GlyphBuffer;
use svg::node::element::{Path, Group, Use}; // Removed Definitions import
use svg::Node; // Added Node


// --- Glyph Cache and Definitions ---
pub type GlyphCache = HashMap<u16, String>; // GlyphId -> SVG ID (e.g., "g123")
// Store Box<dyn Node> because Node trait object is not Sized
pub type GlyphDefs = HashMap<String, Box<dyn Node>>; // SVG ID -> Boxed <path> Node for <defs>


/// path configuration for SVG1.1 https://www.w3.org/TR/SVG11/painting.html
#[derive(Clone, Debug)]
pub struct PathConfig {
    pub stroke_width: f32, // Store as f32 for easier use with svg crate
    pub stroke_linecap: StrokeLineCap,
    pub stroke_linejoin: StrokeLineJoin,
}

impl PathConfig {
    pub fn get_stroke_linejoin(&self) -> String {
        match self.stroke_linejoin {
            StrokeLineJoin::Round => "round".to_string(),
            StrokeLineJoin::Miter => "miter".to_string(),
            StrokeLineJoin::Bevel => "bevel".to_string(),
            StrokeLineJoin::MiterClip => "miter".to_string(),
        }
    }

    pub fn get_stroke_linecap(&self) -> String {
        match self.stroke_linecap {
            StrokeLineCap::Round => "round".to_string(),
            StrokeLineCap::Butt => "butt".to_string(),
            StrokeLineCap::Square => "square".to_string(),
        }
    }
}

impl Default for PathConfig {
    fn default() -> Self {
        Self {
            // Default stroke width, can be overridden by fill/color settings later
            stroke_width: 1.0,
            stroke_linejoin: StrokeLineJoin::Round,
            stroke_linecap: StrokeLineCap::Round,
        }
    }
}

// TextBuilder is now responsible for generating a group of <use> elements
// and managing glyph definitions.
pub struct TextBuilder {
    pub origin: Point, // Top-left origin for the start of the text block
    pub path_config: PathConfig,
}

impl Default for TextBuilder {
    fn default() -> Self {
        Self {
            origin: Point { x: 0.0, y: 0.0 },
            path_config: PathConfig::default(),
        }
    }
}

impl TextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_origin(&mut self, o: Point) -> &mut Self {
        self.origin = o;
        self
    }

    // Removed set_color and set_fill_color, as these are applied later
    // to the group containing the <use> elements.

    /// Builds a group of <use> elements for the given glyphs.
    /// Manages glyph definitions, adding new ones to `glyph_defs` and `glyph_cache`.
    /// Returns the <g> element containing <use> elements and the calculated bounding box.
    pub fn build(
        &self,
        font_config: &FontConfig,
        font_style: &FontStyle,
        glyphs: &GlyphBuffer,
        glyph_cache: &mut GlyphCache,
        glyph_defs: &mut GlyphDefs, // Takes mutable reference to HashMap<String, Box<dyn Node>>
    ) -> (Group, Rect) { // Rect uses i16
        let ft_face = font_config.get_font_by_style(font_style)
            .or_else(|| font_config.get_font_by_style(&FontStyle::Regular))
            .expect("Font face (style or regular) not found during build"); // Should have been checked earlier

        let metrics = ft_face.metrics();
        let origin_glyph_height = metrics.ascent - metrics.descent;
        let target_glyph_height = font_config.get_size() as f32;
        let scale_factor = target_glyph_height / origin_glyph_height.max(1.0); // Avoid division by zero

        if font_config.get_debug() {
            println!(
                "Build Scale: origin_h={:?}, target_h={:?}, scale_factor={:?}, units/em={:?}",
                origin_glyph_height, target_glyph_height, scale_factor, metrics.units_per_em
            );
        }

        let ft_face_data = ft_face.copy_font_data().expect("Failed to copy font data");
        let hb_face = Face::from_slice(&ft_face_data, 0).expect("Failed to create rustybuzz::Face");

        let glyph_num = glyphs.len();
        let glyph_positions = glyphs.glyph_positions();
        let glyph_infos = glyphs.glyph_infos();

        let mut current_x = self.origin.x;
        // The y origin for <use> should account for the font's ascent scaled to the target size.
        // This positions the baseline correctly.
        let base_y = self.origin.y + metrics.ascent * scale_factor;
        let mut use_group = Group::new();

        let letter_space =
            scale_factor * font_config.get_letter_space() * metrics.units_per_em as f32; // Use default if 0

        let mut min_x = current_x;
        let mut max_x = current_x;
        let mut min_y = base_y; // Start with baseline
        let mut max_y = base_y; // Start with baseline

        let mut prev_space_glyph = true; // Add letter spacing except before the first glyph

        for i in 0..glyph_num {
            let glyph_id = glyph_infos[i].glyph_id;
            let glyph_pos = glyph_positions[i];
            let glyph_id_u16 = glyph_id as u16;
            let svg_id = format!("g{}", glyph_id_u16);

            // Add letter spacing before rendering the glyph (if not the first char)
            current_x += if !prev_space_glyph { letter_space } else { 0.0 };
            prev_space_glyph = false; // Reset after potentially adding space

            // --- Manage Glyph Definition ---
            if !glyph_cache.contains_key(&glyph_id_u16) {
                let mut d_str = String::new();
                // Build path at origin (0,0) with scaling
                let mut path_builder = GlyphPathBuilder::new(
                    scale_factor,
                    -scale_factor, // Negative Y scale to flip vertically
                    0.0,           // X origin for definition path
                    0.0,           // Y origin for definition path
                    &mut d_str,
                );

                // Outline the glyph to generate the path data 'd_str'
                let _bbox_def = hb_face.outline_glyph(GlyphId(glyph_id_u16), &mut path_builder);

                // Create the <path> node for <defs>
                // No fill/stroke here; apply to <use> or parent group
                let def_path = Path::new()
                    .set("id", svg_id.clone())
                    .set("d", d_str);

                // Insert the Boxed node into glyph_defs
                glyph_defs.insert(svg_id.clone(), Box::new(def_path));
                glyph_cache.insert(glyph_id_u16, svg_id.clone());

                if font_config.get_debug() {
                    println!("Defined glyph: id={}, svg_id={}", glyph_id_u16, svg_id);
                }
            }

            // --- Create <use> Element ---
            let use_x = current_x + (glyph_pos.x_offset as f32 * scale_factor);
            let use_y = base_y - (glyph_pos.y_offset as f32 * scale_factor); // Adjust y based on rustybuzz offset

            let use_node = Use::new()
                .set("href", format!("#{}", svg_id)) // Use href (SVG 2 standard)
                .set("x", use_x)
                .set("y", use_y);

            use_group = use_group.add(use_node);

            if font_config.get_debug() {
                println!(
                    "Used glyph: id={}, svg_id={}, use_x={}, use_y={}, x_adv={}",
                    glyph_id_u16, svg_id, use_x, use_y, glyph_pos.x_advance
                );
            }

            // --- Update Bounding Box ---
            // Estimate glyph bounds based on advances and offsets.
            // For accurate bounds, we'd need the actual glyph bounding box from hb_face.outline_glyph
            // and transform it, which is more complex. Using advances provides width.
            // Height estimation uses font metrics.
            let advance_width = glyph_pos.x_advance as f32 * scale_factor;
            let estimated_glyph_max_x = use_x + advance_width; // Rough estimate
            let estimated_glyph_min_y = use_y - (target_glyph_height); // Rough estimate based on size
            let estimated_glyph_max_y = use_y; // Baseline is roughly max y

            min_x = min_x.min(use_x);
            max_x = max_x.max(estimated_glyph_max_x);
            min_y = min_y.min(estimated_glyph_min_y);
            max_y = max_y.max(estimated_glyph_max_y);


            // --- Advance cursor for the next glyph ---
            current_x += advance_width;

            // Check if glyph looks like whitespace (no outline, has advance)
             if hb_face.outline_glyph(GlyphId(glyph_id_u16), &mut NullOutlineBuilder).is_none() && glyph_pos.x_advance > 0 {
                 prev_space_glyph = true; // It's likely a space, don't add letter-spacing before next char
             }

        }

        // Add final letter spacing if the last char wasn't space-like
        max_x += if !prev_space_glyph { letter_space } else { 0.0 };


        // Calculate final bounding box using i16 for consistency with Rect
        let bbox = Rect {
            x_min: min_x.floor() as i16,
            y_min: min_y.floor() as i16, // Use estimated min_y
            x_max: max_x.ceil() as i16,
            // Use line height for max_y relative to origin, adjust for scale
            y_max: (self.origin.y + target_glyph_height).ceil() as i16,
        };


        if font_config.get_debug() {
            println!(
                "TextBuilder BBox: x_min={:?} y_min={:?} x_max={:?} y_max={:?} width={:?} height={:?}",
                bbox.x_min, bbox.y_min, bbox.x_max, bbox.y_max, bbox.width(), bbox.height()
            );
        }

        // Apply common path attributes (stroke width etc.) to the group if needed,
        // although fill/stroke color should be applied higher up.
        use_group = use_group
            .set("stroke-width", self.path_config.stroke_width)
            .set("stroke-linecap", self.path_config.get_stroke_linecap())
            .set("stroke-linejoin", self.path_config.get_stroke_linejoin());


        (use_group, bbox)
    }
}

// --- GlyphPathBuilder ---
// Used to convert ttf_parser outline commands into SVG path data string 'd'.
pub struct GlyphPathBuilder<'a> {
    pub scale_x: f32,
    pub scale_y: f32,
    pub x_offset: f32, // Offset to apply to all points (used for positioning in <defs>)
    pub y_offset: f32, // Offset to apply to all points
    pub d: &'a mut String,
}

impl<'a> GlyphPathBuilder<'a> {
    fn new(scale_x: f32, scale_y: f32, x_offset: f32, y_offset: f32, d: &'a mut String) -> Self {
        Self {
            scale_x,
            scale_y,
            x_offset,
            y_offset,
            d,
        }
    }

    // Helper to apply scale and offset
    #[inline]
    fn tx(&self, x: f32) -> f32 {
        self.x_offset + x * self.scale_x
    }

    #[inline]
    fn ty(&self, y: f32) -> f32 {
        self.y_offset + y * self.scale_y
    }
}

impl ttf_parser::OutlineBuilder for GlyphPathBuilder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        write!(self.d, "M {} {}", self.tx(x), self.ty(y)).unwrap();
    }

    fn line_to(&mut self, x: f32, y: f32) {
        write!(self.d, "L {} {}", self.tx(x), self.ty(y)).unwrap();
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        write!(self.d, "Q {} {} {} {}", self.tx(x1), self.ty(y1), self.tx(x), self.ty(y)).unwrap();
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        write!(self.d, "C {} {} {} {} {} {}", self.tx(x1), self.ty(y1), self.tx(x2), self.ty(y2), self.tx(x), self.ty(y)).unwrap();
    }

    fn close(&mut self) {
        write!(self.d, "Z ").unwrap();
    }
}

// --- NullOutlineBuilder ---
// Used to check if a glyph has an outline without generating path data.
struct NullOutlineBuilder;

impl ttf_parser::OutlineBuilder for NullOutlineBuilder {
    fn move_to(&mut self, _x: f32, _y: f32) {}
    fn line_to(&mut self, _x: f32, _y: f32) {}
    fn quad_to(&mut self, _x1: f32, _y1: f32, _x: f32, _y: f32) {}
    fn curve_to(&mut self, _x1: f32, _y1: f32, _x2: f32, _y2: f32, _x: f32, _y: f32) {}
    fn close(&mut self) {}
}

