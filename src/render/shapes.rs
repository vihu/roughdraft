//! Rectangles and diamonds as rough.js drawables, with Excalidraw's corner
//! rounding (`scene/Shape.ts`).
use rough_rs::{Drawable, Generator};

use super::{corner_radius, rough_options};
use crate::scene::Element;

pub(super) fn rectangle(generator: &Generator, element: &Element) -> Drawable {
    let (w, h) = (element.base.width, element.base.height);
    match &element.base.roundness {
        Some(roundness) => {
            let r = corner_radius(w.min(h), roundness);
            let (wr, hr) = (w - r, h - r);
            let d = format!(
                "M {r} 0 L {wr} 0 Q {w} 0, {w} {r} L {w} {hr} Q {w} {h}, {wr} {h} L {r} {h} Q 0 {h}, 0 {hr} L 0 {r} Q 0 0, {r} 0"
            );
            generator.path(&d, Some(rough_options(element, true)))
        }
        None => generator.rectangle(0.0, 0.0, w, h, Some(rough_options(element, false))),
    }
}

pub(super) fn diamond(generator: &Generator, element: &Element) -> Drawable {
    let (w, h) = (element.base.width, element.base.height);
    // `getDiamondPoints`; the +1 keeps rough.js away from zero-length sides.
    let (top_x, top_y) = ((w / 2.0).floor() + 1.0, 0.0);
    let (right_x, right_y) = (w, (h / 2.0).floor() + 1.0);
    let (bottom_x, bottom_y) = (top_x, h);
    let (left_x, left_y) = (0.0, right_y);

    match &element.base.roundness {
        Some(roundness) => {
            let v = corner_radius((top_x - left_x).abs(), roundness);
            let h = corner_radius((right_y - top_y).abs(), roundness);
            let d = format!(
                "M {} {} L {} {} C {right_x} {right_y}, {right_x} {right_y}, {} {} L {} {} C {bottom_x} {bottom_y}, {bottom_x} {bottom_y}, {} {} L {} {} C {left_x} {left_y}, {left_x} {left_y}, {} {} L {} {} C {top_x} {top_y}, {top_x} {top_y}, {} {}",
                top_x + v,
                top_y + h,
                right_x - v,
                right_y - h,
                right_x - v,
                right_y + h,
                bottom_x + v,
                bottom_y - h,
                bottom_x - v,
                bottom_y - h,
                left_x + v,
                left_y + h,
                left_x + v,
                left_y - h,
                top_x - v,
                top_y + h,
                top_x + v,
                top_y + h,
            );
            generator.path(&d, Some(rough_options(element, true)))
        }
        None => {
            let points = [
                [top_x, top_y],
                [right_x, right_y],
                [bottom_x, bottom_y],
                [left_x, left_y],
            ];
            generator.polygon(&points, Some(rough_options(element, false)))
        }
    }
}
