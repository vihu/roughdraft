//! Rows and columns: children side by side or stacked, `gap` apart, lined
//! up across by `align`; groups nest.
use std::collections::HashMap;

/// An item of the layout tree: an element by id, or a group.
#[derive(Debug)]
pub(super) enum Node {
    Element(String),
    Group(Group),
}

/// A row or column.
#[derive(Debug)]
pub(super) struct Group {
    pub(super) axis: Axis,
    pub(super) gap: f64,
    pub(super) align: Align,
    /// Top-left of a top-level group; nested groups are placed by theirs.
    pub(super) at: Option<[f64; 2]>,
    pub(super) children: Vec<Node>,
}

/// Which way a group runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Axis {
    /// Left to right.
    Row,
    /// Top to bottom.
    Column,
}

/// Where children line up across a group: top, middle or bottom of a row;
/// left, centre or right of a column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Align {
    Start,
    Center,
    End,
}

/// Places `node` with its top-left at `at`, recording each element's new
/// top-left in `places`. `boxes` holds each element's width and height.
pub(super) fn place(
    node: &Node,
    at: [f64; 2],
    boxes: &HashMap<String, [f64; 2]>,
    places: &mut HashMap<String, [f64; 2]>,
) {
    match node {
        Node::Element(id) => {
            places.insert(id.clone(), at);
        }
        Node::Group(group) => {
            let (along, across) = match group.axis {
                Axis::Row => (0, 1),
                Axis::Column => (1, 0),
            };
            let extent = size(node, boxes);
            let mut cursor = at[along];
            for child in &group.children {
                let child_size = size(child, boxes);
                let spare = extent[across] - child_size[across];
                let offset = match group.align {
                    Align::Start => 0.0,
                    Align::Center => spare / 2.0,
                    Align::End => spare,
                };
                let mut child_at = [0.0; 2];
                child_at[along] = cursor;
                child_at[across] = at[across] + offset;
                place(child, child_at, boxes, places);
                cursor += child_size[along] + group.gap;
            }
        }
    }
}

/// Width and height of a node: an element's box, or a group's children
/// with the gaps between them.
fn size(node: &Node, boxes: &HashMap<String, [f64; 2]>) -> [f64; 2] {
    match node {
        Node::Element(id) => boxes.get(id).copied().unwrap_or([0.0, 0.0]),
        Node::Group(group) => {
            let (along, across) = match group.axis {
                Axis::Row => (0, 1),
                Axis::Column => (1, 0),
            };
            let sizes: Vec<[f64; 2]> = group.children.iter().map(|c| size(c, boxes)).collect();
            let mut extent = [0.0; 2];
            extent[along] = sizes.iter().map(|s| s[along]).sum::<f64>()
                + group.gap * sizes.len().saturating_sub(1) as f64;
            extent[across] = sizes.iter().map(|s| s[across]).fold(0.0, f64::max);
            extent
        }
    }
}
