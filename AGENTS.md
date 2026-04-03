# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Build and Test Commands

```bash
cargo check --lib --no-default-features   # check lib compiles without CLI deps
cargo check                               # check everything (lib + CLI binary)
cargo fmt -- --check                      # verify formatting
cargo fmt                                 # auto-format
make lint                                 # clippy with -D warnings (all targets, all features)
cargo test                                # run all 74 inline unit tests
cargo test test_name                      # run a single test by name
cargo test --no-default-features          # test lib-only (no clap)
```

Release flow: `cargo fmt` -> `make lint` -> `cargo test` -> commit -> tag -> push -> `cargo publish --registry crates-io`

## Architecture

Single crate that is both a library (`src/lib.rs`) and a CLI binary (`src/main.rs`). The `clap` dependency is optional behind the `cli` feature (default on). Library consumers use `default-features = false` to avoid pulling in CLI deps.

### Module dependency flow

```
main.rs (CLI, clap)
  └─> lib.rs (public API)
        ├── render    ─── core rendering: text->SVG, file->SVG, highlighted file->SVG
        │     ├── font       ─── FontConfig, FontStyle, font loading via font-kit, OpenType features
        │     ├── svg        ─── glyph path building, GlyphCache/GlyphDefs, <use>/<defs> generation
        │     ├── highlight  ─── HighlightSetting wrapping syntect (themes, syntax sets)
        │     └── utils      ─── text wrapping (char-based and pixel-based)
        ├── text_analysis    ─── Unicode segmentation, CJK detection, kinsoku rule merging
        └── line_break       ─── greedy line breaking algorithm using pre-measured segment widths
```

`render` is the main orchestrator. It calls `font` for shaping via rustybuzz, `svg` to build glyph paths, and `utils`/`text_analysis`/`line_break` for text wrapping.

### Key types

- `FontConfig` (`font.rs`) -- holds loaded font faces (HashMap<FontStyle, Font>), OpenType features, colors. Constructed from a font family name via `font-kit::SystemSource`.
- `RenderConfig` (`render.rs`) -- rendering options: animation, font style, alignment, width constraints.
- `TextBuilder` (`svg.rs`) -- builds SVG `<use>` element groups referencing glyph `<path>` definitions in `<defs>`. Uses `GlyphCache` (glyph_id -> svg_id) and `GlyphDefs` (svg_id -> path node) for deduplication.
- `TextAnalysis` / `Segment` (`text_analysis.rs`) -- segments text into typed pieces (Text, Space, HardBreak, SoftHyphen, ZeroWidthBreak) with kinsoku-merged CJK characters.

### Feature gating pattern

`FontStyle` and `TextAlign` use `#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]` so they derive clap traits only when the CLI feature is active. Both implement `FromStr` unconditionally for library use.

## Conventions

- All tests are inline `#[cfg(test)] mod tests` within each module -- no separate `tests/` directory.
- Font-dependent tests in `utils.rs` use `create_test_font_config()` which picks the first available system font.
- `text_analysis.rs` and `line_break.rs` tests use uniform character widths to avoid font dependency.
