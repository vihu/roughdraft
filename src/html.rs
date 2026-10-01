//! A scene as one self-contained web page, for sharing on any static host
//! (`roughdraft render -o page.html`).
use crate::scene::Scene;
use crate::svg::{self, SvgOptions, escape};

/// Returns a page showing `scene` under `title`, with a link that downloads
/// it as `file_name`.
///
/// Everything is inside the page: the SVG with its fonts, and the scene for
/// the download link. It follows the viewer's light or dark mode with
/// Excalidraw's dark filter, and fits the drawing to the window's width.
pub fn page(scene: &Scene, title: &str, file_name: &str) -> String {
    let saved = scene.saved();
    let drawing = svg::export(&saved, &SvgOptions::default());
    let source = crate::base64::encode(saved.to_json().as_bytes());
    let (title, file_name) = (escape(title), escape(file_name));
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  :root {{ color-scheme: light dark; }}
  body {{ margin: 0; padding: 24px; background: #ffffff; color: #1e1e1e; font: 15px/1.5 system-ui, sans-serif; }}
  header {{ display: flex; flex-wrap: wrap; gap: 8px 24px; align-items: baseline; justify-content: space-between; margin-bottom: 16px; }}
  h1 {{ margin: 0; font-size: 20px; font-weight: 600; }}
  a {{ color: inherit; }}
  figure {{ margin: 0; }}
  figure svg {{ display: block; max-width: 100%; height: auto; }}
  @media (prefers-color-scheme: dark) {{
    body {{ background: #121212; color: #e3e3e3; }}
    figure svg {{ filter: invert(93%) hue-rotate(180deg); }}
  }}
</style>
</head>
<body>
<header>
<h1>{title}</h1>
<a download="{file_name}" href="data:application/vnd.excalidraw+json;base64,{source}">Download {file_name}</a>
</header>
<figure>{drawing}</figure>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::page;
    use crate::scene::Scene;

    #[test]
    fn the_page_carries_the_drawing_its_fonts_and_the_scene() {
        let json = std::fs::read_to_string("tests/fixtures/scenes/fonts.excalidraw").unwrap();
        let scene: Scene = serde_json::from_str(&json).unwrap();
        let html = page(&scene, "Fonts <& co>", "fonts.excalidraw");
        assert!(html.contains("<title>Fonts &lt;&amp; co&gt;</title>"));
        assert!(html.contains("<svg ") && html.contains("@font-face"));
        let (_, rest) = html
            .split_once("data:application/vnd.excalidraw+json;base64,")
            .unwrap();
        let encoded = &rest[..rest.find('"').unwrap()];
        let source = crate::base64::decode(encoded).unwrap();
        let back: Scene = serde_json::from_slice(&source).unwrap();
        assert_eq!(back.elements.len(), scene.saved().elements.len());
    }
}
