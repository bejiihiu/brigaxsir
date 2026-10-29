//! `DynamicNCommandExceptionType`. Java:
//! `com.mojang.brigadier.exceptions.DynamicNCommandExceptionType`.

use std::{fmt, sync::Arc};

use super::{
    command_exception_type::{CommandExceptionType, ExceptionTypeHandle},
    command_syntax_exception::CommandSyntaxException,
};
use crate::{message::Message, string_reader::ImmutableStringReader};

/// The stored message function. Java: the nested `Function` interface.
type MessageFunction<A> = Arc<dyn Fn(&[A]) -> Arc<dyn Message> + Send + Sync>;

/// An exception type whose message is computed from any number of arguments.
/// Java: `DynamicNCommandExceptionType` (its custom `Function` becomes a
/// typed `Fn(&[A]) -> Arc<dyn Message>`; Java's `Object...` varargs are a
/// slice here).
///
/// Java's `create(Object a, Object... args)` passes the *whole* argument list
/// — `a` included — to the function; [`Self::create`] does the same with the
/// caller-built slice.
///
/// Every exception produced from one instance shares that instance's type
/// identity (see [`Self::as_type`]).
pub struct DynamicNCommandExceptionType<A> {
    function: MessageFunction<A>,
    as_type: Arc<dyn CommandExceptionType>,
}

impl<A: 'static> DynamicNCommandExceptionType<A> {
    /// A type whose exceptions are built by `function`. Java:
    /// `new DynamicNCommandExceptionType(function)`.
    pub fn new(function: impl Fn(&[A]) -> Arc<dyn Message> + Send + Sync + 'static) -> Self {
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

    /// Build the exception for `args`. Java: `create(Object, Object...)`.
    pub fn create(&self, args: &[A]) -> CommandSyntaxException {
        CommandSyntaxException::new(self.as_type.clone(), (self.function)(args))
    }

    /// Build the exception with the reader's input and cursor attached, so
    /// the error renders its `<--[HERE]` snippet. Java:
    /// `createWithContext(ImmutableStringReader, Object...)`.
    pub fn create_with_context(
        &self,
        reader: &dyn ImmutableStringReader,
        args: &[A],
    ) -> CommandSyntaxException {
        CommandSyntaxException::new_with_context(
            self.as_type.clone(),
            (self.function)(args),
            reader.string(),
            reader.cursor(),
        )
    }
}

impl<A> fmt::Debug for DynamicNCommandExceptionType<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicNCommandExceptionType")
            .finish_non_exhaustive()
    }
}
