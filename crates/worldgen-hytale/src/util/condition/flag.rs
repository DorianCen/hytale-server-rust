// Ported from: `.hytale-source/.../worldgen/util/condition/flag/*.java`
//! Java `com.hypixel.hytale.server.worldgen.util.condition.flag` package —
//! int conditions whose result is a set of bit flags.
//!
//! | Java class            | Rust item                   |
//! |-----------------------|-----------------------------|
//! | `Int2FlagsCondition`  | [`Int2FlagsCondition`]      |
//! | `ConstantInt2Flags`   | [`ConstantInt2Flags`]       |
//! | `FlagOperator`        | [`FlagOperator`]            |
//! | `CompositeInt2Flags`  | [`CompositeInt2Flags`]      |

use std::rc::Rc;

use procedural::noise::condition::IIntCondition;

/// Java `interface Int2FlagsCondition extends IntUnaryOperator` — an
/// int condition that returns one or more boolean values encoded as an int.
pub trait Int2FlagsCondition {
    /// Java `int eval(int input)`.
    fn eval(&self, input: i32) -> i32;
}

/// Java `class ConstantInt2Flags implements Int2FlagsCondition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstantInt2Flags {
    /// Java `private final int result`.
    pub result: i32,
}

impl ConstantInt2Flags {
    /// Java `ConstantInt2Flags(int result)`.
    pub const fn new(result: i32) -> Self {
        Self { result }
    }
}

impl Int2FlagsCondition for ConstantInt2Flags {
    /// Java `int eval(int input)`.
    #[inline]
    fn eval(&self, _input: i32) -> i32 {
        self.result
    }
}

/// Java `enum FlagOperator implements IntBinaryOperator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagOperator {
    /// Java `FlagOperator.And` — `output & flags`.
    And,
    /// Java `FlagOperator.Or` — `output | flags`.
    Or,
    /// Java `FlagOperator.Xor` — `output ^ flags`.
    Xor,
}

impl FlagOperator {
    /// Java `int apply(int output, int flags)`.
    pub const fn apply(self, output: i32, flags: i32) -> i32 {
        match self {
            FlagOperator::And => output & flags,
            FlagOperator::Or => output | flags,
            FlagOperator::Xor => output ^ flags,
        }
    }
}

/// Java `class CompositeInt2Flags implements Int2FlagsCondition`.
pub struct CompositeInt2Flags {
    /// Java `private final FlagCondition[] flags`.
    pub flags: Vec<FlagCondition>,
    /// Java `private final int defaultResult`.
    pub default_result: i32,
}

impl CompositeInt2Flags {
    /// Java `CompositeInt2Flags(int defaultResult, FlagCondition[] flags)`.
    pub fn new(default_result: i32, flags: Vec<FlagCondition>) -> Self {
        Self {
            flags,
            default_result,
        }
    }
}

impl Int2FlagsCondition for CompositeInt2Flags {
    /// Java `int eval(int input)` — the flags are evaluated in order, each one
    /// receiving the output of the previous one.
    fn eval(&self, input: i32) -> i32 {
        let mut output = self.default_result;
        for flag in &self.flags {
            output = flag.eval(input, output);
        }
        output
    }
}

/// Java `static class CompositeInt2Flags.FlagCondition implements IntBinaryOperator`.
pub struct FlagCondition {
    /// Java `private final IIntCondition condition`.
    pub condition: Rc<dyn IIntCondition>,
    /// Java `private final FlagOperator operator`.
    pub operator: FlagOperator,
    /// Java `private final int flags`.
    pub flags: i32,
}

impl FlagCondition {
    /// Java `FlagCondition(IIntCondition condition, FlagOperator operator, int
    /// flags)`.
    pub fn new(condition: Rc<dyn IIntCondition>, operator: FlagOperator, flags: i32) -> Self {
        Self {
            condition,
            operator,
            flags,
        }
    }

    /// Java `int eval(int input, int output)`.
    pub fn eval(&self, input: i32, output: i32) -> i32 {
        if self.condition.eval(input) {
            return self.operator.apply(output, self.flags);
        }
        output
    }

    /// Java `int applyAsInt(int input, int output)` (the `IntBinaryOperator`
    /// method).
    pub fn apply_as_int(&self, input: i32, output: i32) -> i32 {
        self.eval(input, output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::condition::ConstantIntCondition;

    #[test]
    fn constant_flags_ignore_the_input() {
        let constant = ConstantInt2Flags::new(0b1010);
        assert_eq!(constant.eval(0), 0b1010);
        assert_eq!(constant.eval(12345), 0b1010);
    }

    #[test]
    fn flag_operators_apply_the_bitwise_operation() {
        assert_eq!(FlagOperator::And.apply(0b1100, 0b1010), 0b1000);
        assert_eq!(FlagOperator::Or.apply(0b1100, 0b1010), 0b1110);
        assert_eq!(FlagOperator::Xor.apply(0b1100, 0b1010), 0b0110);
    }

    #[test]
    fn composite_flags_start_from_the_default_result() {
        let composite = CompositeInt2Flags::new(0b0001, Vec::new());
        assert_eq!(composite.eval(0), 0b0001);
    }

    #[test]
    fn composite_flags_are_applied_in_order() {
        let composite = CompositeInt2Flags::new(
            0b0000,
            vec![
                FlagCondition::new(
                    Rc::new(ConstantIntCondition::DEFAULT_TRUE),
                    FlagOperator::Or,
                    0b0001,
                ),
                FlagCondition::new(
                    Rc::new(ConstantIntCondition::DEFAULT_TRUE),
                    FlagOperator::Or,
                    0b0010,
                ),
                FlagCondition::new(
                    Rc::new(ConstantIntCondition::DEFAULT_FALSE),
                    FlagOperator::Or,
                    0b0100,
                ),
            ],
        );
        assert_eq!(composite.eval(0), 0b0011);
    }

    #[test]
    fn flag_condition_only_applies_when_the_condition_matches() {
        let condition = FlagCondition::new(
            Rc::new(ConstantIntCondition::DEFAULT_FALSE),
            FlagOperator::Or,
            0b1000,
        );
        assert_eq!(condition.eval(0, 0b0001), 0b0001);
        assert_eq!(condition.apply_as_int(0, 0b0001), 0b0001);

        let condition = FlagCondition::new(
            Rc::new(ConstantIntCondition::DEFAULT_TRUE),
            FlagOperator::Or,
            0b1000,
        );
        assert_eq!(condition.eval(0, 0b0001), 0b1001);
    }
}
