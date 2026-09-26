//! PNG and SVG renderers for a laid-out word cloud.

use crate::layout::Placed;
use crate::palette::Rgb;
use crate::text::rasterize;
use ab_glyph::FontRef;
use image::{Rgba, RgbaImage};

pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// `None` means transparent background.
    pub background: Option<Rgb>,
}

pub fn render_png(font: &FontRef<'_>, canvas: &Canvas, words: &[Placed]) -> RgbaImage {
    let bg = match canvas.background {
        Some(c) => Rgba([c.0, c.1, c.2, 255]),
        None => Rgba([0, 0, 0, 0]),
    };
    let mut img = RgbaImage::from_pixel(canvas.width, canvas.height, bg);

    for p in words {
        let mut mask = rasterize(font, &p.text, p.size);
        if p.vertical {
            mask = mask.rotate_ccw();
        }
        for my in 0..mask.height {
            for mx in 0..mask.width {
                let cov = mask.data[my * mask.width + mx];
                if cov <= 0.0 {
                    continue;
                }
                let x = p.x + mx as i32;
                let y = p.y + my as i32;
                if x < 0 || y < 0 || x >= canvas.width as i32 || y >= canvas.height as i32 {
                    continue;
                }
                let dst = img.get_pixel_mut(x as u32, y as u32);
                *dst = blend(*dst, p.color, cov.min(1.0));
            }
        }
    }
    img
}

/// Alpha-composite `src` (with coverage `a`) over `dst`.
fn blend(dst: Rgba<u8>, src: Rgb, a: f32) -> Rgba<u8> {
    let da = dst[3] as f32 / 255.0;
    let out_a = a + da * (1.0 - a);
    if out_a <= 0.0 {
        return Rgba([0, 0, 0, 0]);
    }
    let ch = |s: u8, d: u8| -> u8 {
        let v = (s as f32 * a + d as f32 * da * (1.0 - a)) / out_a;
        v.round().clamp(0.0, 255.0) as u8
    };
    Rgba([ch(src.0, dst[0]), ch(src.1, dst[1]), ch(src.2, dst[2]), (out_a * 255.0).round() as u8])
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn render_svg(canvas: &Canvas, words: &[Placed]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n",
        w = canvas.width,
        h = canvas.height
    ));
    if let Some(bg) = canvas.background {
        out.push_str(&format!("  <rect width=\"100%\" height=\"100%\" fill=\"{}\"/>\n", bg.to_hex()));
    }
    out.push_str("  <g font-family=\"'DejaVu Sans', Verdana, 'Bitstream Vera Sans', sans-serif\" font-weight=\"bold\">\n");
    for p in words {
        let m = p.metrics;
        let text = xml_escape(&p.text);
        if p.vertical {
            // Draw the text horizontally with its tight box's top-left at the
            // local origin, then rotate -90 about that origin: the box then
            // spans x in [0, h_text] and y in [-w_text, 0], so translating to
            // the bottom-left corner of the placed box lines it up.
            out.push_str(&format!(
                "    <text transform=\"translate({:.1} {:.1}) rotate(-90)\" x=\"{:.1}\" y=\"{:.1}\" font-size=\"{:.1}\" fill=\"{}\">{}</text>\n",
                p.x, p.y + p.h, -m.min_x, m.baseline, p.size, p.color.to_hex(), text
            ));
        } else {
            let x = p.x as f32 - m.min_x;
            let y = p.y as f32 + m.baseline;
            out.push_str(&format!(
                "    <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{:.1}\" fill=\"{}\">{}</text>\n",
                x, y, p.size, p.color.to_hex(), text
            ));
        }
    }
    out.push_str("  </g>\n</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_full_coverage_replaces() {
        let out = blend(Rgba([255, 255, 255, 255]), Rgb(10, 20, 30), 1.0);
        assert_eq!(out, Rgba([10, 20, 30, 255]));
    }

    #[test]
    fn blend_on_transparent_keeps_color() {
        let out = blend(Rgba([0, 0, 0, 0]), Rgb(10, 20, 30), 0.5);
        assert_eq!(out, Rgba([10, 20, 30, 128]));
    }
}
