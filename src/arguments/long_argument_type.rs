//! The long argument type. Java: `com.mojang.brigadier.arguments.LongArgumentType`.

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

/// An argument parsing an `i64`, optionally bounded. Java:
/// `LongArgumentType` — private fields and constructor, public static
/// factories. Construct it through [`long`], [`long_min`] or [`long_range`].
///
/// Equality is Java's: two instances are equal when their bounds are equal
/// ([`PartialEq`] compares `minimum` and `maximum`), and [`Hash`] mirrors
/// Java's `hashCode` formula `31 * Long.hashCode(minimum) +
/// Long.hashCode(maximum)` (with `Long.hashCode(l)` being
/// `(int) (l ^ (l >>> 32))`), so equal instances hash equally,
/// byte-for-byte like Java.
#[derive(Clone, Copy, Debug)]
pub struct LongArgumentType {
    minimum: i64,
    maximum: i64,
}

impl LongArgumentType {
    /// The lower bound. Java: `getMinimum`.
    pub fn get_minimum(&self) -> i64 {
        self.minimum
    }

    /// The upper bound. Java: `getMaximum`.
    pub fn get_maximum(&self) -> i64 {
        self.maximum
    }
}

impl ArgumentType for LongArgumentType {
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
        let result = reader.read_long()?;
        if result < self.minimum {
            reader.set_cursor(start);
            return Err(built_in_exceptions().long_too_low().create_with_context(
                &*reader,
                result,
                self.minimum,
            ));
        }
        if result > self.maximum {
            reader.set_cursor(start);
            return Err(built_in_exceptions().long_too_high().create_with_context(
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

impl PartialEq for LongArgumentType {
    fn eq(&self, other: &Self) -> bool {
        self.minimum == other.minimum && self.maximum == other.maximum
    }
}

impl Eq for LongArgumentType {}

impl Hash for LongArgumentType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Java's exact formula: 31 * Long.hashCode(minimum)
        //                        + Long.hashCode(maximum), int arithmetic.
        let hash = 31i32
            .wrapping_mul(long_hash_code(self.minimum))
            .wrapping_add(long_hash_code(self.maximum));
        state.write_i32(hash);
    }
}

/// Java's `Long.hashCode(long)`: `(int) (l ^ (l >>> 32))` — the unsigned
/// shift folds the two halves together, then the cast truncates to 32 bits.
fn long_hash_code(value: i64) -> i32 {
    (value ^ ((value as u64 >> 32) as i64)) as i32
}

impl fmt::Display for LongArgumentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Java spells the toString prefix `longArg` even though the factory
        // method is also `longArg` — keep the exact strings.
        if self.minimum == i64::MIN && self.maximum == i64::MAX {
            write!(f, "longArg()")
        } else if self.maximum == i64::MAX {
            write!(f, "longArg({})", self.minimum)
        } else {
            write!(f, "longArg({}, {})", self.minimum, self.maximum)
        }
    }
}

/// The shared example list. Java: the `EXAMPLES` constant.
const EXAMPLES: [&str; 3] = ["0", "123", "-123"];

/// An unbounded long argument. Java: `LongArgumentType.longArg()`.
pub fn long() -> LongArgumentType {
    long_range(i64::MIN, i64::MAX)
}

/// A long argument bounded below only. Java:
/// `LongArgumentType.longArg(long min)` — the upper bound stays `i64::MAX`.
pub fn long_min(min: i64) -> LongArgumentType {
    long_range(min, i64::MAX)
}

/// A long argument with both bounds. Java:
/// `LongArgumentType.longArg(long min, long max)`.
pub fn long_range(min: i64, max: i64) -> LongArgumentType {
    LongArgumentType {
        minimum: min,
        maximum: max,
    }
}

/// Read a previously parsed long argument named `name` from `context`.
/// Java: `LongArgumentType.getLong(context, name)`.
///
/// # Panics
///
/// Panics with Java's exact `IllegalArgumentException` messages from
/// `CommandContext#getArgument` when no argument named `name` was parsed
/// (`No such argument '<name>' exists on this command`) or when it holds a
/// different value type (`Argument '<name>' is defined as ..., not ...`).
/// Fallible callers: [`CommandContext::try_get_argument`].
pub fn get_long<S, R>(context: &CommandContext<S, R>, name: &str) -> i64 {
    *context.get_argument::<i64>(name)
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

    fn java_hash_code(value: &LongArgumentType) -> i64 {
        let mut capture = CaptureI32(0);
        value.hash(&mut capture);
        capture.0
    }

    #[test]
    fn hash_matches_java_hash_code() {
        // oracle: compiled LongArgumentType + printed hashCode() on JDK 21
        assert_eq!(java_hash_code(&long()), 0);
        assert_eq!(
            java_hash_code(&long_range(-100, 1_000_000_000_000)),
            -727376667
        );
    }

    #[test]
    fn long_hash_code_folds_halves_like_java() {
        // (int)(v ^ (v >>> 32)) keeps only the low 32 bits, which are
        // low32(v) ^ high32(v) — so the sign-extension of an arithmetic
        // shift is equivalent to Java's unsigned shift here (verified
        // against the JDK 21 oracle values below).
        // Long.MIN_VALUE: low 0x00000000 ^ high 0x80000000 = i32::MIN.
        // Long.MAX_VALUE: low 0xFFFFFFFF ^ high 0x7FFFFFFF = i32::MIN.
        // -100: low 0xFFFFFF9C ^ high 0xFFFFFFFF = 99.
        assert_eq!(long_hash_code(i64::MIN), i32::MIN);
        assert_eq!(long_hash_code(i64::MAX), i32::MIN);
        assert_eq!(long_hash_code(-100), 99);
    }
}
