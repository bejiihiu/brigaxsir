//! Ambiguity detection callbacks. Mirrors Java's
//! `com.mojang.brigadier.AmbiguityConsumer`.

use std::{collections::HashSet, sync::Arc};

use parking_lot::RwLock;

use crate::tree::CommandNode;

/// Notified by [`CommandNode::find_ambiguities`] (and the dispatcher's
/// shortcut over the root) whenever two sibling nodes can both accept the
/// same input. Java: `AmbiguityConsumer<S>#ambiguous`.
///
/// The three node arguments are the actual tree handles, so identity checks
/// (`Arc::ptr_eq`) work like Java's reference comparisons. `inputs` is the
/// set of ambiguous inputs — Java hands over its live `HashSet`, whose
/// iteration order is unspecified; here the set is handed over before it is
/// reset, and its contents are exactly Java's.
pub trait AmbiguityConsumer<S, R> {
    /// Report that `child` and `sibling` (both children of `parent`) both
    /// accept every input in `inputs`.
    fn ambiguous(
        &self,
        parent: &Arc<RwLock<CommandNode<S, R>>>,
        child: &Arc<RwLock<CommandNode<S, R>>>,
        sibling: &Arc<RwLock<CommandNode<S, R>>>,
        inputs: &HashSet<String>,
    );
}
