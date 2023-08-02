use syntect::{parsing::SyntaxSet, highlighting::{ThemeSet, Color, FontStyle, Theme}};
use std::{fmt::Display, path::Path};

use crate::font::FontStyle as FFontStyle;

#[derive(Debug)]
pub enum HighlightTheme{
    GruvboxDark,
    GruvboxLight,
}

pub struct HighlightSetting {
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub theme: String,
}

impl Default for HighlightSetting {
    fn default() -> Self {
        let ss = SyntaxSet::load_defaults_nonewlines();
        let ts = ThemeSet::load_defaults();
        Self {
            syntax_set: ss,
            theme_set: ts,
            theme: "base16-ocean.dark".to_string(),
        }
    }
}

impl HighlightSetting {
    pub fn add_theme<P: AsRef<Path>>(&mut self, name: &str, path:P) -> &mut Self{
        let theme = ThemeSet::get_theme(path).unwrap();
        self.theme_set.themes.insert(name.to_string(),theme);
        self
    }

    pub fn get_theme(&self, name: &str) -> Option<&Theme> {
        self.theme_set.themes.get(name)
    }

    pub fn set_theme(&mut self, name: &str) -> &mut Self {
        self.theme = name.to_string();
        self
    }
}

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
        write!(f,"rgba({},{},{},{})",self.inner.r,self.inner.g,self.inner.b,self.inner.a)
    }
}

pub struct HighlightFontStyle {
    inner: FontStyle,
}

impl HighlightFontStyle {
    pub fn new(style: FontStyle) -> Self {
        Self {
            inner: style
        }
    }
    pub fn get_style(&self) -> FFontStyle {
        if self.inner.intersects(FontStyle::ITALIC) {
            return FFontStyle::ITALIC;
        }
        if self.inner.intersects(FontStyle::BOLD) {
            return FFontStyle::BOLD;
        }
        FFontStyle::REGULAR
    }
}

#[cfg(test)]
mod test_highlight{

use super::*;
  #[test]
  fn test_font_style() {
      let bold_italic =  FontStyle::ITALIC | FontStyle::BOLD;
      let italic_style = HighlightFontStyle::new(bold_italic);
      assert_eq!(italic_style.get_style(),FFontStyle::ITALIC);
      let bold=  FontStyle::BOLD;
      let bold_style = HighlightFontStyle::new(bold);
      assert_eq!(bold_style.get_style(),FFontStyle::BOLD);
  }
}
