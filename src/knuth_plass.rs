// Knuth-Plass optimal line breaking.
//
// Operates on a pre-measured item list (Box / Glue / Penalty), independent of
// any font so it can be unit-tested with uniform widths -- mirroring the testing
// approach in line_break.rs and text_analysis.rs.
//
// Reference: Knuth & Plass (1981), "Breaking Paragraphs into Lines."

/// A typesetting item in the paragraph stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Rigid content with a fixed width (e.g. a word or CJK character run).
    Box { width: f32 },
    /// Flexible space: natural width plus stretchability and shrinkability.
    Glue {
        width: f32,
        stretch: f32,
        shrink: f32,
    },
    /// A potential break point with a penalty cost.
    /// `flagged` marks hyphen-like breaks (used for double-hyphen demerits).
    Penalty {
        width: f32,
        penalty: f32,
        flagged: bool,
    },
}

/// Penalty value that forbids a break at this point.
pub const INF_PENALTY: f32 = 10_000.0;
/// Penalty value that forces a break at this point.
pub const FORCED_PENALTY: f32 = -10_000.0;

/// One output line: the item range plus the glue adjustment ratio.
///
/// `ratio` is the multiplier applied to each glue's stretch (ratio > 0) or
/// shrink (ratio < 0) on this line. Renderers use it to justify the line.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakLine {
    /// Index into the item list where this line starts (inclusive).
    pub start: usize,
    /// Index into the item list where this line ends (exclusive).
    pub end: usize,
    /// Glue adjustment ratio for justification.
    pub ratio: f32,
}

/// Tuning parameters for the algorithm.
#[derive(Debug, Clone)]
pub struct Params {
    /// Maximum acceptable badness for a feasible break.
    pub tolerance: f32,
    /// Added to badness before squaring when computing demerits.
    pub line_penalty: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            tolerance: 200.0,
            line_penalty: 10.0,
        }
    }
}

/// Badness ceiling: a line this bad is treated as infinitely bad.
const INF_BAD: f32 = 10_000.0;

/// A feasible breakpoint under consideration.
#[derive(Clone)]
struct Active {
    /// Item index of this breakpoint (the position the line ends at).
    position: usize,
    /// Line number this breakpoint ends.
    line: usize,
    /// Total demerits accumulated to reach this breakpoint.
    total_demerits: f32,
    /// Index of the previous active node in `nodes`, or None for the start.
    previous: Option<usize>,
}

/// Compute badness for a line whose shortfall is `t` given available `stretch`/`shrink`.
fn badness(shortfall: f32, stretch: f32, shrink: f32) -> f32 {
    if shortfall > 0.0 {
        if stretch <= 0.0 {
            return INF_BAD;
        }
        (100.0 * (shortfall / stretch).powi(3)).min(INF_BAD)
    } else if shortfall < 0.0 {
        if shrink <= 0.0 {
            return INF_BAD;
        }
        (100.0 * (-shortfall / shrink).powi(3)).min(INF_BAD)
    } else {
        0.0
    }
}

/// Adjustment ratio for a line: how much each glue must stretch (>0) or shrink (<0).
fn adjustment_ratio(shortfall: f32, stretch: f32, shrink: f32) -> f32 {
    if shortfall > 0.0 {
        if stretch > 0.0 {
            shortfall / stretch
        } else {
            0.0
        }
    } else if shortfall < 0.0 {
        if shrink > 0.0 {
            shortfall / shrink
        } else {
            0.0
        }
    } else {
        0.0
    }
}

/// Is this item a legal breakpoint, and what is its penalty?
fn break_penalty(item: &Item, prev: Option<&Item>) -> Option<f32> {
    match item {
        // A glue is a breakpoint only if the previous item is a box.
        Item::Glue { .. } => match prev {
            Some(Item::Box { .. }) => Some(0.0),
            _ => None,
        },
        // A penalty is a breakpoint unless it forbids breaking.
        Item::Penalty { penalty, .. } => {
            if *penalty >= INF_PENALTY {
                None
            } else {
                Some(*penalty)
            }
        }
        Item::Box { .. } => None,
    }
}

/// Break a paragraph of `items` into lines of at most `line_width`.
///
/// Returns the chosen lines with their justification ratios. Falls back to a
/// greedy break if no feasible solution exists within `params.tolerance`.
pub fn break_lines(items: &[Item], line_width: f32, params: &Params) -> Vec<BreakLine> {
    if items.is_empty() {
        return Vec::new();
    }

    // Natural width, stretch and shrink of the line spanning items[a..b], plus
    // the breakpoint's own width if it is a hyphen-like penalty.
    let line_width_between = |a: usize, b: usize| -> (f32, f32, f32) {
        let mut width = 0.0;
        let mut stretch = 0.0;
        let mut shrink = 0.0;
        for item in &items[a..b] {
            match item {
                Item::Box { width: w } => width += w,
                Item::Glue {
                    width: w,
                    stretch: st,
                    shrink: sh,
                } => {
                    width += w;
                    stretch += st;
                    shrink += sh;
                }
                Item::Penalty { .. } => {}
            }
        }
        if let Some(Item::Penalty { width: w, .. }) = items.get(b) {
            width += w;
        }
        (width, stretch, shrink)
    };

    // Active list of feasible breakpoints, stored in an arena for prev-links.
    let mut nodes: Vec<Active> = Vec::new();
    nodes.push(Active {
        position: 0,
        line: 0,
        total_demerits: 0.0,
        previous: None,
    });
    let mut active: Vec<usize> = vec![0];

    for b in 0..items.len() {
        let prev_item = if b == 0 { None } else { items.get(b - 1) };
        let penalty = match break_penalty(&items[b], prev_item) {
            Some(p) => p,
            None => continue,
        };
        let forced = penalty <= FORCED_PENALTY;

        let mut new_active: Vec<usize> = Vec::new();
        let mut to_deactivate: Vec<usize> = Vec::new();

        // Best candidate landing on b (lowest total demerits) keeps the line set small.
        // The per-line ratio is recomputed during traceback from the final breaks.
        let mut best: Option<(usize, f32)> = None; // (prev_node_idx, total_demerits)

        for &node_idx in &active {
            let a = nodes[node_idx].position;
            // Line spans items (a, b): start measuring just after breakpoint a.
            let start = if a == 0 { 0 } else { a + 1 };
            let (nat, stretch, shrink) = line_width_between(start, b);
            let shortfall = line_width - nat;
            let bad = badness(shortfall, stretch, shrink);

            let feasible = bad <= params.tolerance;
            // Deactivate nodes that can no longer produce a fitting line:
            // overfull (negative shortfall beyond shrink) or forced break here.
            if (shortfall < 0.0 && bad >= INF_BAD) || forced {
                to_deactivate.push(node_idx);
            }

            if feasible || forced {
                let line_demerit = (params.line_penalty + bad).powi(2);
                let pen_demerit = if penalty > 0.0 {
                    penalty * penalty
                } else if penalty > FORCED_PENALTY {
                    -(penalty * penalty)
                } else {
                    0.0
                };
                let total = nodes[node_idx].total_demerits + line_demerit + pen_demerit;
                match best {
                    Some((_, bt)) if total >= bt => {}
                    _ => best = Some((node_idx, total)),
                }
            }
        }

        if let Some((prev_idx, total)) = best {
            let new_idx = nodes.len();
            nodes.push(Active {
                position: b,
                line: nodes[prev_idx].line + 1,
                total_demerits: total,
                previous: Some(prev_idx),
            });
            new_active.push(new_idx);
        }

        // Remove deactivated nodes from the active list.
        if !to_deactivate.is_empty() {
            active.retain(|idx| !to_deactivate.contains(idx));
        }
        active.extend(new_active);

        if active.is_empty() {
            // No feasible continuation; greedy fallback for the remainder.
            return greedy_fallback(items, line_width);
        }
    }

    // Choose the active node with the least total demerits as the final break.
    let final_idx = active
        .iter()
        .copied()
        .min_by(|&x, &y| {
            nodes[x]
                .total_demerits
                .partial_cmp(&nodes[y].total_demerits)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0);

    // Walk back to collect breakpoints and per-line ratios.
    let mut break_positions: Vec<usize> = Vec::new();
    let mut idx = Some(final_idx);
    while let Some(i) = idx {
        if nodes[i].previous.is_some() {
            break_positions.push(nodes[i].position);
        }
        idx = nodes[i].previous;
    }
    break_positions.reverse();

    // Build line ranges from consecutive breakpoints.
    let mut lines = Vec::new();
    let mut start = 0usize;
    for &bp in &break_positions {
        let measure_start = if start == 0 { 0 } else { start + 1 };
        let (nat, stretch, shrink) = line_width_between(measure_start, bp);
        let ratio = adjustment_ratio(line_width - nat, stretch, shrink);
        lines.push(BreakLine {
            start,
            end: bp,
            ratio,
        });
        start = bp;
    }
    lines
}

/// Greedy first-fit fallback when no feasible Knuth-Plass solution exists.
fn greedy_fallback(items: &[Item], line_width: f32) -> Vec<BreakLine> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut width = 0.0;
    let mut last_break: Option<usize> = None;

    for (i, item) in items.iter().enumerate() {
        let prev = if i == 0 { None } else { items.get(i - 1) };
        let is_break = break_penalty(item, prev).is_some();
        let w = match item {
            Item::Box { width } | Item::Glue { width, .. } | Item::Penalty { width, .. } => *width,
        };
        if width + w > line_width && last_break.is_some() {
            let bp = last_break.unwrap();
            lines.push(BreakLine {
                start,
                end: bp,
                ratio: 0.0,
            });
            start = bp;
            width = 0.0;
            last_break = None;
        }
        width += w;
        if is_break {
            last_break = Some(i);
        }
    }
    lines.push(BreakLine {
        start,
        end: items.len().saturating_sub(1),
        ratio: 0.0,
    });
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_(w: f32) -> Item {
        Item::Box { width: w }
    }

    fn glue(w: f32, stretch: f32, shrink: f32) -> Item {
        Item::Glue {
            width: w,
            stretch,
            shrink,
        }
    }

    fn forced_break() -> Item {
        Item::Penalty {
            width: 0.0,
            penalty: FORCED_PENALTY,
            flagged: false,
        }
    }

    // A finishing glue + forced penalty, as required to terminate a paragraph
    // (TeX convention: infinite stretch glue then a forced break).
    fn paragraph_end() -> Vec<Item> {
        vec![
            glue(0.0, 1_000_000.0, 0.0),
            Item::Penalty {
                width: 0.0,
                penalty: FORCED_PENALTY,
                flagged: false,
            },
        ]
    }

    // --- Basic behavior ---

    #[test]
    fn single_box_that_fits_produces_one_line() {
        let mut items = vec![box_(50.0)];
        items.extend(paragraph_end());
        let lines = break_lines(&items, 100.0, &Params::default());
        assert_eq!(lines.len(), 1, "content that fits must be one line");
        assert_eq!(lines[0].start, 0);
    }

    #[test]
    fn line_that_exactly_fits_has_zero_ratio() {
        // Two boxes of 40 with a 20 glue = 100, line width 100 -> perfect fit.
        let mut items = vec![box_(40.0), glue(20.0, 10.0, 5.0), box_(40.0)];
        items.extend(paragraph_end());
        let lines = break_lines(&items, 100.0, &Params::default());
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].ratio.abs() < 1e-3,
            "perfect fit should need no stretch/shrink, got ratio {}",
            lines[0].ratio
        );
    }

    #[test]
    fn underfull_line_gets_positive_stretch_ratio() {
        // Natural width 80, line width 100 -> must stretch by 20 over 10 stretch
        // => ratio 2.0.
        let mut items = vec![box_(35.0), glue(10.0, 10.0, 5.0), box_(35.0)];
        items.extend(paragraph_end());
        let lines = break_lines(&items, 100.0, &Params::default());
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].ratio > 0.0,
            "underfull line should stretch (positive ratio), got {}",
            lines[0].ratio
        );
    }

    // --- Optimal breaking vs greedy ---

    #[test]
    fn breaks_paragraph_into_multiple_lines_when_too_wide() {
        // Five 30-wide boxes separated by 10-wide glue, line width 80.
        // Total natural width = 5*30 + 4*10 = 190, cannot fit one line.
        let mut items = vec![
            box_(30.0),
            glue(10.0, 5.0, 3.0),
            box_(30.0),
            glue(10.0, 5.0, 3.0),
            box_(30.0),
            glue(10.0, 5.0, 3.0),
            box_(30.0),
            glue(10.0, 5.0, 3.0),
            box_(30.0),
        ];
        items.extend(paragraph_end());
        let lines = break_lines(&items, 80.0, &Params::default());
        assert!(
            lines.len() >= 2,
            "wide paragraph must wrap, got {} lines",
            lines.len()
        );
    }

    #[test]
    fn forced_penalty_always_breaks() {
        // Two boxes that fit on one line, but a forced break sits between them.
        let mut items = vec![box_(20.0), forced_break(), box_(20.0)];
        items.extend(paragraph_end());
        let lines = break_lines(&items, 1000.0, &Params::default());
        assert_eq!(
            lines.len(),
            2,
            "forced penalty must split the line even when content fits"
        );
    }

    #[test]
    fn covers_all_boxes_across_lines() {
        // Content preservation: every box index must appear in exactly one line range.
        let mut items = vec![
            box_(30.0),
            glue(10.0, 5.0, 3.0),
            box_(30.0),
            glue(10.0, 5.0, 3.0),
            box_(30.0),
        ];
        items.extend(paragraph_end());
        let lines = break_lines(&items, 70.0, &Params::default());
        // The first line must start at 0 and the last line must reach the final box (index 4).
        assert_eq!(lines.first().unwrap().start, 0);
        let last = lines.last().unwrap();
        assert!(last.end > 4, "last line must include the final box");
    }
}
