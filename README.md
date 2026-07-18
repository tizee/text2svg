# text2svg

Convert text to SVG with font shaping, advanced typography (Knuth-Plass justification, hyphenation, CJK micro-spacing), syntax highlighting, and glyph-shard animations (settle / scatter / explode, hollow-outline hover).

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
text2svg = { version = "0.5", default-features = false }
```

## Library Usage

```rust
use text2svg::font::{FontConfig, FontStyle};
use text2svg::render::{Animation, RenderConfig, TextAlign};
use text2svg::highlight::HighlightSetting;
use std::path::PathBuf;

// Configure font
let mut font_config = FontConfig::new(
    "Arial".to_string(),
    48,                    // font size in pixels
    "#111827".to_string(), // fill color (solid ink -> readable)
    "none".to_string(),    // stroke color
    false,                 // debug mode
).expect("Failed to load font");

// Configure rendering
let render_config = RenderConfig::new(Animation::None, FontStyle::Regular);

// Render text to SVG file
text2svg::render::render_text_to_svg_file(
    "Hello World",
    &mut font_config,
    &render_config,
    PathBuf::from("output.svg"),
);

// Justified paragraph with optimal (Knuth-Plass) line breaking.
// Setting a pixel width is required; TextAlign::Justify enables the optimizer,
// dictionary hyphenation (English), CJK micro-spacing and CJK<->Latin autospace.
let mut justified = RenderConfig::new(Animation::None, FontStyle::Regular);
justified.set_max_pixel_width(Some(400.0));
justified.set_align(TextAlign::Justify); // auto-enables Knuth-Plass + hyphenation
// justified.set_hyphenate(false);       // optional: turn hyphenation off
text2svg::render::render_text_to_svg_file(
    "A long paragraph that will be justified across the column width...",
    &mut font_config,
    &justified,
    PathBuf::from("justified.svg"),
);

// Glyph-shard animation: assemble the glyphs from their strokes on load.
let mut animated = RenderConfig::new(Animation::Settle, FontStyle::Regular);
animated.set_hollow_hover(true);        // reveal a hollow outline on hover
animated.set_explode(Some(120.0));      // shard travel distance in px
text2svg::render::render_text_to_svg_file(
    "Dashboard",
    &mut font_config,
    &animated,
    PathBuf::from("animated.svg"),
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
| `text2svg::shard` | Glyph-shard geometry: contour split, component grouping, scatter direction |
| `text2svg::utils` | Text wrapping utilities (greedy and Knuth-Plass), hyphenation pre-pass |
| `text2svg::text_analysis` | Unicode-aware segmentation: CJK, kinsoku, UAX #14 break opportunities |
| `text2svg::line_break` | Greedy line breaking with kinsoku rules |
| `text2svg::knuth_plass` | Knuth-Plass optimal line breaking (Box/Glue/Penalty model) |
| `text2svg::hyphenate` | Dictionary hyphenation (embedded English patterns) |

## CLI Usage

```
Usage: text2svg [OPTIONS] [TEXT]

Arguments:
  [TEXT]  input text string

Options:
      --width <WIDTH>              max width per line (characters)
      --pixel-width <PIXEL_WIDTH>  max width per line (pixels)
  -f, --file <FILE>                input file (use "-" for stdin)
  -o, --output <OUTPUT>            output svg file path (use "-" for stdout) [default: output.svg]
      --font <FONT>                font family name (e.g., "Arial", "Times New Roman")
      --font-file <FONT_FILE>      path to a font file (.otf/.ttf/.ttc), bypassing the system font database
      --size <SIZE>                font size in pixels [default: 64]
      --fill <FILL>                svg fill color (solid ink by default, so text is readable) [default: #111827]
      --color <COLOR>              stroke color; "none" by default (stroke only shows on hover with --hollow-hover) [default: none]
      --animation <ANIMATION>      none, draw, settle, scatter [default: none] [possible values: none, draw, settle, scatter]
      --hollow-hover               Reveal a hollow outline on hover (solid fill -> translucent + stroke)
      --explode <EXPLODE>          Shard spread amplitude in px; static exploded image when no animation
      --style <STYLE>              font style by the font's own style name (e.g. "regular", "bold", "9px")
      --space <SPACE>              letter spacing (in em units, e.g., 0.1) [default: 0]
      --features <FEATURES>        font features (e.g., "cv01=1,calt=0,liga=1")
      --var <VAR>                  variable-font axis settings (e.g., "wght=900,opsz=12")
      --highlight                  Enable syntax highlighting mode for files
      --theme <THEME>              Syntax highlighting theme name or path to .tmTheme file [default: base16-ocean.dark]
      --list-syntax                List supported file types/syntax for highlighting
      --list-theme                 List available built-in highlighting themes
      --align <ALIGN>              Text alignment for multi-line output [default: left] [possible values: left, center, right, justify]
      --no-hyphen                  Disable dictionary hyphenation (only affects justified output)
  -d, --debug                      Enable debug logging
      --list-fonts                 List installed font families
      --list-axes                  List the variable-font axes (--var tags) and value ranges
      --list-features              List the OpenType feature tags (--features targets) the font declares
  -h, --help                       Print help
  -V, --version                    Print version
```

### CLI Examples

Basic text conversion:
```bash
text2svg "Hello World" --font "Arial" --size 48 --output hello.svg
```

Glyph-shard animation (assemble the glyph strokes on load) with a hollow-outline hover:
```bash
text2svg "Dashboard" --font "Arial" --animation settle --hollow-hover -o title.svg
```

Static "exploded" glyph -- each stroke pushed out along its outline vector
(use a stroke-decomposed variable font and a bold weight for the best effect):
```bash
text2svg "永" --font-file SourceHanSerifSC-VF.otf --var wght=900 --explode 84 --size 220 -o exploded.svg
```

Select a font's own style name (not just the fixed weights):
```bash
text2svg "Hi" --font DinkieBitmap --style 9px -o pixel.svg
```

Discover a font's variable axes and feature tags:
```bash
text2svg --font-file SourceHanSerifSC-VF.otf --list-axes      # -> wght  min=250 default=250 max=900  Weight
text2svg --font "Arial" --list-features
```

File with syntax highlighting:
```bash
text2svg --file script.js --highlight --theme "base16-ocean.dark" --output code.svg
```

Text wrapping by pixel width:
```bash
text2svg "Long text that needs wrapping" --pixel-width 300 --font "Arial" --output wrapped.svg
```

Justified paragraph (optimal line breaking + hyphenation):
```bash
text2svg "The quick brown fox jumps over the lazy dog and keeps running" \
  --font "Times New Roman" --size 48 --pixel-width 400 --align justify -o justified.svg
```

Justified, hyphenation disabled:
```bash
text2svg "antidisestablishmentarianism is a long word" \
  --font "Arial" --pixel-width 200 --align justify --no-hyphen -o no-hyphen.svg
```

Justified CJK / mixed CJK + Latin:
```bash
text2svg "使用text2svg渲染CJK和Latin混排2026年效果对比" \
  --font "Arial Unicode MS" --size 40 --pixel-width 260 --align justify -o mixed.svg
```

## Glyph Shard Animations

Beyond the legacy per-line `draw` effect, text2svg can split each glyph into
its **shards** (an outer contour plus the counters it encloses) and animate
them. Shards, their scatter directions, and the CSS keyframes are all computed
at generation time, so the output is a self-contained SVG with no runtime
script.

| Mode | `--animation` | Effect |
|------|---------------|--------|
| Settle | `settle` | Shards fade in from their outline vector and assemble once on load |
| Scatter | `scatter` | Shards continuously scatter outward, fade out, then reassemble (loop) |
| Draw | `draw` | Legacy per-line stroke reveal (`stroke-dashoffset`) |
| Explode | `--explode <px>` (no animation) | Static image with each shard pushed out along its vector |

- Each shard flies along its **principal axis** (the largest eigenvector of the
  contour's area covariance), scaled by how elongated the shape is: long strokes
  fly far, round bits barely move.
- **Granularity depends on the font.** A font that stores glyphs as separate,
  overlapping stroke contours (e.g. `SourceHanSerifSC-VF.otf`) fragments into
  many shards; a font with merged outlines yields fewer. Load such a font with
  `--font-file` and pick a weight with `--var`.
- `--hollow-hover` turns the solid fill into a translucent tint plus a visible
  stroke on hover. Animations and hover need a browser / HTML embed; a static
  SVG viewer shows the assembled, filled text.
- `prefers-reduced-motion` is honored (animations are disabled).

## Advanced Typography

text2svg includes a TeX-style typesetting path for high-quality paragraphs.
**All of the features below are enabled by `--align justify` together with
`--pixel-width <N>`** (a column width is required). Other alignments
(`left`/`center`/`right`) use the simpler greedy wrapper and are unaffected.

| Feature | What it does | How to use |
|---------|--------------|------------|
| **Knuth-Plass line breaking** | Globally optimal break points that minimize raggedness, instead of greedy first-fit | `--align justify --pixel-width N` |
| **Justification** | Stretches inter-word spaces so each line fills the column; the last line stays ragged | enabled by `--align justify` |
| **Hyphenation** | Breaks long English words at dictionary points and appends a `-` | on by default with justify; disable with `--no-hyphen` |
| **CJK micro-spacing** | Distributes slack between ideographs so CJK lines justify without visible spaces | automatic for CJK under justify |
| **CJK <-> Latin autospace** | Inserts ~0.25em spacing at CJK/Latin or CJK/digit boundaries (CSS `text-autospace`) | automatic under justify |
| **Kinsoku rules** | Prevents prohibited punctuation at line start/end (CJK) | always on |
| **UAX #14 break opportunities** | Allows breaks after `/`, `-` in compounds, and other Unicode-defined points | always on |

Notes:

- `--pixel-width` is **required** for justification. Without a column width the
  optimizer has no target and the text renders as a single line.
- Hyphenation uses an embedded English (US) dictionary and only takes effect on
  the justify path.
- Under justify, the reported SVG `width`/`viewBox` may be slightly larger than
  `--pixel-width` (the bounding box uses advance-width estimation); glyph
  positions within the column are correct.

## Features

- **Text to SVG Conversion** -- Convert plain text or files to SVG using real font shaping (rustybuzz/HarfBuzz)
- **Font Customization** -- System fonts or direct font files (`--font-file`), the font's own style names (`--style 9px`), variable-font axes (`--var wght=900`), OpenType features, and letter spacing
- **Advanced Typography** -- Knuth-Plass optimal line breaking, full justification, dictionary hyphenation, CJK micro-spacing, and CJK<->Latin autospace (via `--align justify`)
- **Syntax Highlighting** -- Built-in syntax highlighting for code files via syntect
- **Glyph Shard Animations** -- Settle / scatter / explode effects and a hollow-outline hover, plus the legacy line-by-line draw animation
- **Font Introspection** -- List a font's variable axes (`--list-axes`) and feature tags (`--list-features`)
- **Text Wrapping** -- Greedy and Knuth-Plass line wrapping with Unicode-aware (UAX #14) segmentation
- **CJK Support** -- Proper CJK line breaking with kinsoku rules

## Cargo Features

| Feature | Default | Description |
|---------|---------|-------------|
| `cli` | Yes | Enables the CLI binary and clap dependency |

To use as a library without CLI dependencies:

```toml
text2svg = { version = "0.5", default-features = false }
```

## Credits

The glyph shard animation (settle / scatter / explode, hollow-outline hover) and
its covariance principal-axis scatter-direction model are adapted from
[**yuragi**](https://github.com/lawvs/yuragi) (MIT (c) lawvs and CircuitCoder).
Yuragi's title-animation study is itself inspired by
[CircuitCoder/layered](https://github.com/CircuitCoder/layered). Many thanks to
those projects for the visual idea.

