//! The double argument type. Java: `com.mojang.brigadier.arguments.DoubleArgumentType`.

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
        BuiltInExceptionProvider, CommandSyntaxException, built_in_exceptions,
        java_double_to_string,
    },
    string_reader::StringReader,
};

/// An argument parsing an `f64`, optionally bounded. Java:
/// `DoubleArgumentType` — private fields and constructor, public static
/// factories. Construct it through [`double`], [`double_min`] or
/// [`double_range`].
///
/// Java's unbounded factory uses `-Double.MAX_VALUE` as the lower bound (not
/// `-f64::INFINITY`!) — the port keeps that.
///
/// Equality is Java's: two instances are equal when their bounds are equal.
/// [`PartialEq`] compares `minimum` and `maximum` with IEEE `==` — exactly
/// Java's `==` on `double` fields: `-0.0 == 0.0` compares equal and NaN never
/// compares equal, even to itself. [`Hash`] mirrors Java's `hashCode` formula
/// `(int) (31 * minimum + maximum)` — double arithmetic, then Java's
/// saturating cast (NaN to `0`, infinities to `i32::MIN`/`i32::MAX`), which
/// is also what Rust's float-to-int `as` cast does.
#[derive(Clone, Copy, Debug)]
pub struct DoubleArgumentType {
    minimum: f64,
    maximum: f64,
}

impl DoubleArgumentType {
    /// The lower bound. Java: `getMinimum`.
    pub fn get_minimum(&self) -> f64 {
        self.minimum
    }

    /// The upper bound. Java: `getMaximum`.
    pub fn get_maximum(&self) -> f64 {
        self.maximum
    }
}

impl ArgumentType for DoubleArgumentType {
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
        let result = reader.read_double()?;
        if result < self.minimum {
            reader.set_cursor(start);
            return Err(built_in_exceptions().double_too_low().create_with_context(
                &*reader,
                result,
                self.minimum,
            ));
        }
        if result > self.maximum {
            reader.set_cursor(start);
            return Err(built_in_exceptions().double_too_high().create_with_context(
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

impl PartialEq for DoubleArgumentType {
    fn eq(&self, other: &Self) -> bool {
        self.minimum == other.minimum && self.maximum == other.maximum
    }
}

impl Hash for DoubleArgumentType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Java's exact formula: (int) (31 * minimum + maximum), all in
        // double arithmetic. The `as` cast is Java's: NaN -> 0, infinities
        // saturate, finite values truncate.
        state.write_i32((31.0 * self.minimum + self.maximum) as i32);
    }
}

impl fmt::Display for DoubleArgumentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Java renders the bounds with Double.toString, not plain Display.
        if self.minimum == -f64::MAX && self.maximum == f64::MAX {
            write!(f, "double()")
        } else if self.maximum == f64::MAX {
            write!(f, "double({})", java_double_to_string(self.minimum))
        } else {
            write!(
                f,
                "double({}, {})",
                java_double_to_string(self.minimum),
                java_double_to_string(self.maximum)
            )
        }
    }
}

/// The shared example list. Java: the `EXAMPLES` constant.
const EXAMPLES: [&str; 6] = ["0", "1.2", ".5", "-1", "-.5", "-1234.56"];

/// An unbounded double argument. Java: `DoubleArgumentType.doubleArg()`.
/// The lower bound is `-f64::MAX`, Java's `-Double.MAX_VALUE`.
pub fn double() -> DoubleArgumentType {
    double_range(-f64::MAX, f64::MAX)
}

/// A double argument bounded below only. Java:
/// `DoubleArgumentType.doubleArg(double min)` — the upper bound stays
/// `f64::MAX`.
pub fn double_min(min: f64) -> DoubleArgumentType {
    double_range(min, f64::MAX)
}

/// A double argument with both bounds. Java:
/// `DoubleArgumentType.doubleArg(double min, double max)`.
pub fn double_range(min: f64, max: f64) -> DoubleArgumentType {
    DoubleArgumentType {
        minimum: min,
        maximum: max,
    }
}

/// Read a previously parsed double argument named `name` from `context`.
/// Java: `DoubleArgumentType.getDouble(context, name)`.
///
/// # Panics
///
/// Panics with Java's exact `IllegalArgumentException` messages from
/// `CommandContext#getArgument` when no argument named `name` was parsed
/// (`No such argument '<name>' exists on this command`) or when it holds a
/// different value type (`Argument '<name>' is defined as ..., not ...`).
/// Fallible callers: [`CommandContext::try_get_argument`].
pub fn get_double<S, R>(context: &CommandContext<S, R>, name: &str) -> f64 {
    *context.get_argument::<f64>(name)
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

    fn java_hash_code(value: &DoubleArgumentType) -> i64 {
        let mut capture = CaptureI32(0);
        value.hash(&mut capture);
        capture.0
    }

    #[test]
    fn hash_matches_java_hash_code() {
        // oracle: compiled DoubleArgumentType + printed hashCode() on JDK 21.
        // Unbounded: 31 * -MAX overflows double to -Infinity, + MAX is still
        // -Infinity, the cast saturates to i32::MIN.
        assert_eq!(java_hash_code(&double()), i32::MIN as i64);
        assert_eq!(java_hash_code(&double_range(-100.0, 100.0)), -3000);
    }
}
