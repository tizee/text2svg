/// StrokeLineCap specifies the shape to be used at the end of open subpaths when stroked
use resvg::tiny_skia::LineCap as StrokeLineCap;
/// StrokeLineJoin specifies the shape to be used at the corners of paths when stroked
use resvg::tiny_skia::LineJoin as StrokeLineJoin;
use resvg::tiny_skia::Point;
use std::collections::HashMap;
// use resvg::usvg::StrokeWidth; // Removed unused import
use std::fmt::Write;

use crate::font::{FontConfig, FontStyle};
use crate::shard::{compute_shards, Cmd, ContourPen};
use rustybuzz::ttf_parser;
use rustybuzz::ttf_parser::{GlyphId, Rect};
use rustybuzz::Face;

use rustybuzz::GlyphBuffer;
use svg::node::element::{Group, Path, Use}; // Removed Definitions import
use svg::Node; // Added Node

// --- Glyph Cache and Definitions ---
pub type GlyphCache = HashMap<u16, String>; // GlyphId -> SVG ID (e.g., "g123")
                                            // Store Box<dyn Node> because Node trait object is not Sized
pub type GlyphDefs = HashMap<String, Box<dyn Node>>; // SVG ID -> Boxed <path> Node for <defs>

/// glyph_id -> its shards (SVG def id + scatter direction), reused across the
/// document so per-contour path defs are only built once per glyph. Populated
/// only when settle/scatter animation is active.
pub type ShardCache = HashMap<u16, Vec<ShardRef>>;

/// A cached reference to one shard's path definition and its outward direction.
#[derive(Clone, Debug)]
pub struct ShardRef {
    pub svg_id: String,
    pub direction: (f32, f32),
}

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
    /// Knuth-Plass glue adjustment ratio for justification. Extra advance added
    /// after each space-like glyph is `justify_ratio * space_advance * 0.5`
    /// (0.5 matches the per-space stretch fraction used during line breaking).
    justify_ratio: f32,
    /// Source text of the line, used to map glyph clusters back to characters so
    /// inter-ideograph (CJK) gaps can be stretched for justification.
    justify_text: Option<String>,
    /// When true, each glyph is emitted as per-contour shard groups (for the
    /// settle/scatter animations) instead of a single `<use>`.
    emit_shards: bool,
    /// Scatter translation distance in pixels (direction is unitless, so the
    /// `translate` offset is `direction * scatter_distance`).
    scatter_distance: f32,
    /// Static outward displacement in pixels baked onto each shard (the
    /// "exploded" pose). Zero when shards animate instead.
    explode_offset: f32,
}

impl Default for TextBuilder {
    fn default() -> Self {
        Self {
            origin: Point { x: 0.0, y: 0.0 },
            path_config: PathConfig::default(),
            justify_ratio: 0.0,
            justify_text: None,
            emit_shards: false,
            scatter_distance: 0.0,
            explode_offset: 0.0,
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

    /// Set the justification ratio applied to inter-word spaces on this line.
    pub fn set_justify_ratio(&mut self, ratio: f32) -> &mut Self {
        self.justify_ratio = ratio;
        self
    }

    /// Provide the line's source text so CJK ideograph boundaries can be
    /// stretched during justification.
    pub fn set_justify_text(&mut self, text: &str) -> &mut Self {
        self.justify_text = Some(text.to_string());
        self
    }

    /// Enable per-glyph shard emission for the settle/scatter animations.
    /// `distance` is the scatter translation in pixels.
    pub fn set_shards(&mut self, emit: bool, distance: f32) -> &mut Self {
        self.emit_shards = emit;
        self.scatter_distance = distance;
        self
    }

    /// Static outward displacement (px) baked onto each shard for the exploded
    /// pose. Ignored unless shard emission is enabled.
    pub fn set_explode_offset(&mut self, offset: f32) -> &mut Self {
        self.explode_offset = offset;
        self
    }

    /// The character at byte offset `cluster` in the justify text, if any.
    fn char_at_cluster(&self, cluster: u32) -> Option<char> {
        self.justify_text
            .as_ref()
            .and_then(|t| t.get(cluster as usize..))
            .and_then(|s| s.chars().next())
    }

    /// Whether the character at byte offset `cluster` in the justify text is CJK.
    fn cluster_is_cjk(&self, cluster: u32) -> bool {
        self.char_at_cluster(cluster)
            .is_some_and(crate::text_analysis::is_cjk)
    }

    /// Whether the boundary between two clusters is a CJK<->Latin/digit boundary
    /// (CSS text-autospace).
    fn is_cjk_latin_boundary(&self, a: u32, b: u32) -> bool {
        let (ca, cb) = match (self.char_at_cluster(a), self.char_at_cluster(b)) {
            (Some(ca), Some(cb)) => (ca, cb),
            _ => return false,
        };
        let is_latin_alnum = |c: char| c.is_alphanumeric() && !crate::text_analysis::is_cjk(c);
        (crate::text_analysis::is_cjk(ca) && is_latin_alnum(cb))
            || (crate::text_analysis::is_cjk(cb) && is_latin_alnum(ca))
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
        shard_cache: &mut ShardCache,
    ) -> (Group, Rect) {
        // Rect uses i16
        let ft_face = font_config
            .get_font_by_style(font_style)
            .or_else(|| font_config.get_font_by_style(&FontStyle::Regular))
            .expect("Font face (style or regular) not found during build"); // Should have been checked earlier

        let metrics = ft_face.metrics();
        let origin_glyph_height = metrics.ascent - metrics.descent;
        let target_glyph_height = font_config.get_size() as f32;
        let scale_factor = target_glyph_height / origin_glyph_height.max(1.0); // Avoid division by zero

        if font_config.get_debug() {
            eprintln!(
                "Build Scale: origin_h={:?}, target_h={:?}, scale_factor={:?}, units/em={:?}",
                origin_glyph_height, target_glyph_height, scale_factor, metrics.units_per_em
            );
        }

        let ft_face_data = ft_face.copy_font_data().expect("Failed to copy font data");
        let mut hb_face =
            Face::from_slice(&ft_face_data, 0).expect("Failed to create rustybuzz::Face");
        // Variable-font axes must be applied before outlining so shard geometry
        // reflects the requested instance (e.g. wght=900).
        hb_face.set_variations(font_config.get_variations());

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

            // Add letter spacing before rendering the glyph (if not the first char)
            current_x += if !prev_space_glyph { letter_space } else { 0.0 };
            prev_space_glyph = false; // Reset after potentially adding space

            // --- Glyph pen position ---
            let use_x = current_x + (glyph_pos.x_offset as f32 * scale_factor);
            let use_y = base_y - (glyph_pos.y_offset as f32 * scale_factor); // Adjust y based on rustybuzz offset
            let advance_width = glyph_pos.x_advance as f32 * scale_factor;

            if self.emit_shards {
                // --- Per-contour shard emission (settle/scatter) ---
                let shards = shard_cache.entry(glyph_id_u16).or_insert_with(|| {
                    let mut pen = ContourPen::new();
                    let _ = hb_face.outline_glyph(GlyphId(glyph_id_u16), &mut pen);
                    compute_shards(pen.finish())
                        .iter()
                        .enumerate()
                        .map(|(k, geom)| {
                            let sid = format!("g{}s{}", glyph_id_u16, k);
                            let d = cmds_to_path_d(&geom.cmds, scale_factor);
                            let def_path = Path::new().set("id", sid.clone()).set("d", d);
                            glyph_defs.insert(sid.clone(), Box::new(def_path));
                            ShardRef {
                                svg_id: sid,
                                direction: geom.direction,
                            }
                        })
                        .collect()
                });

                // Stagger the flight by the glyph's horizontal centre (1.2ms/px)
                // so shards ripple across the text from left to right.
                let centre_x = use_x + advance_width / 2.0;
                let delay = centre_x * 0.0012;

                for shard in shards.iter() {
                    let (dir_x, dir_y) = shard.direction;
                    let tx = dir_x * self.scatter_distance;
                    let ty = dir_y * self.scatter_distance;
                    let use_node = Use::new()
                        .set("href", format!("#{}", shard.svg_id))
                        .set("x", use_x)
                        .set("y", use_y);
                    let mut shard_group = Group::new().set("class", "shard").set(
                        "style",
                        format!(
                            "--dx:{:.2}px;--dy:{:.2}px;animation-delay:{:.3}s",
                            tx, ty, delay
                        ),
                    );
                    // Static exploded pose: shift the shard out along its vector.
                    if self.explode_offset != 0.0 {
                        shard_group = shard_group.set(
                            "transform",
                            format!(
                                "translate({:.2} {:.2})",
                                dir_x * self.explode_offset,
                                dir_y * self.explode_offset
                            ),
                        );
                    }
                    shard_group = shard_group.add(use_node);
                    use_group = use_group.add(shard_group);
                }
            } else {
                let svg_id = format!("g{}", glyph_id_u16);

                // --- Manage Glyph Definition ---
                if let std::collections::hash_map::Entry::Vacant(e) =
                    glyph_cache.entry(glyph_id_u16)
                {
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
                    let def_path = Path::new().set("id", svg_id.clone()).set("d", d_str);

                    // Insert the Boxed node into glyph_defs
                    glyph_defs.insert(svg_id.clone(), Box::new(def_path));
                    e.insert(svg_id.clone());

                    if font_config.get_debug() {
                        eprintln!("Defined glyph: id={}, svg_id={}", glyph_id_u16, svg_id);
                    }
                }

                // --- Create <use> Element ---
                let use_node = Use::new()
                    .set("href", format!("#{}", svg_id)) // Use href (SVG 2 standard)
                    .set("x", use_x)
                    .set("y", use_y);

                use_group = use_group.add(use_node);

                if font_config.get_debug() {
                    eprintln!(
                        "Used glyph: id={}, svg_id={}, use_x={}, use_y={}, x_adv={}",
                        glyph_id_u16, svg_id, use_x, use_y, glyph_pos.x_advance
                    );
                }
            }

            // --- Update Bounding Box ---
            // Estimate glyph bounds based on advances and offsets.
            // For accurate bounds, we'd need the actual glyph bounding box from hb_face.outline_glyph
            // and transform it, which is more complex. Using advances provides width.
            // Height estimation uses font metrics.
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
            let is_space_like = hb_face
                .outline_glyph(GlyphId(glyph_id_u16), &mut NullOutlineBuilder)
                .is_none()
                && glyph_pos.x_advance > 0;

            if is_space_like {
                prev_space_glyph = true; // It's likely a space, don't add letter-spacing before next char

                // Justification: widen this inter-word space by the line's ratio.
                if self.justify_ratio != 0.0 {
                    current_x += self.justify_ratio * advance_width * 0.5;
                }
            } else if self.justify_ratio != 0.0 && self.cluster_is_cjk(glyph_infos[i].cluster) {
                // CJK micro-glue: widen the gap after an ideograph so CJK lines
                // (which have no space glyphs) can also be justified. Mirrors the
                // `width * 0.5` micro-glue stretch used during line breaking.
                current_x += self.justify_ratio * advance_width * 0.5;
            }

            // CSS text-autospace: add a fixed 0.25em gap at CJK<->Latin/digit
            // boundaries (independent of justify stretch). Only on the justify
            // path, where wrapping already accounted for this gap.
            if self.justify_text.is_some() && i + 1 < glyph_num {
                let this_cluster = glyph_infos[i].cluster;
                let next_cluster = glyph_infos[i + 1].cluster;
                if self.is_cjk_latin_boundary(this_cluster, next_cluster) {
                    current_x += 0.25 * target_glyph_height;
                }
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
            eprintln!(
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
        write!(
            self.d,
            "Q {} {} {} {}",
            self.tx(x1),
            self.ty(y1),
            self.tx(x),
            self.ty(y)
        )
        .unwrap();
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        write!(
            self.d,
            "C {} {} {} {} {} {}",
            self.tx(x1),
            self.ty(y1),
            self.tx(x2),
            self.ty(y2),
            self.tx(x),
            self.ty(y)
        )
        .unwrap();
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

/// Convert raw font-unit shard commands into an SVG path `d` string, applying
/// the same scale and vertical flip used for the whole-glyph `<defs>` paths.
fn cmds_to_path_d(cmds: &[Cmd], scale: f32) -> String {
    let mut d = String::new();
    let tx = |x: f32| x * scale;
    let ty = |y: f32| -y * scale;
    for cmd in cmds {
        match *cmd {
            Cmd::Move(x, y) => write!(d, "M {} {}", tx(x), ty(y)).unwrap(),
            Cmd::Line(x, y) => write!(d, "L {} {}", tx(x), ty(y)).unwrap(),
            Cmd::Quad(cx, cy, x, y) => {
                write!(d, "Q {} {} {} {}", tx(cx), ty(cy), tx(x), ty(y)).unwrap()
            }
            Cmd::Cubic(c1x, c1y, c2x, c2y, x, y) => write!(
                d,
                "C {} {} {} {} {} {}",
                tx(c1x),
                ty(c1y),
                tx(c2x),
                ty(c2y),
                tx(x),
                ty(y)
            )
            .unwrap(),
            Cmd::Close => write!(d, "Z ").unwrap(),
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::fonts;

    fn create_test_font_config() -> FontConfig {
        let available = fonts();
        let name = available
            .first()
            .cloned()
            .unwrap_or_else(|| "Arial".to_string());
        FontConfig::new(name, 32, "#000".to_string(), "#000".to_string(), false)
            .expect("font config")
    }

    fn shape(text: &str, fc: &FontConfig) -> GlyphBuffer {
        let face = fc
            .get_font_by_style(&FontStyle::Regular)
            .expect("regular face");
        let data = face.copy_font_data().expect("font data");
        let hb = Face::from_slice(&data, 0).expect("hb face");
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(text);
        rustybuzz::shape(&hb, &[], buf)
    }

    // Width with autospace disabled (no justify_text set).
    fn line_width_no_autospace(text: &str, fc: &FontConfig) -> i16 {
        let glyphs = shape(text, fc);
        let mut cache: GlyphCache = HashMap::new();
        let mut defs: GlyphDefs = HashMap::new();
        let mut shards: ShardCache = HashMap::new();
        let builder = TextBuilder::new();
        let (_g, bbox) = builder.build(
            fc,
            &FontStyle::Regular,
            &glyphs,
            &mut cache,
            &mut defs,
            &mut shards,
        );
        bbox.width()
    }

    #[test]
    fn autospace_widens_cjk_latin_line() {
        // With justify_text set (justify path), a CJK<->Latin boundary inserts a
        // visible 0.25em gap, so the rendered line is wider than without it.
        let fc = create_test_font_config();
        let text = "中a中";
        let with_autospace = line_width(text, 0.0, &fc); // helper sets justify_text
        let without = line_width_no_autospace(text, &fc);
        assert!(
            with_autospace > without,
            "autospace must widen a CJK/Latin line: with={}, without={}",
            with_autospace,
            without
        );
    }

    fn line_width(text: &str, ratio: f32, fc: &FontConfig) -> i16 {
        let glyphs = shape(text, fc);
        let mut cache: GlyphCache = HashMap::new();
        let mut defs: GlyphDefs = HashMap::new();
        let mut shards: ShardCache = HashMap::new();
        let mut builder = TextBuilder::new();
        builder.set_justify_ratio(ratio);
        builder.set_justify_text(text);
        let (_g, bbox) = builder.build(
            fc,
            &FontStyle::Regular,
            &glyphs,
            &mut cache,
            &mut defs,
            &mut shards,
        );
        bbox.width()
    }

    // A shard build emits per-contour <g class="shard"> groups instead of a
    // single <use>, so a letter with a counter (like "o") produces shard groups.
    #[test]
    fn shard_mode_emits_shard_groups() {
        let fc = create_test_font_config();
        let glyphs = shape("o", &fc);
        let mut cache: GlyphCache = HashMap::new();
        let mut defs: GlyphDefs = HashMap::new();
        let mut shards: ShardCache = HashMap::new();
        let mut builder = TextBuilder::new();
        builder.set_shards(true, 96.0);
        let (group, _bbox) = builder.build(
            &fc,
            &FontStyle::Regular,
            &glyphs,
            &mut cache,
            &mut defs,
            &mut shards,
        );
        let svg = group.to_string();
        assert!(
            svg.contains("class=\"shard\""),
            "shard mode must emit shard groups: {svg}"
        );
        assert!(
            svg.contains("--dx"),
            "shard groups must carry a direction offset: {svg}"
        );
        assert!(
            !shards.is_empty(),
            "shard cache should be populated after a shard build"
        );
    }

    #[test]
    fn justify_ratio_widens_lines_with_spaces() {
        let fc = create_test_font_config();
        let text = "hello world from text2svg";
        let natural = line_width(text, 0.0, &fc);
        let justified = line_width(text, 2.0, &fc);
        assert!(
            justified > natural,
            "positive justify ratio must widen a line containing spaces: natural={}, justified={}",
            natural,
            justified
        );
    }

    #[test]
    fn justify_ratio_widens_cjk_line_without_spaces() {
        // CJK lines have no inter-word space glyphs, but justification must still
        // widen them by distributing slack between ideographs (micro-glue render).
        let fc = create_test_font_config();
        let text = "天地玄黄宇宙洪荒";
        let natural = line_width(text, 0.0, &fc);
        let justified = line_width(text, 2.0, &fc);
        assert!(
            justified > natural,
            "positive justify ratio must widen a CJK line: natural={}, justified={}",
            natural,
            justified
        );
    }

    #[test]
    fn justify_ratio_does_not_affect_line_without_spaces() {
        let fc = create_test_font_config();
        let text = "singleword";
        let natural = line_width(text, 0.0, &fc);
        let justified = line_width(text, 2.0, &fc);
        assert_eq!(
            natural, justified,
            "a line with no inter-word spaces must not change width under justify"
        );
    }
}
