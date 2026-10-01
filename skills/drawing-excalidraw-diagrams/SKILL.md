---
name: drawing-excalidraw-diagrams
description: Draws hand-drawn Excalidraw diagrams (architecture, request flows, pipelines, component maps) as .excalidraw files with the roughdraft CLI, which builds a complete file from a short JSON skeleton, checks it for layout faults, renders a PNG to look at, and exports SVG or a self-contained web page to share. Use when asked for an Excalidraw diagram, a .excalidraw file, or a hand-drawn or whiteboard-style diagram, or to check or fix an existing .excalidraw file.
license: MIT
compatibility: Requires the roughdraft CLI with the build, check and render commands on PATH. Works offline; no browser.
---

# Drawing Excalidraw diagrams

`roughdraft` turns a short JSON skeleton into a complete Excalidraw 0.18
file: text measured in the real fonts, labels fitted to their shapes,
arrows bound to shapes on both sides. Never write `.excalidraw` JSON by
hand for a new diagram.

Run `roughdraft help` first. If it is not found, or it has no `check` and
`render`, stop and ask the user to install roughdraft
(https://github.com/vihu/roughdraft).

## The loop

1. Plan on paper: the boxes, the arrows between them, one reading
   direction.
2. Write `<name>.skeleton.json` in a temporary directory, or next to the
   diagram when the user wants to keep it (later changes are then an edit
   and a rebuild).
3. `roughdraft build <name>.skeleton.json -o <name>.excalidraw`
   Exit 2 means bad input: fix every entry of `errors` (each has a `path`
   into the skeleton). Read `warnings` too.
4. `roughdraft check <name>.excalidraw`
   Fix every problem in the skeleton and build again. Errors are always
   wrong. In a diagram you built, warnings are wrong too.
5. `roughdraft render <name>.excalidraw -o <tmp>/<name>.png`
   Look at the PNG. Fix crowding, long detours, labels that are hard to
   read, and anything that does not say what was asked. The PNG is for
   you: deliver it only when asked.
6. Repeat 3 to 5 until `check` prints `"problems": []` and the PNG reads
   well.

All three commands print JSON on stdout and never prompt. `render` takes
`--dark` (Excalidraw's dark mode, PNG and SVG) and `--scale N` (PNG,
default 2); see Sharing for its formats.

## The skeleton

A JSON array of items. A complete example:

```json
[
  {"type": "text", "id": "title", "x": 0, "y": -90, "text": "Checkout", "fontSize": 28},
  {"type": "row", "x": 0, "y": 0, "gap": 200, "children": [
    {"type": "rectangle", "id": "web", "width": 180, "height": 80,
     "label": "Web app", "backgroundColor": "#a5d8ff", "roundness": {"type": 3}},
    {"type": "rectangle", "id": "api", "width": 180, "height": 80,
     "label": "Orders API", "backgroundColor": "#b2f2bb", "roundness": {"type": 3}},
    {"type": "column", "gap": 60, "children": [
      {"type": "rectangle", "id": "db", "width": 180, "height": 80, "label": "Postgres"},
      {"type": "rectangle", "id": "queue", "width": 180, "height": 80, "label": "Email queue"}
    ]}
  ]},
  {"type": "arrow", "id": "web-api", "start": {"id": "web"}, "end": {"id": "api"},
   "label": {"text": "POST /orders", "fontSize": 16}},
  {"type": "arrow", "id": "api-db", "start": {"id": "api"}, "end": {"id": "db"}},
  {"type": "arrow", "id": "api-queue", "start": {"id": "api"}, "end": {"id": "queue"},
   "strokeStyle": "dashed"}
]
```

- Types: `rectangle`, `ellipse`, `diamond`, `text`, `arrow`, `line`,
  `frame`, and the layout groups `row` and `column`.
- Give each shape a short `id` that names it (`api`, `db`). Arrows name
  shapes in `start` and `end` and are placed between them, ends on the
  outlines. Give arrows ids too (`web-api`): `check` names elements by id.
- `"label": "Orders API"` puts centred text (size 20) in a shape or on an
  arrow. Use `\n` for a line break. A label wraps to the room inside its
  shape (a rectangle's `width` less 10; less in ellipses and diamonds), even
  inside a word, so break long paths and identifiers yourself. The shape
  grows taller to fit, and `build` warns when that overrides your `height`
  or your line breaks.
- Size every shape. Without `width` a labelled shape hugs its label, and
  boxes of one row come out different sizes. Boxes: 160 to 220 wide, 70
  high for one or two lines of label, plus 25 for each line more. Give
  shapes of one role, and the items of one row, the same size.
- `row` lays its children out left to right, `column` top to bottom, in
  order: `gap` (default 40) between them, `align` `start`, `center`
  (default) or `end` across. Groups nest: for a different gap between some
  children, nest a row in the row. Only top-level items take `x` and `y`.
  Arrows and lines go outside groups. For a layout rows and columns cannot
  express, place top-level items at `x`/`y` yourself.
- Style fields pass through as Excalidraw element fields:
  `strokeColor`, `backgroundColor`, `fillStyle` (`solid`, `hachure`,
  `cross-hatch`), `strokeWidth` (1, 2, 4), `strokeStyle` (`solid`,
  `dashed`, `dotted`), `roughness` (0 neat, 1 default, 2 rough),
  `roundness: {"type": 3}` for rounded corners, `opacity` (0 to 100).
- Fonts: `fontFamily` 5 hand-drawn (default), 6 plain (Nunito), 8 code.
  Other ids are not bundled. On a shape or arrow, font fields go in the
  label: `"label": {"text": "API", "fontFamily": 6, "fontSize": 16}`.
- Free text does not wrap: break long text with `\n`.

Arrows with bends, frames, arrowheads and every field:
[references/skeleton.md](references/skeleton.md).

## Layout that reads well

- One direction: left to right for requests and pipelines, top to bottom
  for hierarchies and layers. Arrows point along it.
- Peers in a row, stacks in a column; a column inside a row fans out.
- Leave room for arrow labels: a `gap` of 40 fits a bare arrow; with a
  label, make the gap the label's width plus 80. Arrow labels read best at
  `"fontSize": 16` (`"label": {"text": "...", "fontSize": 16}`). `check`
  reports `arrow-too-short` with how much further apart the shapes go.
- One arrow per pair of shapes. For a request and its reply, give one
  arrow `"startArrowhead": "arrow"`; two arrows between the same shapes
  lie on top of each other (`check`: `arrows-stacked`).
- Connect neighbours. An arrow that must pass a shape gets `points` to go
  around it, or the shapes move (`check` reports `arrow-crosses`). `build`
  prints where every shape landed (`shapes`); with `"x": 0, "y": 0` an
  arrow's `points` are in those coordinates.
- 4 to 12 shapes per diagram. Split bigger subjects into several diagrams.
- A title as free text above the first row, `fontSize` 28, about 90 above
  it. Put a note (free text, `fontSize` 16) in a row or column with what
  it explains: `{"type": "row", "gap": 20, "children": [shape, note]}`.
- Group related shapes in a `frame` with a `name`, rather than a big
  rectangle drawn behind them. The name sits 25 above the frame: leave 60
  between stacked frames, and bring arrows into a frame from the side, not
  down over its name. Arrows cannot end on a frame: to point a group at
  one target, draw an arrow from each member.

## Colours

Excalidraw's palette. Keep strokes and text `#1e1e1e` and use fills for
meaning, one fill per kind of thing:

| Use | Fill | Stroke |
| --- | --- | --- |
| blue: clients, entry points | `#a5d8ff` | `#1971c2` |
| green: services, success | `#b2f2bb` | `#2f9e44` |
| yellow: storage, state | `#ffec99` | `#f08c00` |
| red: errors, danger | `#ffc9c9` | `#e03131` |
| violet: external systems | `#d0bfff` | `#6741d9` |
| grey: infrastructure, muted | `#e9ecef` | `#868e96` |

Coloured strokes suit arrows and frames that carry a meaning (a failure
path in red). Fills stay light so text on them reads. Colours are stored
for light mode; `render --dark` shows how Excalidraw's dark mode shows them.

## What check reports

| Kind | Fix in the skeleton |
| --- | --- |
| `label-overflow` | widen or heighten the shape, or shorten the label |
| `arrow-too-short` | raise the `gap` between its shapes by the amount given, or shorten the label |
| `arrow-label-overlap` | move the shape or the arrow the label covers |
| `arrows-stacked` | one arrow with `startArrowhead`, or route one with `points` |
| `overlap` | move one shape, raise a `gap`, or put one wholly inside the other |
| `arrow-crosses` | move what it crosses (a shape, a frame, a frame's name), or route it around with `points` |
| `text-size`, `binding-*`, `arrow-end-off-shape`, `zero-size`, `duplicate-id`, `empty-text` | files written by hand; built files do not have them. Rebuild. |
| `font-not-bundled` | use `fontFamily` 5, 6 or 8 |

Each problem names its elements in `ids`. A label's id is its shape's id
plus `-label`.

## Sharing

`render` picks the format from the output's extension. Choose by where the
diagram goes:

- `.svg`: Markdown, READMEs and docs sites. Sharp at any zoom, fonts inside.
- `.png`: chat, email and slides.
- `.html`: a web page for any static host. One self-contained file with the
  drawing, light and dark mode, and a link to download the `.excalidraw`;
  `--title` sets its heading.
- The `.excalidraw` itself, for anyone who will edit it.

Never publish on your own. When asked to publish, hand the file to the
tool the user has for it (a publishing skill, `gh`, a deploy script).

## An existing .excalidraw file

`check` and `render` work on any Excalidraw file. With a skeleton next to
it, change the skeleton and rebuild. Without one, edit the JSON in place
only for colours, styles and moves, keeping every `id`; move a shape's
label and bound arrows with it (or rebuild). For new text or new shapes,
write a skeleton that reproduces the diagram, reusing its ids, and build
it. Run `check` and `render` after any edit.
