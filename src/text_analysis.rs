// Text analysis module: Unicode-aware text segmentation for line breaking.
//
// Inspired by pretext.chenglou's analysis.ts. Segments text into typed pieces
// with CJK detection, kinsoku rules, and punctuation merging.

use unicode_segmentation::UnicodeSegmentation;

// --- Segment types ---

/// Classification of a text segment for line-breaking purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    /// Normal text content (words, CJK characters)
    Text,
    /// Collapsible whitespace (break opportunity)
    Space,
    /// Forced line break (\n)
    HardBreak,
    /// Zero-width space U+200B (break opportunity, no width)
    ZeroWidthBreak,
    /// Soft hyphen U+00AD (break opportunity, shows '-' when broken)
    SoftHyphen,
}

/// A segment of text with its classification.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub text: String,
    pub kind: SegmentKind,
    /// Whether this segment can have a line break placed after it
    pub break_after: bool,
}

/// Result of text analysis: segments ready for line breaking.
#[derive(Debug, Clone)]
pub struct TextAnalysis {
    pub segments: Vec<Segment>,
}

// --- CJK detection ---

/// Returns true if the character is a CJK ideograph, kana, hangul, or fullwidth form.
pub fn is_cjk(ch: char) -> bool {
    let c = ch as u32;
    // CJK Unified Ideographs
    (0x4E00..=0x9FFF).contains(&c) ||
    // CJK Extension A
    (0x3400..=0x4DBF).contains(&c) ||
    // CJK Extension B
    (0x20000..=0x2A6DF).contains(&c) ||
    // CJK Extension C
    (0x2A700..=0x2B73F).contains(&c) ||
    // CJK Extension D
    (0x2B740..=0x2B81F).contains(&c) ||
    // CJK Extension E
    (0x2B820..=0x2CEAF).contains(&c) ||
    // CJK Extension F
    (0x2CEB0..=0x2EBEF).contains(&c) ||
    // CJK Extension G
    (0x30000..=0x3134F).contains(&c) ||
    // CJK Compatibility Ideographs
    (0xF900..=0xFAFF).contains(&c) ||
    // CJK Compatibility Ideographs Supplement
    (0x2F800..=0x2FA1F).contains(&c) ||
    // CJK Symbols and Punctuation
    (0x3000..=0x303F).contains(&c) ||
    // Hiragana
    (0x3040..=0x309F).contains(&c) ||
    // Katakana
    (0x30A0..=0x30FF).contains(&c) ||
    // Hangul Syllables
    (0xAC00..=0xD7AF).contains(&c) ||
    // Halfwidth and Fullwidth Forms
    (0xFF00..=0xFFEF).contains(&c)
}

// --- Kinsoku (禁則処理) rules ---

/// Characters that must not appear at the start of a line (line-start prohibition).
/// Japanese: 行頭禁則文字
fn is_kinsoku_start(ch: char) -> bool {
    matches!(ch,
        // Fullwidth punctuation
        '\u{FF0C}' | // ，
        '\u{FF0E}' | // ．
        '\u{FF01}' | // ！
        '\u{FF1A}' | // ：
        '\u{FF1B}' | // ；
        '\u{FF1F}' | // ？
        // CJK punctuation
        '\u{3001}' | // 、
        '\u{3002}' | // 。
        '\u{30FB}' | // ・
        // Closing brackets/quotes
        '\u{FF09}' | // ）
        '\u{3015}' | // 〕
        '\u{3009}' | // 〉
        '\u{300B}' | // 》
        '\u{300D}' | // 」
        '\u{300F}' | // 』
        '\u{3011}' | // 】
        '\u{3017}' | // 〗
        '\u{3019}' | // 〙
        '\u{301B}' | // 〛
        // Prolonged sound mark, iteration marks
        '\u{30FC}' | // ー
        '\u{3005}' | // 々
        '\u{303B}' | // 〻
        '\u{309D}' | // ゝ
        '\u{309E}' | // ゞ
        '\u{30FD}' | // ヽ
        '\u{30FE}'   // ヾ
    )
}

/// Characters that must not appear at the end of a line (line-end prohibition).
/// Japanese: 行末禁則文字
fn is_kinsoku_end(ch: char) -> bool {
    matches!(ch,
        '"' | '(' | '[' | '{' |
        '\u{201C}' | // "
        '\u{2018}' | // '
        '\u{00AB}' | // «
        '\u{2039}' | // ‹
        // CJK opening brackets
        '\u{FF08}' | // （
        '\u{3014}' | // 〔
        '\u{3008}' | // 〈
        '\u{300A}' | // 《
        '\u{300C}' | // 「
        '\u{300E}' | // 『
        '\u{3010}' | // 【
        '\u{3016}' | // 〖
        '\u{3018}' | // 〘
        '\u{301A}'   // 〚
    )
}

/// Punctuation that sticks to the left (preceding) word -- must not start a line.
fn is_left_sticky_punctuation(ch: char) -> bool {
    matches!(ch,
        '.' | ',' | '!' | '?' | ':' | ';' |
        ')' | ']' | '}' | '%' |
        '\u{2026}' | // …
        '\u{201D}' | // "
        '\u{2019}' | // '
        '\u{00BB}' | // »
        '\u{203A}'   // ›
    )
}

// --- Text analysis ---

/// Classify a single character into a segment kind.
fn classify_char(ch: char) -> SegmentKind {
    match ch {
        '\n' => SegmentKind::HardBreak,
        ' ' | '\t' => SegmentKind::Space,
        '\u{200B}' => SegmentKind::ZeroWidthBreak,
        '\u{00AD}' => SegmentKind::SoftHyphen,
        _ => SegmentKind::Text,
    }
}

/// Analyze text into segments suitable for line breaking.
///
/// The analysis pipeline:
/// 1. Split text by hard breaks (newlines)
/// 2. For each chunk, use Unicode word segmentation
/// 3. Split CJK words into individual characters (each is a break opportunity)
/// 4. Apply kinsoku merging (merge prohibited chars with neighbors)
/// 5. Merge left-sticky punctuation with preceding word
pub fn analyze(text: &str) -> TextAnalysis {
    if text.is_empty() {
        return TextAnalysis { segments: vec![] };
    }

    // Phase 1: Split into raw segments using Unicode word boundaries
    let raw_segments = segment_raw(text);

    // Phase 2: Split CJK text segments into individual characters
    let cjk_split = split_cjk_segments(raw_segments);

    // Phase 3: Apply kinsoku merging
    let kinsoku_merged = merge_kinsoku(cjk_split);

    // Phase 4: Merge left-sticky punctuation with preceding word
    let mut segments = merge_left_sticky(kinsoku_merged);

    // Phase 5: Set break_after flags
    set_break_flags(&mut segments);

    TextAnalysis { segments }
}

/// Phase 1: Split text into raw segments using Unicode word boundaries.
/// Also splits at special characters (soft hyphen, zero-width space) that
/// the Unicode word segmenter treats as part of a word.
fn segment_raw(text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();

    for word in text.split_word_bounds() {
        if word.is_empty() {
            continue;
        }

        // Check first char to determine segment kind
        let first_char = word.chars().next().unwrap();
        let kind = classify_char(first_char);

        if kind != SegmentKind::Text {
            // Split non-text segments by individual special chars
            for ch in word.chars() {
                let char_kind = classify_char(ch);
                segments.push(Segment {
                    text: ch.to_string(),
                    kind: char_kind,
                    break_after: false,
                });
            }
        } else {
            // Text segment: check for embedded special chars (soft hyphen, zero-width space)
            // that unicode-segmentation keeps inside word boundaries
            split_text_at_special_chars(word, &mut segments);
        }
    }

    segments
}

/// Split a text word at embedded special characters (soft hyphen U+00AD,
/// zero-width space U+200B) that the Unicode word segmenter doesn't split on.
fn split_text_at_special_chars(word: &str, segments: &mut Vec<Segment>) {
    let mut current = String::new();

    for ch in word.chars() {
        let kind = classify_char(ch);
        if kind != SegmentKind::Text {
            // Flush accumulated text
            if !current.is_empty() {
                segments.push(Segment {
                    text: std::mem::take(&mut current),
                    kind: SegmentKind::Text,
                    break_after: false,
                });
            }
            // Push the special character as its own segment
            segments.push(Segment {
                text: ch.to_string(),
                kind,
                break_after: false,
            });
        } else {
            current.push(ch);
        }
    }

    // Flush remaining text
    if !current.is_empty() {
        segments.push(Segment {
            text: current,
            kind: SegmentKind::Text,
            break_after: false,
        });
    }
}

/// Phase 2: Split CJK text segments into individual characters.
/// Each CJK character becomes its own segment (since CJK allows breaks between any chars).
fn split_cjk_segments(segments: Vec<Segment>) -> Vec<Segment> {
    let mut result = Vec::new();

    for segment in segments {
        if segment.kind != SegmentKind::Text {
            result.push(segment);
            continue;
        }

        let has_cjk = segment.text.chars().any(is_cjk);
        if !has_cjk {
            result.push(segment);
            continue;
        }

        // Split into runs of CJK vs non-CJK
        let mut current_text = String::new();
        let mut current_is_cjk = false;

        for ch in segment.text.chars() {
            let ch_is_cjk = is_cjk(ch);

            if ch_is_cjk {
                // Flush non-CJK buffer
                if !current_text.is_empty() && !current_is_cjk {
                    result.push(Segment {
                        text: current_text.clone(),
                        kind: SegmentKind::Text,
                        break_after: false,
                    });
                    current_text.clear();
                }
                // Each CJK char is its own segment
                if !current_text.is_empty() && current_is_cjk {
                    // Flush previous CJK char
                    result.push(Segment {
                        text: current_text.clone(),
                        kind: SegmentKind::Text,
                        break_after: false,
                    });
                    current_text.clear();
                }
                current_text.push(ch);
                current_is_cjk = true;
                // Immediately flush each CJK char
                result.push(Segment {
                    text: current_text.clone(),
                    kind: SegmentKind::Text,
                    break_after: false,
                });
                current_text.clear();
            } else {
                if !current_text.is_empty() && current_is_cjk {
                    // CJK chars already flushed individually above
                    current_text.clear();
                }
                current_text.push(ch);
                current_is_cjk = false;
            }
        }

        // Flush remaining
        if !current_text.is_empty() {
            result.push(Segment {
                text: current_text,
                kind: SegmentKind::Text,
                break_after: false,
            });
        }
    }

    result
}

/// Phase 3: Apply kinsoku merging.
/// - Merge kinsoku-start chars with the preceding segment (they can't start a line)
/// - Merge kinsoku-end chars with the following segment (they can't end a line)
fn merge_kinsoku(segments: Vec<Segment>) -> Vec<Segment> {
    if segments.is_empty() {
        return segments;
    }

    let mut result: Vec<Segment> = Vec::new();

    for segment in segments {
        if segment.kind != SegmentKind::Text {
            result.push(segment);
            continue;
        }

        let first_char = segment.text.chars().next().unwrap();

        // Kinsoku-start: merge with preceding text segment
        if is_kinsoku_start(first_char) || is_left_sticky_punctuation(first_char) {
            if let Some(prev) = result.last_mut() {
                if prev.kind == SegmentKind::Text {
                    prev.text.push_str(&segment.text);
                    continue;
                }
            }
        }

        // Kinsoku-end: this char can't end a line, so it should stick to next segment.
        // We handle this by checking if the previous segment ends with a kinsoku-end char
        // and the current segment is text -- if so, merge.
        if !result.is_empty() {
            if let Some(prev) = result.last() {
                if prev.kind == SegmentKind::Text {
                    if let Some(last_char) = prev.text.chars().last() {
                        if is_kinsoku_end(last_char) {
                            let prev = result.last_mut().unwrap();
                            prev.text.push_str(&segment.text);
                            continue;
                        }
                    }
                }
            }
        }

        result.push(segment);
    }

    result
}

/// Phase 4: Merge left-sticky punctuation (., ! ? etc.) with the preceding word.
fn merge_left_sticky(segments: Vec<Segment>) -> Vec<Segment> {
    if segments.is_empty() {
        return segments;
    }

    let mut result: Vec<Segment> = Vec::new();

    for segment in segments {
        if segment.kind != SegmentKind::Text {
            result.push(segment);
            continue;
        }

        // Check if this segment is purely left-sticky punctuation
        let is_pure_sticky = !segment.text.is_empty()
            && segment.text.chars().all(|ch| is_left_sticky_punctuation(ch));

        if is_pure_sticky {
            // Merge with preceding text segment
            if let Some(prev) = result.last_mut() {
                if prev.kind == SegmentKind::Text {
                    prev.text.push_str(&segment.text);
                    continue;
                }
            }
        }

        result.push(segment);
    }

    result
}

/// Phase 5: Set break_after flags on segments.
fn set_break_flags(segments: &mut [Segment]) {
    let len = segments.len();
    for i in 0..len {
        segments[i].break_after = match segments[i].kind {
            SegmentKind::Space | SegmentKind::ZeroWidthBreak | SegmentKind::SoftHyphen => true,
            SegmentKind::HardBreak => true,
            SegmentKind::Text => {
                // CJK characters allow breaks between them (unless kinsoku prevents it)
                if i + 1 < len && segments[i + 1].kind == SegmentKind::Text {
                    let last_char = segments[i].text.chars().last().unwrap_or(' ');
                    let next_first = segments[i + 1].text.chars().next().unwrap_or(' ');
                    // Allow break between CJK characters (but not if next is kinsoku-start
                    // or current ends with kinsoku-end -- those were already merged)
                    is_cjk(last_char) || is_cjk(next_first)
                } else {
                    false
                }
            }
        };
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // --- CJK detection tests ---

    #[test]
    fn cjk_detects_chinese_characters() {
        assert!(is_cjk('中'));
        assert!(is_cjk('国'));
        assert!(is_cjk('人'));
    }

    #[test]
    fn cjk_detects_japanese_kana() {
        assert!(is_cjk('あ')); // Hiragana
        assert!(is_cjk('ア')); // Katakana
    }

    #[test]
    fn cjk_detects_hangul() {
        assert!(is_cjk('한')); // Hangul
    }

    #[test]
    fn cjk_rejects_latin() {
        assert!(!is_cjk('a'));
        assert!(!is_cjk('Z'));
        assert!(!is_cjk('0'));
    }

    #[test]
    fn cjk_detects_fullwidth_forms() {
        assert!(is_cjk('Ａ')); // Fullwidth A
    }

    #[test]
    fn cjk_detects_cjk_punctuation() {
        assert!(is_cjk('。')); // CJK period (U+3002, in CJK Symbols range)
    }

    // --- Kinsoku rule tests ---

    #[test]
    fn kinsoku_start_chars_identified() {
        assert!(is_kinsoku_start('。')); // CJK period
        assert!(is_kinsoku_start('、')); // CJK comma
        assert!(is_kinsoku_start('）')); // Fullwidth closing paren
        assert!(is_kinsoku_start('」')); // CJK closing quote
    }

    #[test]
    fn kinsoku_end_chars_identified() {
        assert!(is_kinsoku_end('（')); // Fullwidth opening paren
        assert!(is_kinsoku_end('「')); // CJK opening quote
        assert!(is_kinsoku_end('【')); // CJK opening bracket
    }

    #[test]
    fn left_sticky_punctuation_identified() {
        assert!(is_left_sticky_punctuation('.'));
        assert!(is_left_sticky_punctuation(','));
        assert!(is_left_sticky_punctuation('!'));
        assert!(is_left_sticky_punctuation(')'));
        assert!(!is_left_sticky_punctuation('a'));
    }

    // --- Segment analysis behavior tests ---

    #[test]
    fn empty_text_produces_no_segments() {
        let analysis = analyze("");
        assert!(analysis.segments.is_empty());
    }

    #[test]
    fn single_word_produces_one_text_segment() {
        let analysis = analyze("Hello");
        assert_eq!(analysis.segments.len(), 1);
        assert_eq!(analysis.segments[0].text, "Hello");
        assert_eq!(analysis.segments[0].kind, SegmentKind::Text);
    }

    #[test]
    fn words_separated_by_space_produce_three_segments() {
        let analysis = analyze("Hello world");
        assert_eq!(analysis.segments.len(), 3);
        assert_eq!(analysis.segments[0].text, "Hello");
        assert_eq!(analysis.segments[0].kind, SegmentKind::Text);
        assert_eq!(analysis.segments[1].text, " ");
        assert_eq!(analysis.segments[1].kind, SegmentKind::Space);
        assert_eq!(analysis.segments[2].text, "world");
        assert_eq!(analysis.segments[2].kind, SegmentKind::Text);
    }

    #[test]
    fn space_segments_allow_break_after() {
        let analysis = analyze("Hello world");
        let space = &analysis.segments[1];
        assert_eq!(space.kind, SegmentKind::Space);
        assert!(space.break_after);
    }

    #[test]
    fn newline_produces_hard_break() {
        let analysis = analyze("line1\nline2");
        let kinds: Vec<_> = analysis.segments.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&SegmentKind::HardBreak));
    }

    #[test]
    fn cjk_text_split_into_individual_characters() {
        let analysis = analyze("你好世界");
        // Each CJK character should be its own segment
        let texts: Vec<_> = analysis.segments.iter().map(|s| s.text.as_str()).collect();
        // After kinsoku merging, each char should still be separate (no kinsoku chars here)
        assert!(texts.contains(&"你"));
        assert!(texts.contains(&"好"));
        assert!(texts.contains(&"世"));
        assert!(texts.contains(&"界"));
    }

    #[test]
    fn cjk_chars_allow_break_between_them() {
        let analysis = analyze("你好世界");
        // All but the last CJK segment should allow break after
        let breakable_count = analysis.segments.iter()
            .filter(|s| s.kind == SegmentKind::Text && s.break_after)
            .count();
        // At least some should be breakable (all except possibly the last)
        assert!(breakable_count > 0);
    }

    #[test]
    fn kinsoku_start_char_merges_with_preceding() {
        // 。should not start a line, so it merges with the preceding character
        let analysis = analyze("好。");
        // The 。should be merged with 好
        let texts: Vec<_> = analysis.segments.iter().map(|s| s.text.as_str()).collect();
        assert!(texts.contains(&"好。"), "Expected '好。' merged, got {:?}", texts);
    }

    #[test]
    fn kinsoku_end_char_merges_with_following() {
        // 「 should not end a line, so it merges with the following character
        let analysis = analyze("「好");
        let texts: Vec<_> = analysis.segments.iter().map(|s| s.text.as_str()).collect();
        assert!(texts.contains(&"「好"), "Expected '「好' merged, got {:?}", texts);
    }

    #[test]
    fn sentence_ending_punctuation_stays_with_word() {
        // "Hello." should keep the period with "Hello"
        let analysis = analyze("Hello. World");
        let texts: Vec<_> = analysis.segments.iter().map(|s| s.text.as_str()).collect();
        // The period should be merged with Hello
        assert!(texts.contains(&"Hello."), "Expected 'Hello.' merged, got {:?}", texts);
    }

    #[test]
    fn mixed_cjk_and_latin_segmented_correctly() {
        let analysis = analyze("Hello世界test");
        let texts: Vec<_> = analysis.segments.iter().map(|s| s.text.as_str()).collect();
        // Should have "Hello", individual CJK chars, and "test"
        assert!(texts.contains(&"Hello"));
        assert!(texts.contains(&"test"));
        // CJK chars should be individual
        assert!(texts.contains(&"世"));
        assert!(texts.contains(&"界"));
    }

    #[test]
    fn soft_hyphen_detected() {
        let analysis = analyze("su\u{00AD}per");
        let kinds: Vec<_> = analysis.segments.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&SegmentKind::SoftHyphen));
    }

    #[test]
    fn zero_width_space_detected() {
        let analysis = analyze("hello\u{200B}world");
        let kinds: Vec<_> = analysis.segments.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&SegmentKind::ZeroWidthBreak));
    }

    #[test]
    fn cjk_comma_does_not_start_line() {
        // In "你好，世界", the ，should merge with 好
        let analysis = analyze("你好，世界");
        let texts: Vec<_> = analysis.segments.iter().map(|s| s.text.as_str()).collect();
        // 好 and ， should be merged since ， is fullwidth comma (kinsoku start)
        let has_good_merge = texts.iter().any(|t| t.contains("好") && t.contains('，'));
        assert!(has_good_merge, "Expected 好 and ， merged, got {:?}", texts);
    }

    #[test]
    fn multiple_kinsoku_chars_chain_merge() {
        // 」。 should all merge with preceding
        let analysis = analyze("好」。");
        let texts: Vec<_> = analysis.segments.iter().map(|s| s.text.as_str()).collect();
        // All should merge into one segment
        assert_eq!(texts.len(), 1, "Expected single merged segment, got {:?}", texts);
        assert_eq!(texts[0], "好」。");
    }

    #[test]
    fn long_chinese_text_breaks_at_character_boundaries() {
        let text = "当我发现我童年和少年时期的旧日记时";
        let analysis = analyze(text);
        // Each character should be an individual segment (no kinsoku chars)
        // Count text segments
        let text_segments: Vec<_> = analysis.segments.iter()
            .filter(|s| s.kind == SegmentKind::Text)
            .collect();
        // Should have many segments (roughly one per character)
        assert!(text_segments.len() > 5, "Expected many segments, got {}", text_segments.len());
    }

    #[test]
    fn parenthesized_expression_stays_together() {
        // Opening paren is kinsoku-end, should merge with next
        let analysis = analyze("（テスト）");
        let texts: Vec<_> = analysis.segments.iter().map(|s| s.text.as_str()).collect();
        // （ should merge with テ, and ） is kinsoku-start so merges with previous
        // Result should be a small number of segments
        assert!(texts.len() <= 3, "Expected few segments for parenthesized text, got {:?}", texts);
    }
}
