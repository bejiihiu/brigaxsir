//! The marker trait for command exception types. Java:
//! `com.mojang.brigadier.exceptions.CommandExceptionType`.

/// Marker for everything that can label a
/// [`CommandSyntaxException`](crate::errors::CommandSyntaxException). Java:
/// `CommandExceptionType` (an empty interface).
///
/// Implementations are the exception *type objects*: identity is what
/// matters, not contents. Errors are told apart by comparing type objects
/// (`exception.getType() == type` in Java,
/// [`CommandSyntaxException::is_type`](crate::errors::CommandSyntaxException::is_type)
/// or `Arc::ptr_eq` here), not by matching on the trait object — there is no
/// downcast on a bare trait object.
///
/// The library's own implementations are
/// [`SimpleCommandExceptionType`](crate::errors::SimpleCommandExceptionType)
/// and the `Dynamic*CommandExceptionType` family; user argument types are
/// free to implement this trait on their own types and hand them to
/// [`CommandSyntaxException::new`](crate::errors::CommandSyntaxException::new)
/// directly, exactly like Java classes implementing the marker interface.
pub trait CommandExceptionType: Send + Sync {}

/// The opaque shared identity handle the library's exception-type structs
/// hand out through their `as_type()` accessors.
///
/// Why this exists: a type struct cannot own an `Arc` pointing at itself
/// (that would leak), and its `create*` methods take `&self`, so there is no
/// way to reach "the `Arc` that wraps me" from inside a method. Each instance
/// instead mints exactly one of these handles on construction and reuses it
/// for every exception it produces, so
/// `Arc::ptr_eq(exception.get_type(), instance.as_type())` holds for the
/// lifetime of the instance — the Java `exception.getType() == type`
/// identity check. Two separately constructed instances always get distinct
/// handles, like two distinct Java objects.
#[derive(Debug)]
pub(crate) struct ExceptionTypeHandle;

impl CommandExceptionType for ExceptionTypeHandle {}
