//! The error model: messages, exception types, syntax exceptions, and the
//! built-in provider. Mirrors Java's `com.mojang.brigadier.exceptions`
//! package; the `Message`/`LiteralMessage` pair (root package in Java) lives
//! in [`crate::message`].
//!
//! The shape follows Java 1:1: a marker [`CommandExceptionType`] trait,
//! [`SimpleCommandExceptionType`] and the `Dynamic*` family as concrete type
//! structs, [`CommandSyntaxException`] carrying type/message/input/cursor,
//! and a 27-method [`BuiltInExceptionProvider`] behind the global
//! [`built_in_exceptions()`]. Custom argument types raise their own errors by
//! implementing [`CommandExceptionType`] or reusing the `Simple`/`Dynamic`
//! structs — the same two routes Java code has.

mod builtin_exceptions;
mod command_exception_type;
mod command_syntax_exception;
mod dynamic2_command_exception_type;
mod dynamic3_command_exception_type;
mod dynamic4_command_exception_type;
mod dynamic_command_exception_type;
mod dynamic_n_command_exception_type;
mod simple_command_exception_type;

pub use builtin_exceptions::{BuiltInExceptionProvider, BuiltInExceptions, built_in_exceptions};
pub(crate) use builtin_exceptions::{java_double_to_string, java_float_to_string};
pub use command_exception_type::CommandExceptionType;
pub use command_syntax_exception::CommandSyntaxException;
pub use dynamic_command_exception_type::DynamicCommandExceptionType;
pub use dynamic_n_command_exception_type::DynamicNCommandExceptionType;
pub use dynamic2_command_exception_type::Dynamic2CommandExceptionType;
pub use dynamic3_command_exception_type::Dynamic3CommandExceptionType;
pub use dynamic4_command_exception_type::Dynamic4CommandExceptionType;
pub use simple_command_exception_type::SimpleCommandExceptionType;

/// Helper trait so the dispatcher can work with any command result type while
/// still feeding `int`-shaped results to consumers and fork-mode accounting,
/// like Java's always-`int` command results.
///
/// `R = i32` is the direct analogue of Java's `int` return value;
/// `Result<i32, _>` lets a command return an error naturally instead of
/// throwing.
pub trait CommandResultTrait {
    /// Lift a plain `int` result into this type, the way a command closure's
    /// return value is stored. Java has no such step (its commands return
    /// `int` unconditionally).
    fn new(i: i32) -> Self;

    /// The `int` view behind this result, when it is one — what consumers
    /// and fork accounting receive. Java: the result *is* the `int`.
    fn as_i32(&self) -> Option<i32>;
}
impl CommandResultTrait for i32 {
    fn new(i: i32) -> Self {
        i
    }
    fn as_i32(&self) -> Option<i32> {
        Some(*self)
    }
}
impl<E> CommandResultTrait for Result<i32, E> {
    fn new(i: i32) -> Self {
        Ok(i)
    }
    fn as_i32(&self) -> Option<i32> {
        self.as_ref().copied().ok()
    }
}
