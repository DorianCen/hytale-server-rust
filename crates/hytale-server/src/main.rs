//! `hytale-server` — CoreServer rewrite binary.
//!
//! Startup wiring for the asset registry, universe, network listener, auth
//! providers, command framework, and world threads. The binary owns no format
//! or gameplay logic of its own; it composes the library crates.

fn main() {
    println!("hytale-server {}", env!("CARGO_PKG_VERSION"));
}
