//! Color palettes and hex parsing.

/// An RGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// Perceived luminance in 0..=1 (sRGB, rough).
    pub fn luma(self) -> f32 {
        (0.2126 * self.0 as f32 + 0.7152 * self.1 as f32 + 0.0722 * self.2 as f32) / 255.0
    }
}

/// Parse `#rgb`, `#rrggbb`, `rrggbb`, or a handful of CSS color names.
pub fn parse_color(s: &str) -> Result<Rgb, String> {
    let s = s.trim();
    let named = match s.to_ascii_lowercase().as_str() {
        "white" => Some(Rgb(255, 255, 255)),
        "black" => Some(Rgb(0, 0, 0)),
        "red" => Some(Rgb(227, 73, 72)),
        "green" => Some(Rgb(0, 131, 0)),
        "blue" => Some(Rgb(42, 120, 214)),
        "orange" => Some(Rgb(235, 104, 52)),
        "yellow" => Some(Rgb(237, 161, 0)),
        "gray" | "grey" => Some(Rgb(137, 135, 129)),
        "navy" => Some(Rgb(13, 54, 107)),
        "cream" => Some(Rgb(252, 252, 251)),
        "charcoal" => Some(Rgb(26, 26, 25)),
        _ => None,
    };
    if let Some(c) = named {
        return Ok(c);
    }
    let hex = s.strip_prefix('#').unwrap_or(s);
    let bad = || format!("invalid color `{s}` (expected #rrggbb, #rgb, or a color name)");
    match hex.len() {
        3 => {
            let v = u16::from_str_radix(hex, 16).map_err(|_| bad())?;
            let r = ((v >> 8) & 0xf) as u8;
            let g = ((v >> 4) & 0xf) as u8;
            let b = (v & 0xf) as u8;
            Ok(Rgb(r * 17, g * 17, b * 17))
        }
        6 => {
            let v = u32::from_str_radix(hex, 16).map_err(|_| bad())?;
            Ok(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
        }
        _ => Err(bad()),
    }
}

/// Parse a comma-separated list of colors.
pub fn parse_color_list(s: &str) -> Result<Vec<Rgb>, String> {
    let colors: Result<Vec<_>, _> = s
        .split(',')
        .filter(|p| !p.trim().is_empty())
        .map(parse_color)
        .collect();
    let colors = colors?;
    if colors.is_empty() {
        return Err("color list is empty".into());
    }
    Ok(colors)
}

/// Named palettes. Each is tuned for a light background; `dark_variant` returns
/// the same palette re-stepped for a dark surface where one exists.
pub fn named_palette(name: &str, on_dark: bool) -> Option<Vec<Rgb>> {
    let hexes: &[&str] = match (name.to_ascii_lowercase().as_str(), on_dark) {
        // Validated 8-hue categorical palette (blue, orange, aqua, yellow,
        // magenta, green, violet, red) - light-surface steps.
        ("vivid", false) => &[
            "#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#4a3aa7", "#e34948",
        ],
        // Same hues, dark-surface steps.
        ("vivid", true) => &[
            "#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#4dbf4d", "#9085e9", "#e66767",
        ],
        ("pastel", _) => &[
            "#86b6ef", "#f5a889", "#7fd6b3", "#f7cf6e", "#f3b5cb", "#9ccf8c", "#b5aef0", "#f19a99",
        ],
        ("ocean", false) => &["#0d366b", "#184f95", "#256abf", "#3987e5", "#1baf7a", "#0e7a8c"],
        ("ocean", true) => &["#86b6ef", "#5598e7", "#3987e5", "#6dd9b0", "#3cc9c9", "#cde2fb"],
        ("sunset", false) => &["#8a1c3b", "#c62a4a", "#e34948", "#eb6834", "#eda100", "#d55181"],
        ("sunset", true) => &["#ff9a8b", "#f7b267", "#f4845f", "#e66767", "#fab219", "#e87ba4"],
        ("mono", false) => &["#0b0b0b", "#2c2c2a", "#52514e", "#6e6c66", "#898781"],
        ("mono", true) => &["#ffffff", "#e1e0d9", "#c3c2b7", "#a8a69f", "#898781"],
        _ => return None,
    };
    Some(hexes.iter().map(|h| parse_color(h).expect("static hex")).collect())
}

pub const PALETTE_NAMES: &[&str] = &["vivid", "pastel", "ocean", "sunset", "mono"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_forms() {
        assert_eq!(parse_color("#fff").unwrap(), Rgb(255, 255, 255));
        assert_eq!(parse_color("2a78d6").unwrap(), Rgb(0x2a, 0x78, 0xd6));
        assert!(parse_color("#12345").is_err());
        assert!(parse_color("nope").is_err());
    }

    #[test]
    fn all_named_palettes_parse() {
        for n in PALETTE_NAMES {
            assert!(!named_palette(n, false).unwrap().is_empty());
            assert!(!named_palette(n, true).unwrap().is_empty());
        }
    }
}
