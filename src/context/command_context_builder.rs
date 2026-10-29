//! The mutable context under construction while parsing. Java:
//! `com.mojang.brigadier.context.CommandContextBuilder`.

use std::{
    fmt::{self, Debug},
    sync::Arc,
};

use indexmap::IndexMap;
use parking_lot::RwLock;

use super::{
    ParsedArgument, command_context::CommandContext, parsed_command_node::ParsedCommandNode,
    string_range::StringRange, suggestion_context::SuggestionContext,
};
use crate::{
    command_dispatcher::CommandDispatcher,
    modifier::RedirectModifier,
    tree::{Command, CommandNode},
};

/// The mutable counterpart of [`CommandContext`]: the dispatcher fills this
/// in node by node during parsing, then `build` freezes it into the final
/// context handed to commands. Java: `CommandContextBuilder<S>`.
///
/// The arguments map is insertion-ordered (`IndexMap`), Java's
/// `LinkedHashMap` — `get_arguments` iterates in argument order. (The built
/// [`CommandContext`] uses a plain `HashMap` like Java's `HashMap` field —
/// the order is lost at `build` time in Java too.)
///
/// # Thread safety
///
/// The child chain is shared through `Arc` handles (Java shares plain
/// references), so `CommandContextBuilder<'a, S, R>` is `Send + Sync`
/// exactly when `S: Send + Sync` — a `ParseResults` built by one thread can
/// be handed to another.
pub struct CommandContextBuilder<'a, S, R> {
    /// Arguments parsed so far, by name, in parse order.
    arguments: IndexMap<String, ParsedArgument>,
    /// The root of the tree being parsed against. Java's `rootNode`.
    root_node: Arc<RwLock<CommandNode<S, R>>>,
    /// Nodes matched so far, in order.
    nodes: Vec<ParsedCommandNode<S, R>>,
    /// The dispatcher doing the parsing.
    dispatcher: &'a CommandDispatcher<S, R>,
    /// The source this parse runs for.
    source: Arc<S>,
    /// The command to execute at this level, once a node with one matched.
    command: Command<S, R>,
    /// The child context, when a redirect produced one.
    child: Option<Arc<CommandContextBuilder<'a, S, R>>>,
    /// The span of input consumed so far.
    range: StringRange,
    /// The redirect modifier of the last node matched at this level.
    modifier: Option<Arc<RedirectModifier<S, R>>>,
    /// Whether the last node matched at this level forks.
    forks: bool,
}

impl<S, R> Clone for CommandContextBuilder<'_, S, R> {
    /// Java's builder has no `clone()` — the one sanctioned copy operation is
    /// [`Self::copy`], and this impl is exactly that, so nothing can
    /// accidentally diverge from Java's semantics (in particular: the
    /// redirect modifier is *not* carried over).
    fn clone(&self) -> Self {
        self.copy()
    }
}

impl<'a, S, R> CommandContextBuilder<'a, S, R> {
    /// A fresh builder for `source`, anchored at input position `start`.
    /// Java: `CommandContextBuilder(dispatcher, source, rootNode, start)`.
    pub fn new(
        dispatcher: &'a CommandDispatcher<S, R>,
        source: Arc<S>,
        root_node: Arc<RwLock<CommandNode<S, R>>>,
        start: usize,
    ) -> Self {
        Self {
            arguments: IndexMap::new(),
            root_node,
            source,
            range: StringRange::at(start),
            command: None,
            dispatcher,
            nodes: vec![],
            child: None,
            modifier: None,
            forks: false,
        }
    }

    /// Replace the source this parse runs for. Java: `withSource`.
    pub fn with_source(&mut self, source: Arc<S>) -> &mut Self {
        self.source = source;
        self
    }

    /// The source this parse runs for. Java: `getSource`.
    pub fn get_source(&self) -> &S {
        &self.source
    }

    /// The source as the owned `Arc<S>` handle, for callers that hand the
    /// source onward (the dispatcher building child builders). Crate-internal:
    /// the public surface is [`Self::get_source`].
    pub(crate) fn get_source_arc(&self) -> Arc<S> {
        self.source.clone()
    }

    /// The root of the tree being parsed against. Java: `getRootNode`.
    pub fn get_root_node(&self) -> &Arc<RwLock<CommandNode<S, R>>> {
        &self.root_node
    }

    /// Record a parsed argument under `name`. Java: `withArgument`.
    pub fn with_argument(&mut self, name: &str, argument: ParsedArgument) -> &mut Self {
        self.arguments.insert(name.to_owned(), argument);
        self
    }

    /// Every argument parsed so far, keyed by name in parse order. Java:
    /// `getArguments`.
    pub fn get_arguments(&self) -> &IndexMap<String, ParsedArgument> {
        &self.arguments
    }

    /// Attach the command to execute at this level. Java: `withCommand`.
    pub fn with_command(&mut self, command: &Command<S, R>) -> &mut Self {
        self.command.clone_from(command);
        self
    }

    /// The command to execute at this level, `None` when none matched. Java:
    /// `getCommand`.
    pub fn get_command(&self) -> &Command<S, R> {
        &self.command
    }

    /// Record that `node` matched `range`; widens this builder's range and
    /// inherits the node's redirect modifier and fork flag. Java: `withNode`.
    pub fn with_node(
        &mut self,
        node: Arc<RwLock<CommandNode<S, R>>>,
        range: StringRange,
    ) -> &mut Self {
        self.nodes.push(ParsedCommandNode::new(node.clone(), range));
        self.range = StringRange::encompassing(&self.range, &range);
        // one read guard for both fields (Java reads the node once; the
        // previous form re-locked for `is_fork`)
        let node_data = node.read();
        self.modifier
            .clone_from(&node_data.get_redirect_modifier().cloned());
        self.forks = node_data.is_fork();
        self
    }

    /// The nodes matched so far, in order. Java: `getNodes`.
    pub fn get_nodes(&self) -> &[ParsedCommandNode<S, R>] {
        &self.nodes
    }

    /// Copy this builder for a sibling candidate. Java: `copy` — exactly:
    /// the command, arguments, nodes, child, range and fork flag carry over,
    /// the child stays the *same shared* builder (Java copies the reference,
    /// the clone of the `Arc` is that), and the redirect modifier does **not**
    /// carry over — `copy` starts from a fresh `modifier = null` and the next
    /// [`Self::with_node`] sets it. That looks like a Java oversight, but it
    /// is the semantics every parse depends on, so the port reproduces it
    /// byte for byte (see [`Clone`] for why there is no "fuller" clone).
    pub fn copy(&self) -> Self {
        let mut copy = Self::new(
            self.dispatcher,
            self.source.clone(),
            self.root_node.clone(),
            self.range.start(),
        );
        copy.command = self.command.clone();
        copy.arguments = self.arguments.clone();
        copy.nodes = self.nodes.clone();
        copy.child = self.child.clone();
        copy.range = self.range;
        copy.forks = self.forks;
        copy
    }

    /// Attach the child context produced by a redirect. Java: `withChild`.
    pub fn with_child(&mut self, child: Arc<CommandContextBuilder<'a, S, R>>) -> &mut Self {
        self.child = Some(child);
        self
    }

    /// The child context, when a redirect produced one. Java: `getChild` —
    /// returned as the shared `Arc` handle, the same currency
    /// [`Self::with_child`] takes.
    pub fn get_child(&self) -> Option<Arc<CommandContextBuilder<'a, S, R>>> {
        self.child.clone()
    }

    /// Walks down the child chain to the deepest builder. Java:
    /// `getLastChild`.
    pub fn get_last_child(&self) -> &CommandContextBuilder<'a, S, R> {
        let mut result = self;
        while let Some(child) = result.child.as_deref() {
            result = child;
        }
        result
    }

    /// Freeze this builder into a [`CommandContext`]. `input` is the full
    /// command text; child contexts are built recursively. Java: `build`.
    ///
    /// Java shares the arguments map and the nodes list between the builder
    /// and the built context; the port clones them (Rust cannot hand out a
    /// mutable-shared map) — no Java test observes a post-`build` mutation,
    /// so the observable behavior is identical.
    pub fn build(&self, input: &str) -> CommandContext<S, R> {
        CommandContext::new(
            self.source.clone(),
            input.to_owned(),
            // Java's context field is a plain HashMap and the builder map is
            // a LinkedHashMap — the order is lost at build time in Java too
            self.arguments.clone().into_iter().collect(),
            self.command.clone(),
            self.root_node.clone(),
            self.nodes.clone(),
            self.range,
            self.child.clone().map(|c| Arc::new(c.build(input))),
            self.modifier.clone(),
            self.forks,
        )
    }

    /// The dispatcher doing the parsing. Java: `getDispatcher`.
    pub fn get_dispatcher(&self) -> &'a CommandDispatcher<S, R> {
        self.dispatcher
    }

    /// The span of input consumed so far. Java: `getRange`.
    pub fn get_range(&self) -> StringRange {
        self.range
    }

    /// Find which node's children should provide suggestions at `cursor`.
    /// Java: `findSuggestionContext`, including its panics and fallbacks:
    /// a cursor before this builder's range start panics with Java's exact
    /// `IllegalStateException` message; past-the-end cursors delegate to the
    /// child chain and fall back to the last parsed node (or the root). The
    /// returned context is the builder at the level the parent node was
    /// found at, like Java passing `this` down the recursion.
    ///
    /// Java's `if (prev == null) throw` after the loop is dead code (`prev`
    /// starts at the non-null root) and is not reproduced.
    ///
    /// # Panics
    ///
    /// Panics with `Can't find node before cursor` when `cursor` lies before
    /// this builder's range start.
    pub fn find_suggestion_context(&self, cursor: usize) -> SuggestionContext<'_, 'a, S, R> {
        if self.range.start() > cursor {
            panic!("Can't find node before cursor");
        }

        if self.range.end() < cursor {
            match &self.child {
                Some(child) => child.find_suggestion_context(cursor),
                _ => match self.nodes.last() {
                    Some(last) => SuggestionContext {
                        context: self,
                        parent: last.get_node(),
                        start_pos: last.get_range().end() + 1,
                    },
                    _ => SuggestionContext {
                        context: self,
                        parent: Arc::clone(&self.root_node),
                        start_pos: self.range.start(),
                    },
                },
            }
        } else {
            let mut prev = self.root_node.clone();
            for node in &self.nodes {
                let node_range = node.get_range();
                if node_range.start() <= cursor && cursor <= node_range.end() {
                    return SuggestionContext {
                        context: self,
                        parent: prev,
                        start_pos: node_range.start(),
                    };
                }
                prev = node.get_node();
            }
            SuggestionContext {
                context: self,
                parent: prev,
                start_pos: self.range.start(),
            }
        }
    }
}

impl<S, R> Debug for CommandContextBuilder<'_, S, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandContextBuilder")
            .field("root_node", &self.root_node)
            .field("child", &self.child)
            .field("range", &self.range)
            .field("forks", &self.forks)
            .finish()
    }
}
