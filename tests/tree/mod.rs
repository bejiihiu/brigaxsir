//! The port of Java's `tree` test package: the shared
//! `AbstractCommandNodeTest` cases run once per node kind, plus the
//! `LiteralCommandNodeTest` / `RootCommandNodeTest` /
//! `ArgumentCommandNodeTest` files.

pub(crate) mod abstract_command_node_test;
pub(crate) mod argument_command_node_test;
pub(crate) mod literal_command_node_test;
pub(crate) mod root_command_node_test;
