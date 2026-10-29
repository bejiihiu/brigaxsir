//! The root marker payload. Java:
//! `com.mojang.brigadier.tree.RootCommandNode` — there it is a full node
//! subclass; here the tree keeps one `CommandNode` type and this marker is
//! the data of the root kind (see
//! [`CommandNodeKind`](crate::tree::CommandNodeKind)). Build an actual root
//! node with [`CommandNode::root`](crate::tree::CommandNode::root).

/// Marker data for the root kind; carries no state of its own (Java's
/// `RootCommandNode` only fixes the constructor defaults).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RootCommandNode;

impl RootCommandNode {
    /// A root marker. Java: `new RootCommandNode<>()`.
    pub fn new() -> Self {
        Self
    }
}
