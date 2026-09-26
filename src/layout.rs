//! Spiral word placement with axis-aligned bounding-box collision.

use crate::palette::Rgb;
use crate::text::{measure, Metrics};
use ab_glyph::FontRef;
use rand::Rng;

/// A word to place, with a relative weight in (0, 1].
#[derive(Clone, Debug)]
pub struct Item {
    pub text: String,
    pub weight: f32,
}

/// A placed word.
#[derive(Clone, Debug)]
pub struct Placed {
    pub text: String,
    pub size: f32,
    pub color: Rgb,
    pub vertical: bool,
    /// Top-left corner of the tight glyph box on the canvas.
    pub x: i32,
    pub y: i32,
    /// Size of the tight box on the canvas (already swapped when vertical).
    pub w: i32,
    pub h: i32,
    pub metrics: Metrics,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub width: u32,
    pub height: u32,
    pub min_size: f32,
    /// Nominal size of the heaviest word before the fill search scales it.
    pub max_size: f32,
    /// Hard ceiling on the heaviest word's size after the fill search.
    pub size_cap: f32,
    pub padding: i32,
    pub margin: i32,
    /// Probability that a word is drawn rotated 90 degrees.
    pub vertical_prob: f32,
    pub colors: Vec<Rgb>,
    /// Pick colors randomly rather than in palette order.
    pub shuffle_colors: bool,
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Rect {
    fn overlaps(&self, o: &Rect, pad: i32) -> bool {
        self.x - pad < o.x + o.w
            && self.x + self.w + pad > o.x
            && self.y - pad < o.y + o.h
            && self.y + self.h + pad > o.y
    }
}

/// Lay out `items` on the canvas. Returns placed words plus the words that
/// did not fit even at the minimum size.
///
/// Sizes are searched: every word gets a nominal size from its weight, then
/// one common scale factor is grown (or shrunk) until the cloud fills the
/// canvas as fully as possible with every word placed.
pub fn layout<R: Rng + Clone>(
    font: &FontRef<'_>,
    items: &[Item],
    opts: &Options,
    rng: &mut R,
) -> (Vec<Placed>, Vec<String>) {
    let mut order: Vec<&Item> = items.iter().collect();
    // Largest first so the big words get the center.
    order.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));

    // Decide orientation, color, and nominal size up front so that every
    // attempt in the scale search uses the same choices.
    let plan: Vec<Plan> = order
        .iter()
        .enumerate()
        .map(|(i, item)| Plan {
            item,
            // The heaviest word anchors the cloud; keep it horizontal.
            vertical: i > 0 && rng.gen::<f32>() < opts.vertical_prob,
            color: if opts.shuffle_colors {
                opts.colors[rng.gen_range(0..opts.colors.len())]
            } else {
                opts.colors[i % opts.colors.len()]
            },
            size: opts.min_size + (opts.max_size - opts.min_size) * item.weight.clamp(0.0, 1.0).sqrt(),
        })
        .collect();

    // Starting scale: shrink so the longest phrase fits its axis.
    let (cw, ch) = (opts.width as i32, opts.height as i32);
    let usable_w = (cw - 2 * opts.margin) as f32 * 0.9;
    let usable_h = (ch - 2 * opts.margin) as f32 * 0.8;
    let mut scale = 1.0f32;
    for p in &plan {
        let m = measure(font, &p.item.text, p.size);
        let limit = if p.vertical { usable_h } else { usable_w };
        if m.width > limit {
            scale = scale.min(limit / m.width);
        }
    }
    // Never blow the heaviest word past the caller's max size.
    let cap = (opts.size_cap / plan.first().map(|p| p.size).unwrap_or(opts.max_size)).max(scale);

    let attempt = |scale: f32, rng: &R| -> Option<Vec<Placed>> {
        let mut r = rng.clone();
        let (placed, skipped) = place_all(font, &plan, opts, scale, false, &mut r);
        if skipped.is_empty() { Some(placed) } else { None }
    };

    let mut best: Option<(f32, Vec<Placed>)> = None;
    let mut lo; // largest known-good scale
    let mut hi; // smallest known-bad scale
    if let Some(p) = attempt(scale, rng) {
        best = Some((scale, p));
        lo = scale;
        hi = scale;
        // Grow until it stops fitting or hits the cap.
        for _ in 0..12 {
            let next = (lo * 1.25).min(cap);
            if next <= lo { hi = lo; break; }
            match attempt(next, rng) {
                Some(p) => { best = Some((next, p)); lo = next; if next >= cap { hi = next; break; } }
                None => { hi = next; break; }
            }
        }
    } else {
        hi = scale;
        lo = 0.0;
        for _ in 0..12 {
            let next = hi / 1.25;
            if let Some(p) = attempt(next, rng) {
                best = Some((next, p));
                lo = next;
                break;
            }
            hi = next;
        }
    }
    // Refine between lo and hi.
    if best.is_some() && hi > lo {
        for _ in 0..5 {
            let mid = (lo + hi) / 2.0;
            match attempt(mid, rng) {
                Some(p) => { best = Some((mid, p)); lo = mid; }
                None => hi = mid,
            }
        }
    }

    match best {
        Some((_, placed)) => (placed, Vec::new()),
        // Nothing fit at any uniform scale: fall back to per-word shrinking.
        None => {
            let mut r = rng.clone();
            place_all(font, &plan, opts, scale, true, &mut r)
        }
    }
}

struct Plan<'a> {
    item: &'a Item,
    vertical: bool,
    color: Rgb,
    size: f32,
}

/// One placement pass at a fixed scale. With `shrink` set, a word that does not
/// fit is retried smaller until it does or reaches the minimum size.
fn place_all<R: Rng>(
    font: &FontRef<'_>,
    plan: &[Plan<'_>],
    opts: &Options,
    scale: f32,
    shrink: bool,
    rng: &mut R,
) -> (Vec<Placed>, Vec<String>) {
    let (cw, ch) = (opts.width as i32, opts.height as i32);
    let aspect = cw as f32 / ch as f32;
    let (cx, cy) = (cw as f32 / 2.0, ch as f32 / 2.0);
    let max_radius = (cw.max(ch) as f32) * 0.75;

    let mut placed: Vec<Placed> = Vec::new();
    let mut taken: Vec<Rect> = Vec::new();
    let mut skipped = Vec::new();

    for p in plan {
        let mut size = (p.size * scale).max(opts.min_size);
        // Jitter the starting angle so runs with different seeds differ.
        let theta0 = rng.gen_range(0.0..std::f32::consts::TAU);

        let mut done = false;
        loop {
            let m = measure(font, &p.item.text, size);
            // A vertical phrase that would span most of the canvas height
            // looks wrong and blocks the fill search; draw it horizontally.
            let vertical = p.vertical && m.width <= ch as f32 * 0.6;
            let (w, h) = if vertical {
                (m.height as i32, m.width as i32)
            } else {
                (m.width as i32, m.height as i32)
            };

            if w + 2 * opts.margin <= cw && h + 2 * opts.margin <= ch {
                let mut t = 0.0f32;
                let step = 0.08f32;
                let spacing = 1.2f32;
                while t * spacing <= max_radius {
                    let r = t * spacing;
                    let px = cx + r * (t + theta0).cos() * aspect;
                    let py = cy + r * (t + theta0).sin();
                    let rect = Rect {
                        x: (px - w as f32 / 2.0).round() as i32,
                        y: (py - h as f32 / 2.0).round() as i32,
                        w,
                        h,
                    };
                    let inside = rect.x >= opts.margin
                        && rect.y >= opts.margin
                        && rect.x + rect.w <= cw - opts.margin
                        && rect.y + rect.h <= ch - opts.margin;
                    if inside && !taken.iter().any(|o| rect.overlaps(o, opts.padding)) {
                        taken.push(rect);
                        placed.push(Placed {
                            text: p.item.text.clone(),
                            size,
                            color: p.color,
                            vertical,
                            x: rect.x,
                            y: rect.y,
                            w,
                            h,
                            metrics: m,
                        });
                        done = true;
                        break;
                    }
                    t += step;
                }
            }
            if done || !shrink || size <= opts.min_size {
                break;
            }
            size = (size * 0.9).max(opts.min_size);
        }
        if !done {
            skipped.push(p.item.text.clone());
        }
    }
    (placed, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::parse_color;
    use crate::text::load_font;
    use rand::SeedableRng;

    fn opts() -> Options {
        Options {
            width: 800,
            height: 500,
            min_size: 14.0,
            max_size: 90.0,
            padding: 4,
            margin: 8,
            size_cap: 200.0,
            vertical_prob: 0.3,
            colors: vec![parse_color("#2a78d6").unwrap(), parse_color("#eb6834").unwrap()],
            shuffle_colors: false,
        }
    }

    #[test]
    fn places_all_and_never_overlaps() {
        let font = load_font();
        let items: Vec<Item> = (0..12)
            .map(|i| Item { text: format!("Word{i}"), weight: 1.0 - i as f32 / 12.0 })
            .collect();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
        let (placed, skipped) = layout(&font, &items, &opts(), &mut rng);
        assert!(skipped.is_empty(), "skipped: {skipped:?}");
        assert_eq!(placed.len(), 12);
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                let ra = Rect { x: a.x, y: a.y, w: a.w, h: a.h };
                let rb = Rect { x: b.x, y: b.y, w: b.w, h: b.h };
                assert!(!ra.overlaps(&rb, 0), "{} overlaps {}", a.text, b.text);
            }
            assert!(a.x >= 0 && a.y >= 0 && a.x + a.w <= 800 && a.y + a.h <= 500);
        }
    }

    #[test]
    fn deterministic_for_same_seed() {
        let font = load_font();
        let items = vec![
            Item { text: "Alpha".into(), weight: 1.0 },
            Item { text: "Beta".into(), weight: 0.5 },
        ];
        let run = |seed| {
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
            layout(&font, &items, &opts(), &mut rng).0.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>()
        };
        assert_eq!(run(7), run(7));
    }
}
