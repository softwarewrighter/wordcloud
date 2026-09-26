mod layout;
mod palette;
mod render;
mod text;

use clap::{ArgAction, Parser, ValueEnum};
use layout::{Item, Options};
use palette::{named_palette, parse_color, parse_color_list, Rgb, PALETTE_NAMES};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use render::Canvas;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Sizing {
    /// First word is largest, sizes decrease in the order given (default).
    Ordered,
    /// All words the same size (unless `word=weight` is used).
    Equal,
    /// Random sizes, reproducible with --seed.
    Random,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Format {
    Png,
    Svg,
}

/// Generate a colored word cloud from words given on the command line.
///
/// Each argument is one entry (quote multi-word phrases). Append `=N` to give
/// a word an explicit weight, e.g. `"Rust=5" "Go=2"`; larger weights are drawn
/// bigger. Words without a weight are sized by --sizing.
#[derive(Parser, Debug)]
#[command(name = "wordcloud", version, about, long_about = None, after_help = format!(
    "PALETTES:\n  {}\n\nEXAMPLES:\n  wordcloud \"Operating Systems\" \"Programming Languages\" Platforms Tools\n  wordcloud -o cloud.svg --palette sunset --bg charcoal Rust Go Zig\n  wordcloud --seed 42 --sizing random \"Rust=5\" \"Go=3\" \"Zig=1\"",
    PALETTE_NAMES.join(", ")
))]
struct Cli {
    /// Words or phrases to include. Use `word=weight` for explicit weights.
    #[arg(required = true, num_args = 1..)]
    words: Vec<String>,

    /// Output file. Format is inferred from the extension unless --format is given.
    #[arg(short, long, default_value = "wordcloud.png")]
    output: PathBuf,

    /// Output format (overrides the extension of --output).
    #[arg(short, long, value_enum)]
    format: Option<Format>,

    /// Canvas width in pixels.
    #[arg(short = 'W', long, default_value_t = 1200)]
    width: u32,

    /// Canvas height in pixels.
    #[arg(short = 'H', long, default_value_t = 800)]
    height: u32,

    /// Named palette, or a comma-separated list of colors (e.g. "#ff0000,#00aa00").
    #[arg(short, long, default_value = "vivid")]
    palette: String,

    /// Background color (name or hex).
    #[arg(short, long, default_value = "white", conflicts_with = "transparent")]
    bg: String,

    /// Transparent background (PNG and SVG).
    #[arg(short = 't', long, action = ArgAction::SetTrue)]
    transparent: bool,

    /// How to size words that have no explicit weight.
    #[arg(short, long, value_enum, default_value_t = Sizing::Ordered)]
    sizing: Sizing,

    /// Largest font size in pixels. By default words grow to fill the canvas,
    /// up to half its smaller dimension.
    #[arg(long)]
    max_size: Option<f32>,

    /// Smallest font size in pixels.
    #[arg(long, default_value_t = 16.0)]
    min_size: f32,

    /// Fraction of words drawn vertically (0 = none, 1 = all).
    #[arg(short = 'v', long, default_value_t = 0.3)]
    vertical: f32,

    /// Minimum gap between words in pixels.
    #[arg(long, default_value_t = 6)]
    padding: i32,

    /// Random seed for reproducible layouts.
    #[arg(long)]
    seed: Option<u64>,

    /// Assign palette colors randomly instead of in order.
    #[arg(long, action = ArgAction::SetTrue)]
    shuffle_colors: bool,

    /// Print the layout (word, size, position) to stderr.
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
}

/// Split `text=weight` into its parts; `\=` escapes a literal `=`.
fn parse_item(arg: &str) -> Result<(String, Option<f32>), String> {
    if let Some((word, w)) = arg.rsplit_once('=') {
        if !word.ends_with('\\') {
            let weight: f32 = w
                .trim()
                .parse()
                .map_err(|_| format!("bad weight `{w}` in `{arg}` (expected a number)"))?;
            if weight <= 0.0 {
                return Err(format!("weight must be positive in `{arg}`"));
            }
            return Ok((word.trim().to_string(), Some(weight)));
        }
    }
    Ok((arg.replace("\\=", "=").trim().to_string(), None))
}

fn build_items(words: &[String], sizing: Sizing, rng: &mut ChaCha8Rng) -> Result<Vec<Item>, String> {
    use rand::Rng;
    let n = words.len();
    let mut raw = Vec::with_capacity(n);
    for (i, w) in words.iter().enumerate() {
        let (text, weight) = parse_item(w)?;
        if text.is_empty() {
            return Err(format!("argument {} is empty", i + 1));
        }
        let weight = weight.unwrap_or(match sizing {
            Sizing::Ordered => 1.0 - i as f32 / n as f32,
            Sizing::Equal => 1.0,
            Sizing::Random => rng.gen_range(0.15..=1.0),
        });
        raw.push((text, weight));
    }
    let max = raw.iter().map(|(_, w)| *w).fold(f32::MIN, f32::max);
    let min = raw.iter().map(|(_, w)| *w).fold(f32::MAX, f32::min);
    Ok(raw
        .into_iter()
        .map(|(text, w)| Item {
            text,
            // Normalize so the heaviest word is 1.0 and the lightest keeps some size.
            weight: if (max - min).abs() < f32::EPSILON { 1.0 } else { 0.12 + 0.88 * (w - min) / (max - min) },
        })
        .collect())
}

fn run(cli: Cli) -> Result<(), String> {
    if cli.width < 50 || cli.height < 50 {
        return Err("canvas must be at least 50x50 pixels".into());
    }
    if !(0.0..=1.0).contains(&cli.vertical) {
        return Err("--vertical must be between 0 and 1".into());
    }

    let format = cli.format.unwrap_or_else(|| {
        match cli.output.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) {
            Some(e) if e == "svg" => Format::Svg,
            _ => Format::Png,
        }
    });

    let background: Option<Rgb> = if cli.transparent { None } else { Some(parse_color(&cli.bg)?) };
    let on_dark = background.map(|c| c.luma() < 0.45).unwrap_or(false);
    let colors = match named_palette(&cli.palette, on_dark) {
        Some(c) => c,
        None if cli.palette.contains(',') || cli.palette.starts_with('#') => parse_color_list(&cli.palette)?,
        None => {
            return Err(format!(
                "unknown palette `{}` (choose one of {} or give a list of colors)",
                cli.palette,
                PALETTE_NAMES.join(", ")
            ))
        }
    };

    let seed = cli.seed.unwrap_or_else(|| {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
    });
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    let items = build_items(&cli.words, cli.sizing, &mut rng)?;
    // Nominal size of the heaviest word; the layout then grows or shrinks all
    // words together until the canvas is filled, up to `size_cap`.
    let max_size = cli.max_size.unwrap_or(cli.height.min(cli.width) as f32 / 4.0);
    let size_cap = cli.max_size.unwrap_or(cli.height.min(cli.width) as f32 / 2.0);
    if cli.min_size <= 0.0 || max_size < cli.min_size {
        return Err("--min-size must be positive and no larger than --max-size".into());
    }

    let font = text::load_font();
    let opts = Options {
        width: cli.width,
        height: cli.height,
        min_size: cli.min_size,
        max_size,
        size_cap,
        padding: cli.padding.max(0),
        margin: (cli.width.min(cli.height) / 40) as i32,
        vertical_prob: cli.vertical,
        colors,
        shuffle_colors: cli.shuffle_colors,
    };
    let (placed, skipped) = layout::layout(&font, &items, &opts, &mut rng);

    if cli.verbose {
        eprintln!("seed: {seed}");
        for p in &placed {
            eprintln!(
                "{:>6.1}px at {:>4},{:<4} {:>4}x{:<4} {}{}",
                p.size,
                p.x,
                p.y,
                p.w,
                p.h,
                p.text,
                if p.vertical { " (vertical)" } else { "" }
            );
        }
    }
    for s in &skipped {
        eprintln!("warning: could not fit \"{s}\"; try a larger canvas or smaller --min-size");
    }
    if placed.is_empty() {
        return Err("no words could be placed".into());
    }

    let canvas = Canvas { width: cli.width, height: cli.height, background };
    match format {
        Format::Png => {
            let img = render::render_png(&font, &canvas, &placed);
            img.save(&cli.output).map_err(|e| format!("writing {}: {e}", cli.output.display()))?;
        }
        Format::Svg => {
            std::fs::write(&cli.output, render::render_svg(&canvas, &placed))
                .map_err(|e| format!("writing {}: {e}", cli.output.display()))?;
        }
    }
    println!("wrote {} ({} words{})", cli.output.display(), placed.len(), if skipped.is_empty() { String::new() } else { format!(", {} skipped", skipped.len()) });
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_weights() {
        assert_eq!(parse_item("Rust=5").unwrap(), ("Rust".into(), Some(5.0)));
        assert_eq!(parse_item("Operating Systems").unwrap(), ("Operating Systems".into(), None));
        assert_eq!(parse_item("a\\=b").unwrap(), ("a=b".into(), None));
        assert!(parse_item("Rust=lots").is_err());
        assert!(parse_item("Rust=0").is_err());
    }

    #[test]
    fn ordered_sizing_descends() {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let words: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let items = build_items(&words, Sizing::Ordered, &mut rng).unwrap();
        assert!(items[0].weight > items[1].weight && items[1].weight > items[2].weight);
        assert!((items[0].weight - 1.0).abs() < 1e-6);
    }
}
