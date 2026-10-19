//! Trivial `short` arithmetic with Java's silent-overflow semantics.
//!
//! Ported from: `HytaleServer/Math/src/main/java/com/hypixel/hytale/math/util/NumberUtil.java`

/// Java `NumberUtil.sum(short, short)`: `(short)(a + b)`.
pub fn sum(a: i16, b: i16) -> i16 {
    a.wrapping_add(b)
}

/// Java `NumberUtil.subtract(short, short)`: `(short)(a - b)`.
pub fn subtract(a: i16, b: i16) -> i16 {
    a.wrapping_sub(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_like_java_short_cast() {
        assert_eq!(sum(1, 2), 3);
        // Java: (short)(32767 + 1) == -32768
        assert_eq!(sum(i16::MAX, 1), i16::MIN);
        assert_eq!(subtract(i16::MIN, 1), i16::MAX);
    }
}
