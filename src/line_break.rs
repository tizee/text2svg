// Line breaking module: segment-aware greedy line breaking with kinsoku support.
//
// Uses pre-measured segment widths and a greedy first-fit algorithm.
// Handles CJK break opportunities, kinsoku rules, overflow-wrap, and soft hyphens.

use crate::text_analysis::{SegmentKind, TextAnalysis};

/// A range of segments forming one line.
#[derive(Debug, Clone, PartialEq)]
pub struct LineRange {
    /// Start index in the segments array (inclusive)
    pub start: usize,
    /// End index in the segments array (exclusive)
    pub end: usize,
    /// Total width of this line in font units
    pub width: f32,
}

/// Pre-measured segment data ready for line breaking.
#[derive(Debug, Clone)]
pub struct PreparedLineBreak {
    /// Width of each segment in font units
    pub widths: Vec<f32>,
    /// The text analysis (segment kinds, break_after flags, etc.)
    pub analysis: TextAnalysis,
}

impl PreparedLineBreak {
    /// Create from pre-measured widths and analysis.
    pub fn new(analysis: TextAnalysis, widths: Vec<f32>) -> Self {
        assert_eq!(
            analysis.segments.len(),
            widths.len(),
            "widths must match segments count"
        );
        Self { widths, analysis }
    }
}

/// Perform greedy line breaking on prepared segments.
///
/// Algorithm:
/// 1. Walk segments left-to-right, accumulating width
/// 2. Track the latest break opportunity and the content width at that point
/// 3. When accumulated width exceeds max_width:
///    a. Break at the last break opportunity
///    b. If no break opportunity exists, force break at current position (overflow-wrap)
/// 4. Hard breaks always force a new line
/// 5. Trailing spaces at line ends don't count toward line width
pub fn layout(prepared: &PreparedLineBreak, max_width: f32) -> Vec<LineRange> {
    let segments = &prepared.analysis.segments;
    let widths = &prepared.widths;

    if segments.is_empty() {
        return vec![];
    }

    let mut lines: Vec<LineRange> = Vec::new();
    let mut line_start = 0;
    let mut line_width: f32 = 0.0;
    // The break opportunity: segment index after which we can break, and the
    // content width of the line up to (and including) that segment's content.
    // For spaces, content_width excludes the space itself.
    let mut last_break: Option<(usize, f32)> = None;

    let mut i = 0;
    while i < segments.len() {
        let seg = &segments[i];
        let seg_width = widths[i];

        // Hard break: emit current line and start fresh
        if seg.kind == SegmentKind::HardBreak {
            lines.push(LineRange {
                start: line_start,
                end: i,
                width: line_width,
            });
            line_start = i + 1;
            line_width = 0.0;
            last_break = None;
            i += 1;
            continue;
        }

        let new_width = line_width + seg_width;

        // Does adding this segment overflow?
        if new_width > max_width && i > line_start {
            // If this segment itself is a space (break opportunity) and the content
            // before it fits, we should break HERE -- the space caused overflow but
            // the preceding content fits.
            if seg.break_after && seg.kind == SegmentKind::Space && line_width <= max_width {
                // Record this as the break point, then emit line
                let content_width = line_width; // content before this space
                lines.push(LineRange {
                    start: line_start,
                    end: i, // exclude the space
                    width: content_width,
                });
                // Skip this space and any following spaces
                i += 1;
                while i < segments.len() && segments[i].kind == SegmentKind::Space {
                    i += 1;
                }
                line_start = i;
                line_width = 0.0;
                last_break = None;
                continue;
            }

            if let Some((break_idx, content_width)) = last_break {
                let end = break_idx + 1;
                lines.push(LineRange {
                    start: line_start,
                    end,
                    width: content_width,
                });
                // Skip leading spaces on new line
                line_start = end;
                while line_start < segments.len() && segments[line_start].kind == SegmentKind::Space
                {
                    line_start += 1;
                }
                // Recalculate width from new start to current position (inclusive)
                line_width = widths[line_start..=i].iter().sum();
                last_break = None;
                // Don't increment i -- we need to re-check overflow for current segment
                // with the new line_width. But we already computed it, so check inline.
                if line_width > max_width && i > line_start {
                    // Still overflows -- continue loop to handle it
                    continue;
                }
                // Update break opportunity if current segment allows it
                if seg.break_after {
                    let bw = if seg.kind == SegmentKind::Space {
                        line_width - seg_width
                    } else {
                        line_width
                    };
                    last_break = Some((i, bw));
                }
                i += 1;
                continue;
            } else {
                // No break opportunity: emergency break before current segment
                lines.push(LineRange {
                    start: line_start,
                    end: i,
                    width: line_width,
                });
                line_start = i;
                line_width = seg_width;
                last_break = None;
                if seg.break_after {
                    let bw = if seg.kind == SegmentKind::Space {
                        0.0
                    } else {
                        seg_width
                    };
                    last_break = Some((i, bw));
                }
                i += 1;
                continue;
            }
        }

        line_width = new_width;

        // Track break opportunities
        if seg.break_after {
            let content_width = if seg.kind == SegmentKind::Space {
                line_width - seg_width
            } else {
                line_width
            };
            last_break = Some((i, content_width));
        }

        i += 1;
    }

    // Emit final line if there's remaining content
    if line_start < segments.len() {
        lines.push(LineRange {
            start: line_start,
            end: segments.len(),
            width: line_width,
        });
    }

    lines
}

/// Extract the text content for a line range from the analysis.
pub fn line_text(analysis: &TextAnalysis, range: &LineRange) -> String {
    let mut text = String::new();
    for i in range.start..range.end {
        let seg = &analysis.segments[i];
        match seg.kind {
            SegmentKind::SoftHyphen => {
                // Soft hyphens are invisible unless at line end
                if i == range.end - 1 {
                    text.push('-');
                }
            }
            SegmentKind::HardBreak | SegmentKind::ZeroWidthBreak => {
                // Don't include in output
            }
            _ => {
                text.push_str(&seg.text);
            }
        }
    }
    // Trim trailing spaces
    text.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_analysis::{analyze, SegmentKind};

    // Helper: create a PreparedLineBreak with uniform character width
    fn prepare_uniform(text: &str, char_width: f32) -> PreparedLineBreak {
        let analysis = analyze(text);
        let widths: Vec<f32> = analysis
            .segments
            .iter()
            .map(|seg| {
                match seg.kind {
                    SegmentKind::Space => char_width, // space has width
                    SegmentKind::HardBreak => 0.0,
                    SegmentKind::ZeroWidthBreak => 0.0,
                    SegmentKind::SoftHyphen => 0.0, // invisible unless broken
                    SegmentKind::Text => seg.text.chars().count() as f32 * char_width,
                }
            })
            .collect();
        PreparedLineBreak::new(analysis, widths)
    }

    // --- Basic line breaking ---

    #[test]
    fn empty_text_produces_no_lines() {
        let prepared = prepare_uniform("", 10.0);
        let lines = layout(&prepared, 100.0);
        assert!(lines.is_empty());
    }

    #[test]
    fn short_text_fits_in_one_line() {
        let prepared = prepare_uniform("Hello world", 10.0);
        // "Hello" = 50, " " = 10, "world" = 50 => total 110
        let lines = layout(&prepared, 120.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&prepared.analysis, &lines[0]), "Hello world");
    }

    #[test]
    fn text_wraps_at_space_when_exceeding_width() {
        let prepared = prepare_uniform("Hello world", 10.0);
        // "Hello" = 50, " " = 10, "world" = 50 => total 110
        let lines = layout(&prepared, 60.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&prepared.analysis, &lines[0]), "Hello");
        assert_eq!(line_text(&prepared.analysis, &lines[1]), "world");
    }

    #[test]
    fn multiple_words_wrap_correctly() {
        let prepared = prepare_uniform("aa bb cc dd", 10.0);
        // "aa"=20, " "=10, "bb"=20, " "=10, "cc"=20, " "=10, "dd"=20
        // Line 1: "aa bb" = 50 (fits in 55)
        // Line 2: "cc dd" = 50 (fits in 55)
        let lines = layout(&prepared, 55.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&prepared.analysis, &lines[0]), "aa bb");
        assert_eq!(line_text(&prepared.analysis, &lines[1]), "cc dd");
    }

    // --- Hard break ---

    #[test]
    fn hard_break_forces_new_line() {
        let prepared = prepare_uniform("line1\nline2", 10.0);
        let lines = layout(&prepared, 1000.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&prepared.analysis, &lines[0]), "line1");
        assert_eq!(line_text(&prepared.analysis, &lines[1]), "line2");
    }

    #[test]
    fn hard_break_at_end_produces_empty_trailing_line() {
        let prepared = prepare_uniform("hello\n", 10.0);
        let lines = layout(&prepared, 1000.0);
        // "hello" then hard break, nothing after
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&prepared.analysis, &lines[0]), "hello");
    }

    // --- CJK line breaking ---

    #[test]
    fn cjk_text_breaks_between_characters() {
        // Each CJK char is ~10px wide, max_width allows 3 chars per line
        let prepared = prepare_uniform("你好世界", 10.0);
        let lines = layout(&prepared, 30.0);
        // Should break into groups of ~3 chars
        assert!(
            lines.len() >= 2,
            "Expected multiple lines, got {}",
            lines.len()
        );
        // Each line should have content
        for line in &lines {
            let text = line_text(&prepared.analysis, line);
            assert!(!text.is_empty());
        }
    }

    #[test]
    fn kinsoku_start_char_does_not_start_a_line() {
        // In "好。世", the 。 merges with 好 in analysis, so "好。" is one segment
        let prepared = prepare_uniform("好。世", 10.0);
        let lines = layout(&prepared, 15.0);
        // Check that no line starts with 。
        for line in &lines {
            let text = line_text(&prepared.analysis, line);
            assert!(
                !text.starts_with('。'),
                "Line should not start with kinsoku-start char: {:?}",
                text
            );
        }
    }

    #[test]
    fn mixed_cjk_latin_breaks_correctly() {
        let prepared = prepare_uniform("Hello世界test", 10.0);
        // "Hello" = 50, CJK chars = 10 each, "test" = 40
        let lines = layout(&prepared, 60.0);
        assert!(lines.len() >= 2);
        // Verify content is preserved
        let all_text: String = lines
            .iter()
            .map(|l| line_text(&prepared.analysis, l))
            .collect::<Vec<_>>()
            .join("");
        // Remove spaces that might have been trimmed
        let expected = "Hello世界test";
        assert_eq!(
            all_text, expected,
            "Content should be preserved across lines"
        );
    }

    // --- Emergency break (overflow-wrap) ---

    #[test]
    fn very_long_word_gets_emergency_break() {
        let prepared = prepare_uniform("abcdefghij", 10.0);
        // One word, 100px total, max 30px
        let lines = layout(&prepared, 30.0);
        // Should still produce output (not hang or panic)
        assert!(!lines.is_empty());
        // Total content preserved
        let all_text: String = lines
            .iter()
            .map(|l| line_text(&prepared.analysis, l))
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(all_text, "abcdefghij");
    }

    // --- Trailing space handling ---

    #[test]
    fn trailing_spaces_trimmed_from_line_text() {
        let prepared = prepare_uniform("Hello   world", 10.0);
        let lines = layout(&prepared, 60.0);
        for line in &lines {
            let text = line_text(&prepared.analysis, line);
            assert_eq!(
                text,
                text.trim_end(),
                "Line should not have trailing spaces"
            );
        }
    }

    // --- Soft hyphen ---

    #[test]
    fn soft_hyphen_shows_dash_when_line_breaks_there() {
        // "su[SHY]per" -- if broken at soft hyphen, should show "su-"
        let text = "su\u{00AD}per";
        let analysis = analyze(text);
        // Give widths: "su"=20, SHY=0, "per"=30
        let widths: Vec<f32> = analysis
            .segments
            .iter()
            .map(|seg| match seg.kind {
                SegmentKind::SoftHyphen => 0.0,
                SegmentKind::Text => seg.text.chars().count() as f32 * 10.0,
                SegmentKind::Space => 10.0,
                _ => 0.0,
            })
            .collect();
        let prepared = PreparedLineBreak::new(analysis, widths);
        // Force break: max_width = 25 (fits "su" + SHY but not "per")
        let lines = layout(&prepared, 25.0);
        assert!(lines.len() >= 2, "Should break at soft hyphen");
        let first_line = line_text(&prepared.analysis, &lines[0]);
        assert!(
            first_line.ends_with('-'),
            "First line should end with hyphen: {:?}",
            first_line
        );
    }

    #[test]
    fn soft_hyphen_invisible_when_not_broken() {
        let text = "su\u{00AD}per";
        let analysis = analyze(text);
        let widths: Vec<f32> = analysis
            .segments
            .iter()
            .map(|seg| match seg.kind {
                SegmentKind::SoftHyphen => 0.0,
                SegmentKind::Text => seg.text.chars().count() as f32 * 10.0,
                _ => 0.0,
            })
            .collect();
        let prepared = PreparedLineBreak::new(analysis, widths);
        // max_width = 100, everything fits on one line
        let lines = layout(&prepared, 100.0);
        assert_eq!(lines.len(), 1);
        let text = line_text(&prepared.analysis, &lines[0]);
        assert!(
            !text.contains('-'),
            "Soft hyphen should be invisible: {:?}",
            text
        );
        assert_eq!(text, "super");
    }

    // --- Content preservation ---

    #[test]
    fn all_content_preserved_across_line_breaks() {
        let original = "The quick brown fox jumps over the lazy dog";
        let prepared = prepare_uniform(original, 10.0);
        let lines = layout(&prepared, 100.0);

        let reconstructed: String = lines
            .iter()
            .map(|l| line_text(&prepared.analysis, l))
            .collect::<Vec<_>>()
            .join(" ");

        assert_eq!(reconstructed, original);
    }

    // --- Regression: Chinese text with punctuation ---

    #[test]
    fn chinese_text_with_punctuation_breaks_correctly() {
        let text = "当我发现我童年和少年时期的旧日记时，它们已经被尘埃所覆盖。";
        let prepared = prepare_uniform(text, 10.0);
        // Allow ~10 chars per line
        let lines = layout(&prepared, 100.0);
        assert!(!lines.is_empty());

        // Verify no line starts with a punctuation mark
        for line in &lines {
            let text = line_text(&prepared.analysis, line);
            if let Some(first) = text.chars().next() {
                assert!(
                    first != '，' && first != '。',
                    "Line should not start with CJK punctuation: {:?}",
                    text
                );
            }
        }
    }
}
