//! Messages carried by errors and suggestions. Mirrors Java's root-package
//! `Message` interface and `LiteralMessage` implementation.

use std::fmt;

/// A message: the human-readable text an error or a suggestion carries.
/// Java: `com.mojang.brigadier.Message` (interface).
///
/// The indirection exists so error and tooltip texts can be richer than plain
/// strings — Minecraft, the reference consumer, implements this with styled
/// chat components. Implementations must return the same text on every call.
pub trait Message: fmt::Debug + Send + Sync {
    /// The message text. Java: `getString()`.
    fn get_string(&self) -> &str;
}

/// A plain, unstyled message wrapping a `String`. Java: `LiteralMessage`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct LiteralMessage(String);

impl LiteralMessage {
    /// A message wrapping `string`. Java: `new LiteralMessage(string)`.
    pub fn new(string: impl Into<String>) -> Self {
        Self(string.into())
    }
}

impl Message for LiteralMessage {
    fn get_string(&self) -> &str {
        &self.0
    }
}

// Java: LiteralMessage#toString returns the wrapped string.
impl fmt::Display for LiteralMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for LiteralMessage {
    fn from(string: String) -> Self {
        Self::new(string)
    }
}

impl From<&str> for LiteralMessage {
    fn from(string: &str) -> Self {
        Self::new(string)
    }
}
