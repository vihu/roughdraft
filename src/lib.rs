#![doc = include_str!("../README.md")]
#![deny(unsafe_code, missing_docs, rustdoc::broken_intra_doc_links)]

mod base64;
pub mod build;
pub mod check;
pub mod color;
pub mod edit;
pub mod fonts;
pub mod geometry;
pub mod history;
pub mod hit;
pub mod html;
mod random;
pub mod render;
pub mod scene;
pub mod svg;
pub mod widget;
