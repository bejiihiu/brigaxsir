//! The integer argument type. Java: `com.mojang.brigadier.arguments.IntegerArgumentType`.

use std::{
    any::Any,
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};

use super::ArgumentType;
use crate::{
    context::{CommandContext, ParsedValue},
    errors::{BuiltInExceptionProvider, CommandSyntaxException, built_in_exceptions},
    string_reader::StringReader,
};

/// An argument parsing an `i32`, optionally bounded. Java:
/// `IntegerArgumentType` — private fields and constructor, public static
/// factories. Construct it through [`integer`], [`integer_min`] or
/// [`integer_range`].
///
/// Equality is Java's: two instances are equal when their bounds are equal
/// ([`PartialEq`] compares `minimum` and `maximum`), and [`Hash`] mirrors
/// Java's `hashCode` formula `31 * minimum + maximum` (wrapping `int`
/// arithmetic) so equal instances hash equally, byte-for-byte like Java.
#[derive(Clone, Copy, Debug)]
pub struct IntegerArgumentType {
    minimum: i32,
    maximum: i32,
}

impl IntegerArgumentType {
    /// The lower bound. Java: `getMinimum`.
    pub fn get_minimum(&self) -> i32 {
        self.minimum
    }

    /// The upper bound. Java: `getMaximum`.
    pub fn get_maximum(&self) -> i32 {
        self.maximum
    }
}

impl ArgumentType for IntegerArgumentType {
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
        let result = reader.read_int()?;
        if result < self.minimum {
            reader.set_cursor(start);
            return Err(built_in_exceptions().integer_too_low().create_with_context(
                &*reader,
                result,
                self.minimum,
            ));
        }
        if result > self.maximum {
            reader.set_cursor(start);
            return Err(built_in_exceptions()
                .integer_too_high()
                .create_with_context(&*reader, result, self.maximum));
        }
        Ok(Arc::new(result))
    }

    fn examples(&self) -> Vec<String> {
        EXAMPLES.iter().map(|s| s.to_string()).collect()
    }
}

impl PartialEq for IntegerArgumentType {
    fn eq(&self, other: &Self) -> bool {
        self.minimum == other.minimum && self.maximum == other.maximum
    }
}

impl Eq for IntegerArgumentType {}

impl Hash for IntegerArgumentType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Java's exact formula: 31 * minimum + maximum, wrapping int math.
        state.write_i32(self.minimum.wrapping_mul(31).wrapping_add(self.maximum));
    }
}

impl fmt::Display for IntegerArgumentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.minimum == i32::MIN && self.maximum == i32::MAX {
            write!(f, "integer()")
        } else if self.maximum == i32::MAX {
            write!(f, "integer({})", self.minimum)
        } else {
            write!(f, "integer({}, {})", self.minimum, self.maximum)
        }
    }
}

/// The shared example list. Java: the `EXAMPLES` constant.
const EXAMPLES: [&str; 3] = ["0", "123", "-123"];

/// An unbounded integer argument. Java: `IntegerArgumentType.integer()`.
pub fn integer() -> IntegerArgumentType {
    integer_range(i32::MIN, i32::MAX)
}

/// An integer argument bounded below only. Java:
/// `IntegerArgumentType.integer(int min)` — the upper bound stays
/// `i32::MAX`.
pub fn integer_min(min: i32) -> IntegerArgumentType {
    integer_range(min, i32::MAX)
}

/// An integer argument with both bounds. Java:
/// `IntegerArgumentType.integer(int min, int max)`.
pub fn integer_range(min: i32, max: i32) -> IntegerArgumentType {
    IntegerArgumentType {
        minimum: min,
        maximum: max,
    }
}

/// Read a previously parsed integer argument named `name` from `context`.
/// Java: `IntegerArgumentType.getInteger(context, name)`.
///
/// # Panics
///
/// Panics with Java's exact `IllegalArgumentException` messages from
/// `CommandContext#getArgument` when no argument named `name` was parsed
/// (`No such argument '<name>' exists on this command`) or when it holds a
/// different value type (`Argument '<name>' is defined as ..., not ...`).
/// Fallible callers: [`CommandContext::try_get_argument`].
pub fn get_integer<S, R>(context: &CommandContext<S, R>, name: &str) -> i32 {
    *context.get_argument::<i32>(name)
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

    fn java_hash_code(value: &IntegerArgumentType) -> i64 {
        let mut capture = CaptureI32(0);
        value.hash(&mut capture);
        capture.0
    }

    #[test]
    fn hash_matches_java_hash_code() {
        // oracle: compiled IntegerArgumentType + printed hashCode() on JDK 21
        assert_eq!(java_hash_code(&integer()), -1);
        assert_eq!(java_hash_code(&integer_range(-100, 100)), -3000);
        assert_eq!(java_hash_code(&integer_range(-100, 50)), -3050);
        assert_eq!(java_hash_code(&integer_range(-50, 100)), -1450);
    }
}
