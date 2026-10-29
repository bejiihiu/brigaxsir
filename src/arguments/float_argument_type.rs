//! The float argument type. Java: `com.mojang.brigadier.arguments.FloatArgumentType`.

use std::{
    any::Any,
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};

use super::ArgumentType;
use crate::{
    context::{CommandContext, ParsedValue},
    errors::{
        BuiltInExceptionProvider, CommandSyntaxException, built_in_exceptions, java_float_to_string,
    },
    string_reader::StringReader,
};

/// An argument parsing an `f32`, optionally bounded. Java:
/// `FloatArgumentType` — private fields and constructor, public static
/// factories. Construct it through [`float`], [`float_min`] or
/// [`float_range`].
///
/// Java's unbounded factory uses `-Float.MAX_VALUE` as the lower bound (not
/// `-f32::INFINITY`!) — the port keeps that.
///
/// Equality is Java's: two instances are equal when their bounds are equal.
/// [`PartialEq`] compares `minimum` and `maximum` with IEEE `==` — exactly
/// Java's `==` on `float` fields: `-0.0 == 0.0` compares equal and NaN never
/// compares equal, even to itself. [`Hash`] mirrors Java's `hashCode` formula
/// `(int) (31 * minimum + maximum)` — float arithmetic, then Java's
/// saturating cast (NaN to `0`, infinities to `i32::MIN`/`i32::MAX`), which
/// is also what Rust's float-to-int `as` cast does.
#[derive(Clone, Copy, Debug)]
pub struct FloatArgumentType {
    minimum: f32,
    maximum: f32,
}

impl FloatArgumentType {
    /// The lower bound. Java: `getMinimum`.
    pub fn get_minimum(&self) -> f32 {
        self.minimum
    }

    /// The upper bound. Java: `getMaximum`.
    pub fn get_maximum(&self) -> f32 {
        self.maximum
    }
}

impl ArgumentType for FloatArgumentType {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn type_equals(&self, other: &dyn ArgumentType) -> bool {
        match other.as_any().downcast_ref::<Self>() {
            Some(other) => self == other,
            None => false,
        }
    }

    fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
        let start = reader.cursor();
        let result = reader.read_float()?;
        if result < self.minimum {
            reader.set_cursor(start);
            return Err(built_in_exceptions().float_too_low().create_with_context(
                &*reader,
                result,
                self.minimum,
            ));
        }
        if result > self.maximum {
            reader.set_cursor(start);
            return Err(built_in_exceptions().float_too_high().create_with_context(
                &*reader,
                result,
                self.maximum,
            ));
        }
        Ok(Arc::new(result))
    }

    fn examples(&self) -> Vec<String> {
        EXAMPLES.iter().map(|s| s.to_string()).collect()
    }
}

impl PartialEq for FloatArgumentType {
    fn eq(&self, other: &Self) -> bool {
        self.minimum == other.minimum && self.maximum == other.maximum
    }
}

impl Hash for FloatArgumentType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Java's exact formula: (int) (31 * minimum + maximum), all in
        // float arithmetic. The `as` cast is Java's: NaN -> 0, infinities
        // saturate, finite values truncate.
        state.write_i32((31.0 * self.minimum + self.maximum) as i32);
    }
}

impl fmt::Display for FloatArgumentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Java renders the bounds with Float.toString, not plain Display.
        if self.minimum == -f32::MAX && self.maximum == f32::MAX {
            write!(f, "float()")
        } else if self.maximum == f32::MAX {
            write!(f, "float({})", java_float_to_string(self.minimum))
        } else {
            write!(
                f,
                "float({}, {})",
                java_float_to_string(self.minimum),
                java_float_to_string(self.maximum)
            )
        }
    }
}

/// The shared example list. Java: the `EXAMPLES` constant.
const EXAMPLES: [&str; 6] = ["0", "1.2", ".5", "-1", "-.5", "-1234.56"];

/// An unbounded float argument. Java: `FloatArgumentType.floatArg()`.
/// The lower bound is `-f32::MAX`, Java's `-Float.MAX_VALUE`.
pub fn float() -> FloatArgumentType {
    float_range(-f32::MAX, f32::MAX)
}

/// A float argument bounded below only. Java:
/// `FloatArgumentType.floatArg(float min)` — the upper bound stays
/// `f32::MAX`.
pub fn float_min(min: f32) -> FloatArgumentType {
    float_range(min, f32::MAX)
}

/// A float argument with both bounds. Java:
/// `FloatArgumentType.floatArg(float min, float max)`.
pub fn float_range(min: f32, max: f32) -> FloatArgumentType {
    FloatArgumentType {
        minimum: min,
        maximum: max,
    }
}

/// Read a previously parsed float argument named `name` from `context`.
/// Java: `FloatArgumentType.getFloat(context, name)`.
///
/// # Panics
///
/// Panics with Java's exact `IllegalArgumentException` messages from
/// `CommandContext#getArgument` when no argument named `name` was parsed
/// (`No such argument '<name>' exists on this command`) or when it holds a
/// different value type (`Argument '<name>' is defined as ..., not ...`).
/// Fallible callers: [`CommandContext::try_get_argument`].
pub fn get_float<S, R>(context: &CommandContext<S, R>, name: &str) -> f32 {
    *context.get_argument::<f32>(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hasher that records the single `i32` the Java-formula impl writes,
    /// so the tests can pin the exact `hashCode` values Java computes.
    struct CaptureI32(i64);
    impl Hasher for CaptureI32 {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, _bytes: &[u8]) {}
        fn write_i32(&mut self, value: i32) {
            self.0 = value as i64;
        }
    }

    fn java_hash_code(value: &FloatArgumentType) -> i64 {
        let mut capture = CaptureI32(0);
        value.hash(&mut capture);
        capture.0
    }

    #[test]
    fn hash_matches_java_hash_code() {
        // oracle: compiled FloatArgumentType + printed hashCode() on JDK 21.
        // Unbounded: 31 * -MAX overflows float to -Infinity, + MAX is still
        // -Infinity, the cast saturates to i32::MIN.
        assert_eq!(java_hash_code(&float()), i32::MIN as i64);
        assert_eq!(java_hash_code(&float_range(-100.0, 100.0)), -3000);
    }
}
