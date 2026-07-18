use std::collections::HashMap;
use std::str::FromStr;

use font_kit::error::{FontLoadingError, SelectionError};
use font_kit::font::Font;
use font_kit::properties::{Style, Weight};
use font_kit::source::SystemSource;
use rustybuzz::{ttf_parser::Tag, Feature, Variation};

/// names of installed fonts
pub fn fonts() -> Vec<String> {
    let arr: Vec<String> = Vec::new();
    let sys_fonts = SystemSource::new();
    let _families = sys_fonts.all_families();
    match _families {
        Ok(families) => families,
        Err(_) => arr,
    }
}

#[derive(Debug, PartialEq, Clone, Eq, Hash)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "lower"))]
pub enum FontStyle {
    // Weight
    Thin,
    ExtraLight,
    Light,
    Regular,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
    // Style
    Italic,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseFontStyleErr;

impl std::str::FromStr for FontStyle {
    type Err = ParseFontStyleErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "thin" => Ok(FontStyle::Thin),
            "extralight" | "extra_light" => Ok(FontStyle::ExtraLight),
            "light" => Ok(FontStyle::Light),
            "regular" => Ok(FontStyle::Regular),
            "medium" => Ok(FontStyle::Medium),
            "semibold" | "semi_bold" => Ok(FontStyle::SemiBold),
            "bold" => Ok(FontStyle::Bold),
            "extrabold" | "extra_bold" => Ok(FontStyle::ExtraBold),
            "black" => Ok(FontStyle::Black),
            "italic" => Ok(FontStyle::Italic),
            _ => Err(ParseFontStyleErr),
        }
    }
}

impl Display for FontStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            FontStyle::Thin => write!(f, "thin"),
            FontStyle::Light => write!(f, "light"),
            FontStyle::ExtraLight => write!(f, "extra_light"),
            FontStyle::Regular => write!(f, "regular"),
            FontStyle::Medium => write!(f, "medium"),
            FontStyle::Bold => write!(f, "bold"),
            FontStyle::SemiBold => write!(f, "semi_bold"),
            FontStyle::ExtraBold => write!(f, "extra_bold"),
            FontStyle::Black => write!(f, "black"),
            FontStyle::Italic => write!(f, "italic"),
        }
    }
}

#[derive(Debug)]
pub enum FontError {
    SelectionError(SelectionError),
    FontLoadingError(FontLoadingError),
}

use std::error::Error;
use std::fmt::Display;

impl Error for FontError {}

impl Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontError::SelectionError(e) => {
                write!(f, "Font Error: {}", e)
            }
            FontError::FontLoadingError(e) => {
                write!(f, "Font Error: {}", e)
            }
        }
    }
}

impl From<SelectionError> for FontError {
    fn from(value: SelectionError) -> Self {
        Self::SelectionError(value)
    }
}

impl From<FontLoadingError> for FontError {
    fn from(value: FontLoadingError) -> Self {
        Self::FontLoadingError(value)
    }
}

#[derive(Debug)]
pub struct FontConfig {
    font_name: String,
    size: u32,
    feature_map: HashMap<String, Feature>,
    features: Vec<Feature>,
    /// Variable-font axis settings (e.g. wght=900), applied when shaping and
    /// outlining glyphs. Empty for static fonts.
    variations: Vec<Variation>,
    faces: HashMap<FontStyle, Font>,
    /// Faces keyed by their real OpenType subfamily name (lowercased), e.g.
    /// "regular", "bold", "9px". Enables selecting a font's own style names
    /// rather than the fixed `FontStyle` enum.
    named_faces: HashMap<String, Font>,
    /// When set, shaping uses this named face regardless of the requested
    /// `FontStyle` (the plain-text path selects one style up front). Highlight
    /// mode leaves this `None` and keeps using the `FontStyle` map.
    active_style: Option<String>,
    letter_space: f32,
    fill_color: String,
    color: String,
    debug: bool,
}

/// A variable-font axis and the value range it accepts (for `--var`).
#[derive(Debug, Clone)]
pub struct AxisInfo {
    pub tag: String,
    pub min: f32,
    pub default: f32,
    pub max: f32,
    /// Human-readable axis name from the `name` table, if present.
    pub name: Option<String>,
    /// Whether the axis is meant to be hidden from UIs.
    pub hidden: bool,
}

/// Render a 4-byte OpenType tag as a trimmed string (e.g. "wght").
fn tag_to_string(tag: Tag) -> String {
    String::from_utf8_lossy(&tag.to_bytes()).trim().to_string()
}

/// Best-effort style key for a face, lowercased. Tries the OpenType subfamily
/// name (typographic id 17, else legacy id 2), then the full name with the
/// family prefix stripped (e.g. "DinkieBitmap 9px" -> "9px"), then "regular".
fn style_key(font: &Font) -> String {
    if let Some(data) = font.copy_font_data() {
        if let Ok(face) = rustybuzz::ttf_parser::Face::parse(&data, 0) {
            let names = face.names();
            for id in [17u16, 2u16] {
                if let Some(name) = names
                    .into_iter()
                    .find(|n| n.name_id == id)
                    .and_then(|n| n.to_string())
                {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        return trimmed.to_lowercase();
                    }
                }
            }
        }
    }

    let full = font.full_name();
    let family = font.family_name();
    let stripped = full.strip_prefix(&family).unwrap_or(&full).trim();
    if !stripped.is_empty() {
        return stripped.to_lowercase();
    }

    "regular".to_string()
}

// Get font style from keywords in its full name
fn font_full_name_to_weight(name: String) -> Option<FontStyle> {
    let name = name.to_lowercase();
    // Search longer patterns first
    if name.contains("extralight") {
        return Some(FontStyle::ExtraLight);
    }
    if name.contains("light") {
        return Some(FontStyle::Light);
    }
    if name.contains("medium") {
        return Some(FontStyle::Medium);
    }
    if name.contains("regular") {
        return Some(FontStyle::Regular);
    }
    if name.contains("semibold") {
        return Some(FontStyle::SemiBold);
    }
    if name.contains("bold") {
        return Some(FontStyle::Bold);
    }
    // This means we cannot determine its style from the full name.
    // Then we could use its weight to determine its style.
    None
}

// Approximate font weight as flooring operation in math
fn approximate_font_weight(weight: Weight) -> FontStyle {
    let w = weight.0;
    if (Weight::THIN.0..Weight::EXTRA_LIGHT.0).contains(&w) {
        return FontStyle::Thin;
    }
    if (Weight::EXTRA_LIGHT.0..Weight::LIGHT.0).contains(&w) {
        return FontStyle::ExtraLight;
    }
    if (Weight::LIGHT.0..Weight::NORMAL.0).contains(&w) {
        return FontStyle::Light;
    }
    if (Weight::NORMAL.0..Weight::MEDIUM.0).contains(&w) {
        return FontStyle::Regular;
    }
    if (Weight::MEDIUM.0..Weight::SEMIBOLD.0).contains(&w) {
        return FontStyle::Medium;
    }
    if (Weight::SEMIBOLD.0..Weight::BOLD.0).contains(&w) {
        return FontStyle::SemiBold;
    }
    if (Weight::BOLD.0..Weight::EXTRA_BOLD.0).contains(&w) {
        return FontStyle::Bold;
    }
    if (Weight::EXTRA_BOLD.0..Weight::BLACK.0).contains(&w) {
        return FontStyle::ExtraBold;
    }
    FontStyle::Black
}

impl FontConfig {
    pub fn new(
        font_name: String,
        size: u32,
        fill_color: String,
        color: String,
        debug: bool,
    ) -> Result<Self, FontError> {
        let font_family = SystemSource::new().select_family_by_name(&font_name)?;

        let mut faces = HashMap::new();
        let mut named_faces: HashMap<String, Font> = HashMap::new();

        for handle in font_family.fonts() {
            let font = handle.load()?;
            let properties = font.properties();

            if debug {
                eprintln!("font name:\n {:?}", font.full_name());
                eprintln!("font properties:\n {:?}", properties);
            }

            // Record the font's own advertised style name (e.g. "9px").
            named_faces.insert(style_key(&font), font.clone());

            if let Some(style) = font_full_name_to_weight(font.full_name()) {
                faces.insert(style, font);
                continue;
            }

            match properties.style {
                Style::Normal => {
                    let weight = approximate_font_weight(properties.weight);
                    faces.insert(weight, font);
                }
                Style::Italic => {
                    faces.insert(FontStyle::Italic, font);
                }
                _ => {
                    eprintln!("Unsupported font style\n {:?}", properties);
                }
            }
        }
        let mut feature_map = HashMap::new();
        feature_map.insert("kern".to_owned(), Feature::from_str("kern").unwrap());
        feature_map.insert("liga".to_owned(), Feature::from_str("liga").unwrap());
        feature_map.insert("calt".to_owned(), Feature::from_str("calt").unwrap());
        feature_map.insert("clig".to_owned(), Feature::from_str("clig").unwrap());
        let features = feature_map.values().cloned().collect();

        if debug {
            eprintln!("faces:\n {:?}", faces);
            eprintln!(
                "named styles:\n {:?}",
                named_faces.keys().collect::<Vec<_>>()
            );
        }

        // now only supports horizontal writing mode default features
        Ok(Self {
            font_name,
            size,
            feature_map,
            features,
            variations: Vec::new(),
            fill_color,
            color,
            faces,
            named_faces,
            active_style: None,
            letter_space: 0.0,
            debug,
        })
    }

    /// Build a config from a font file path directly, bypassing the system font
    /// database. Loads the first face (for a variable font, its default
    /// instance). Lets callers render with a specific font file, e.g. a
    /// stroke-decomposed variable font.
    pub fn from_file(
        path: &std::path::Path,
        size: u32,
        fill_color: String,
        color: String,
        debug: bool,
    ) -> Result<Self, FontError> {
        let font = Font::from_path(path, 0)?;
        let font_name = font.family_name();
        let style_name = style_key(&font);

        if debug {
            eprintln!("loaded font from file: {}", path.display());
            eprintln!("font family: {}", font_name);
            eprintln!("font style: {}", style_name);
        }

        // A single loaded face answers for every requested style; shaping falls
        // back to Regular when a specific style is missing.
        let mut faces = HashMap::new();
        let mut named_faces: HashMap<String, Font> = HashMap::new();
        named_faces.insert(style_name, font.clone());
        faces.insert(FontStyle::Regular, font);

        let mut feature_map = HashMap::new();
        feature_map.insert("kern".to_owned(), Feature::from_str("kern").unwrap());
        feature_map.insert("liga".to_owned(), Feature::from_str("liga").unwrap());
        feature_map.insert("calt".to_owned(), Feature::from_str("calt").unwrap());
        feature_map.insert("clig".to_owned(), Feature::from_str("clig").unwrap());
        let features = feature_map.values().cloned().collect();

        Ok(Self {
            font_name,
            size,
            feature_map,
            features,
            variations: Vec::new(),
            fill_color,
            color,
            faces,
            named_faces,
            active_style: None,
            letter_space: 0.0,
            debug,
        })
    }

    /// Parse and set font features from a string like "cv01=1,calt=0,liga=1"
    /// This will override existing features for the same tags, but keeps defaults for unspecified features
    pub fn set_features_from_string(&mut self, features_str: &str) -> Result<(), String> {
        // Don't clear existing features - we want to override/add to defaults

        // Parse the features string
        for feature_str in features_str.split(',') {
            let feature_str = feature_str.trim();
            if feature_str.is_empty() {
                continue;
            }

            // Parse "feature=value" or just "feature" (defaults to 1)
            let (tag, value) = if let Some(eq_pos) = feature_str.find('=') {
                let tag = &feature_str[..eq_pos].trim();
                let value_str = &feature_str[eq_pos + 1..].trim();
                let value = value_str.parse::<u32>().map_err(|_| {
                    format!(
                        "Invalid feature value '{}' for feature '{}'",
                        value_str, tag
                    )
                })?;
                (tag.to_string(), value)
            } else {
                // Default value is 1 if not specified
                (feature_str.to_string(), 1)
            };

            // Validate tag length (OpenType feature tags are exactly 4 characters)
            if tag.len() != 4 {
                return Err(format!(
                    "Invalid feature tag '{}': feature tags must be exactly 4 characters",
                    tag
                ));
            }

            // Handle feature enable/disable
            if value == 0 {
                // Remove feature when value is 0 (disable)
                self.feature_map.remove(&tag);
                if self.debug {
                    eprintln!("Disabled font feature: {}", tag);
                }
            } else {
                // Add/enable feature when value > 0
                // Convert tag string to 4-byte array
                let mut tag_bytes = [0u8; 4];
                let tag_str_bytes = tag.as_bytes();
                let len = tag_str_bytes.len().min(4);
                tag_bytes[..len].copy_from_slice(&tag_str_bytes[..len]);

                let feature = Feature::new(
                    Tag::from_bytes(&tag_bytes),
                    value,
                    .., // Apply to entire text range
                );
                self.feature_map.insert(tag.clone(), feature);
                if self.debug {
                    eprintln!("Enabled font feature: {}={}", tag, value);
                }
            }
        }

        // Update the features vector
        self.features = self.feature_map.values().cloned().collect();

        if self.debug {
            eprintln!(
                "Set font features: {:?}",
                self.feature_map.keys().collect::<Vec<_>>()
            );
        }

        Ok(())
    }

    /// Get a summary of currently active features
    pub fn get_features_summary(&self) -> String {
        if self.feature_map.is_empty() {
            "none".to_string()
        } else {
            self.feature_map
                .iter()
                .map(|(tag, feature)| format!("{}={}", tag, feature.value))
                .collect::<Vec<_>>()
                .join(",")
        }
    }

    pub fn get_features(&self) -> &Vec<Feature> {
        &self.features
    }

    /// Parse and set variable-font axis settings like "wght=900,opsz=12".
    /// Later values override earlier ones for the same axis.
    pub fn set_variations_from_string(&mut self, variations_str: &str) -> Result<(), String> {
        for part in variations_str.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let variation: Variation = part.parse().map_err(|_| {
                format!("invalid font variation '{}' (expected e.g. wght=900)", part)
            })?;
            match self.variations.iter_mut().find(|v| v.tag == variation.tag) {
                Some(existing) => existing.value = variation.value,
                None => self.variations.push(variation),
            }
        }
        Ok(())
    }

    /// Variable-font axis settings to apply when shaping and outlining.
    pub fn get_variations(&self) -> &[Variation] {
        &self.variations
    }

    /// A representative loaded face for introspection: the active named face,
    /// else Regular, else any available face.
    fn representative_font(&self) -> Option<&Font> {
        if let Some(active) = &self.active_style {
            if let Some(font) = self.named_faces.get(active) {
                return Some(font);
            }
        }
        self.faces
            .get(&FontStyle::Regular)
            .or_else(|| self.faces.values().next())
            .or_else(|| self.named_faces.values().next())
    }

    /// The font's variable axes (`--var` targets) with their value ranges.
    pub fn list_axes(&self) -> Vec<AxisInfo> {
        let Some(font) = self.representative_font() else {
            return Vec::new();
        };
        let Some(data) = font.copy_font_data() else {
            return Vec::new();
        };
        let Ok(face) = rustybuzz::ttf_parser::Face::parse(&data, 0) else {
            return Vec::new();
        };

        face.variation_axes()
            .into_iter()
            .map(|axis| {
                let name = face
                    .names()
                    .into_iter()
                    .find(|n| n.name_id == axis.name_id)
                    .and_then(|n| n.to_string());
                AxisInfo {
                    tag: tag_to_string(axis.tag),
                    min: axis.min_value,
                    default: axis.def_value,
                    max: axis.max_value,
                    name,
                    hidden: axis.hidden,
                }
            })
            .collect()
    }

    /// The OpenType feature tags the font declares (`--features` targets), sorted
    /// and de-duplicated across GSUB and GPOS.
    pub fn list_feature_tags(&self) -> Vec<String> {
        let Some(font) = self.representative_font() else {
            return Vec::new();
        };
        let Some(data) = font.copy_font_data() else {
            return Vec::new();
        };
        let Ok(face) = rustybuzz::ttf_parser::Face::parse(&data, 0) else {
            return Vec::new();
        };

        let mut tags = std::collections::BTreeSet::new();
        for table in [face.tables().gsub, face.tables().gpos]
            .into_iter()
            .flatten()
        {
            let features = table.features;
            for i in 0..features.len() {
                if let Some(feature) = features.get(i) {
                    tags.insert(tag_to_string(feature.tag));
                }
            }
        }
        tags.into_iter().collect()
    }

    pub fn get_font_by_style(&self, style: &FontStyle) -> Option<&Font> {
        // The plain-text path pins one named face up front; honour it regardless
        // of the requested enum style. Highlight mode leaves `active_style` unset
        // and keeps using the enum-keyed faces for bold/italic tokens.
        if let Some(active) = &self.active_style {
            return self.named_faces.get(active);
        }
        self.faces.get(style)
    }

    /// The font's available style names (real subfamily names), sorted.
    pub fn available_styles(&self) -> Vec<String> {
        let mut names: Vec<String> = self.named_faces.keys().cloned().collect();
        names.sort();
        names
    }

    /// Resolve a requested style name against the font's real styles. When
    /// `requested` is `None`, prefer "regular", otherwise the first style sorted
    /// by name. Returns `None` if an explicit request has no matching style.
    pub fn resolve_style_name(&self, requested: Option<&str>) -> Option<String> {
        match requested {
            Some(name) => {
                let name = name.to_lowercase();
                self.named_faces.contains_key(&name).then_some(name)
            }
            None => {
                if self.named_faces.contains_key("regular") {
                    Some("regular".to_string())
                } else {
                    self.available_styles().into_iter().next()
                }
            }
        }
    }

    /// Pin the named face used by the plain-text rendering path.
    pub fn set_active_style(&mut self, style: String) -> &mut Self {
        self.active_style = Some(style);
        self
    }

    pub fn set_letter_space(&mut self, space: f32) -> &mut Self {
        self.letter_space = space;
        self
    }

    pub fn get_letter_space(&self) -> f32 {
        self.letter_space
    }

    pub fn get_font_name(&self) -> &String {
        &self.font_name
    }

    pub fn get_color(&self) -> &String {
        &self.color
    }

    pub fn get_fill_color(&self) -> &String {
        &self.fill_color
    }

    pub fn get_size(&self) -> u32 {
        self.size
    }

    pub fn get_debug(&self) -> bool {
        self.debug
    }
}

#[cfg(test)]
mod test_font_features {
    use super::*;

    // Helper function to create a minimal font config for testing
    fn create_test_font_config() -> FontConfig {
        // Create a font config that may fail in test environment but allows us to test the API
        FontConfig::new(
            "NonExistentTestFont".to_string(),
            16,
            "#000".to_string(),
            "#000".to_string(),
            false, // debug off for cleaner tests
        )
        .unwrap_or_else(|_| {
            // Create a mock config by directly constructing the struct for testing
            // This is a bit of a hack but allows us to test the feature parsing logic
            use std::collections::HashMap;

            let mut feature_map = HashMap::new();
            // Add default features like in the actual constructor
            feature_map.insert("kern".to_owned(), Feature::from_str("kern").unwrap());
            feature_map.insert("liga".to_owned(), Feature::from_str("liga").unwrap());
            feature_map.insert("calt".to_owned(), Feature::from_str("calt").unwrap());
            feature_map.insert("clig".to_owned(), Feature::from_str("clig").unwrap());
            let features = feature_map.values().cloned().collect();

            FontConfig {
                font_name: "TestFont".to_string(),
                size: 16,
                feature_map,
                features,
                variations: Vec::new(),
                fill_color: "#000".to_string(),
                color: "#000".to_string(),
                faces: HashMap::new(), // Empty faces for testing
                named_faces: HashMap::new(),
                active_style: None,
                letter_space: 0.0,
                debug: false,
            }
        })
    }

    #[test]
    fn test_font_features_default() {
        let font_config = create_test_font_config();

        // Should have default features enabled
        let summary = font_config.get_features_summary();
        assert!(summary.contains("kern="));
        assert!(summary.contains("liga="));
        assert!(summary.contains("calt="));
        assert!(summary.contains("clig="));
    }

    #[test]
    fn test_set_features_from_string_enable() {
        let mut font_config = create_test_font_config();

        // Test enabling a new feature
        let result = font_config.set_features_from_string("cv01=1");
        assert!(result.is_ok());

        let summary = font_config.get_features_summary();
        assert!(summary.contains("cv01=1"));
    }

    #[test]
    fn test_set_features_from_string_disable() {
        let mut font_config = create_test_font_config();

        // Test disabling an existing feature
        let result = font_config.set_features_from_string("liga=0");
        assert!(result.is_ok());

        let summary = font_config.get_features_summary();
        // liga should be removed when set to 0
        assert!(!summary.contains("liga="));
    }

    #[test]
    fn test_set_features_from_string_multiple() {
        let mut font_config = create_test_font_config();

        // Test setting multiple features
        let result = font_config.set_features_from_string("cv01=1,calt=0,smcp=1");
        assert!(result.is_ok());

        let summary = font_config.get_features_summary();
        assert!(summary.contains("cv01=1"));
        assert!(!summary.contains("calt=")); // Should be disabled
        assert!(summary.contains("smcp=1"));
    }

    #[test]
    fn test_set_features_from_string_default_value() {
        let mut font_config = create_test_font_config();

        // Test feature without explicit value (should default to 1)
        let result = font_config.set_features_from_string("swsh");
        assert!(result.is_ok());

        let summary = font_config.get_features_summary();
        assert!(summary.contains("swsh=1"));
    }

    #[test]
    fn test_set_features_from_string_invalid_tag_length() {
        let mut font_config = create_test_font_config();

        // Test invalid tag length
        let result = font_config.set_features_from_string("cv=1"); // Too short
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exactly 4 characters"));

        let result = font_config.set_features_from_string("cv01xx=1"); // Too long
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exactly 4 characters"));
    }

    #[test]
    fn test_set_features_from_string_invalid_value() {
        let mut font_config = create_test_font_config();

        // Test invalid value
        let result = font_config.set_features_from_string("cv01=abc");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid feature value"));
    }

    #[test]
    fn test_set_features_from_string_empty_and_whitespace() {
        let mut font_config = create_test_font_config();

        // Test empty string and whitespace handling
        let result = font_config.set_features_from_string("  cv01=1  ,  , liga=0  ");
        assert!(result.is_ok());

        let summary = font_config.get_features_summary();
        assert!(summary.contains("cv01=1"));
        assert!(!summary.contains("liga=")); // Should be disabled
    }

    #[test]
    fn test_get_features_summary_empty() {
        let mut font_config = create_test_font_config();

        // Disable all features
        let _result = font_config.set_features_from_string("kern=0,liga=0,calt=0,clig=0");

        let summary = font_config.get_features_summary();
        assert_eq!(summary, "none");
    }

    #[test]
    fn test_features_override_defaults() {
        let mut font_config = create_test_font_config();

        // Verify default features exist
        assert!(font_config.feature_map.contains_key("liga"));

        // Override with custom value
        let result = font_config.set_features_from_string("liga=2");
        assert!(result.is_ok());

        let summary = font_config.get_features_summary();
        assert!(summary.contains("liga=2"));
    }
}
