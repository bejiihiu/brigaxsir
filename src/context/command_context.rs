//! The finished execution context handed to commands. Java:
//! `com.mojang.brigadier.context.CommandContext`.

use std::{
    collections::HashMap,
    fmt::{self, Debug},
    sync::Arc,
};

use parking_lot::RwLock;

use super::{
    ParsedArgument, argument_error::ArgumentError, parsed_command_node::ParsedCommandNode,
    parsed_value::ParsedValue, string_range::StringRange,
};
use crate::{
    modifier::RedirectModifier,
    tree::{Command, CommandNode},
};

/// An immutable snapshot of one parsed command: the source it runs for, the
/// raw input, every parsed argument, the command to run, and the chain of
/// child contexts built by redirects. Java: `CommandContext<S>`.
///
/// Java builds one with the all-arguments constructor; the crate's
/// canonical route is
/// [`CommandContextBuilder::build`](crate::context::CommandContextBuilder::build),
/// which freezes the parse state into this type.
///
/// # Thread safety
///
/// The child chain is shared through `Arc` handles (Java shares plain
/// references), so `CommandContext<S, R>` is `Send + Sync` exactly when
/// `S: Send + Sync` — a finished context can be built on one thread and
/// read (or executed) on another.
pub struct CommandContext<S, R = i32> {
    /// The source this command runs for (e.g. a player or console).
    source: Arc<S>,
    input: String,
    /// Executable part of the command; run only when this context is last in
    /// the chain. Java's `command` field doc.
    command: Command<S, R>,
    arguments: HashMap<String, ParsedArgument>,
    root_node: Arc<RwLock<CommandNode<S, R>>>,
    nodes: Vec<ParsedCommandNode<S, R>>,
    range: StringRange,
    child: Option<Arc<CommandContext<S, R>>>,
    /// Modifier of the source; run only when this context has children (is
    /// not last in the chain). Java's `modifier` field doc.
    modifier: Option<Arc<RedirectModifier<S, R>>>,
    /// Special modifier for running this context and its children; only
    /// relevant when this context is not last in the chain. Exceptions from
    /// the command or modifier are ignored and the result is the number of
    /// elements run (instead of the sum of command results). Java's `forks`
    /// field doc.
    forks: bool,
}

impl<S, R> Clone for CommandContext<S, R> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            input: self.input.clone(),
            arguments: self.arguments.clone(),
            command: self.command.clone(),
            root_node: self.root_node.clone(),
            nodes: self.nodes.clone(),
            range: self.range,
            child: self.child.clone(),
            modifier: self.modifier.clone(),
            forks: self.forks,
        }
    }
}

impl<S, R> Debug for CommandContext<S, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandContext")
            .field("input", &self.input)
            .field("range", &self.range)
            .field("child", &self.child)
            .field("forks", &self.forks)
            .finish()
    }
}

impl<S, R> CommandContext<S, R> {
    /// The all-fields constructor. Java: `CommandContext(source, input,
    /// arguments, command, rootNode, nodes, range, child, modifier, forks)`.
    ///
    /// The canonical route is
    /// [`CommandContextBuilder::build`](crate::context::CommandContextBuilder::build);
    /// this mirrors Java's public constructor for the direct-construction
    /// cases, so custom parse flows can assemble a context by hand too.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: Arc<S>,
        input: String,
        arguments: HashMap<String, ParsedArgument>,
        command: Command<S, R>,
        root_node: Arc<RwLock<CommandNode<S, R>>>,
        nodes: Vec<ParsedCommandNode<S, R>>,
        range: StringRange,
        child: Option<Arc<CommandContext<S, R>>>,
        modifier: Option<Arc<RedirectModifier<S, R>>>,
        forks: bool,
    ) -> Self {
        Self {
            source,
            input,
            arguments,
            command,
            root_node,
            nodes,
            range,
            child,
            modifier,
            forks,
        }
    }

    /// Return this context but with a different source. Java:
    /// `copyFor(source)`.
    ///
    /// Identity decision: Java compares `this.source == source` (reference
    /// equality); the crate stores the source as an `Arc<S>` handle, so the
    /// same check is [`Arc::ptr_eq`] — two handles onto the same allocation
    /// are the Java "same object". On the identity path Java returns `this`
    /// itself; from `&self` the port can only return an equivalent clone
    /// (all fields are shared handles or cheap copies, so the clone is
    /// observationally the same object).
    pub fn copy_for(&self, source: Arc<S>) -> Self {
        if Arc::ptr_eq(&source, &self.source) {
            // same source instance: Java returns `this`, the clone stands in
            return self.clone();
        }

        CommandContext {
            source,
            input: self.input.clone(),
            arguments: self.arguments.clone(),
            command: self.command.clone(),
            root_node: self.root_node.clone(),
            nodes: self.nodes.clone(),
            range: self.range,
            child: self.child.clone(),
            modifier: self.modifier.clone(),
            forks: self.forks,
        }
    }

    /// The context created by the redirect this command forwarded to, if
    /// any. Java: `getChild` — it hands out the child object itself; the
    /// crate's shared handle for contexts is an [`Arc`] (the same currency
    /// [`ResultConsumer`](crate::result_consumer::ResultConsumer) and
    /// [`ContextChain`](crate::context::ContextChain) speak), so this is the
    /// handle, the way tree children come back as `Arc` handles. An `Arc`
    /// clone is the same underlying context, exactly what Java's reference
    /// return means.
    pub fn get_child(&self) -> Option<Arc<CommandContext<S, R>>> {
        self.child.clone()
    }

    /// Walks down the child chain to the deepest context. Java:
    /// `getLastChild`.
    pub fn get_last_child(&self) -> &CommandContext<S, R> {
        let mut result = self;
        while let Some(child) = result.child.as_deref() {
            result = child;
        }
        result
    }

    /// The command this context will (or did) execute, `None` when nothing
    /// executable matched. Java: `getCommand`.
    pub fn get_command(&self) -> &Command<S, R> {
        &self.command
    }

    /// The source this command runs for. Java: `getSource`.
    pub fn get_source(&self) -> &S {
        &self.source
    }

    /// The source as the owned `Arc<S>` handle.
    ///
    /// Java's `getSource` returns the source object itself and callers pass
    /// it freely; the crate stores sources as `Arc<S>` handles, and redirect
    /// modifiers have to hand ownership back
    /// (`RedirectModifier<S, R>` returns `Vec<Arc<S>>`), so the handle has to
    /// be obtainable.
    pub fn get_source_arc(&self) -> Arc<S> {
        self.source.clone()
    }

    /// Look up a parsed argument by name and read it back as `V`. Java:
    /// `getArgument(name, Class)` — same two failure modes, same two
    /// messages, panicking like Java throws `IllegalArgumentException`.
    /// Fallible callers: [`Self::try_get_argument`].
    ///
    /// Java checks `clazz.isAssignableFrom(result.getClass())`, so any
    /// supertype accepts a more specific value; Rust has no class hierarchy,
    /// so the check is "the value's concrete type is exactly `V`" (its
    /// `TypeId` matches). For every built-in argument type the two agree —
    /// Java callers ask for `int.class`/`String.class`/... and the parsed
    /// values are exactly those types.
    ///
    /// Java's `PRIMITIVE_TO_WRAPPER` map (asking for `int.class` when an
    /// `Integer` was parsed) has no Rust analogue: there are no boxed
    /// primitives, `i32` is the value type.
    ///
    /// # Panics
    ///
    /// Panics when no argument named `name` was parsed, with Java's exact
    /// message `No such argument '<name>' exists on this command`, or when
    /// the parsed value is not a `V`, with Java's exact message
    /// `Argument '<name>' is defined as <X>, not <Y>` — there `X` is the
    /// Java-style simple name of the stored value type
    /// ([`ParsedValue::type_name`], e.g. `Integer` for `i32`) and `Y` the
    /// simple name of `V`. Java prints `Y` through `Class.toString()`
    /// (fully qualified); the port renders both sides as simple names, the
    /// one meaningful reading left without a package system.
    ///
    /// [`ParsedValue::type_name`]: crate::context::ParsedValue::type_name
    ///
    /// ```
    /// # use brigaxsir::prelude::*;
    /// # let mut subject = CommandDispatcher::<()>::new();
    /// subject.register(
    ///     literal("echo")
    ///         .then(argument("value", integer()).executes(|ctx| {
    ///             // Java: ctx.getArgument("value", int.class)
    ///             *ctx.get_argument::<i32>("value")
    ///         })),
    /// );
    /// assert_eq!(subject.execute("echo 5", ()).unwrap(), 5);
    /// ```
    pub fn get_argument<V: ParsedValue>(&self, name: &str) -> &V {
        match self.try_get_argument::<V>(name) {
            Ok(value) => value,
            Err(error) => panic!("{error}"),
        }
    }

    /// The fallible companion of [`Self::get_argument`] — the documented
    /// crate extension where Java's `getArgument` only throws: same two
    /// failure modes, same exact messages (see the [`ArgumentError`]
    /// variants), returned as data instead of panicking.
    pub fn try_get_argument<V: ParsedValue>(&self, name: &str) -> Result<&V, ArgumentError> {
        let argument = self
            .arguments
            .get(name)
            .ok_or_else(|| ArgumentError::no_such_argument(name))?;
        let value = argument.get_result();
        value.as_any().downcast_ref::<V>().ok_or_else(|| {
            ArgumentError::type_mismatch(
                name,
                value.type_name(),
                super::parsed_value::display_name_of::<V>(),
            )
        })
    }

    /// The redirect modifier attached to this context, if any. Java:
    /// `getRedirectModifier`.
    pub fn get_redirect_modifier(&self) -> Option<&RedirectModifier<S, R>> {
        self.modifier.as_ref().map(|m| m.as_ref())
    }

    /// The span of input this context covers. Java: `getRange`.
    pub fn get_range(&self) -> StringRange {
        self.range
    }

    /// The raw input this command was parsed from. Java: `getInput`.
    pub fn get_input(&self) -> &str {
        &self.input
    }

    /// The root node of the tree this command was parsed against. Java:
    /// `getRootNode`.
    pub fn get_root_node(&self) -> &Arc<RwLock<CommandNode<S, R>>> {
        &self.root_node
    }

    /// Every node that matched while parsing, in order. Java: `getNodes`.
    pub fn get_nodes(&self) -> &[ParsedCommandNode<S, R>] {
        &self.nodes
    }

    /// Whether any node matched at all. Java: `hasNodes`.
    pub fn has_nodes(&self) -> bool {
        !self.nodes.is_empty()
    }

    /// Whether this context forks execution into multiple sources. Java:
    /// `isForked`.
    pub fn is_forked(&self) -> bool {
        self.forks
    }
}

impl<S, R> PartialEq for CommandContext<S, R> {
    /// Java's `equals`, field for field: `arguments`, `rootNode`, `nodes`,
    /// `command`, `source`, `child` — and *only* those: `range`, `input`,
    /// `modifier` and `forks` are deliberately not compared in Java either
    /// (probably an oversight, but the port mirrors it exactly, so two
    /// contexts that differ only in their input range stay `equals`-equal
    /// here too).
    ///
    /// Representation decisions for the erased/opaque fields:
    /// - `command`: the command closures compare by `Arc::ptr_eq` (Java's
    ///   lambda identity, the crate-wide rule from the tree layer).
    /// - `source`: `Arc::ptr_eq` — Java compares `S.equals`; for the mock
    ///   sources the Java tests use (plain `Object`s) identity *is* their
    ///   equality, and the crate has no `S: PartialEq` bound to lean on.
    /// - `root_node` and `nodes`: compared by value through
    ///   [`CommandNode`]/[`ParsedCommandNode`] equality, like Java's
    ///   `rootNode.equals(...)` / list `equals`.
    fn eq(&self, other: &Self) -> bool {
        self.arguments == other.arguments
            && *self.root_node.read() == *other.root_node.read()
            && self.nodes == other.nodes
            && match (&self.command, &other.command) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && Arc::ptr_eq(&self.source, &other.source)
            && self.child == other.child
    }
}
