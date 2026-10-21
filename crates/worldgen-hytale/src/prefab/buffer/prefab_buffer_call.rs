// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/buffer/PrefabBufferCall.java`
//! Java `PrefabBufferCall` — the context of a prefab buffer traversal.

use std::cell::RefCell;

use hytale_math::fast_random::FastRandom;

use crate::prefab::prefab_rotation::PrefabRotation;

/// Java `public class PrefabBufferCall`.
///
/// The Java class carries the random source and the rotation of the
/// traversal; the concrete world-generator subclass
/// ([`PrefabPasteBuffer`](crate::prefab::prefab_paste_util::PrefabPasteBuffer))
/// adds the paste state.
///
/// # Why the random source is a `RefCell`
///
/// The Java traversal is **re-entrant**: the child-prefab consumer calls
/// back into the traversal (`PrefabPasteUtil.generate0` → `forEach` → the
/// child consumer → `generate0` → …). The random source is therefore
/// reached through interior mutability, exactly like the Java field (which
/// is simply a mutable field of the shared call object).
pub trait PrefabBufferCall {
    /// Java `public Random random` — the random source of the traversal.
    fn call_random(&self) -> &RefCell<FastRandom>;

    /// Java `public PrefabRotation rotation` — the rotation of the
    /// traversal.
    fn call_rotation(&self) -> PrefabRotation;
}

/// The plain Java `PrefabBufferCall` (random + rotation).
#[derive(Debug, Clone)]
pub struct PrefabBufferCallState {
    /// Java `public Random random`.
    pub random: RefCell<FastRandom>,
    /// Java `public PrefabRotation rotation`.
    pub rotation: PrefabRotation,
}

impl PrefabBufferCallState {
    /// Java `new PrefabBufferCall(Random random, PrefabRotation rotation)`.
    pub fn new(random: FastRandom, rotation: PrefabRotation) -> Self {
        Self {
            random: RefCell::new(random),
            rotation,
        }
    }
}

impl PrefabBufferCall for PrefabBufferCallState {
    fn call_random(&self) -> &RefCell<FastRandom> {
        &self.random
    }

    fn call_rotation(&self) -> PrefabRotation {
        self.rotation
    }
}
