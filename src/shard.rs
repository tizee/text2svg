//! Glyph "shard" geometry for the settle/scatter animations.
//!
//! A glyph outline is split into contours; contours are grouped into components
//! (an outer contour plus the counters/holes it directly contains) using an
//! even-odd containment tree. Each component becomes one shard. A shard flies
//! along its principal axis (the largest eigenvector of the contour's area
//! covariance matrix), scaled by how elongated the shape is: round shapes barely
//! move, long strokes fly far.
//!
//! Everything here runs at generation time in pure Rust, so the emitted SVG
//! needs no runtime script to know which way each fragment scatters.
//!
//! Credit: the glyph shard/scatter animation concept and the covariance
//! principal-axis direction model are adapted from yuragi
//! (<https://github.com/lawvs/yuragi>, MIT (c) lawvs and CircuitCoder), whose
//! title-animation study is itself inspired by CircuitCoder/layered.

use std::collections::HashSet;

use rustybuzz::ttf_parser::OutlineBuilder;

/// Below this the eigenvalue/area is treated as zero (font-unit scale).
const DIRECTION_EPSILON: f32 = 1e-1;
/// Curve flattening resolution used only for the direction integral and the
/// point-in-polygon containment test. The emitted `<path>` keeps its original
/// curves, so this never affects rendered sharpness.
const FLATTEN_STEPS: usize = 16;

/// A single outline command in raw font units (Y up, unscaled).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Move(f32, f32),
    Line(f32, f32),
    /// control point, end point
    Quad(f32, f32, f32, f32),
    /// control1, control2, end point
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

impl Cmd {
    /// Destination point of the command, if it has one.
    fn dst(&self) -> Option<(f32, f32)> {
        match *self {
            Cmd::Move(x, y) | Cmd::Line(x, y) => Some((x, y)),
            Cmd::Quad(_, _, x, y) => Some((x, y)),
            Cmd::Cubic(_, _, _, _, x, y) => Some((x, y)),
            Cmd::Close => None,
        }
    }
}

/// One shard: a merged component in raw font units plus its outward direction.
#[derive(Clone, Debug)]
pub struct ShardGeom {
    /// Component commands (outer contour followed by any hole contours).
    pub cmds: Vec<Cmd>,
    /// Unitless outward direction (principal axis * elongation), font space.
    pub direction: (f32, f32),
}

/// Collects a glyph outline into separate contours (one per `Move..Close`),
/// keeping raw font units.
#[derive(Default)]
pub struct ContourPen {
    contours: Vec<Vec<Cmd>>,
    current: Vec<Cmd>,
}

impl ContourPen {
    pub fn new() -> Self {
        Self::default()
    }

    fn flush(&mut self) {
        if !self.current.is_empty() {
            self.contours.push(std::mem::take(&mut self.current));
        }
    }

    /// Consume the pen, returning one command list per contour.
    pub fn finish(mut self) -> Vec<Vec<Cmd>> {
        self.flush();
        self.contours
    }
}

impl OutlineBuilder for ContourPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        self.current.push(Cmd::Move(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push(Cmd::Line(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.current.push(Cmd::Quad(x1, y1, x, y));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.current.push(Cmd::Cubic(x1, y1, x2, y2, x, y));
    }

    fn close(&mut self) {
        self.current.push(Cmd::Close);
    }
}

/// Split a glyph's contours into shards with computed scatter directions.
pub fn compute_shards(contours: Vec<Vec<Cmd>>) -> Vec<ShardGeom> {
    if contours.is_empty() {
        return Vec::new();
    }

    split_components(&contours)
        .into_iter()
        .map(|indices| {
            let mut cmds = Vec::new();
            for i in indices {
                cmds.extend_from_slice(&contours[i]);
            }
            let direction = compute_direction(&cmds);
            ShardGeom { cmds, direction }
        })
        .collect()
}

// --- Component grouping (even-odd containment tree) ---

/// Group contours into components. A root contour and the contours it directly
/// contains (its holes) form one component; grandchildren recurse as their own
/// components, so an "8" splits into two rings while an "o" stays whole.
fn split_components(contours: &[Vec<Cmd>]) -> Vec<Vec<usize>> {
    let n = contours.len();
    let polygons: Vec<Vec<(f32, f32)>> = contours.iter().map(|c| contour_polygon(c)).collect();
    let dst_points: Vec<Vec<(f32, f32)>> = contours
        .iter()
        .map(|c| c.iter().filter_map(Cmd::dst).collect())
        .collect();

    // containers[i] = set of contours that contain contour i.
    let mut containers: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    for i in 0..n {
        for (j, polygon) in polygons.iter().enumerate() {
            if i == j {
                continue;
            }
            let inside = !dst_points[i].is_empty()
                && dst_points[i].iter().all(|p| point_in_polygon(*p, polygon));
            if inside {
                containers[i].insert(j);
            }
        }
    }

    // Peel outermost contours first, wiring each removed contour as a child of
    // the contours it was directly inside.
    let mut processed = vec![false; n];
    let mut is_root = vec![true; n];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];

    loop {
        let selected = (0..n).find(|&i| containers[i].is_empty() && !processed[i]);
        let Some(selected) = selected else {
            break;
        };

        for (i, container) in containers.iter_mut().enumerate() {
            if container.remove(&selected) && container.is_empty() {
                children[selected].push(i);
                is_root[i] = false;
            }
        }
        processed[selected] = true;
    }

    // Each root spawns a component of itself plus its direct holes; an island
    // sitting inside a hole (e.g. the inner box of 回) starts its own shard.
    let mut components = Vec::new();
    let mut roots: Vec<usize> = is_root
        .iter()
        .enumerate()
        .filter_map(|(i, &root)| root.then_some(i))
        .collect();
    while let Some(id) = roots.pop() {
        let mut component = vec![id];
        for &hole in &children[id] {
            component.push(hole);
            roots.extend(children[hole].iter().copied());
        }
        components.push(component);
    }
    components
}

// --- Direction: principal axis of the area covariance matrix ---

/// Compute a shard's outward direction from its area covariance matrix.
/// Returns a unitless vector: the principal-axis unit vector scaled by an
/// elongation factor in `[0, 1)` (round shapes barely move, long shapes fly).
fn compute_direction(cmds: &[Cmd]) -> (f32, f32) {
    // Green's-theorem line integrals over the closed contour(s). Simpson's rule
    // is exact for these cubic integrands, so straight edges need no subdivision
    // and flattened curve segments stay accurate.
    let mut area = 0.0f32;
    let mut a2_ex = 0.0f32;
    let mut na2_ey = 0.0f32;
    let mut a3_ex2 = 0.0f32;
    let mut na3_ey2 = 0.0f32;
    let mut a4_exy = 0.0f32;

    let simpson = |g0: f32, gm: f32, g1: f32| (g0 + 4.0 * gm + g1) / 6.0;

    for polygon in subpath_polygons(cmds) {
        let len = polygon.len();
        for i in 0..len {
            let (x0, y0) = polygon[i];
            let (x1, y1) = polygon[(i + 1) % len];
            let xm = (x0 + x1) / 2.0;
            let ym = (y0 + y1) / 2.0;
            let dx = x1 - x0;
            let dy = y1 - y0;
            area += dy * simpson(x0, xm, x1);
            a2_ex += dy * simpson(x0 * x0, xm * xm, x1 * x1);
            na2_ey += dx * simpson(y0 * y0, ym * ym, y1 * y1);
            a3_ex2 += dy * simpson(x0.powi(3), xm.powi(3), x1.powi(3));
            na3_ey2 += dx * simpson(y0.powi(3), ym.powi(3), y1.powi(3));
            a4_exy += dy * simpson(x0 * x0 * y0, xm * xm * ym, x1 * x1 * y1)
                - dx * simpson(x0 * y0 * y0, xm * ym * ym, x1 * y1 * y1);
        }
    }

    if area.abs() < DIRECTION_EPSILON {
        return (0.0, 0.0);
    }

    let ex = a2_ex / area / 2.0;
    let ey = -na2_ey / area / 2.0;
    let ex2 = a3_ex2 / area / 3.0;
    let ey2 = -na3_ey2 / area / 3.0;
    let exy = a4_exy / area / 4.0;

    let var_x = ex2 - ex * ex;
    let var_y = ey2 - ey * ey;
    let cov_xy = exy - ex * ey;

    let det = var_x * var_y - cov_xy * cov_xy;
    let mean_trace = (var_x + var_y) / 2.0;
    let discriminant = (mean_trace * mean_trace - det).max(0.0);
    let larger_ev = mean_trace + discriminant.sqrt();
    let shorter_ev = mean_trace - discriminant.sqrt();

    if larger_ev < DIRECTION_EPSILON {
        return (0.0, 0.0);
    }

    let ev_x = cov_xy;
    let ev_y = larger_ev - var_x;
    let ev_len = (ev_x * ev_x + ev_y * ev_y).sqrt();
    if ev_len < DIRECTION_EPSILON {
        return (0.0, 0.0);
    }

    let wanted_len = 1.0 - (shorter_ev / larger_ev).abs().sqrt();
    (ev_x / ev_len * wanted_len, ev_y / ev_len * wanted_len)
}

// --- Flattening helpers ---

/// Flatten one contour (a single subpath) into a closed polygon.
fn contour_polygon(cmds: &[Cmd]) -> Vec<(f32, f32)> {
    subpath_polygons(cmds)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Flatten every subpath in `cmds` into closed polygons (curves sampled).
fn subpath_polygons(cmds: &[Cmd]) -> Vec<Vec<(f32, f32)>> {
    let mut polygons = Vec::new();
    let mut current: Vec<(f32, f32)> = Vec::new();
    let mut last = (0.0f32, 0.0f32);

    for cmd in cmds {
        match *cmd {
            Cmd::Move(x, y) => {
                if current.len() >= 2 {
                    polygons.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                last = (x, y);
                current.push(last);
            }
            Cmd::Line(x, y) => {
                last = (x, y);
                current.push(last);
            }
            Cmd::Quad(cx, cy, x, y) => {
                flatten_quad(last, (cx, cy), (x, y), &mut current);
                last = (x, y);
            }
            Cmd::Cubic(c1x, c1y, c2x, c2y, x, y) => {
                flatten_cubic(last, (c1x, c1y), (c2x, c2y), (x, y), &mut current);
                last = (x, y);
            }
            Cmd::Close => {}
        }
    }

    if current.len() >= 2 {
        polygons.push(current);
    }
    polygons
}

fn flatten_quad(p0: (f32, f32), c: (f32, f32), p1: (f32, f32), out: &mut Vec<(f32, f32)>) {
    for step in 1..=FLATTEN_STEPS {
        let t = step as f32 / FLATTEN_STEPS as f32;
        let mt = 1.0 - t;
        let x = mt * mt * p0.0 + 2.0 * mt * t * c.0 + t * t * p1.0;
        let y = mt * mt * p0.1 + 2.0 * mt * t * c.1 + t * t * p1.1;
        out.push((x, y));
    }
}

fn flatten_cubic(
    p0: (f32, f32),
    c1: (f32, f32),
    c2: (f32, f32),
    p1: (f32, f32),
    out: &mut Vec<(f32, f32)>,
) {
    for step in 1..=FLATTEN_STEPS {
        let t = step as f32 / FLATTEN_STEPS as f32;
        let mt = 1.0 - t;
        let x = mt * mt * mt * p0.0
            + 3.0 * mt * mt * t * c1.0
            + 3.0 * mt * t * t * c2.0
            + t * t * t * p1.0;
        let y = mt * mt * mt * p0.1
            + 3.0 * mt * mt * t * c1.1
            + 3.0 * mt * t * t * c2.1
            + t * t * t * p1.1;
        out.push((x, y));
    }
}

/// Even-odd point-in-polygon test (ray casting).
fn point_in_polygon(point: (f32, f32), polygon: &[(f32, f32)]) -> bool {
    let (px, py) = point;
    let mut inside = false;
    let n = polygon.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];
        let crosses = (yi > py) != (yj > py);
        if crosses {
            let x_cross = (xj - xi) * (py - yi) / (yj - yi) + xi;
            if px < x_cross {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    // A thin strip running along the (1, 1) diagonal should fly along that
    // diagonal (both components non-trivial and same sign). Real glyph contours
    // are never perfectly axis-aligned, where the principal-axis eigenvector
    // formula degenerates to zero.
    #[test]
    fn diagonal_strip_flies_along_its_axis() {
        let cmds = vec![
            Cmd::Move(0.0, 0.0),
            Cmd::Line(100.0, 100.0),
            Cmd::Line(103.5, 96.5),
            Cmd::Line(3.5, -3.5),
            Cmd::Close,
        ];
        let (dx, dy) = compute_direction(&cmds);
        assert!(
            dx.abs() > 0.3 && dy.abs() > 0.3,
            "elongated diagonal shape should have real magnitude: ({dx}, {dy})"
        );
        assert!(
            dx.signum() == dy.signum(),
            "direction should follow the (1, 1) diagonal: ({dx}, {dy})"
        );
    }

    // A square is not elongated, so its magnitude should be near zero.
    #[test]
    fn square_direction_is_small() {
        let cmds = vec![
            Cmd::Move(0.0, 0.0),
            Cmd::Line(50.0, 0.0),
            Cmd::Line(50.0, 50.0),
            Cmd::Line(0.0, 50.0),
            Cmd::Close,
        ];
        let (dx, dy) = compute_direction(&cmds);
        let mag = (dx * dx + dy * dy).sqrt();
        assert!(mag < 0.2, "square should barely move: mag={mag}");
    }

    // A ring (outer square with an inner hole) groups into ONE shard.
    #[test]
    fn ring_groups_into_single_shard() {
        let outer = vec![
            Cmd::Move(0.0, 0.0),
            Cmd::Line(100.0, 0.0),
            Cmd::Line(100.0, 100.0),
            Cmd::Line(0.0, 100.0),
            Cmd::Close,
        ];
        let hole = vec![
            Cmd::Move(30.0, 30.0),
            Cmd::Line(70.0, 30.0),
            Cmd::Line(70.0, 70.0),
            Cmd::Line(30.0, 70.0),
            Cmd::Close,
        ];
        let shards = compute_shards(vec![outer, hole]);
        assert_eq!(shards.len(), 1, "outer + hole must merge into one shard");
    }

    // Two separate rectangles are two independent shards.
    #[test]
    fn disjoint_contours_are_separate_shards() {
        let a = vec![
            Cmd::Move(0.0, 0.0),
            Cmd::Line(10.0, 0.0),
            Cmd::Line(10.0, 10.0),
            Cmd::Line(0.0, 10.0),
            Cmd::Close,
        ];
        let b = vec![
            Cmd::Move(50.0, 50.0),
            Cmd::Line(60.0, 50.0),
            Cmd::Line(60.0, 60.0),
            Cmd::Line(50.0, 60.0),
            Cmd::Close,
        ];
        let shards = compute_shards(vec![a, b]);
        assert_eq!(shards.len(), 2, "disjoint contours must stay separate");
    }

    // Concentric "回" nesting: an island inside a counter starts its own shard.
    // outer + hole = one shard, inner + inner-hole = a second shard.
    #[test]
    fn nested_island_starts_new_shard() {
        let square = |min: f32, max: f32| {
            vec![
                Cmd::Move(min, min),
                Cmd::Line(max, min),
                Cmd::Line(max, max),
                Cmd::Line(min, max),
                Cmd::Close,
            ]
        };
        // outer wall, its counter, inner wall, inner counter.
        let contours = vec![
            square(0.0, 100.0),
            square(15.0, 85.0),
            square(30.0, 70.0),
            square(45.0, 55.0),
        ];
        let shards = compute_shards(contours);
        assert_eq!(
            shards.len(),
            2,
            "a nested island must become its own shard, not merge into the outer"
        );
    }
}
