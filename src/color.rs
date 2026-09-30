//! CSS colors as Excalidraw stores them, and its dark-theme canvas filter.
mod names;

/// Straight-alpha sRGB color with components in `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Alpha.
    pub a: f32,
}

impl Rgba {
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);

    /// Opaque white.
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);

    /// Returns an opaque color.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Parses a CSS color the way a browser would for Excalidraw: hex
    /// (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`), `transparent`, the named
    /// colors, and `rgb()`/`rgba()`/`hsl()`/`hsla()` in comma or space
    /// syntax.
    ///
    /// Returns `None` for anything else.
    pub fn parse(css: &str) -> Option<Self> {
        let css = css.trim().to_ascii_lowercase();
        if css == "transparent" {
            return Some(Self {
                a: 0.0,
                ..Self::BLACK
            });
        }
        if let Some(hex) = css.strip_prefix('#') {
            return Self::parse_hex(hex);
        }
        if let Some(args) = arguments(&css, &["rgb", "rgba"]) {
            let [r, g, b] = [args.first()?, args.get(1)?, args.get(2)?].map(|c| channel(c));
            let a = args.get(3).map_or(Some(1.0), |a| alpha(a))?;
            return Some(Self {
                r: r?,
                g: g?,
                b: b?,
                a,
            });
        }
        if let Some(args) = arguments(&css, &["hsl", "hsla"]) {
            let (h, s, l) = (
                hue(args.first()?)?,
                percent(args.get(1)?)?,
                percent(args.get(2)?)?,
            );
            let a = args.get(3).map_or(Some(1.0), |a| alpha(a))?;
            let [r, g, b] = hsl_to_rgb(h, s, l);
            return Some(Self { r, g, b, a });
        }
        let i = names::NAMED
            .binary_search_by(|(name, _)| name.cmp(&css.as_str()))
            .ok()?;
        let rgb = names::NAMED[i].1;
        let byte = |shift: u32| f32::from((rgb >> shift) as u8) / 255.0;
        Some(Self::rgb(byte(16), byte(8), byte(0)))
    }

    /// Returns this color with its alpha multiplied by `opacity`.
    #[must_use]
    pub fn fade(self, opacity: f32) -> Self {
        Self {
            a: self.a * opacity,
            ..self
        }
    }

    /// Applies Excalidraw's dark theme filter, `invert(93%) hue-rotate(180deg)`.
    ///
    /// The filter is affine in sRGB, so applying it per color before blending
    /// matches applying it to the finished canvas, up to clamping.
    #[must_use]
    pub fn to_dark(self) -> Self {
        /// `invert(93%)`.
        const INVERT: f32 = 0.93;
        /// CSS `hue-rotate` matrix (Filter Effects spec) at 180deg: cos -1, sin 0.
        const HUE_ROTATE_180: [[f32; 3]; 3] = [
            [0.213 - 0.787, 0.715 + 0.715, 0.072 + 0.072],
            [0.213 + 0.213, 0.715 - 0.285, 0.072 + 0.072],
            [0.213 + 0.213, 0.715 + 0.715, 0.072 - 0.928],
        ];

        let inverted = [self.r, self.g, self.b].map(|c| INVERT + c * (1.0 - 2.0 * INVERT));
        let [r, g, b] = HUE_ROTATE_180.map(|row| {
            let dot: f32 = row.iter().zip(inverted).map(|(m, c)| m * c).sum();
            dot.clamp(0.0, 1.0)
        });
        Self { r, g, b, a: self.a }
    }
}

// Private API
impl Rgba {
    fn parse_hex(hex: &str) -> Option<Self> {
        let digit = |i: usize| u8::from_str_radix(hex.get(i..=i)?, 16).ok();
        let pair = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        let [r, g, b, a] = match hex.len() {
            3 | 4 => {
                let short = |i| digit(i).map(|d| d * 0x11);
                [short(0)?, short(1)?, short(2)?, short(3).unwrap_or(0xff)]
            }
            6 | 8 => [pair(0)?, pair(2)?, pair(4)?, pair(6).unwrap_or(0xff)],
            _ => return None,
        };
        let unit = |c: u8| f32::from(c) / 255.0;
        Some(Self {
            r: unit(r),
            g: unit(g),
            b: unit(b),
            a: unit(a),
        })
    }
}

/// The arguments of `name(...)` for one of `names`, split on commas,
/// spaces and the `/` before alpha.
fn arguments<'a>(css: &'a str, names: &[&str]) -> Option<Vec<&'a str>> {
    let (name, rest) = css.split_once('(')?;
    if !names.contains(&name.trim()) {
        return None;
    }
    let inner = rest.strip_suffix(')')?;
    let separator = |c: char| c == ',' || c == '/' || c.is_whitespace();
    Some(inner.split(separator).filter(|s| !s.is_empty()).collect())
}

/// A number, or a percentage of `whole`.
fn number(value: &str, whole: f32) -> Option<f32> {
    match value.strip_suffix('%') {
        Some(pct) => pct.parse::<f32>().ok().map(|p| p / 100.0 * whole),
        None => value.parse().ok(),
    }
}

/// An `rgb()` channel, 0 to 255 or a percentage.
fn channel(value: &str) -> Option<f32> {
    number(value, 255.0).map(|c| (c / 255.0).clamp(0.0, 1.0))
}

/// Alpha, 0 to 1 or a percentage.
fn alpha(value: &str) -> Option<f32> {
    number(value, 1.0).map(|a| a.clamp(0.0, 1.0))
}

/// Saturation or lightness, as a fraction.
fn percent(value: &str) -> Option<f32> {
    number(value.trim_end_matches('%'), 100.0).map(|p| (p / 100.0).clamp(0.0, 1.0))
}

/// A hue in degrees (`deg`, `turn`, `rad` or a bare number).
fn hue(value: &str) -> Option<f32> {
    if let Some(turns) = value.strip_suffix("turn") {
        return turns.parse::<f32>().ok().map(|t| t * 360.0);
    }
    if let Some(radians) = value.strip_suffix("rad") {
        return radians.parse::<f32>().ok().map(f32::to_degrees);
    }
    value.trim_end_matches("deg").parse().ok()
}

/// CSS Color 4's `hslToRgb`.
fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let hue = hue.rem_euclid(360.0);
    let f = |n: f32| {
        let k = (n + hue / 30.0) % 12.0;
        let a = saturation * lightness.min(1.0 - lightness);
        lightness - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    [f(0.0), f(8.0), f(4.0)]
}

#[cfg(test)]
mod tests {
    use super::Rgba;

    fn assert_close(got: Rgba, want: Rgba) {
        let pairs = [
            (got.r, want.r),
            (got.g, want.g),
            (got.b, want.b),
            (got.a, want.a),
        ];
        assert!(
            pairs.iter().all(|(g, w)| (g - w).abs() < 1e-3),
            "{got:?} != {want:?}"
        );
    }

    #[test]
    fn parses_names_and_functions_like_a_browser() {
        let red = Rgba::rgb(1.0, 0.0, 0.0);
        for css in [
            "red",
            " RED ",
            "rgb(255, 0, 0)",
            "rgb(255 0 0)",
            "rgb(100% 0% 0%)",
            "hsl(0, 100%, 50%)",
            "hsl(360deg 100% 50%)",
        ] {
            assert_close(Rgba::parse(css).unwrap_or_else(|| panic!("{css}")), red);
        }
        assert_close(
            Rgba::parse("rgba(0, 0, 255, 0.5)").unwrap(),
            Rgba {
                a: 0.5,
                ..Rgba::rgb(0.0, 0.0, 1.0)
            },
        );
        assert_close(
            Rgba::parse("rgb(0 0 255 / 25%)").unwrap(),
            Rgba {
                a: 0.25,
                ..Rgba::rgb(0.0, 0.0, 1.0)
            },
        );
        assert_close(
            Rgba::parse("hsla(120, 100%, 25%, 1)").unwrap(),
            Rgba::rgb(0.0, 0.5, 0.0),
        );
        assert_close(
            Rgba::parse("rebeccapurple").unwrap(),
            Rgba::rgb(0.4, 0.2, 0.6),
        );
        assert_eq!(Rgba::parse("gray"), Rgba::parse("#808080"));
        for bad in ["nonsense", "rgb(1, 2)", "hsl(x, 1%, 1%)", "rgb(1,2,3"] {
            assert_eq!(Rgba::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn parses_hex_forms() {
        assert_eq!(
            Rgba::parse("#1e1e1e"),
            Some(Rgba::rgb(30.0 / 255.0, 30.0 / 255.0, 30.0 / 255.0))
        );
        assert_eq!(Rgba::parse("#fff"), Some(Rgba::WHITE));
        assert_eq!(Rgba::parse("#ffffff00").map(|c| c.a), Some(0.0));
        assert_eq!(Rgba::parse("#f008").map(|c| c.a), Some(0x88 as f32 / 255.0));
        assert_eq!(Rgba::parse("transparent").map(|c| c.a), Some(0.0));
    }

    #[test]
    fn rejects_malformed_hex() {
        for css in ["#12345", "#ggg", "1e1e1e", "", "#"] {
            assert_eq!(Rgba::parse(css), None, "{css}");
        }
    }

    #[test]
    fn dark_filter_matches_excalidraw_greys() {
        // White canvas becomes Excalidraw's dark background, #121212.
        assert_close(Rgba::WHITE.to_dark(), Rgba::rgb(0.07, 0.07, 0.07));
        // Default stroke #1e1e1e becomes light grey; greys stay grey.
        let grey = 0.93 - 0.86 * 30.0 / 255.0;
        assert_close(
            Rgba::parse("#1e1e1e").unwrap().to_dark(),
            Rgba::rgb(grey, grey, grey),
        );
        assert_eq!(Rgba::WHITE.fade(0.5).to_dark().a, 0.5);
    }
}
