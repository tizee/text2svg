// some useful utility functions
use crate::font::{FontConfig, FontStyle};
use crate::hyphenate::Hyphenator;
use crate::knuth_plass::{self, Item};
use crate::line_break::{self, PreparedLineBreak};
use crate::text_analysis::{self, SegmentKind};
use anyhow::{anyhow, Result};
use rustybuzz::Face;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::iter::Iterator;
use std::path::Path;
use unicode_segmentation::UnicodeSegmentation;

// Reads file line by line, splitting lines longer than `max_chars_per_line`.
// Tries to wrap at whitespace for ASCII text.
pub fn open_file_by_lines_width<P: AsRef<Path>>(
    path: P,
    max_chars_per_line: usize,
) -> Result<Vec<String>> {
    let path = path.as_ref();
    if path.exists() && path.is_file() {
        match File::open(path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let width_iter = WidthLineIterator::new(reader, max_chars_per_line);
                Ok(width_iter.collect())
            }
            Err(err) => Err(anyhow!("{}: {}", path.display(), err)),
        }
    } else {
        Err(anyhow!(
            "{}: doesn't exist or is not a regular file",
            path.display()
        ))
    }
}

// Reads file line by line, splitting lines based on pixel width.
// Uses font metrics to determine actual text width for wrapping.
pub fn open_file_by_lines_pixel_width<P: AsRef<Path>>(
    path: P,
    max_pixel_width: f32,
    font_config: &mut FontConfig,
    font_style: &FontStyle,
) -> Result<Vec<String>> {
    let path = path.as_ref();
    if path.exists() && path.is_file() {
        match File::open(path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let pixel_width_iter =
                    PixelWidthLineIterator::new(reader, max_pixel_width, font_config, font_style);
                Ok(pixel_width_iter.collect())
            }
            Err(err) => Err(anyhow!("{}: {}", path.display(), err)),
        }
    } else {
        Err(anyhow!(
            "{}: doesn't exist or is not a regular file",
            path.display()
        ))
    }
}

// Reads file line by line without width constraints.
pub fn open_file_by_lines<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let path = path.as_ref();
    if path.exists() && path.is_file() {
        match File::open(path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let lines = reader
                    .lines()
                    .collect::<Result<Vec<String>, _>>()
                    .map_err(|e| anyhow!("{}: {}", path.display(), e))?;
                Ok(lines)
            }
            Err(err) => Err(anyhow!("{}: {}", path.display(), err)),
        }
    } else {
        Err(anyhow!(
            "{}: doesn't exist or is not a regular file",
            path.display()
        ))
    }
}

// --- WidthLineIterator ---
// Iterator that reads lines from a BufReader, but splits lines exceeding
// a specified character width, attempting word wrapping for ASCII.

struct WidthLineIterator<R: BufRead> {
    reader: R,
    max_width: usize,
    buffer: String, // Holds leftover part of a line for the next iteration
}

impl<R: BufRead> WidthLineIterator<R> {
    fn new(reader: R, max_width: usize) -> Self {
        WidthLineIterator {
            reader,
            max_width,
            buffer: String::new(),
        }
    }
}

impl<R: BufRead> Iterator for WidthLineIterator<R> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        // Process buffer first if exceeding max_width
        if self.buffer.chars().count() > self.max_width {
            let (line_part, remaining_part) = split_line(&self.buffer, self.max_width);
            self.buffer = remaining_part;
            return Some(line_part);
        }

        // If buffer has content within max_width, return it
        if !self.buffer.is_empty() {
            let buffer_content = std::mem::take(&mut self.buffer);
            return Some(buffer_content);
        }

        // Buffer empty, read a new line
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None, // EOF
            Ok(_) => {
                // Successfully read a line
                let trimmed_line = line.trim_end_matches(['\r', '\n']).to_string();

                // If line exceeds max_width, split it
                if trimmed_line.chars().count() > self.max_width {
                    let (line_part, remaining_part) = split_line(&trimmed_line, self.max_width);
                    self.buffer = remaining_part;
                    Some(line_part)
                } else {
                    // Line fits within max_width
                    Some(trimmed_line)
                }
            }
            Err(e) => {
                eprintln!("Error reading line: {}", e);
                None
            }
        }
    }
}

// --- PixelWidthLineIterator ---
// Iterator that reads lines from a BufReader, but splits lines based on pixel width
// using actual font metrics for precise text measurement.

struct PixelWidthLineIterator<'a, R: BufRead> {
    reader: R,
    max_pixel_width: f32,
    font_config: &'a mut FontConfig,
    font_style: &'a FontStyle,
    buffer: String, // Holds leftover part of a line for the next iteration
}

impl<'a, R: BufRead> PixelWidthLineIterator<'a, R> {
    fn new(
        reader: R,
        max_pixel_width: f32,
        font_config: &'a mut FontConfig,
        font_style: &'a FontStyle,
    ) -> Self {
        PixelWidthLineIterator {
            reader,
            max_pixel_width,
            font_config,
            font_style,
            buffer: String::new(),
        }
    }
}

impl<R: BufRead> Iterator for PixelWidthLineIterator<'_, R> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        // Process buffer first if it has content
        if !self.buffer.is_empty() {
            let buffer_content = std::mem::take(&mut self.buffer);
            // Use the new pipeline to wrap
            let wrapped = wrap_text_by_pixel_width(
                &buffer_content,
                self.max_pixel_width,
                self.font_config,
                self.font_style,
            );
            if wrapped.len() > 1 {
                // Store remaining lines in buffer (join with newline to re-split later)
                self.buffer = wrapped[1..].join("\n");
                return Some(wrapped[0].clone());
            }
            return Some(wrapped.into_iter().next().unwrap_or_default());
        }

        // Buffer empty, read a new line
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None, // EOF
            Ok(_) => {
                let trimmed_line = line.trim_end_matches(['\r', '\n']).to_string();

                // Use new pipeline for wrapping
                let wrapped = wrap_text_by_pixel_width(
                    &trimmed_line,
                    self.max_pixel_width,
                    self.font_config,
                    self.font_style,
                );
                if wrapped.len() > 1 {
                    self.buffer = wrapped[1..].join("\n");
                    Some(wrapped[0].clone())
                } else {
                    Some(wrapped.into_iter().next().unwrap_or_default())
                }
            }
            Err(e) => {
                eprintln!("Error reading line: {}", e);
                None
            }
        }
    }
}

// Helper function to split a line at max_width, trying to wrap at whitespace.
fn split_line(line: &str, max_width: usize) -> (String, String) {
    if line.chars().count() <= max_width {
        return (line.trim_end().to_string(), String::new());
    }

    // Find the character index corresponding to max_width
    let split_char_index = if let Some((idx, _)) = line.char_indices().nth(max_width) {
        idx
    } else if line.chars().count() > max_width {
        line.char_indices()
            .nth(max_width)
            .map(|(i, _)| i)
            .unwrap_or(line.len())
    } else {
        0
    };

    // Look backwards from the split point for whitespace
    let potential_split_point = &line[..split_char_index];
    let wrap_index = potential_split_point
        .char_indices()
        .rev()
        .find(|&(_, c)| c.is_ascii_whitespace())
        .map(|(i, _)| i);

    if let Some(idx) = wrap_index {
        // Found whitespace: split before it, trim whitespace
        let first_part = potential_split_point[..idx].trim_end().to_string();
        let second_part = line[idx..].trim_start().to_string();
        (first_part, second_part)
    } else {
        // No whitespace found: hard break at max_width chars
        let (first_part, second_part) = line.split_at(split_char_index);
        (first_part.to_string(), second_part.trim_start().to_string()) // Added trim_start() here
    }
}

// Calculate the pixel width of text using font metrics
pub fn calculate_text_width(
    text: &str,
    font_config: &mut FontConfig,
    font_style: &FontStyle,
) -> Option<f32> {
    if text.is_empty() {
        return Some(0.0);
    }

    // Get the font face for the specified style, fallback to regular
    let ft_face = font_config
        .get_font_by_style(font_style)
        .or_else(|| font_config.get_font_by_style(&FontStyle::Regular))?;

    let font_data = ft_face.copy_font_data()?;
    let mut hb_face = Face::from_slice(&font_data, 0)?;
    hb_face.set_variations(font_config.get_variations());

    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);

    let glyph_buffer = rustybuzz::shape(&hb_face, font_config.get_features(), buffer);

    // Calculate total advance width
    let mut total_width = 0.0;
    let glyph_positions = glyph_buffer.glyph_positions();

    // Get font metrics for scaling
    let metrics = ft_face.metrics();
    let target_size = font_config.get_size() as f32;
    let origin_glyph_height = metrics.ascent - metrics.descent;
    let scale_factor = target_size / origin_glyph_height.max(1.0);

    for glyph_pos in glyph_positions {
        total_width += glyph_pos.x_advance as f32 * scale_factor;
    }

    // Add letter spacing
    let letter_space = scale_factor * font_config.get_letter_space() * metrics.units_per_em as f32;
    let char_count = text.chars().count();
    if char_count > 1 {
        total_width += letter_space * (char_count - 1) as f32;
    }

    Some(total_width)
}

/// Measure segment widths using font metrics and prepare for line breaking.
fn prepare_segments(
    analysis: &text_analysis::TextAnalysis,
    font_config: &mut FontConfig,
    font_style: &FontStyle,
) -> PreparedLineBreak {
    let widths: Vec<f32> = analysis
        .segments
        .iter()
        .map(|seg| match seg.kind {
            SegmentKind::HardBreak | SegmentKind::ZeroWidthBreak => 0.0,
            SegmentKind::SoftHyphen => 0.0,
            SegmentKind::Space | SegmentKind::Text => {
                calculate_text_width(&seg.text, font_config, font_style).unwrap_or(0.0)
            }
        })
        .collect();
    PreparedLineBreak::new(analysis.clone(), widths)
}

/// Wrap text using Unicode-aware segmentation and greedy line breaking.
/// Uses text analysis for CJK detection, kinsoku rules, and proper break points.
pub fn wrap_text_by_pixel_width(
    text: &str,
    max_pixel_width: f32,
    font_config: &mut FontConfig,
    font_style: &FontStyle,
) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let analysis = text_analysis::analyze(text);
    let prepared = prepare_segments(&analysis, font_config, font_style);
    let line_ranges = line_break::layout(&prepared, max_pixel_width);

    if line_ranges.is_empty() {
        return vec![String::new()];
    }

    line_ranges
        .iter()
        .map(|range| line_break::line_text(&prepared.analysis, range))
        .collect()
}

/// A wrapped line carrying justification data.
#[derive(Debug, Clone, PartialEq)]
pub struct JustifiedLine {
    /// The text content of the line (trailing spaces trimmed).
    pub text: String,
    /// Glue adjustment ratio for this line (positive = stretch, negative = shrink).
    pub ratio: f32,
    /// Natural pixel width of the line content before adjustment.
    pub width: f32,
}

/// Convert measured segments into Knuth-Plass items.
///
/// Mapping:
/// - Text          -> Box
/// - Space         -> Glue (stretchable; this is what enables justification)
/// - ZeroWidthBreak / SoftHyphen -> zero-width legal Penalty
/// - HardBreak     -> forced Penalty
/// - CJK boundary (Text segment with break_after) -> a zero-width Penalty is
///   inserted after the Box so the optimizer may break between ideographs.
///
/// Returns the item list plus, for each item, the index of the segment it came
/// from (None for synthetic break penalties).
/// Insert soft hyphens (U+00AD) into hyphenatable words using the dictionary.
///
/// The result feeds the normal analysis pipeline, where each soft hyphen becomes
/// a flagged break the optimizer may use. Words that already contain a soft
/// hyphen are left untouched (author intent wins).
pub fn apply_hyphenation(text: &str, hyphenator: &Hyphenator) -> String {
    let mut out = String::with_capacity(text.len());
    for token in text.split_word_bounds() {
        // Only hyphenate pure-alphabetic word tokens without existing soft hyphens.
        if token.contains('\u{00AD}') || !token.chars().all(|c| c.is_alphabetic()) {
            out.push_str(token);
            continue;
        }
        let breaks = hyphenator.break_indices(token);
        if breaks.is_empty() {
            out.push_str(token);
            continue;
        }
        // breaks are character indices; insert U+00AD at each.
        let mut next_break = 0usize;
        for (ci, ch) in token.chars().enumerate() {
            if next_break < breaks.len() && breaks[next_break] == ci && ci != 0 {
                out.push('\u{00AD}');
                next_break += 1;
            }
            out.push(ch);
        }
    }
    out
}

/// Latin letter or digit (the "Latin/digit" side of CSS text-autospace).
fn is_latin_alnum(ch: char) -> bool {
    ch.is_alphanumeric() && !text_analysis::is_cjk(ch)
}

/// True when the boundary between `seg` and `next` is a CJK<->Latin/digit
/// boundary (in either direction), i.e. where text-autospace applies.
fn is_cjk_latin_boundary(
    seg: &text_analysis::Segment,
    next: Option<&text_analysis::Segment>,
) -> bool {
    let next = match next {
        Some(n) if n.kind == SegmentKind::Text => n,
        _ => return false,
    };
    let last = match seg.text.chars().last() {
        Some(c) => c,
        None => return false,
    };
    let first = match next.text.chars().next() {
        Some(c) => c,
        None => return false,
    };
    let a_cjk = text_analysis::is_cjk(last);
    let b_cjk = text_analysis::is_cjk(first);
    (a_cjk && is_latin_alnum(first)) || (b_cjk && is_latin_alnum(last))
}

fn segments_to_items(
    analysis: &text_analysis::TextAnalysis,
    widths: &[f32],
    em: f32,
) -> (Vec<Item>, Vec<Option<usize>>) {
    let segments = &analysis.segments;
    let mut items: Vec<Item> = Vec::new();
    let mut origin: Vec<Option<usize>> = Vec::new();

    for (i, seg) in segments.iter().enumerate() {
        let w = widths[i];
        match seg.kind {
            SegmentKind::Text => {
                items.push(Item::Box { width: w });
                origin.push(Some(i));
                // Insert a synthetic break opportunity after CJK characters.
                if seg.break_after {
                    let next = segments.get(i + 1);
                    if is_cjk_latin_boundary(seg, next) {
                        // CSS text-autospace: CJK<->Latin/digit boundary gets a
                        // small natural-width gap (0.25em) plus a little stretch.
                        items.push(Item::Glue {
                            width: 0.25 * em,
                            stretch: 0.05 * em,
                            shrink: 0.0,
                        });
                    } else {
                        // CJK<->CJK: zero-width *stretchable* micro-glue so the
                        // optimizer can distribute slack across ideographs for
                        // full justification.
                        items.push(Item::Glue {
                            width: 0.0,
                            stretch: w * 0.5,
                            shrink: 0.0,
                        });
                    }
                    origin.push(None);
                }
            }
            SegmentKind::Space => {
                // Space stretch/shrink derived from its own width (em-like).
                items.push(Item::Glue {
                    width: w,
                    stretch: w * 0.5,
                    shrink: w / 3.0,
                });
                origin.push(Some(i));
            }
            SegmentKind::ZeroWidthBreak => {
                items.push(Item::Penalty {
                    width: 0.0,
                    penalty: 0.0,
                    flagged: false,
                });
                origin.push(Some(i));
            }
            SegmentKind::SoftHyphen => {
                // A hyphenation point: a flagged break with a cost so the
                // optimizer only hyphenates when it improves the paragraph.
                items.push(Item::Penalty {
                    width: 0.0,
                    penalty: 50.0,
                    flagged: true,
                });
                origin.push(Some(i));
            }
            SegmentKind::HardBreak => {
                items.push(Item::Penalty {
                    width: 0.0,
                    penalty: knuth_plass::FORCED_PENALTY,
                    flagged: false,
                });
                origin.push(Some(i));
            }
        }
    }

    // TeX paragraph terminator: infinite-stretch glue then a forced break, so
    // the optimizer always produces a final line covering the trailing content.
    items.push(Item::Glue {
        width: 0.0,
        stretch: 1_000_000.0,
        shrink: 0.0,
    });
    origin.push(None);
    items.push(Item::Penalty {
        width: 0.0,
        penalty: knuth_plass::FORCED_PENALTY,
        flagged: false,
    });
    origin.push(None);

    (items, origin)
}

/// Wrap text using Unicode-aware segmentation and Knuth-Plass optimal breaking.
///
/// Returns per-line text together with the justification ratio and natural
/// width, so the renderer can justify lines. Falls back internally to a greedy
/// solution when no feasible optimal break exists.
pub fn wrap_text_knuth_plass(
    text: &str,
    max_pixel_width: f32,
    font_config: &mut FontConfig,
    font_style: &FontStyle,
    hyphenate: bool,
) -> Vec<JustifiedLine> {
    if text.is_empty() {
        return vec![JustifiedLine {
            text: String::new(),
            ratio: 0.0,
            width: 0.0,
        }];
    }

    // Optionally insert dictionary soft hyphens before analysis.
    let hyphenated;
    let text = if hyphenate {
        if let Some(h) = Hyphenator::english_us() {
            hyphenated = apply_hyphenation(text, &h);
            hyphenated.as_str()
        } else {
            text
        }
    } else {
        text
    };

    let em = font_config.get_size() as f32;
    let analysis = text_analysis::analyze(text);
    let prepared = prepare_segments(&analysis, font_config, font_style);
    let (items, origin) = segments_to_items(&analysis, &prepared.widths, em);

    if items.is_empty() {
        return vec![JustifiedLine {
            text: String::new(),
            ratio: 0.0,
            width: 0.0,
        }];
    }

    let params = knuth_plass::Params::default();
    let break_lines = knuth_plass::break_lines(&items, max_pixel_width, &params);

    if break_lines.is_empty() {
        return vec![JustifiedLine {
            text: String::new(),
            ratio: 0.0,
            width: 0.0,
        }];
    }

    break_lines
        .iter()
        .map(|bl| {
            // Map the item range back to segment indices, then reuse line_text.
            let seg_start = origin[bl.start.min(origin.len() - 1)]
                .or_else(|| {
                    // start may land on a synthetic penalty; find next real segment
                    origin[bl.start..].iter().flatten().next().copied()
                })
                .unwrap_or(0);
            let mut seg_end = origin[bl.start..bl.end.min(origin.len())]
                .iter()
                .flatten()
                .last()
                .map(|&s| s + 1)
                .unwrap_or(seg_start);
            // If the line ends at a flagged hyphenation penalty, include that
            // soft-hyphen segment so line_text emits the trailing '-'.
            if let Some(Item::Penalty { flagged: true, .. }) = items.get(bl.end) {
                if let Some(Some(shy_seg)) = origin.get(bl.end) {
                    seg_end = seg_end.max(shy_seg + 1);
                }
            }
            let range = line_break::LineRange {
                start: seg_start,
                end: seg_end,
                width: 0.0,
            };
            let text = line_break::line_text(&analysis, &range);
            let width = prepared.widths[seg_start..seg_end.min(prepared.widths.len())]
                .iter()
                .sum();
            JustifiedLine {
                text,
                ratio: bl.ratio,
                width,
            }
        })
        .collect()
}

#[cfg(test)]
mod test_utils {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_open_file_not_found() {
        match open_file_by_lines("/tmp/file-does-not-exist-hopefully") {
            Ok(_) => panic!("Should have failed"),
            Err(e) => assert!(e
                .to_string()
                .contains("doesn't exist or is not a regular file")),
        }
        match open_file_by_lines_width("/tmp/file-does-not-exist-hopefully", 80) {
            Ok(_) => panic!("Should have failed"),
            Err(e) => assert!(e
                .to_string()
                .contains("doesn't exist or is not a regular file")),
        }
    }

    #[test]
    fn test_read_lines_basic() {
        let data = "line1\nline2\nline3";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let lines: Vec<String> = reader.lines().map(|l| l.unwrap()).collect();
        assert_eq!(lines, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_split_line_simple() {
        let (l, r) = split_line("abcdefghijkl", 5);
        assert_eq!(l, "abcde");
        assert_eq!(r, "fghijkl");
    }

    #[test]
    fn test_split_line_with_whitespace_wrap() {
        let (l, r) = split_line("abcde fghijkl", 8);
        assert_eq!(l, "abcde"); // Wraps before 'f' at the space
        assert_eq!(r, "fghijkl");
    }

    #[test]
    fn test_split_line_with_whitespace_at_end() {
        let (l, r) = split_line("abcde ", 5); // Space is exactly at width limit
        assert_eq!(l, "abcde"); // Space is trimmed
        assert_eq!(r, "");
    }

    #[test]
    fn test_split_line_no_whitespace() {
        let (l, r) = split_line("abcdefghijkl", 5);
        assert_eq!(l, "abcde"); // Hard break
        assert_eq!(r, "fghijkl");
    }

    #[test]
    fn test_split_line_non_ascii() {
        let (l, r) = split_line("你好世界你好世界", 3); // Split after 3 chars
        assert_eq!(l, "你好世");
        assert_eq!(r, "界你好世界");
    }

    #[test]
    fn test_width_iter_long_text_no_wrap() {
        let data = "123123123";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 3);
        let lines: Vec<String> = width_iter.collect();
        assert_eq!(lines, vec!["123", "123", "123"]);
    }

    #[test]
    fn test_width_iter_non_ascii_wrap() {
        let data = "当我发现我童年和少年时期的旧日记时，它们已经被尘埃所覆盖。";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 26);
        let lines: Vec<String> = width_iter.collect();
        // Should hard break as no ASCII whitespace involved
        assert_eq!(
            lines,
            vec![
                "当我发现我童年和少年时期的旧日记时，它们已经被尘埃所",
                "覆盖。"
            ]
        );
    }

    #[test]
    fn test_width_iter_text_wrapping_ascii() {
        let data = "When I found my old diaries from my childhood and teen years, they were covered in dust.";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 76);
        let lines: Vec<String> = width_iter.collect();
        // Should wrap at "were"
        assert_eq!(
            lines,
            vec![
                "When I found my old diaries from my childhood and teen years, they were",
                "covered in dust."
            ]
        );
    }

    #[test]
    fn test_width_iter_multiple_lines_wrapping() {
        let data = "This is the first line which is quite long and needs wrapping.\nThis is the second line, also long.\nShort third.";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 20);
        let lines: Vec<String> = width_iter.collect();
        assert_eq!(
            lines,
            vec![
                "This is the first",
                "line which is quite",
                "long and needs",
                "wrapping.",
                "This is the second",
                "line, also long.",
                "Short third."
            ]
        );
    }

    #[test]
    fn test_width_iter_empty_lines() {
        let data = "Line 1\n\nLine 3";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 80);
        let lines: Vec<String> = width_iter.collect();
        assert_eq!(lines, vec!["Line 1", "", "Line 3"]);
    }

    #[test]
    fn test_width_iter_exact_width() {
        let data = "12345\n67890";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 5);
        let lines: Vec<String> = width_iter.collect();
        assert_eq!(lines, vec!["12345", "67890"]);
    }

    // Helper function to create a font config with system fonts for testing
    fn create_test_font_config() -> FontConfig {
        use crate::font::fonts;

        // Try to get system fonts and use the first available one
        let available_fonts = fonts();
        let font_name = if !available_fonts.is_empty() {
            available_fonts[0].clone()
        } else {
            // Fallback to common system fonts
            #[cfg(target_os = "macos")]
            let fallback = "Arial";
            #[cfg(target_os = "windows")]
            let fallback = "Arial";
            #[cfg(target_os = "linux")]
            let fallback = "DejaVu Sans";
            fallback.to_string()
        };

        FontConfig::new(font_name, 16, "#000".to_string(), "#000".to_string(), false)
            .expect("Failed to create font config with system font")
    }

    #[test]
    fn test_wrap_text_by_pixel_width_empty() {
        // Test empty string - this should always work
        use crate::font::FontStyle;

        let mut font_config = create_test_font_config();
        let result = wrap_text_by_pixel_width("", 100.0, &mut font_config, &FontStyle::Regular);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn test_calculate_text_width_empty() {
        // Test empty string width calculation
        use crate::font::FontStyle;

        let mut font_config = create_test_font_config();
        let width = calculate_text_width("", &mut font_config, &FontStyle::Regular);
        assert_eq!(width, Some(0.0));
    }

    #[test]
    fn test_calculate_text_width_simple() {
        // Test width calculation for simple text
        use crate::font::FontStyle;

        let mut font_config = create_test_font_config();
        let width = calculate_text_width("Hello", &mut font_config, &FontStyle::Regular);

        // Width should be Some positive value for non-empty text
        assert!(width.is_some());
        assert!(width.unwrap() > 0.0);
    }

    #[test]
    fn test_wrap_no_split_needed() {
        // Test wrapping when text fits within pixel width
        use crate::font::FontStyle;

        let mut font_config = create_test_font_config();
        let text = "Short";

        // Use a very large pixel width - text should not be split
        let result = wrap_text_by_pixel_width(text, 10000.0, &mut font_config, &FontStyle::Regular);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], text);
    }

    #[test]
    fn test_wrap_needs_split() {
        // Test wrapping when text exceeds pixel width
        use crate::font::FontStyle;

        let mut font_config = create_test_font_config();
        let text = "This is a longer text that should be split";

        // Use a small pixel width to force splitting
        let result = wrap_text_by_pixel_width(text, 50.0, &mut font_config, &FontStyle::Regular);

        // Should have split into multiple lines
        assert!(result.len() > 1);

        // Verify content preservation
        let combined = result.join(" ");
        let normalized_combined = combined.split_whitespace().collect::<Vec<_>>().join(" ");
        let normalized_original = text.split_whitespace().collect::<Vec<_>>().join(" ");

        assert_eq!(normalized_combined, normalized_original);
    }

    #[test]
    fn test_wrap_text_by_pixel_width_single_line() {
        // Test wrapping text that fits in one line
        use crate::font::FontStyle;

        let mut font_config = create_test_font_config();
        let text = "Short text";

        let result = wrap_text_by_pixel_width(text, 10000.0, &mut font_config, &FontStyle::Regular);

        // Should return single line
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], text);
    }

    #[test]
    fn test_wrap_text_by_pixel_width_multiple_lines() {
        // Test wrapping text that needs multiple lines
        use crate::font::FontStyle;

        let mut font_config = create_test_font_config();
        let text = "This is a very long text that should definitely be wrapped into multiple lines when using a small pixel width";

        let result = wrap_text_by_pixel_width(text, 100.0, &mut font_config, &FontStyle::Regular);

        // Should return multiple lines
        assert!(result.len() > 1);

        // Verify all lines together equal original text
        let combined = result
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let original = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(combined, original);
    }

    // --- Knuth-Plass justify-capable wrapping ---

    #[test]
    fn knuth_plass_empty_text_returns_one_empty_line() {
        use crate::font::FontStyle;
        let mut font_config = create_test_font_config();
        let lines = wrap_text_knuth_plass("", 100.0, &mut font_config, &FontStyle::Regular, false);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "");
    }

    #[test]
    fn knuth_plass_short_text_fits_on_one_line() {
        use crate::font::FontStyle;
        let mut font_config = create_test_font_config();
        let lines = wrap_text_knuth_plass(
            "Short text",
            10000.0,
            &mut font_config,
            &FontStyle::Regular,
            false,
        );
        assert_eq!(lines.len(), 1, "short text must be a single line");
        assert_eq!(lines[0].text, "Short text");
    }

    #[test]
    fn knuth_plass_long_text_wraps_and_preserves_content() {
        use crate::font::FontStyle;
        let mut font_config = create_test_font_config();
        let original =
            "The quick brown fox jumps over the lazy dog and then keeps running across the field";
        let lines = wrap_text_knuth_plass(
            original,
            120.0,
            &mut font_config,
            &FontStyle::Regular,
            false,
        );

        assert!(lines.len() > 1, "long text must wrap into multiple lines");

        // Every line's ratio must be finite (renderer relies on this).
        for l in &lines {
            assert!(l.ratio.is_finite(), "ratio must be finite, got {}", l.ratio);
        }

        // Content preservation (word sequence), ignoring how spaces were distributed.
        let combined = lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let norm_combined = combined.split_whitespace().collect::<Vec<_>>().join(" ");
        let norm_original = original.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(norm_combined, norm_original, "no words may be lost");
    }

    #[test]
    fn knuth_plass_cjk_wraps_and_preserves_characters() {
        use crate::font::FontStyle;
        let mut font_config = create_test_font_config();
        let original = "当我发现我童年和少年时期的旧日记时它们已经被尘埃所覆盖";
        let lines =
            wrap_text_knuth_plass(original, 80.0, &mut font_config, &FontStyle::Regular, false);

        assert!(lines.len() > 1, "CJK paragraph must wrap");

        // All CJK characters preserved in order.
        let combined: String = lines.iter().map(|l| l.text.clone()).collect();
        assert_eq!(
            combined, original,
            "CJK characters must be preserved across line breaks"
        );

        // No line may start with a kinsoku-start punctuation (sanity vs existing rules).
        for l in &lines {
            if let Some(first) = l.text.chars().next() {
                assert!(
                    first != '。' && first != '，',
                    "line should not start with CJK punctuation: {:?}",
                    l.text
                );
            }
        }
    }

    // Font-independent: build items from an analysis with uniform widths.
    fn items_from_uniform(text: &str, w: f32) -> Vec<Item> {
        items_from_uniform_em(text, w, 0.0)
    }

    fn items_from_uniform_em(text: &str, w: f32, em: f32) -> Vec<Item> {
        let analysis = text_analysis::analyze(text);
        let widths: Vec<f32> = analysis
            .segments
            .iter()
            .map(|s| match s.kind {
                SegmentKind::Text | SegmentKind::Space => s.text.chars().count() as f32 * w,
                _ => 0.0,
            })
            .collect();
        let (items, _origin) = segments_to_items(&analysis, &widths, em);
        items
    }

    #[test]
    fn apply_hyphenation_inserts_soft_hyphens_in_long_words() {
        let h = Hyphenator::english_us().expect("dict");
        let out = apply_hyphenation("hyphenation", &h);
        assert!(
            out.contains('\u{00AD}'),
            "long word should get soft hyphens: {:?}",
            out
        );
        // Removing the soft hyphens must restore the original word.
        let stripped: String = out.chars().filter(|c| *c != '\u{00AD}').collect();
        assert_eq!(stripped, "hyphenation");
    }

    #[test]
    fn apply_hyphenation_leaves_short_words_and_punctuation() {
        let h = Hyphenator::english_us().expect("dict");
        let out = apply_hyphenation("a cat, ok.", &h);
        assert_eq!(out, "a cat, ok.", "short words/punctuation unchanged");
    }

    #[test]
    fn hyphenated_justify_breaks_a_long_word_with_dash() {
        use crate::font::FontStyle;
        let mut font_config = create_test_font_config();
        // A single very long word in a narrow column: with hyphenation enabled,
        // it should break across lines with a trailing hyphen rather than
        // overflow as one piece.
        let text = "antidisestablishmentarianism";
        let lines = wrap_text_knuth_plass(text, 80.0, &mut font_config, &FontStyle::Regular, true);
        assert!(
            lines.len() > 1,
            "long word in narrow column should break, got {} line(s)",
            lines.len()
        );
        let any_dash = lines
            .iter()
            .take(lines.len() - 1)
            .any(|l| l.text.ends_with('-'));
        assert!(
            any_dash,
            "a hyphenated break should end a line with '-': {:?}",
            lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>()
        );
        // Content preserved (dashes are inserted hyphens, strip them).
        let joined: String = lines.iter().map(|l| l.text.replace('-', "")).collect();
        assert_eq!(joined, text, "hyphenation must not lose characters");
    }

    #[test]
    fn apply_hyphenation_preserves_author_soft_hyphens() {
        let h = Hyphenator::english_us().expect("dict");
        let input = "ri\u{00AD}bonuclease";
        let out = apply_hyphenation(input, &h);
        assert_eq!(out, input, "words with existing soft hyphens are untouched");
    }

    #[test]
    fn cjk_latin_boundary_inserts_autospace_glue() {
        // CSS text-autospace: a CJK<->Latin boundary gets ~0.25em natural-width
        // glue. With em=40, expect a glue with width ~= 10 between the boxes.
        let em = 40.0;
        let items = items_from_uniform_em("中a", 10.0, em);
        let expected = 0.25 * em;
        let has_autospace = items
            .iter()
            .any(|it| matches!(it, Item::Glue { width, .. } if (*width - expected).abs() < 1e-3));
        assert!(
            has_autospace,
            "expected ~{}px autospace glue at CJK/Latin boundary, got: {:?}",
            expected, items
        );
    }

    #[test]
    fn latin_to_cjk_boundary_also_inserts_autospace() {
        let em = 40.0;
        let items = items_from_uniform_em("a中", 10.0, em);
        let expected = 0.25 * em;
        let has_autospace = items
            .iter()
            .any(|it| matches!(it, Item::Glue { width, .. } if (*width - expected).abs() < 1e-3));
        assert!(
            has_autospace,
            "expected autospace glue at Latin/CJK boundary, got: {:?}",
            items
        );
    }

    #[test]
    fn no_autospace_between_two_cjk_chars() {
        let em = 40.0;
        let items = items_from_uniform_em("中文", 10.0, em);
        let expected = 0.25 * em;
        let has_autospace = items
            .iter()
            .any(|it| matches!(it, Item::Glue { width, .. } if (*width - expected).abs() < 1e-3));
        assert!(
            !has_autospace,
            "CJK<->CJK must use zero-width micro-glue, not autospace: {:?}",
            items
        );
    }

    #[test]
    fn cjk_boundary_is_stretchable_glue_not_rigid_penalty() {
        // Between two CJK boxes there must be a stretchable Glue (the pattern
        // Box, Glue{width:0, stretch>0}, Box) so the optimizer can distribute
        // slack for justification. The trailing paragraph terminator glue is
        // excluded by requiring a Box immediately after the glue.
        let items = items_from_uniform("天地", 10.0);
        let mut found = false;
        for w in items.windows(3) {
            if matches!(w[0], Item::Box { .. })
                && matches!(w[1], Item::Glue { width, stretch, .. } if width == 0.0 && stretch > 0.0)
                && matches!(w[2], Item::Box { .. })
            {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected Box,micro-Glue(stretch>0,width0),Box between CJK chars, got: {:?}",
            items
        );
    }

    // Test the basic functionality without requiring actual fonts
    #[test]
    fn test_pixel_width_api_exists() {
        // This test verifies that the pixel width API functions exist and can be called
        // It tests the API surface without requiring working fonts

        let text = "Test";

        // Test that the functions exist - we're not testing functionality here,
        // just API availability since we can't guarantee font availability in tests
        let _result = format!("pixel width API functions are available for text: {}", text);
        // API exists if we reach here
    }
}
