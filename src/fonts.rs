//! Bundled fonts, one per Excalidraw font family that ships with it. Each
//! has its licence next to it under `assets/fonts/`.

/// Excalifont (family 5), Excalidraw's default hand-drawn font (OFL-1.1).
pub const EXCALIFONT: &[u8] = include_bytes!("../assets/fonts/Excalifont/Excalifont-Regular.ttf");

/// Virgil (family 1), the default before Excalifont (OFL-1.1).
pub const VIRGIL: &[u8] = include_bytes!("../assets/fonts/Virgil/Virgil-Regular.ttf");

/// Nunito at weight 500, Excalidraw's "Normal" (family 6, OFL-1.1).
///
/// Merged from the unicode-range subsets Excalidraw 0.18.1 serves, so the
/// glyphs are the ones Excalidraw draws, and renamed to plain "Nunito".
pub const NUNITO: &[u8] = include_bytes!("../assets/fonts/Nunito/Nunito-Medium.ttf");

/// Lilita One (family 7), unmodified from Google Fonts (OFL-1.1).
pub const LILITA_ONE: &[u8] = include_bytes!("../assets/fonts/LilitaOne/LilitaOne-Regular.ttf");

/// Comic Shanns, Excalidraw's "Code" (family 8, MIT).
///
/// Merged from the unicode-range subsets Excalidraw 0.18.1 serves.
pub const COMIC_SHANNS: &[u8] =
    include_bytes!("../assets/fonts/ComicShanns/ComicShanns-Regular.ttf");

/// Every bundled font, for registering with iced.
pub const ALL: [&[u8]; 5] = [EXCALIFONT, VIRGIL, NUNITO, LILITA_ONE, COMIC_SHANNS];

/// Returns the bundled font for an Excalidraw `fontFamily` id, `None` for
/// Helvetica (2), Cascadia (3) and Liberation Sans (9), which are not
/// bundled. Unknown ids draw in Excalifont.
pub const fn for_family(id: u32) -> Option<&'static [u8]> {
    match id {
        1 => Some(VIRGIL),
        6 => Some(NUNITO),
        7 => Some(LILITA_ONE),
        8 => Some(COMIC_SHANNS),
        2 | 3 | 9 => None,
        _ => Some(EXCALIFONT),
    }
}
