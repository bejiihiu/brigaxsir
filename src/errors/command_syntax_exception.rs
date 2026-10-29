//! `CommandSyntaxException`, the Rust analogue of Java's
//! `com.mojang.brigadier.exceptions.CommandSyntaxException`.

use std::{
    cmp,
    fmt::{self, Write},
    sync::Arc,
};

use super::command_exception_type::CommandExceptionType;
use crate::{
    message::Message,
    string_reader::{char_len, char_to_byte},
};

/// A parsing or execution failure. Java: `CommandSyntaxException`.
///
/// Carries the exception *type* (a shared [`CommandExceptionType`] object),
/// the [`Message`], and — when raised during parsing — the input string plus
/// the cursor the failure happened at, so [`Self::get_context`] can render
/// its `<--[HERE]` snippet.
///
/// Construct one directly through the two constructors (Java's public
/// constructors), or through the exception types —
/// [`SimpleCommandExceptionType::create`](crate::errors::SimpleCommandExceptionType::create)
/// and the `Dynamic*` family — which is what user argument types normally do.
///
/// Two Java statics have no Rust counterpart and are documented divergences:
/// `ENABLE_COMMAND_STACK_TRACES` controls Java's stack-trace capture inside
/// the `Exception` superclass; this struct is a plain value with no stack
/// capture at all, so there is nothing to switch off (construction is cheap
/// and allocation-only, as if the flag were `false`). `BUILT_IN_EXCEPTIONS`
/// is a mutable static in Java and a fixed global here — see
/// [`built_in_exceptions`](crate::errors::built_in_exceptions).
#[derive(Clone)]
pub struct CommandSyntaxException {
    type_: Arc<dyn CommandExceptionType>,
    message: Arc<dyn Message>,
    input: Option<String>,
    cursor: Option<usize>,
}

impl CommandSyntaxException {
    /// How many characters of trailing input [`Self::get_context`] shows
    /// before the `<--[HERE]` marker. Java: `CONTEXT_AMOUNT`.
    pub const CONTEXT_AMOUNT: usize = 10;

    /// A context-free exception: no input, no cursor, no `<--[HERE]` snippet.
    /// Java: `new CommandSyntaxException(type, message)` (which stores
    /// `input = null, cursor = -1`).
    pub fn new(type_: Arc<dyn CommandExceptionType>, message: Arc<dyn Message>) -> Self {
        Self {
            type_,
            message,
            input: None,
            cursor: None,
        }
    }

    /// An exception with the failing `input` and `cursor` attached, so
    /// [`Self::get_context`] can render its snippet. Java:
    /// `new CommandSyntaxException(type, message, input, cursor)`.
    pub fn new_with_context(
        type_: Arc<dyn CommandExceptionType>,
        message: Arc<dyn Message>,
        input: impl Into<String>,
        cursor: usize,
    ) -> Self {
        Self {
            type_,
            message,
            input: Some(input.into()),
            cursor: Some(cursor),
        }
    }

    /// The message, with the ` at position N: ...` context suffix appended
    /// when input context is attached. Java: `getMessage()`.
    pub fn get_message(&self) -> String {
        let mut message = String::from(self.message.get_string());
        if let Some(context) = self.get_context() {
            // get_context() is Some only when the cursor is, so the
            // unwrap_or fallback is unreachable
            write!(
                message,
                " at position {}: {context}",
                self.cursor.unwrap_or(0)
            )
            .expect("writing to a String never fails");
        }
        message
    }

    /// The bare message, no position suffix. Java: `getRawMessage()`.
    pub fn get_raw_message(&self) -> &Arc<dyn Message> {
        &self.message
    }

    /// A snippet of the failing input: up to [`Self::CONTEXT_AMOUNT`]
    /// characters before the cursor, prefixed with `...` when truncated and
    /// always ending in `<--[HERE]`. `None` when the exception carries no
    /// input context. Java: `getContext()`.
    ///
    /// The window counts characters (Unicode scalar values) — Java counts
    /// UTF-16 units, identical for BMP input, wider only for astral-plane
    /// characters. Cursor and window saturate at the input bounds where
    /// Java's `substring` would throw.
    pub fn get_context(&self) -> Option<String> {
        let input = self.input.as_ref()?;
        let cursor = self.cursor?;

        let mut builder = String::new();
        let end = cmp::min(char_len(input), cursor);

        if end > Self::CONTEXT_AMOUNT {
            builder.push_str("...");
        }

        builder.push_str(
            &input[char_to_byte(input, end.saturating_sub(Self::CONTEXT_AMOUNT))
                ..char_to_byte(input, end)],
        );
        builder.push_str("<--[HERE]");

        Some(builder)
    }

    /// The type object that raised this exception. Java: `getType()`.
    ///
    /// Identity, not contents: compare against an expected type with
    /// [`Self::is_type`].
    pub fn get_type(&self) -> &Arc<dyn CommandExceptionType> {
        &self.type_
    }

    /// Whether this exception was raised by `type_`: pointer identity of the
    /// type objects, Java's `exception.getType() == type` check.
    pub fn is_type(&self, type_: &Arc<dyn CommandExceptionType>) -> bool {
        Arc::ptr_eq(&self.type_, type_)
    }

    /// The input the failure happened in, when attached. Java: `getInput()`;
    /// its `null` is `None` here.
    pub fn get_input(&self) -> Option<&str> {
        self.input.as_deref()
    }

    /// The cursor at the point of failure, when attached. Java:
    /// `getCursor()`; its `-1` sentinel is `None` here.
    pub fn get_cursor(&self) -> Option<usize> {
        self.cursor
    }
}

impl fmt::Display for CommandSyntaxException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.get_message())
    }
}

impl std::error::Error for CommandSyntaxException {}

impl fmt::Debug for CommandSyntaxException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.get_message())
    }
}
