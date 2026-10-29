//! Command builders, mirroring Java's `com.mojang.brigadier.builder`
//! package: the shared [`ArgumentBuilder`] state, the concrete
//! [`LiteralArgumentBuilder`] / [`RequiredArgumentBuilder`] (type aliases
//! over the shared struct with the kind data pinned), and the
//! [`AnyArgumentBuilder`] enum Java's `ArgumentBuilder<S, ?>` return types
//! map to.

pub mod argument_builder;
pub mod literal_argument_builder;
pub mod required_argument_builder;

pub use argument_builder::{AnyArgumentBuilder, ArgumentBuilder, IntoCommandNode};
pub use literal_argument_builder::{LiteralArgumentBuilder, literal};
pub use required_argument_builder::{RequiredArgumentBuilder, argument};
