use syntect::{highlighting::{Color, FontStyle as SynFontStyle, Theme, ThemeSet}, parsing::SyntaxSet, LoadingError}; // Renamed FontStyle to avoid clash
use std::{fmt::Display, path::Path, fs};
use anyhow::{Result, anyhow};

use crate::font::FontStyle as AppFontStyle; // Renamed our FontStyle

#[derive(Debug)]
pub enum HighlightTheme{
    GruvboxDark,
    GruvboxLight,
}

pub struct HighlightSetting {
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub theme: String, // Name of the currently selected theme
}

impl Default for HighlightSetting {
    fn default() -> Self {
        let ss = SyntaxSet::load_defaults_nonewlines();
        let ts = ThemeSet::load_defaults();
        Self {
            syntax_set: ss,
            theme_set: ts,
            theme: "base16-ocean.dark".to_string(), // Default theme name
        }
    }
}

impl HighlightSetting {
    /// Adds a theme from a .tmTheme file path.
    pub fn add_theme_from_path<P: AsRef<Path>>(&mut self, name: &str, path: P) -> Result<(), LoadingError> {
        let theme = ThemeSet::get_theme(path)?; // This handles reading the file
        self.theme_set.themes.insert(name.to_string(), theme);
        Ok(())
    }

    /// Gets a reference to a loaded theme by name.
    pub fn get_theme(&self, name: &str) -> Option<&Theme> {
        self.theme_set.themes.get(name)
    }

    /// Sets the name of the theme to be used for rendering.
    pub fn set_theme(&mut self, name: &str) -> &mut Self {
        // We could add validation here to ensure the theme exists in theme_set
        self.theme = name.to_string();
        self
    }
}

// Wrapper for syntect::highlighting::Color to provide Display impl for rgba()
pub struct HighlightColor {
    inner: Color
}

impl HighlightColor {
    pub fn new(color: Color) -> Self {
        Self {
            inner: color
        }
    }
}

impl Display for HighlightColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format as rgba(r,g,b,a) where alpha is normalized to 0.0-1.0
        write!(f,"rgba({},{},{},{:.3})", self.inner.r, self.inner.g, self.inner.b, self.inner.a as f32 / 255.0)
    }
}

// Wrapper for syntect::highlighting::FontStyle to map to our application's FontStyle enum
pub struct HighlightFontStyle {
    inner: SynFontStyle,
}

impl HighlightFontStyle {
    pub fn new(style: SynFontStyle) -> Self {
        Self {
            inner: style
        }
    }

    /// Gets the corresponding application FontStyle.
    /// Prioritizes Italic over Bold if both are present.
    pub fn get_style(&self) -> AppFontStyle {
        if self.inner.contains(SynFontStyle::ITALIC) {
            return AppFontStyle::Italic; // Map syntect italic to our Italic
        }
        if self.inner.contains(SynFontStyle::BOLD) {
            return AppFontStyle::Bold; // Map syntect bold to our Bold
        }
        // Default to Regular if neither bold nor italic
        AppFontStyle::Regular
    }
}

#[cfg(test)]
mod test_highlight{
    use super::*;
    use crate::font::FontStyle as AppFontStyle; // Use the aliased name

    #[test]
    fn test_font_style_mapping() {
        // Test Italic preference
        let bold_italic = SynFontStyle::ITALIC | SynFontStyle::BOLD;
        let italic_style = HighlightFontStyle::new(bold_italic);
        assert_eq!(italic_style.get_style(), AppFontStyle::Italic);

        // Test Italic alone
        let italic_only = SynFontStyle::ITALIC;
        let italic_style_only = HighlightFontStyle::new(italic_only);
        assert_eq!(italic_style_only.get_style(), AppFontStyle::Italic);

        // Test Bold alone
        let bold_only = SynFontStyle::BOLD;
        let bold_style = HighlightFontStyle::new(bold_only);
        assert_eq!(bold_style.get_style(), AppFontStyle::Bold);

        // Test Normal (no flags)
        let normal_style = SynFontStyle::empty();
        let normal_app_style = HighlightFontStyle::new(normal_style);
        assert_eq!(normal_app_style.get_style(), AppFontStyle::Regular);

        // Test Underline (should map to Regular as we don't handle it)
        let underline_style = SynFontStyle::UNDERLINE;
        let underline_app_style = HighlightFontStyle::new(underline_style);
        assert_eq!(underline_app_style.get_style(), AppFontStyle::Regular);
    }

     #[test]
    fn test_color_display() {
        // Opaque black
        let black = HighlightColor::new(Color { r: 0, g: 0, b: 0, a: 255 });
        assert_eq!(black.to_string(), "rgba(0,0,0,1.000)");

        // Opaque white
        let white = HighlightColor::new(Color { r: 255, g: 255, b: 255, a: 255 });
        assert_eq!(white.to_string(), "rgba(255,255,255,1.000)");

        // Semi-transparent red
        let red_transparent = HighlightColor::new(Color { r: 255, g: 0, b: 0, a: 128 }); // 128 is approx 0.5 alpha
        assert_eq!(red_transparent.to_string(), "rgba(255,0,0,0.502)"); // Check precision

         // Fully transparent
        let fully_transparent = HighlightColor::new(Color { r: 100, g: 100, b: 100, a: 0 });
        assert_eq!(fully_transparent.to_string(), "rgba(100,100,100,0.000)");
    }
}

