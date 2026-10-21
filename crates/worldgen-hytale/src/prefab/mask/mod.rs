// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/mask/**`
//! Java `com.hypixel.hytale.server.core.prefab.selection.mask` package — the
//! block masks of the interactive editor and of the prefab placement.
//!
//! | Java class       | Rust module                |
//! |------------------|----------------------------|
//! | `BlockMask`      | [`block_mask`]             |
//! | `MultiBlockMask` | [`multi_block_mask`]       |
//! | `BlockFilter`    | [`block_filter`]           |
//! | `BlockPattern`   | [`block_pattern`]          |

pub mod block_filter;
pub mod block_mask;
pub mod block_pattern;
pub mod multi_block_mask;

pub use block_filter::{BlockFilter, BlockReader, FilterType};
pub use block_mask::BlockMask;
pub use block_pattern::BlockPattern;
pub use multi_block_mask::MultiBlockMask;
