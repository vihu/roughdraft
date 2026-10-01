//! A scene as one self-contained web page, for sharing on any static host
//! (`roughdraft render -o page.html`).
use crate::scene::Scene;
use crate::svg::{self, SvgOptions, escape};

/// The theme a page starts in, until the reader picks one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartTheme {
    /// The reader's system setting.
    #[default]
    System,
    /// Dark, whatever the system setting.
    Dark,
}

/// Returns a page showing `scene` under `title`, with a link that downloads
/// it as `file_name`.
///
/// Everything is inside the page: the SVG with its fonts, and the scene for
/// the download link. It starts in `start`, with Excalidraw's dark filter
/// for dark; a button switches between light and dark and remembers the
/// choice in the browser. Without JavaScript the button stays hidden and the
/// page keeps its start theme. The drawing fits the window's width.
pub fn page(scene: &Scene, title: &str, file_name: &str, start: StartTheme) -> String {
    let saved = scene.saved();
    let drawing = svg::export(&saved, &SvgOptions::default());
    let source = crate::base64::encode(saved.to_json().as_bytes());
    let (title, file_name) = (escape(title), escape(file_name));
    let theme = match start {
        StartTheme::System => "",
        StartTheme::Dark => r#" data-theme="dark""#,
    };
    format!(
        r#"<!doctype html>
<html lang="en"{theme}>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>{STYLE}</style>
<script>{REMEMBERED}</script>
</head>
<body>
<header>
<h1>{title}</h1>
<div class="actions">
<button type="button" id="theme" hidden aria-label="Switch to dark mode">Dark mode</button>
<a download="{file_name}" href="data:application/vnd.excalidraw+json;base64,{source}">Download {file_name}</a>
</div>
</header>
<figure>{drawing}</figure>
<script>{TOGGLE}</script>
</body>
</html>
"#
    )
}

/// The page's look. An explicit `data-theme` on `<html>` wins over the
/// system setting in both directions.
const STYLE: &str = r#"
  :root { color-scheme: light dark; }
  :root[data-theme="light"] { color-scheme: light; }
  :root[data-theme="dark"] { color-scheme: dark; }
  body { margin: 0; padding: 24px; background: #ffffff; color: #1e1e1e; font: 15px/1.5 system-ui, sans-serif; }
  header { display: flex; flex-wrap: wrap; gap: 8px 24px; align-items: baseline; justify-content: space-between; margin-bottom: 16px; }
  h1 { margin: 0; font-size: 20px; font-weight: 600; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px 16px; align-items: baseline; }
  a { color: inherit; }
  button { font: inherit; color: inherit; background: none; border: 1px solid currentColor; border-radius: 6px; padding: 2px 10px; cursor: pointer; }
  button[hidden] { display: none; }
  a:focus-visible, button:focus-visible { outline: 2px solid currentColor; outline-offset: 2px; }
  figure { margin: 0; }
  figure svg { display: block; max-width: 100%; height: auto; }
  :root[data-theme="dark"] body { background: #121212; color: #e3e3e3; }
  :root[data-theme="dark"] figure svg { filter: invert(93%) hue-rotate(180deg); }
  @media (prefers-color-scheme: dark) {
    :root:not([data-theme="light"]) body { background: #121212; color: #e3e3e3; }
    :root:not([data-theme="light"]) figure svg { filter: invert(93%) hue-rotate(180deg); }
  }
"#;

/// Applies the reader's saved choice before the first paint. Storage can be
/// blocked (private windows, file pages): then the start theme stays.
const REMEMBERED: &str = r#"
  try {
    var theme = localStorage.getItem("roughdraft-theme");
    if (theme === "light" || theme === "dark") document.documentElement.setAttribute("data-theme", theme);
  } catch (error) {}
"#;

/// Shows the button and switches the theme, naming the next one.
const TOGGLE: &str = r#"
  (function () {
    var root = document.documentElement;
    var button = document.getElementById("theme");
    var system = matchMedia("(prefers-color-scheme: dark)");
    function dark() {
      var theme = root.getAttribute("data-theme");
      return theme ? theme === "dark" : system.matches;
    }
    function label() {
      var next = dark() ? "light" : "dark";
      button.textContent = next === "dark" ? "Dark mode" : "Light mode";
      button.setAttribute("aria-label", "Switch to " + next + " mode");
    }
    button.addEventListener("click", function () {
      var next = dark() ? "light" : "dark";
      root.setAttribute("data-theme", next);
      try { localStorage.setItem("roughdraft-theme", next); } catch (error) {}
      label();
    });
    system.addEventListener("change", label);
    label();
    button.hidden = false;
  })();
"#;

#[cfg(test)]
mod tests {
    use super::{StartTheme, page};
    use crate::scene::Scene;

    fn fonts() -> Scene {
        let json = std::fs::read_to_string("tests/fixtures/scenes/fonts.excalidraw").unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn the_page_carries_the_drawing_its_fonts_and_the_scene() {
        let scene = fonts();
        let html = page(
            &scene,
            "Fonts <& co>",
            "fonts.excalidraw",
            StartTheme::System,
        );
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

    #[test]
    fn a_hidden_button_switches_themes_that_override_the_system_setting() {
        let html = page(&fonts(), "Fonts", "fonts.excalidraw", StartTheme::System);
        assert!(html.contains("<html lang=\"en\">"), "follows the system");
        assert!(html.contains(
            r#"<button type="button" id="theme" hidden aria-label="Switch to dark mode">"#
        ));
        // Explicit choices win over the media query both ways.
        assert!(html.contains(r#":root[data-theme="dark"] figure svg { filter: invert(93%)"#));
        assert!(html.contains(r#":root:not([data-theme="light"]) figure svg"#));
        // The saved choice is read before the body, and storage may be blocked.
        let head = &html[..html.find("<body>").unwrap()];
        assert!(head.contains("try {\n    var theme = localStorage.getItem(\"roughdraft-theme\")"));
        assert!(html.contains(
            "try { localStorage.setItem(\"roughdraft-theme\", next); } catch (error) {}"
        ));
        let dark = page(&fonts(), "Fonts", "fonts.excalidraw", StartTheme::Dark);
        assert!(dark.contains(r#"<html lang="en" data-theme="dark">"#));
    }
}
