# Skeleton reference

The input to `roughdraft build`: Excalidraw's element skeleton
(`convertToExcalidrawElements`), with ids kept, string labels, arrows
placed between shapes, and `row` / `column` groups.

## Contents

- Top level
- Fields every element takes
- Shapes: `rectangle`, `ellipse`, `diamond`
- `text`
- `arrow` and `line`
- `frame`
- `row` and `column`
- What build writes
- Build errors and warnings

## Top level

A JSON array of items, or an object:

```json
{"elements": [...], "appState": {"viewBackgroundColor": "#ffffff"}}
```

`appState` is written as given; without it the file gets a white
background and a grid size of 20.

## Fields every element takes

| Field | Default | Notes |
| --- | --- | --- |
| `type` | required | `rectangle`, `ellipse`, `diamond`, `text`, `arrow`, `line`, `frame`, `row`, `column` |
| `id` | random | unique; arrows and frames refer to it |
| `x`, `y` | 0, 0 | top-left; ignored inside a row or column (with a warning) |
| `strokeColor` | `#1e1e1e` | |
| `backgroundColor` | `transparent` | |
| `fillStyle` | `solid` | `solid`, `hachure`, `cross-hatch`, `zigzag` |
| `strokeWidth` | 2 | 1 thin, 2 bold, 4 extra bold |
| `strokeStyle` | `solid` | `solid`, `dashed`, `dotted` |
| `roughness` | 1 | 0 architect, 1 artist, 2 cartoonist |
| `roundness` | none (sharp) | `{"type": 3}` rounds a rectangle's corners; `{"type": 2}` rounds a diamond's, and curves lines and arrows through their points |
| `opacity` | 100 | 0 to 100 |
| `angle` | 0 | radians; `check` does not test rotated elements for overlaps |
| `groupIds` | `[]` | elements sharing a group id select and move together |
| `link` | none | a URL |

Any other field is copied into the element as is.

## Shapes: `rectangle`, `ellipse`, `diamond`

| Field | Notes |
| --- | --- |
| `width`, `height` | default 100 × 100; with a `label` and no `width`, the shape fits the label |
| `label` | `"text"` or `{"text", "fontSize", "fontFamily", "strokeColor", "textAlign", "verticalAlign"}`; `fontSize` 20 and `fontFamily` 5 unless given |

A label wraps to the room inside the shape (less inside ellipses and
diamonds), and the shape grows taller when the wrapped label needs it. It
is centred (`textAlign` `left`/`center`/`right`, `verticalAlign`
`top`/`middle`/`bottom`).

## `text`

Free text: `{"type": "text", "x": 0, "y": -90, "text": "Title", "fontSize": 28}`.

| Field | Default | Notes |
| --- | --- | --- |
| `text` | required | `\n` breaks lines; free text never wraps |
| `fontSize` | 20 | 16 small, 20 medium, 28 large, 36 very large |
| `fontFamily` | 5 | 5 Excalifont (hand-drawn), 6 Nunito (plain), 8 Comic Shanns (code); 1 Virgil and 7 Lilita One also bundled |
| `textAlign` | `left` | |

Width and height are measured; a given `width` is replaced.

## `arrow` and `line`

Between two shapes:

```json
{"type": "arrow", "id": "r1", "start": {"id": "api"}, "end": {"id": "db"}, "label": "SQL"}
```

- `start` and `end` name a rectangle, ellipse, diamond or text. The arrow
  runs from one centre towards the other; each end stops 6 units short of
  the outline and is bound there, so it follows the shape when moved in
  Excalidraw.
- Both ends, or `points`. An arrow from a shape to itself needs `points`.
- `startArrowhead` (default none) and `endArrowhead` (default `arrow`):
  `arrow`, `bar`, `dot`, `circle`, `circle_outline`, `triangle`,
  `triangle_outline`, `diamond`, `diamond_outline`, `crowfoot_one`,
  `crowfoot_many`, `crowfoot_one_or_many`, or `null`. A connector without a
  head is an arrow with `"endArrowhead": null`.
- `label` sits at the arrow's middle: on a straight arrow, halfway; with
  `points`, on the middle point when there is an odd number of points, and
  halfway along the middle segment when there is an even number. The
  stretch it sits on must be about 70 longer than the label, or the label
  hides the arrowhead (`check`: `arrow-too-short`).
- `start` and `end` cannot name a frame: point each member at the target.
- A request and its reply are one arrow with `"startArrowhead": "arrow"`,
  not two arrows (`check`: `arrows-stacked`).

With bends, give `points`, relative to the arrow's `x`, `y`. The easy
way: build once, read where the shapes landed in `shapes` of build's
output (`[x, y, width, height]` by id), then give the arrow `"x": 0,
"y": 0`, which makes its points scene coordinates:

```json
{"type": "arrow", "start": {"id": "c"}, "end": {"id": "a"}, "x": 0, "y": 0,
 "points": [[600, 35], [600, 150], [80, 150], [80, 35]], "label": "retry"}
```

Here `c` is `[520, 0, 160, 70]` and `a` is `[0, 0, 160, 70]`: the arrow
leaves `c` downwards, runs below the row and comes up into `a`. The first
and last points move onto the outlines (start them at the shapes'
centres); the middle points stay where the path turns, clear of other
shapes. Elbow (right-angle) routing is not automatic: place the corners
yourself.

A `line` takes `points` (default a 100-long horizontal line) and no
`start`, `end` or `label`.

## `frame`

```json
{"type": "frame", "id": "backend", "name": "Backend", "children": ["api", "db", "queue"]}
```

- `children`: ids of items inside it, notes included. Their labels come
  along.
- Without `x`, `y`, `width` and `height` it wraps its children with 10
  units to spare. The `name` is drawn in the 25 above its top-left corner:
  leave room there, and keep arrows from crossing it (`check`:
  `arrow-crosses`).
- Frames cut off what sticks out of them. Leave the children whole inside.
- Frames are not layout groups: put the children in a `row` or `column`
  and list the frame after it, at the top level.

## `row` and `column`

```json
{"type": "row", "x": 0, "y": 0, "gap": 40, "align": "center", "children": [...]}
```

| Field | Default | Notes |
| --- | --- | --- |
| `children` | required | shapes, text, frames, and nested rows and columns; not arrows or lines |
| `gap` | 40 | space between neighbouring children |
| `align` | `center` | `start`, `center` or `end`, across the direction: vertical in a row, horizontal in a column |
| `x`, `y` | 0, 0 | top-left of the group; top-level groups only |

Groups have no `id` and are not saved: only their children's positions
are. A row's `gap` is the same between all its children; for a different
gap somewhere, nest a row.

## What build writes

- Every field Excalidraw 0.18 writes for a new element, with fresh seeds.
- Labels as text elements with id `<shape id>-label`, bound both ways
  (`containerId` and the shape's `boundElements`).
- Arrow bindings on both sides (`startBinding`/`endBinding` with focus 0,
  gap 6, and the arrow in each shape's `boundElements`).
- Text sizes measured in the bundled fonts, as Excalidraw measures them.

stdout on success:

```json
{"ok": true, "output": "diagram.excalidraw", "elements": 12,
 "shapes": {"api": [260, 0, 180, 80], "db": [580, -60, 180, 80]}, "warnings": []}
```

`shapes` has every shape, free text and frame, rounded to whole units.

## Build errors and warnings

Errors (exit 2, nothing written) name a `path` such as
`elements[2].children[0].label`:

- unknown `type`, missing `text`, a label on text, a line or a frame
- duplicate `id`, or a label id (`<id>-label`) taken by another item
- `start`/`end` naming a missing item, or something that is not a shape or
  text
- an arrow without both ends and without `points`
- an arrow or line inside a row or column
- shapes that overlap so much that an arrow between them has no length

Warnings (the file is written):

- `x`/`y` ignored inside a row or column
- no `x`/`y` on a top-level item: placed at 0, 0
- a `fontFamily` that is not bundled
- `fontFamily` or `fontSize` on a shape or arrow: it goes in the label
- a label that made its shape taller than the given `height`, or re-wrapped
  lines broken by hand with `\n`
