//! OMNIC SPA parsing without changing the input or discarding unknown data.
//!
//! Directory and ordering rules follow OMNIC's native reader. Undecoded blocks,
//! padding and original floating-point bits remain available in the source bytes.
//! See `docs/spa-format.md` for supported behavior and verification boundaries.

mod export;
mod model;
mod parser;

pub use model::{
    Document, EmbeddedObject, Metadata, ObjectAttribute, ReadOptions, Section, Series, TextEncoding,
};
pub use parser::{Error, parse};
