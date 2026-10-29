//! `DynamicCommandExceptionType`. Java:
//! `com.mojang.brigadier.exceptions.DynamicCommandExceptionType`.

use std::{fmt, sync::Arc};

use super::{
    command_exception_type::{CommandExceptionType, ExceptionTypeHandle},
    command_syntax_exception::CommandSyntaxException,
};
use crate::{message::Message, string_reader::ImmutableStringReader};

/// An exception type whose message is computed from one argument. Java:
/// `DynamicCommandExceptionType` (its `Function<Object, Message>` becomes a
/// typed `Fn(A) -> Arc<dyn Message>` — the generic `A` replaces Java's
/// `Object`, so the argument keeps its type end to end).
///
/// Every exception produced from one instance shares that instance's type
/// identity (see [`Self::as_type`]).
pub struct DynamicCommandExceptionType<A> {
    function: Arc<dyn Fn(A) -> Arc<dyn Message> + Send + Sync>,
    as_type: Arc<dyn CommandExceptionType>,
}

impl<A: 'static> DynamicCommandExceptionType<A> {
    /// A type whose exceptions are built by `function`. Java:
    /// `new DynamicCommandExceptionType(function)`.
    pub fn new(function: impl Fn(A) -> Arc<dyn Message> + Send + Sync + 'static) -> Self {
        Self {
            function: Arc::new(function),
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

    /// Build the exception for `arg`. Java: `create(Object)`.
    pub fn create(&self, arg: A) -> CommandSyntaxException {
        CommandSyntaxException::new(self.as_type.clone(), (self.function)(arg))
    }

    /// Build the exception with the reader's input and cursor attached, so
    /// the error renders its `<--[HERE]` snippet. Java:
    /// `createWithContext(ImmutableStringReader, Object)`.
    pub fn create_with_context(
        &self,
        reader: &dyn ImmutableStringReader,
        arg: A,
    ) -> CommandSyntaxException {
        CommandSyntaxException::new_with_context(
            self.as_type.clone(),
            (self.function)(arg),
            reader.string(),
            reader.cursor(),
        )
    }
}

impl<A> fmt::Debug for DynamicCommandExceptionType<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicCommandExceptionType")
            .finish_non_exhaustive()
    }
}
