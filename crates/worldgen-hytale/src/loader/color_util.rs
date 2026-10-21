// Ported from: `.hytale-source/.../worldgen/loader/util/ColorUtil.java`
//! Java `loader.util.ColorUtil`.

/// Java `ColorUtil`.
pub struct ColorUtil;

impl ColorUtil {
    /// Java `static int hexString(String s)` — parses `#rrggbb` / `0xrrggbb`
    /// and masks to 24 bits.
    pub fn hex_string(s: &str) -> i32 {
        let stripped = s.replace('#', "").replace("0x", "");
        i32::from_str_radix(&stripped, 16).unwrap() & 0xFF_FFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_string_parses_and_masks() {
        assert_eq!(ColorUtil::hex_string("#ff0000"), 0xFF0000);
        assert_eq!(ColorUtil::hex_string("#6a59c1"), 0x6A59C1);
        assert_eq!(ColorUtil::hex_string("0xffffff"), 0xFFFFFF);
        // Java `Integer.parseInt(s, 16)` overflows past 7 hex digits — only
        // 24-bit values are valid inputs (the mask is the documented
        // behavior for the 24-bit range).
        assert_eq!(ColorUtil::hex_string("#ffffff"), 0xFFFFFF);
    }
}
