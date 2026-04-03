# text2svg

Convert text to SVG with font shaping, syntax highlighting, and animation support.

Works both as a **CLI tool** and as a **reusable Rust library**.

## Installation

### CLI

```bash
cargo install text2svg
```

Or build from source:

```bash
git clone https://github.com/tizee/text2svg
cd text2svg
cargo install --path .
```

### Library

Add to your `Cargo.toml` with default features disabled to avoid pulling in CLI dependencies (clap):

```toml
[dependencies]
text2svg = { version = "0.3.0", default-features = false }
```

## Library Usage

```rust
use text2svg::font::{FontConfig, FontStyle};
use text2svg::render::{RenderConfig, TextAlign};
use text2svg::highlight::HighlightSetting;
use std::path::PathBuf;

// Configure font
let mut font_config = FontConfig::new(
    "Arial".to_string(),
    48,                    // font size in pixels
    "none".to_string(),    // fill color
    "#000".to_string(),    // stroke color
    false,                 // debug mode
).expect("Failed to load font");

// Configure rendering
let render_config = RenderConfig::new(false, FontStyle::Regular);

// Render text to SVG file
text2svg::render::render_text_to_svg_file(
    "Hello World",
    &mut font_config,
    &render_config,
    PathBuf::from("output.svg"),
);

// Render a file with syntax highlighting
let highlight_setting = HighlightSetting::default();
text2svg::render::render_file_highlight(
    &PathBuf::from("example.rs"),
    &mut font_config,
    &highlight_setting,
    PathBuf::from("highlighted.svg"),
);
```

### Available modules

| Module | Description |
|--------|-------------|
| `text2svg::font` | Font loading, configuration, and OpenType feature control |
| `text2svg::render` | SVG rendering for plain text and syntax-highlighted files |
| `text2svg::highlight` | Syntax highlighting settings (themes, syntax sets) |
| `text2svg::svg` | Low-level SVG glyph path building and caching |
| `text2svg::utils` | Text wrapping utilities (character-based and pixel-based) |
| `text2svg::text_analysis` | Unicode-aware text segmentation with CJK support |
| `text2svg::line_break` | Line breaking algorithm with kinsoku rules |

## CLI Usage

```
Usage: text2svg [OPTIONS] [TEXT]

Arguments:
  [TEXT]  input text string

Options:
      --width <WIDTH>              max width per line (characters)
      --pixel-width <PIXEL_WIDTH>  max width per line (pixels)
  -f, --file <FILE>                input file
  -o, --output <OUTPUT>            output svg file path [default: output.svg]
      --font <FONT>                font family name (e.g., "Arial", "Times New Roman")
      --size <SIZE>                font size in pixels [default: 64]
      --fill <FILL>                svg fill color (e.g., "#ff0000", "none"). Overridden by highlight [default: none]
      --color <COLOR>              font stroke color (e.g., "#000", "currentColor"). Overridden by highlight [default: #000]
      --animate                    Add progressive line-by-line draw animation effect (works best with stroke only)
      --style <STYLE>              font style (regular, bold, italic, etc.). Overridden by highlight [default: regular] [possible values: thin, extralight, light, regular, medium, semibold, bold, extrabold, black, italic]
      --space <SPACE>              letter spacing (in em units, e.g., 0.1) [default: 0]
      --features <FEATURES>        font features (e.g., "cv01=1,calt=0,liga=1")
      --highlight                  Enable syntax highlighting mode for files
      --theme <THEME>              Syntax highlighting theme name or path to .tmTheme file [default: base16-ocean.dark]
      --list-syntax                List supported file types/syntax for highlighting
      --list-theme                 List available built-in highlighting themes
      --align <ALIGN>              Text alignment for multi-line output [default: left] [possible values: left, center, right]
  -d, --debug                      Enable debug logging
      --list-fonts                 List installed font families
  -h, --help                       Print help
  -V, --version                    Print version
```

### CLI Examples

Basic text conversion:
```bash
text2svg "Hello World" --font "Arial" --size 48 --output hello.svg
```

Animated text with stroke:
```bash
text2svg "Multi-line\nText Animation" --font "Arial" --animate --fill none --color "#000" --output animated.svg
```

File with syntax highlighting:
```bash
text2svg --file script.js --highlight --theme "base16-ocean.dark" --output code.svg
```

Text wrapping by pixel width:
```bash
text2svg "Long text that needs wrapping" --pixel-width 300 --font "Arial" --output wrapped.svg
```

## Features

- **Text to SVG Conversion** -- Convert plain text or files to SVG using real font shaping (rustybuzz/HarfBuzz)
- **Font Customization** -- Font families, sizes, styles, OpenType features, and letter spacing
- **Syntax Highlighting** -- Built-in syntax highlighting for code files via syntect
- **Animation Effects** -- Progressive line-by-line drawing animation via SVG stroke-dasharray
- **Text Wrapping** -- Character-based and pixel-based line wrapping with Unicode-aware segmentation
- **CJK Support** -- Proper CJK line breaking with kinsoku rules

## Cargo Features

| Feature | Default | Description |
|---------|---------|-------------|
| `cli` | Yes | Enables the CLI binary and clap dependency |

To use as a library without CLI dependencies:

```toml
text2svg = { version = "0.3.0", default-features = false }
```

