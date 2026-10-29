//! `SimpleCommandExceptionType`. Java:
//! `com.mojang.brigadier.exceptions.SimpleCommandExceptionType`.

use std::{fmt, sync::Arc};

use super::{
    command_exception_type::{CommandExceptionType, ExceptionTypeHandle},
    command_syntax_exception::CommandSyntaxException,
};
use crate::{message::Message, string_reader::ImmutableStringReader};

/// An exception type with a fixed message. Java: `SimpleCommandExceptionType`.
///
/// Every exception produced from one instance shares that instance's type
/// identity (see [`Self::as_type`]); two separately constructed instances are
/// distinct types, like two Java objects.
///
/// ```
/// # use std::sync::Arc;
/// # use brigaxsir::errors::SimpleCommandExceptionType;
/// # use brigaxsir::message::LiteralMessage;
/// # use brigaxsir::string_reader::StringReader;
/// let already = SimpleCommandExceptionType::new(Arc::new(LiteralMessage::new("Already built")));
///
/// let mut reader = StringReader::from("build");
/// reader.set_cursor(5);
/// let error = already.create_with_context(&reader);
/// // the message carries the input snippet, like Java's
/// assert_eq!(error.get_message(), "Already built at position 5: build<--[HERE]");
/// assert!(error.is_type(already.as_type()));
/// ```
pub struct SimpleCommandExceptionType {
    message: Arc<dyn Message>,
    as_type: Arc<dyn CommandExceptionType>,
}

impl SimpleCommandExceptionType {
    /// A type that always fails with `message`. Java:
    /// `new SimpleCommandExceptionType(message)`.
    pub fn new(message: Arc<dyn Message>) -> Self {
        Self {
            message,
            as_type: Arc::new(ExceptionTypeHandle),
        }
    }

    /// The shared identity of this type object: every exception created from
    /// this instance carries exactly this handle, so
    /// `exception.is_type(self.as_type())` is the Java
    /// `exception.getType() == type` check. Rust-only helper — Java has no
    /// equivalent because the type object *is* the instance.
    pub fn as_type(&self) -> &Arc<dyn CommandExceptionType> {
        &self.as_type
    }

    /// Build the exception. Java: `create()`.
    pub fn create(&self) -> CommandSyntaxException {
        CommandSyntaxException::new(self.as_type.clone(), self.message.clone())
    }

    /// Build the exception with the reader's input and cursor attached, so
    /// the error renders its `<--[HERE]` snippet. Java:
    /// `createWithContext(ImmutableStringReader)`.
    pub fn create_with_context(
        &self,
        reader: &dyn ImmutableStringReader,
    ) -> CommandSyntaxException {
        CommandSyntaxException::new_with_context(
            self.as_type.clone(),
            self.message.clone(),
            reader.string(),
            reader.cursor(),
        )
    }
}

// Java: toString() returns the message string.
impl fmt::Display for SimpleCommandExceptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message.get_string())
    }
}

impl fmt::Debug for SimpleCommandExceptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message.get_string())
    }
}
