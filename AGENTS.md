# Rust Style Guidance

Condensed from the Google Rust Style Guide, keeping only rules that apply to a standalone Cargo project. Rules specific to Chromium and Fuchsia are intentionally left out.

**Precedence:** the project's existing conventions and lint config win. Match surrounding code. Follow the official Rust Style Guide (rustfmt) and Rust API Guidelines where this file is silent. Run `cargo fmt` and `cargo clippy` before finishing.

**Library vs. application code:** API-design rules below are binding for code meant to be reused by other crates. In code used by a single binary, treat them as strong recommendations and favor simplicity over generality. Don't generalize prematurely.

## Ownership and borrowing
- Prefer unique ownership and borrows over `Rc`/`Arc`. Use them only when the borrow structure would be unreasonably complex. Be especially wary when porting Java: don't translate every object reference into a shared pointer; redesign the ownership graph (e.g. IDs or indices into a central store).
- Don't use `.clone()` just to silence the borrow checker. Clone when a deep copy is genuinely needed, or when borrowing relationships get excessively complex. Cloning an `Rc`/`Arc` is fine.
- Accept the most flexible borrowed type for read-only inputs: `&T` not `&Box<T>`, `&[T]` not `&Vec<T>`, `&str` not `&String`, `Option<&T>` not `&Option<T>`.

## Function inputs and outputs
- Library code: prefer returning iterators over `Vec<T>`, and accepting `impl IntoIterator` over slices when you only iterate. In application code, `Vec` and slices are fine when simpler.
- Use `impl Into<T>` parameters only in library APIs where call-site ergonomics matter. Application code should almost never use them.
- Use an `enum` instead of `bool` parameters when the meaning isn't obvious at the call site. If you keep a `bool`, add an inline comment.
- Error reporting:
  - `Option` for simple failures with an obvious cause.
  - `Result` for recoverable failures where the caller needs to know why (I/O, network, malformed external input).
  - `panic!` only for bugs and violated invariants, never for expected failures.

## Types and traits
- Use a newtype instead of a type alias when the type has distinct semantics, needs its own API, or is easy to mix up (e.g. entity IDs, chunk vs. block coordinates).
- Prefer structs with named fields over tuples and multi-field tuple structs. Single-field newtypes are fine.
- Implement standard traits instead of custom methods (`FromStr`, `From`, `Default`, `Display`). Use `#[derive]` whenever possible.
- Derive or implement `Clone` for most types, `Copy` when the type will clearly stay `Copy`, and `Hash` + `Eq` for likely map keys. Error types should be `Copy` when possible.
- Implement `PartialEq`/`PartialOrd` before `Eq`/`Ord` unless you actually need the total guarantees.
- Define a trait only for generic programming, `impl Trait`/`dyn Trait` parameters or returns, or extension traits. Otherwise use inherent `impl` blocks. Don't use traits just to organize code.
- In libraries, prefer generics over `dyn Trait` unless code size matters.
- Extension traits (traits that only add methods to a foreign type) should be named with an `Ext` suffix.
- Add trait impls when they become necessary in application code, but proactively in low-level library code.
- Pair an important `From` impl with an explicit constructor (`new`, `from_foo`).
- Options/config structs with all-public fields should implement `Default`.
- If you add `len()`, also add `is_empty()`.

## Code organization and imports
- Don't use glob imports. Exceptions: `use super::*` in test modules, `use Enum::*` inside enum-focused functions, and `pub use submodule::*` re-exports. Don't define preludes.
- Import types and traits unqualified (`use regex::Regex`, then `Regex`). Qualify free functions with their module (`mem::swap`, not a bare `swap`).
- Import traits inside function bodies unless used widely in the file.
- Name files and leaf modules after their primary type (`player.rs` for `Player`).
- Keep non-leaf modules to declarations and re-exports. Put implementations in leaf modules.
- Keep crates and modules focused on one abstraction. If a module is growing sprawling, split it.

## Naming
- Follow the standard library: `as_`, `to_`, `into_` prefixes with the matching `self` convention.
- Constructors are `new()`, `new_with_foo()`, or `try_new()` if they return `Result`. Conversions are `from_foo()`.
- Accessors: `foo()`, `foo_mut()`, `set_foo()`, `take_foo()`, `replace_foo()`, and `with_foo(|foo| ...)` for closure-based access (e.g. inside a `Mutex`).
- Give lifetimes descriptive names, except trivial input/output relationships (up to two single-letter lifetimes) and `'_`.
- Generic parameter names should be short and indicative of the role (`T` is fine for fully generic APIs).
- Avoid abbreviations unless they're widely known (`IpAddr` is fine). Don't drop letters from words.

## Implementation
- Use pattern matching instead of check-then-unwrap (`if let Some(x) = opt`, not `if opt.is_some() { opt.unwrap() }`).
- If a panic is truly necessary, use `expect("reason the invariant holds")` instead of `unwrap()`.
- Add context strings to `assert!`s that explain why the condition should hold or print the offending value. Don't merely restate the condition.
- Don't overuse type inference. Annotate variable types for `.into()`, `.parse()`, `.collect()` and complex iterator chains. Prefer annotating the variable over a turbofish.
- Prefer a `for` loop when an iterator chain is long, has conditionals, or mutates outside state. Don't build chains in a `for` header (assign them to a named variable first). Avoid `Iterator::for_each` without good reason. Short chains and `any`/`all`/`sum`-style combinators are good.
- Use `format!`/`write!` for concatenating more than two strings or substituting into a literal, instead of `+` or `str::replace`.
- Don't call `panic::catch_unwind`.
- Don't run code before `main()` (no `ctor`).
- Don't use unstable features.
- Don't use `#[doc(hidden)]` items from crates you don't own.
- Avoid operator overloading for non-mathematical purposes.

## Enums, structs, and attributes
- For public enums and public structs with public fields that other crates will use, add `#[non_exhaustive]`. Skip it for fixed domains (e.g. `Red/Green/Blue`) or types used in only a few places.
- When matching on a `non_exhaustive` enum, list the variants you care about and use `_` (or `if let`) for the rest.
- Use `#[must_use]` on functions whose return value must not be ignored (pure functions, error/critical-info returns) and on types meant to be returned by them (error types, lazy computations). `Result` is already covered.
- Mark a public function `const` only if it will stay `const`.

## Macros
Prefer in order: plain functions/types/traits, then `macro_rules!`, then other codegen (build scripts), then proc macros. Macros hurt tooling and readability, so use them only when the benefit is clear.

## Unsafe code
- Don't write `unsafe` unless there is no safe way to do it.
- If you must: encapsulate it behind a safe abstraction, isolate it from business logic, and put a `// SAFETY:` comment on each block explaining why it's sound. If the reason is performance, point to benchmarks.
- Document the invariants of `unsafe` functions and traits.

## Documentation
- Document public APIs with `///` (items) and `//!` (module/crate). Use Markdown.
- Document panic conditions, and include examples for important functionality.
- Comments should say something the code doesn't already say. Skip comments that only restate the function name. Don't explain basic Rust.
- Use `//` for implementation notes and TODOs, and `///` only for public-facing documentation. Mixing both on one item is fine.
- Variant docs can be omitted when names are self-explanatory.

## Formatting
- Use `rustfmt` and don't hand-format. Opt out with `#[rustfmt::skip]` only for deliberate alignment (e.g. matrix literals) or generated code, never for personal taste.
