//! Excalidraw-compatible hand-drawn sketch canvas for iced.
//!
//! Reads, renders, edits and writes Excalidraw 0.18 scene JSON. Same `seed`
//! gives the same hand-drawn wobble as rough.js, so a scene looks identical
//! in web Excalidraw and here.
#![deny(unsafe_code, missing_docs, rustdoc::broken_intra_doc_links)]

mod base64;
pub mod color;
pub mod edit;
pub mod fonts;
pub mod geometry;
pub mod history;
pub mod hit;
mod random;
pub mod render;
pub mod scene;
pub mod svg;
pub mod widget;
