use std::collections::HashMap;
use std::str::FromStr;

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

#[derive(Debug, PartialEq, Clone, Eq, Hash)]
pub enum FontStyle {
    // Weight
    REGULAR,
    MEDIUM,
    BOLD,
    // Style
    ITALIC,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseFontStyleErr;

impl FromStr for FontStyle {
    type Err = ParseFontStyleErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "regular" => Ok(FontStyle::REGULAR),
            "medium" => Ok(FontStyle::MEDIUM),
            "bold" => Ok(FontStyle::BOLD),
            "italic" => Ok(FontStyle::ITALIC),
            _ => Err(ParseFontStyleErr),
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
                println!("font properties {:?}", properties);
            }

            match properties.style {
                Style::Normal => {
                    if properties.weight == Weight::NORMAL {
                        faces.insert(FontStyle::REGULAR, font);
                    } else if properties.weight == Weight::BOLD {
                        faces.insert(FontStyle::BOLD, font);
                    } else if properties.weight == Weight::MEDIUM {
                        faces.insert(FontStyle::MEDIUM, font);
                    }
                }
                Style::Italic => {
                    faces.insert(FontStyle::ITALIC, font);
                }
                _ => (),
            }
        }
        let mut feature_map = HashMap::new();
        feature_map.insert("kern".to_owned(),Feature::from_str("kern").unwrap());
        feature_map.insert("liga".to_owned(),Feature::from_str("liga").unwrap());
        feature_map.insert("calt".to_owned(),Feature::from_str("calt").unwrap());
        feature_map.insert("clig".to_owned(),Feature::from_str("clig").unwrap());
        let features = feature_map.values().cloned().collect();

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
        self.faces.get(&FontStyle::REGULAR)
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
