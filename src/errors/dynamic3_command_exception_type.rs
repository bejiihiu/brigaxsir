//! `Dynamic3CommandExceptionType`. Java:
//! `com.mojang.brigadier.exceptions.Dynamic3CommandExceptionType`.

use std::{fmt, sync::Arc};

use super::{
    command_exception_type::{CommandExceptionType, ExceptionTypeHandle},
    command_syntax_exception::CommandSyntaxException,
};
use crate::{message::Message, string_reader::ImmutableStringReader};

/// An exception type whose message is computed from three arguments. Java:
/// `Dynamic3CommandExceptionType` (its custom `Function` interface becomes a
/// typed `Fn(A, B, C) -> Arc<dyn Message>`).
///
/// Every exception produced from one instance shares that instance's type
/// identity (see [`Self::as_type`]).
pub struct Dynamic3CommandExceptionType<A, B, C> {
    function: Arc<dyn Fn(A, B, C) -> Arc<dyn Message> + Send + Sync>,
    as_type: Arc<dyn CommandExceptionType>,
}

impl<A: 'static, B: 'static, C: 'static> Dynamic3CommandExceptionType<A, B, C> {
    /// A type whose exceptions are built by `function`. Java:
    /// `new Dynamic3CommandExceptionType(function)`.
    pub fn new(function: impl Fn(A, B, C) -> Arc<dyn Message> + Send + Sync + 'static) -> Self {
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

    /// Build the exception for `(a, b, c)`. Java:
    /// `create(Object, Object, Object)`.
    pub fn create(&self, a: A, b: B, c: C) -> CommandSyntaxException {
        CommandSyntaxException::new(self.as_type.clone(), (self.function)(a, b, c))
    }

    /// Build the exception with the reader's input and cursor attached, so
    /// the error renders its `<--[HERE]` snippet. Java:
    /// `createWithContext(ImmutableStringReader, Object, Object, Object)`.
    pub fn create_with_context(
        &self,
        reader: &dyn ImmutableStringReader,
        a: A,
        b: B,
        c: C,
    ) -> CommandSyntaxException {
        CommandSyntaxException::new_with_context(
            self.as_type.clone(),
            (self.function)(a, b, c),
            reader.string(),
            reader.cursor(),
        )
    }
}

impl<A, B, C> fmt::Debug for Dynamic3CommandExceptionType<A, B, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dynamic3CommandExceptionType")
            .finish_non_exhaustive()
    }
}
