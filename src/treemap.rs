//! Squarified treemap layout (Bruls, Huizing & van Wijk, 1999).
//!
//! Given a list of item sizes and a rectangle, this produces one output
//! rectangle per item, with area proportional to that item's size, while
//! trying to keep rectangles close to square (much easier to click and
//! label than the long slivers a naive slice-and-dice layout produces).
//!
//! This module knows nothing about egui, files, or pixels vs. bytes — it
//! just does the geometry, which makes it easy to unit test in isolation.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn area(&self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }
}

/// Lays out `sizes` (must already be sorted largest-first by the caller for
/// good results, though this function does not require it) into `rect`,
/// returning one [`Rect`] per input size **in the same order as `sizes`**,
/// so callers can zip the result back up with their original items by
/// index.
///
/// Zero or negative-area rectangles are handled gracefully rather than
/// panicking: an empty `sizes` returns an empty vec, and a degenerate
/// `rect` (zero width or height) returns zero-sized rects for every item.
pub fn squarify(sizes: &[u64], rect: Rect) -> Vec<Rect> {
    let n = sizes.len();
    if n == 0 {
        return Vec::new();
    }
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return vec![Rect::new(rect.x, rect.y, 0.0, 0.0); n];
    }

    let total: f64 = sizes.iter().map(|&s| s as f64).sum();
    let mut out = vec![Rect::new(rect.x, rect.y, 0.0, 0.0); n];

    if total <= 0.0 {
        // Every item is zero-sized (e.g. a folder of empty files). Split
        // the space evenly so items are still visible and clickable
        // instead of collapsing to nothing.
        let w = rect.w / n as f32;
        for (i, r) in out.iter_mut().enumerate() {
            *r = Rect::new(rect.x + w * i as f32, rect.y, w, rect.h);
        }
        return out;
    }

    // Scale raw sizes into area units so they sum to exactly rect's area.
    let scale = (rect.area() as f64) / total;
    let areas: Vec<f64> = sizes.iter().map(|&s| s as f64 * scale).collect();

    layout_rows(&areas, rect, &mut out);
    out
}

/// Worst aspect ratio among a row of items with given min/max/sum area,
/// laid out along a strip whose fixed dimension is `side`. Lower is
/// squarer/better. Formula from the original squarified-treemap paper.
fn worst_ratio(min_area: f64, max_area: f64, sum: f64, side: f64) -> f64 {
    if sum <= 0.0 || side <= 0.0 {
        return f64::INFINITY;
    }
    let side2 = side * side;
    let sum2 = sum * sum;
    ((side2 * max_area) / sum2).max(sum2 / (side2 * min_area))
}

fn layout_rows(areas: &[f64], mut rect: Rect, out: &mut [Rect]) {
    let n = areas.len();
    let mut i = 0;

    while i < n {
        if rect.w <= 0.0 || rect.h <= 0.0 {
            // No room left; park any leftover items at zero size rather
            // than producing negative dimensions.
            for r in out.iter_mut().skip(i) {
                *r = Rect::new(rect.x, rect.y, 0.0, 0.0);
            }
            return;
        }

        let side = rect.w.min(rect.h) as f64;

        // Greedily grow the current row while doing so doesn't worsen the
        // best achievable aspect ratio.
        let mut row_end = i;
        let mut row_sum = 0.0f64;
        let mut row_min = f64::INFINITY;
        let mut row_max = 0.0f64;
        let mut best_ratio = f64::INFINITY;

        loop {
            let val = areas[row_end];
            let candidate_sum = row_sum + val;
            let candidate_min = row_min.min(val);
            let candidate_max = row_max.max(val);
            let candidate_ratio = worst_ratio(candidate_min, candidate_max, candidate_sum, side);

            if row_end == i || candidate_ratio <= best_ratio {
                row_sum = candidate_sum;
                row_min = candidate_min;
                row_max = candidate_max;
                best_ratio = candidate_ratio;
                row_end += 1;
                if row_end >= n {
                    break;
                }
            } else {
                break;
            }
        }

        rect = place_row(areas, i, row_end, row_sum, rect, out);
        i = row_end;
    }
}

/// Places items `areas[start..end]` (whose total area is `row_sum`) as a
/// single strip along the shorter side of `rect`, then returns the
/// remaining rect after removing that strip.
fn place_row(
    areas: &[f64],
    start: usize,
    end: usize,
    row_sum: f64,
    rect: Rect,
    out: &mut [Rect],
) -> Rect {
    if row_sum <= 0.0 {
        return rect;
    }

    if rect.w <= rect.h {
        // Width is the shorter side: lay a horizontal band across the full
        // width at the top, thickness determined by how much area the row
        // needs to occupy.
        let band_h = ((row_sum / rect.w as f64) as f32).min(rect.h);
        let mut x = rect.x;
        for k in start..end {
            let item_w = ((areas[k] / row_sum) as f32) * rect.w;
            out[k] = Rect::new(x, rect.y, item_w, band_h);
            x += item_w;
        }
        Rect::new(rect.x, rect.y + band_h, rect.w, (rect.h - band_h).max(0.0))
    } else {
        // Height is the shorter side: lay a vertical band down the full
        // height at the left.
        let band_w = ((row_sum / rect.h as f64) as f32).min(rect.w);
        let mut y = rect.y;
        for k in start..end {
            let item_h = ((areas[k] / row_sum) as f32) * rect.h;
            out[k] = Rect::new(rect.x, y, band_w, item_h);
            y += item_h;
        }
        Rect::new(rect.x + band_w, rect.y, (rect.w - band_w).max(0.0), rect.h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTAINER: Rect = Rect { x: 0.0, y: 0.0, w: 800.0, h: 600.0 };

    #[test]
    fn empty_input_returns_empty_output() {
        assert!(squarify(&[], CONTAINER).is_empty());
    }

    #[test]
    fn single_item_fills_the_whole_rect() {
        let out = squarify(&[42], CONTAINER);
        assert_eq!(out.len(), 1);
        assert!((out[0].area() - CONTAINER.area()).abs() < 0.01);
        assert_eq!(out[0].x, 0.0);
        assert_eq!(out[0].y, 0.0);
    }

    #[test]
    fn output_order_matches_input_order() {
        // Descending sizes, as scanner.rs would hand us after sorting.
        let sizes = [500u64, 300, 100, 50, 50];
        let out = squarify(&sizes, CONTAINER);
        assert_eq!(out.len(), sizes.len());
        // The biggest item should have gotten the most area, in position 0.
        let areas: Vec<f32> = out.iter().map(Rect::area).collect();
        for w in areas.windows(2) {
            assert!(
                w[0] >= w[1] - 0.5,
                "areas should be non-increasing to match descending input sizes: {areas:?}"
            );
        }
    }

    #[test]
    fn total_area_is_conserved() {
        let sizes = [1_000_000u64, 500_000, 250_000, 125_000, 1, 2, 3];
        let out = squarify(&sizes, CONTAINER);
        let total_out_area: f32 = out.iter().map(Rect::area).sum();
        let expected = CONTAINER.area();
        // Allow a small tolerance for floating point accumulation.
        assert!(
            (total_out_area - expected).abs() < expected * 0.001,
            "total_out_area={total_out_area}, expected={expected}"
        );
    }

    #[test]
    fn no_rect_escapes_the_container_bounds() {
        let sizes = [37u64, 91, 12, 500, 8, 250, 64, 3, 3, 3];
        let out = squarify(&sizes, CONTAINER);
        for r in &out {
            assert!(r.x >= CONTAINER.x - 0.01);
            assert!(r.y >= CONTAINER.y - 0.01);
            assert!(r.x + r.w <= CONTAINER.x + CONTAINER.w + 0.01);
            assert!(r.y + r.h <= CONTAINER.y + CONTAINER.h + 0.01);
        }
    }

    #[test]
    fn all_zero_sizes_still_produce_visible_slots() {
        let sizes = [0u64, 0, 0, 0];
        let out = squarify(&sizes, CONTAINER);
        assert_eq!(out.len(), 4);
        for r in &out {
            assert!(r.w > 0.0);
            assert!(r.h > 0.0);
        }
    }

    #[test]
    fn degenerate_rect_does_not_panic() {
        let sizes = [10u64, 20, 30];
        let out = squarify(&sizes, Rect::new(0.0, 0.0, 0.0, 100.0));
        assert_eq!(out.len(), 3);
        for r in &out {
            assert_eq!(r.w, 0.0);
        }
    }

    #[test]
    fn favors_squarer_rectangles_than_naive_slicing() {
        // A classic example from treemap literature: with a naive
        // slice-and-dice layout on these sizes you get very thin slivers;
        // squarify should keep the worst aspect ratio much more reasonable.
        let sizes = [6u64, 6, 4, 3, 2, 2, 1];
        let out = squarify(&sizes, CONTAINER);
        for r in &out {
            let ratio = (r.w / r.h).max(r.h / r.w);
            assert!(ratio < 6.0, "rect {r:?} has aspect ratio {ratio}, too sliver-y");
        }
    }
}