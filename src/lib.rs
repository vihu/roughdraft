//! Excalidraw-compatible hand-drawn sketch canvas for iced.
//!
//! Reads, renders, edits and writes Excalidraw 0.18 scene JSON. Same `seed`
//! gives the same hand-drawn wobble as rough.js, so a scene looks identical
//! in web Excalidraw and here.
#![deny(unsafe_code, missing_docs, rustdoc::broken_intra_doc_links)]

pub mod scene;
