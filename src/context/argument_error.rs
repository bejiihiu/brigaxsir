//! Why a typed argument lookup failed. Java surfaces both cases as
//! `IllegalArgumentException` thrown straight out of
//! `CommandContext#getArgument`; the port carries them as data so callers can
//! match on them, and [`CommandContext::get_argument`] panics with exactly
//! these strings for Java parity.
//!
//! [`CommandContext::get_argument`]: crate::context::CommandContext::get_argument

use std::fmt;

/// The failure modes of
/// [`CommandContext::try_get_argument`](crate::context::CommandContext::try_get_argument).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArgumentError {
    /// No argument with this name was parsed. Java throws
    /// `IllegalArgumentException("No such argument '<name>' exists on this command")`.
    NoSuchArgument {
        /// The requested argument name.
        name: String,
    },
    /// The argument exists but holds a different value type. Java throws
    /// `IllegalArgumentException("Argument '<name>' is defined as <X>, not <Y>")`
    /// — there `X` is `result.getClass().getSimpleName()` and `Y` the
    /// requested `Class`; the port renders both as the Java-style simple
    /// names from [`ParsedValue::type_name`].
    ///
    /// [`ParsedValue::type_name`]: crate::context::ParsedValue::type_name
    TypeMismatch {
        /// The requested argument name.
        name: String,
        /// The simple name of the value type actually stored.
        actual: &'static str,
        /// The simple name of the requested value type.
        expected: &'static str,
    },
}

impl ArgumentError {
    pub(crate) fn no_such_argument(name: &str) -> Self {
        Self::NoSuchArgument {
            name: name.to_owned(),
        }
    }

    pub(crate) fn type_mismatch(name: &str, actual: &'static str, expected: &'static str) -> Self {
        Self::TypeMismatch {
            name: name.to_owned(),
            actual,
            expected,
        }
    }
}

impl fmt::Display for ArgumentError {
    /// Byte-identical to Java's `IllegalArgumentException` messages from
    /// `CommandContext#getArgument`, with both type names rendered as simple
    /// names (Java prints the requested `Class` through `Class.toString()`,
    /// i.e. fully qualified — a `java.lang` package prefix has no meaning in
    /// Rust, so the port uses the simple style on both sides).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchArgument { name } => {
                write!(f, "No such argument '{name}' exists on this command")
            }
            Self::TypeMismatch {
                name,
                actual,
                expected,
            } => write!(
                f,
                "Argument '{name}' is defined as {actual}, not {expected}"
            ),
        }
    }
}

impl std::error::Error for ArgumentError {}
