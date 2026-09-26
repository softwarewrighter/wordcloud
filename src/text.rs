//! Text measurement and rasterization using the bundled font.

use ab_glyph::{point, Font, FontRef, Glyph, GlyphId, PxScale, ScaleFont};

pub static FONT_BYTES: &[u8] = include_bytes!("../assets/DejaVuSans-Bold.ttf");

pub fn load_font() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_BYTES).expect("bundled font is valid")
}

/// Tight pixel bounds of a laid-out string, relative to the pen origin at
/// (0, ascent). `min_x/min_y` may be non-zero (e.g. glyph side bearings).
#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    pub min_x: f32,
    pub min_y: f32,
    pub width: f32,
    pub height: f32,
    /// Distance from the top of the tight box down to the baseline.
    pub baseline: f32,
}

/// Coverage mask of a rasterized string: `width * height` values in 0..=1.
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Mask {
    /// Rotate 90 degrees counter-clockwise so horizontal text reads bottom-to-top.
    pub fn rotate_ccw(&self) -> Mask {
        let (w, h) = (self.width, self.height);
        let mut data = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                // (x, y) -> (y, w - 1 - x); new width = h, new height = w
                data[(w - 1 - x) * h + y] = self.data[y * w + x];
            }
        }
        Mask { width: h, height: w, data }
    }
}

fn layout_glyphs(font: &FontRef<'_>, text: &str, size: f32) -> Vec<Glyph> {
    let scale = PxScale::from(size);
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();
    let mut caret = 0.0f32;
    let mut last: Option<GlyphId> = None;
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(prev) = last {
            caret += scaled.kern(prev, id);
        }
        out.push(id.with_scale_and_position(scale, point(caret, ascent)));
        caret += scaled.h_advance(id);
        last = Some(id);
    }
    out
}

pub fn measure(font: &FontRef<'_>, text: &str, size: f32) -> Metrics {
    let glyphs = layout_glyphs(font, text, size);
    let ascent = font.as_scaled(PxScale::from(size)).ascent();
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for g in glyphs {
        if let Some(og) = font.outline_glyph(g) {
            let b = og.px_bounds();
            min_x = min_x.min(b.min.x);
            min_y = min_y.min(b.min.y);
            max_x = max_x.max(b.max.x);
            max_y = max_y.max(b.max.y);
        }
    }
    if min_x == f32::MAX {
        // Nothing outlined (e.g. all whitespace): fall back to advance width.
        let scaled = font.as_scaled(PxScale::from(size));
        let w: f32 = text.chars().map(|c| scaled.h_advance(font.glyph_id(c))).sum();
        return Metrics { min_x: 0.0, min_y: 0.0, width: w.max(1.0), height: size, baseline: ascent };
    }
    Metrics {
        min_x,
        min_y,
        width: (max_x - min_x).ceil().max(1.0),
        height: (max_y - min_y).ceil().max(1.0),
        baseline: ascent - min_y,
    }
}

pub fn rasterize(font: &FontRef<'_>, text: &str, size: f32) -> Mask {
    let m = measure(font, text, size);
    let width = m.width as usize;
    let height = m.height as usize;
    let mut data = vec![0.0f32; width * height];
    for g in layout_glyphs(font, text, size) {
        if let Some(og) = font.outline_glyph(g) {
            let b = og.px_bounds();
            let ox = (b.min.x - m.min_x).round() as i64;
            let oy = (b.min.y - m.min_y).round() as i64;
            og.draw(|x, y, c| {
                let px = ox + x as i64;
                let py = oy + y as i64;
                if px >= 0 && py >= 0 && (px as usize) < width && (py as usize) < height {
                    let i = py as usize * width + px as usize;
                    data[i] = data[i].max(c);
                }
            });
        }
    }
    Mask { width, height, data }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_grows_with_size() {
        let f = load_font();
        let a = measure(&f, "Rust", 20.0);
        let b = measure(&f, "Rust", 40.0);
        assert!(b.width > a.width * 1.8);
        assert!(b.height > a.height * 1.8);
    }

    #[test]
    fn rotate_swaps_dimensions() {
        let m = Mask { width: 3, height: 2, data: vec![1., 2., 3., 4., 5., 6.] };
        let r = m.rotate_ccw();
        assert_eq!((r.width, r.height), (2, 3));
        // top-right (2,0)=3 -> top-left (0,0)
        assert_eq!(r.data[0], 3.0);
        // top-left (0,0)=1 -> bottom-left (0,2)
        assert_eq!(r.data[2 * 2], 1.0);
    }
}
