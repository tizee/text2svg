use std::collections::HashMap;
use std::str::FromStr;

use font_kit::font::Font;
use font_kit::source::SystemSource;
use font_kit::error::{FontLoadingError,SelectionError};
use font_kit::properties::{Weight,Style};
use rustybuzz::Feature;

/// names of installed fonts
pub fn fonts() -> Vec<String> {
    let arr: Vec<String> = Vec::new();
    let sys_fonts = SystemSource::new();
    let _families = sys_fonts.all_families();
    match _families {
        Ok(families) => {
            families
        },
        Err(_) => {
            arr
        }
    }
}

#[derive(Debug,PartialEq,Clone,Eq,Hash)]
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
           "regular" => {
               Ok(FontStyle::REGULAR)
           },
           "medium" => {
               Ok(FontStyle::MEDIUM)
           },
           "bold" => {
               Ok(FontStyle::BOLD)
           },
           "italic" => {
               Ok(FontStyle::ITALIC)
           },
           _ => {
               Err(ParseFontStyleErr)
           }
        }
    }
}

#[derive(Debug)]
pub enum FontError {
    SelectionError(SelectionError),
    FontLoadingError(FontLoadingError)
}

use std::error::Error;
use std::fmt::Display;

impl Error for FontError {}

impl Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontError::SelectionError(e) => {
                write!(f, "Font Error: {}",e)
            },
            FontError::FontLoadingError(e) => {
                write!(f, "Font Error: {}",e)
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
    pub font_name: String,
    pub size: u32,
    pub features: Vec<Feature>,
    pub faces: HashMap<FontStyle,Font>,
    pub letter_space: f32,
    pub fill_color: String,
    pub color: String,
    pub debug: bool,
}

impl FontConfig  {
    pub fn new(font_name: String, size: u32, fill_color: String, color: String) -> Result<Self,FontError> {
        let font_family = SystemSource::new().select_family_by_name(&font_name)?;

        let mut faces = HashMap::new();

        for handle in font_family.fonts() {
            let font = handle.load()?;

            let properties = font.properties();

            match properties.style {
                Style::Normal => {
                    if properties.weight == Weight::NORMAL {
                        faces.insert(FontStyle::REGULAR,font);
                    }else if properties.weight == Weight::BOLD {
                        faces.insert(FontStyle::BOLD,font);
                    }else if properties.weight == Weight::MEDIUM {
                        faces.insert(FontStyle::MEDIUM,font);
                    }
                },
                Style::Italic => {
                    faces.insert(FontStyle::ITALIC,font);
                },
                _ => ()
            }
        }

        // now only supports horizontal writing mode default features
        Ok(Self {
            font_name,
            size,
            features: vec![
                Feature::from_str("kern").unwrap(),
                Feature::from_str("size").unwrap(),
                Feature::from_str("liga").unwrap(),
                Feature::from_str("calt").unwrap(),
                Feature::from_str("clig").unwrap()
            ],
            fill_color,
            color,
            faces,
            letter_space: 0.0,
            debug: false,
        })
    }

    pub fn set_debug(&mut self, debug: bool) -> &mut Self {
        self.debug = debug;
        self
    }

    pub fn get_regular_font(&self) -> Option<&Font> {
        self.faces.get(&FontStyle::REGULAR)
    }

    pub fn set_letter_space(&mut self, space: f32) -> &mut Self {
        self.letter_space = space;
        self
    }
}


