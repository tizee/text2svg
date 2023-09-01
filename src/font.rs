use std::collections::HashMap;
use std::str::FromStr;

use clap::ValueEnum;
use font_kit::error::{FontLoadingError, SelectionError};
use font_kit::font::Font;
use font_kit::properties::{Style, Weight};
use font_kit::source::SystemSource;
use rustybuzz::Feature;

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

#[derive(ValueEnum, Debug, PartialEq, Clone, Eq, Hash)]
#[value(rename_all="lower")]
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

impl ToString for FontStyle {
    fn to_string(&self) -> String {
        match *self {
            FontStyle::Thin => "thin".to_string(),
            FontStyle::Light => "light".to_string(),
            FontStyle::ExtraLight => "extra_light".to_string(),
            FontStyle::Regular => "regular".to_string(),
            FontStyle::Medium => "medium".to_string(),
            FontStyle::Bold => "bold".to_string(),
            FontStyle::SemiBold => "semi_bold".to_string(),
            FontStyle::ExtraBold => "extra_bold".to_string(),
            FontStyle::Black => "black".to_string(),
            FontStyle::Italic => "italic".to_string(),
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
    feature_map: HashMap<String,Feature>,
    features: Vec<Feature>,
    faces: HashMap<FontStyle, Font>,
    letter_space: f32,
    fill_color: String,
    color: String,
    debug: bool,
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
    if w >= Weight::THIN.0 &&  w < Weight::EXTRA_LIGHT.0 {
        return FontStyle::Thin;
    }
    if w >= Weight::EXTRA_LIGHT.0 &&  w < Weight::LIGHT.0 {
        return FontStyle::ExtraLight;
    }
    if w >= Weight::LIGHT.0 &&  w < Weight::NORMAL.0 {
        return FontStyle::Light;
    }
    if w >= Weight::NORMAL.0 &&  w < Weight::MEDIUM.0 {
        return FontStyle::Regular;
    }
    if w >= Weight::MEDIUM.0 &&  w < Weight::SEMIBOLD.0 {
        return FontStyle::Medium;
    }
    if w >= Weight::SEMIBOLD.0 &&  w < Weight::BOLD.0 {
        return FontStyle::SemiBold;
    }
    if w >= Weight::BOLD.0 &&  w < Weight::EXTRA_BOLD.0 {
        return FontStyle::Bold;
    }
    if w >= Weight::EXTRA_BOLD.0 &&  w < Weight::BLACK.0 {
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

        for handle in font_family.fonts() {
            let font = handle.load()?;
            let properties = font.properties();

            if debug {
                println!("font name:\n {:?}", font.full_name());
                println!("font properties:\n {:?}", properties);
            }

            if let Some(style) = font_full_name_to_weight(font.full_name()) {
                faces.insert(style, font);
                continue;
            }

            match properties.style {
                Style::Normal => {
                    let weight = approximate_font_weight(properties.weight);
                    faces.insert(weight, font);
                },
                Style::Italic => {
                    faces.insert(FontStyle::Italic, font);
                }
                _ => {
                    eprintln!("Unsupported font style\n {:?}", properties);
                },
            }
        }
        let mut feature_map = HashMap::new();
        feature_map.insert("kern".to_owned(),Feature::from_str("kern").unwrap());
        feature_map.insert("liga".to_owned(),Feature::from_str("liga").unwrap());
        feature_map.insert("calt".to_owned(),Feature::from_str("calt").unwrap());
        feature_map.insert("clig".to_owned(),Feature::from_str("clig").unwrap());
        let features = feature_map.values().cloned().collect();

        if debug {
            println!("faces:\n {:?}", faces);
        }

        // now only supports horizontal writing mode default features
        Ok(Self {
            font_name,
            size,
            feature_map,
            features,
            fill_color,
            color,
            faces,
            letter_space:0.0,
            debug,
        })
    }

    pub fn has_feature(&mut self, name: &str) -> bool {
        self.feature_map.get(name).is_some()
    }

    pub fn add_feature(&mut self, name: &str)  {
        self.feature_map.insert(name.to_owned(),Feature::from_str(name).unwrap());
        self.features = self.feature_map.values().cloned().collect();
    }

    pub fn remove_feature(&mut self, name: &str) {
        if self.has_feature(name) {
            self.feature_map.remove(name);
            self.features = self.feature_map.values().cloned().collect();
        }
    }

    pub fn get_features(&self) -> &Vec<Feature> {
        &self.features
    }

    pub fn get_regular_font(&self) -> Option<&Font> {
        self.faces.get(&FontStyle::Regular)
    }

    pub fn get_font_by_style(&self, style: &FontStyle) -> Option<&Font> {
        self.faces.get(style)
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
