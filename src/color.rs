//! CSS colors as Excalidraw stores them, and its dark-theme canvas filter.

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

    /// Parses `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` or `transparent`.
    ///
    /// Returns `None` for anything else.
    // ponytail: hex only, add CSS names and rgb() if a fixture uses them
    pub fn parse(css: &str) -> Option<Self> {
        if css == "transparent" {
            return Some(Self {
                a: 0.0,
                ..Self::BLACK
            });
        }
        let hex = css.strip_prefix('#')?;
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
    fn rejects_non_hex() {
        for css in ["red", "#12345", "#ggg", "1e1e1e", "", "#"] {
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
