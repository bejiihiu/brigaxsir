//! `Dynamic4CommandExceptionType`. Java:
//! `com.mojang.brigadier.exceptions.Dynamic4CommandExceptionType`.

use std::{fmt, sync::Arc};

use super::{
    command_exception_type::{CommandExceptionType, ExceptionTypeHandle},
    command_syntax_exception::CommandSyntaxException,
};
use crate::{message::Message, string_reader::ImmutableStringReader};

/// The stored message function. Java: the nested `Function` interface.
type MessageFunction<A, B, C, D> = Arc<dyn Fn(A, B, C, D) -> Arc<dyn Message> + Send + Sync>;

/// An exception type whose message is computed from four arguments. Java:
/// `Dynamic4CommandExceptionType` (its custom `Function` interface becomes a
/// typed `Fn(A, B, C, D) -> Arc<dyn Message>`).
///
/// Every exception produced from one instance shares that instance's type
/// identity (see [`Self::as_type`]).
pub struct Dynamic4CommandExceptionType<A, B, C, D> {
    function: MessageFunction<A, B, C, D>,
    as_type: Arc<dyn CommandExceptionType>,
}

impl<A: 'static, B: 'static, C: 'static, D: 'static> Dynamic4CommandExceptionType<A, B, C, D> {
    /// A type whose exceptions are built by `function`. Java:
    /// `new Dynamic4CommandExceptionType(function)`.
    pub fn new(function: impl Fn(A, B, C, D) -> Arc<dyn Message> + Send + Sync + 'static) -> Self {
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

    /// Build the exception for `(a, b, c, d)`. Java:
    /// `create(Object, Object, Object, Object)`.
    pub fn create(&self, a: A, b: B, c: C, d: D) -> CommandSyntaxException {
        CommandSyntaxException::new(self.as_type.clone(), (self.function)(a, b, c, d))
    }

    /// Build the exception with the reader's input and cursor attached, so
    /// the error renders its `<--[HERE]` snippet. Java:
    /// `createWithContext(ImmutableStringReader, Object, Object, Object,
    /// Object)`.
    pub fn create_with_context(
        &self,
        reader: &dyn ImmutableStringReader,
        a: A,
        b: B,
        c: C,
        d: D,
    ) -> CommandSyntaxException {
        CommandSyntaxException::new_with_context(
            self.as_type.clone(),
            (self.function)(a, b, c, d),
            reader.string(),
            reader.cursor(),
        )
    }
}

impl<A, B, C, D> fmt::Debug for Dynamic4CommandExceptionType<A, B, C, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dynamic4CommandExceptionType")
            .finish_non_exhaustive()
    }
}
