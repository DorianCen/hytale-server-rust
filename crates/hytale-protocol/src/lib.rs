//! `hytale-protocol` — Hytale wire protocol types and codecs.
//!
//! This crate is the single owner of the wire format: the hand-written
//! primitives ([`io`]), packet framing and compression ([`frame`]), the generated
//! packet/struct/enum types, and the packet registry. It is a leaf crate and may
//! only depend on wire-format libraries such as `bytes` and `zstd`; it never
//! reaches into the asset, world, or network layers.
//!
//! Generated sources are produced by `tools/protocol-gen` from the protocol IR and
//! are write-only; edit the IR or the generator instead of the generated files.

#![deny(clippy::all)]

pub mod frame;
pub mod generated;
pub mod io;

pub use frame::{
    compress_payload, decode_frame, decompress_payload, encode_frame, Frame, MAX_PACKET_SIZE,
    PACKET_HEADER_SIZE,
};
pub use io::{NullableBitset, ProtocolError, Reader, Uuid, Writer};
