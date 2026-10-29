//! The fluent builder used to assemble command nodes, mirroring Java's
//! `com.mojang.brigadier.builder.ArgumentBuilder`.
//!
//! Java models the builder family as the abstract `ArgumentBuilder<S, T>`
//! (the shared state and fluent surface; `T` is the recursive self type that
//! keeps chaining on the concrete subclass) with the two concrete subclasses
//! `LiteralArgumentBuilder<S>` / `RequiredArgumentBuilder<S, T>`. The Rust
//! port keeps **one** struct here — [`ArgumentBuilder<S, R, K>`] — where `K`
//! is the kind data Java's subclasses carry in their own fields, and the two
//! concrete builders are type aliases over it:
//!
//! - [`LiteralArgumentBuilder`] = `ArgumentBuilder<S, R, LiteralCommandNode>`
//! - [`RequiredArgumentBuilder`] = `ArgumentBuilder<S, R,
//!   ArgumentCommandNode<S, R>>`
//!
//! Every fluent method consumes `self` and returns `Self`, so a chain keeps
//! its concrete builder type — that is Java's `getThis()` trick without a
//! trait. Kind-specific methods (`get_literal`, `suggests`, ...) live on
//! impls with `K` pinned, so they simply don't exist on the wrong kind (no
//! runtime check needed, unlike the Java `instanceof` world).
//!
//! `K` is unbounded on purpose: a custom builder kind is your own struct
//! passed to [`ArgumentBuilder::new`], exactly like Java's "extend the
//! abstract builder" extension point.

use std::{
    fmt::{self, Debug},
    sync::Arc,
};

use parking_lot::RwLock;

use crate::{
    context::CommandContext,
    errors::CommandSyntaxException,
    modifier::{RedirectModifier, SingleRedirectModifier},
    tree::{Command, CommandNode},
};

use super::{
    literal_argument_builder::LiteralArgumentBuilder,
    required_argument_builder::RequiredArgumentBuilder,
};

/// The shared builder state plus the fluent surface every builder kind
/// offers. Java: the abstract `ArgumentBuilder<S, T extends
/// ArgumentBuilder<S, T>>` — its fields and methods; the `K` type parameter
/// stands in for what the concrete subclasses add (the literal keyword / the
/// argument name + type + suggestion provider), so the two concrete builders
/// are this struct with `K` pinned.
pub struct ArgumentBuilder<S, R, K> {
    // visible to the sibling alias modules: they carry the kind-specific
    // impls (get_literal / suggests / build) and touch these fields directly
    pub(super) kind: K,
    /// The child container. Java: a `RootCommandNode<S>` field — `then` adds
    /// through its `addChild`, which *merges* same-name children right away
    /// (later command wins, grandchildren absorbed), so [`Self::get_arguments`]
    /// observes the merged state, not a raw call list. The container is a
    /// plain node handle for exactly that reason.
    pub(super) arguments: Arc<RwLock<CommandNode<S, R>>>,

    pub(super) command: Command<S, R>,
    pub(super) requirement: Arc<dyn Fn(&S) -> bool + Send + Sync>,
    pub(super) target: Option<Arc<RwLock<CommandNode<S, R>>>>,

    pub(super) forks: bool,
    pub(super) modifier: Option<Arc<RedirectModifier<S, R>>>,
}

impl<S, R, K> ArgumentBuilder<S, R, K> {
    /// A builder for the given kind data, with no command, an always-true
    /// requirement, and no redirect. Java: the `ArgumentBuilder` constructor
    /// (protected there, called by the subclass constructors) — here it is
    /// the public extension point for custom builder kinds; the built-in
    /// kinds are built through [`literal`](crate::builder::literal_argument_builder::literal)
    /// and [`argument`](crate::builder::required_argument_builder::argument).
    pub fn new(kind: K) -> Self {
        Self {
            kind,
            arguments: Arc::new(RwLock::new(CommandNode::root())),
            command: None,
            requirement: Arc::new(|_| true),
            target: None,
            forks: false,
            modifier: None,
        }
    }

    /// The crate-internal constructor `create_builder` builds from: every
    /// part is set directly, like Java's builder fields being assigned.
    pub(crate) fn from_parts(
        kind: K,
        command: Command<S, R>,
        requirement: Arc<dyn Fn(&S) -> bool + Send + Sync>,
        target: Option<Arc<RwLock<CommandNode<S, R>>>>,
        modifier: Option<Arc<RedirectModifier<S, R>>>,
        forks: bool,
    ) -> Self {
        Self {
            kind,
            arguments: Arc::new(RwLock::new(CommandNode::root())),
            command,
            requirement,
            target,
            forks,
            modifier,
        }
    }

    /// Continue building this node with a child node. Any builder kind can be
    /// the child — it is built right away and its node becomes this node's
    /// child. Java: `then(ArgumentBuilder<S, ?>)`.
    ///
    /// ```
    /// # use brigaxsir::prelude::*;
    /// # let mut subject = CommandDispatcher::<()>::new();
    /// literal("foo").then(literal("bar").executes(|ctx: &CommandContext<()>| 42))
    /// # ;
    /// ```
    ///
    /// # Panics
    ///
    /// Panics when this node already has a redirect target — Java throws
    /// `IllegalStateException("Cannot add children to a redirected node")`.
    pub fn then(self, argument: impl IntoCommandNode<S, R>) -> Self {
        if self.target.is_some() {
            panic!("Cannot add children to a redirected node");
        }
        self.then_built(argument.into_node())
    }

    /// Add an already built child node to this node. The child merges into
    /// the builder's container immediately (Java's `arguments.addChild`):
    /// a same-name child already present keeps its position, takes this
    /// node's command when there is one, and absorbs this node's
    /// grandchildren. The handle is kept as the tree's own child (Java:
    /// `then(CommandNode)` — the very same object ends up in the tree), so
    /// identity-based lookups keep working on it.
    ///
    /// You should usually use [`Self::then`] instead.
    ///
    /// # Panics
    ///
    /// Panics when this node already has a redirect target — Java throws
    /// `IllegalStateException("Cannot add children to a redirected node")`.
    pub fn then_built(self, argument: Arc<RwLock<CommandNode<S, R>>>) -> Self {
        if self.target.is_some() {
            panic!("Cannot add children to a redirected node");
        }
        self.arguments.write().add_child(&argument);
        self
    }

    /// Set the command to be executed when this node is reached.
    ///
    /// If this is not present on a node, it is not a valid command.
    ///
    /// ```
    /// # use brigaxsir::prelude::*;
    /// # let mut subject = CommandDispatcher::<()>::new();
    /// # subject.register(
    /// literal("foo").executes(|ctx: &CommandContext<()>| 42)
    /// # );
    /// ```
    pub fn executes<F>(mut self, f: F) -> Self
    where
        F: Fn(&CommandContext<S, R>) -> R + Send + Sync + 'static,
    {
        self.command = Some(Arc::new(move |ctx: &CommandContext<S, R>| Ok(f(ctx))));
        self
    }

    /// Same as [`Self::executes`] but the closure returns a
    /// `Result<R, CommandSyntaxException>`, so a command can fail with a proper
    /// syntax error instead of a panic. Java's commands throw; this is the
    /// Rust-shaped equivalent.
    pub fn executes_result<F>(mut self, f: F) -> Self
    where
        F: Fn(&CommandContext<S, R>) -> Result<R, CommandSyntaxException> + Send + Sync + 'static,
    {
        self.command = Some(Arc::new(f));
        self
    }

    /// Set the requirement for this node to be considered.
    ///
    /// If this is not present on a node, it is considered to always pass.
    ///
    /// ```
    /// # use brigaxsir::prelude::*;
    /// # use std::sync::Arc;
    /// # pub struct CommandSource {
    /// #     pub opped: bool,
    /// # }
    /// # let mut subject = CommandDispatcher::<CommandSource>::new();
    /// # subject.register(
    /// literal("foo")
    ///     .requires(|s: &CommandSource| s.opped)
    ///     // ...
    ///     # .executes(|ctx: &CommandContext<CommandSource>| 42)
    /// # );
    pub fn requires<F>(mut self, requirement: F) -> Self
    where
        F: Fn(&S) -> bool + Send + Sync + 'static,
    {
        self.requirement = Arc::new(requirement);
        self
    }

    /// Redirect to `target` without a source modifier. Java:
    /// `redirect(target)`.
    pub fn redirect(self, target: Arc<RwLock<CommandNode<S, R>>>) -> Self {
        self.forward(target, None, false)
    }

    /// Redirect to `target` with a single-source modifier. Java:
    /// `redirect(target, SingleRedirectModifier)` — the modifier is wrapped
    /// into a singleton [`RedirectModifier`] (`Collections.singleton(...)`
    /// there, a one-element vector here), so this redirect never forks.
    /// Java's `redirect(target, null)` is [`Self::redirect`].
    pub fn redirect_with(
        self,
        target: Arc<RwLock<CommandNode<S, R>>>,
        modifier: Arc<SingleRedirectModifier<S, R>>,
    ) -> Self
    where
        // the singleton wrapper is a `dyn Fn ... + Send + Sync` (implicitly
        // `'static`), so both parameters must outlive any borrow
        S: 'static,
        R: 'static,
    {
        let wrapped: Arc<RedirectModifier<S, R>> = Arc::new(move |context| {
            let source = modifier(context)?;
            Ok(vec![source])
        });
        self.forward(target, Some(wrapped), false)
    }

    /// Redirect to `target` with a modifier and fork the execution, so every
    /// modified source runs the target independently. Java: `fork(target,
    /// modifier)`.
    pub fn fork(
        self,
        target: Arc<RwLock<CommandNode<S, R>>>,
        modifier: Arc<RedirectModifier<S, R>>,
    ) -> Self {
        self.forward(target, Some(modifier), true)
    }

    /// The shared implementation behind [`Self::redirect`],
    /// [`Self::redirect_with`] and [`Self::fork`]. Java:
    /// `forward(target, modifier, fork)`.
    ///
    /// # Panics
    ///
    /// Panics if this node already has children (Java throws
    /// `IllegalStateException("Cannot forward a node with children")`).
    pub fn forward(
        mut self,
        target: Arc<RwLock<CommandNode<S, R>>>,
        modifier: Option<Arc<RedirectModifier<S, R>>>,
        fork: bool,
    ) -> Self {
        if !self.arguments.read().get_children().is_empty() {
            panic!("Cannot forward a node with children");
        }
        self.target = Some(target);
        self.modifier = modifier;
        self.forks = fork;
        self
    }

    /// The child nodes collected so far, merged by name (Java's
    /// `getArguments` returns the container's children — insertion order,
    /// same-name additions already collapsed by [`Self::then_built`]). Java:
    /// `getArguments`.
    pub fn get_arguments(&self) -> Vec<Arc<RwLock<CommandNode<S, R>>>> {
        self.arguments.read().get_children()
    }

    /// The command set with [`Self::executes`], if any. Java: `getCommand`.
    pub fn get_command(&self) -> &Command<S, R> {
        &self.command
    }

    /// The requirement set with [`Self::requires`]. Java: `getRequirement`.
    pub fn get_requirement(&self) -> &Arc<dyn Fn(&S) -> bool + Send + Sync> {
        &self.requirement
    }

    /// The redirect target, if any. Java: `getRedirect`.
    pub fn get_redirect(&self) -> Option<Arc<RwLock<CommandNode<S, R>>>> {
        self.target.clone()
    }

    /// The redirect modifier, if any. Java: `getRedirectModifier`.
    pub fn get_redirect_modifier(&self) -> Option<&Arc<RedirectModifier<S, R>>> {
        self.modifier.as_ref()
    }

    /// Whether execution forks across modified sources. Java: `isFork`.
    pub fn is_fork(&self) -> bool {
        self.forks
    }
}

impl<S, R, K: Debug> Debug for ArgumentBuilder<S, R, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArgumentBuilder")
            .field("kind", &self.kind)
            .field("children", &self.arguments.read().get_children().len())
            .field("target", &self.target)
            .field("forks", &self.forks)
            .finish()
    }
}

impl<S, R, K: Clone> Clone for ArgumentBuilder<S, R, K> {
    fn clone(&self) -> Self {
        let cloned = Self {
            kind: self.kind.clone(),
            arguments: Arc::new(RwLock::new(CommandNode::root())),
            command: self.command.clone(),
            requirement: self.requirement.clone(),
            target: self.target.clone(),
            forks: self.forks,
            modifier: self.modifier.clone(),
        };
        // re-add each child so the clone is an independent snapshot with the
        // same merged container state (a shared container would make clones
        // alias each other's children; Java's builder has no clone at all)
        for child in self.arguments.read().get_children() {
            cloned.arguments.write().add_child(&child);
        }
        cloned
    }
}

/// The child of [`ArgumentBuilder::then`]: any builder that can produce a
/// finished node. Java: the `then(ArgumentBuilder<S, ?>)` parameter — every
/// builder builds into a node, so both concrete builders (and
/// [`AnyArgumentBuilder`]) implement this; your own builder kinds can too.
pub trait IntoCommandNode<S, R> {
    /// Build this into the node handle `then` stores. Java: `build()` inside
    /// `arguments.addChild(...)`.
    fn into_node(self) -> Arc<RwLock<CommandNode<S, R>>>;
}

/// Java's `ArgumentBuilder<S, ?>` as a value: the result of
/// [`CommandNode::create_builder`](crate::tree::CommandNode::create_builder),
/// which hands back whichever concrete builder matches the node's kind. Java
/// returns the concrete subclass through the covariant `createBuilder`
/// override; a single Rust method can't pick a return type, so the enum
/// stands in — pattern match on it for the downcast, or use the kind getters
/// (which panic on the wrong kind, the Java `ClassCastException` parity).
pub enum AnyArgumentBuilder<S, R = i32> {
    /// A literal builder. Java: `LiteralArgumentBuilder<S>`.
    Literal(LiteralArgumentBuilder<S, R>),
    /// A required-argument builder. Java: `RequiredArgumentBuilder<S, T>`.
    Required(RequiredArgumentBuilder<S, R>),
}

impl<S, R> AnyArgumentBuilder<S, R> {
    /// Delegates to the inner builder's [`ArgumentBuilder::then`].
    pub fn then(self, argument: impl IntoCommandNode<S, R>) -> Self {
        match self {
            Self::Literal(builder) => Self::Literal(builder.then(argument)),
            Self::Required(builder) => Self::Required(builder.then(argument)),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::then_built`].
    pub fn then_built(self, argument: Arc<RwLock<CommandNode<S, R>>>) -> Self {
        match self {
            Self::Literal(builder) => Self::Literal(builder.then_built(argument)),
            Self::Required(builder) => Self::Required(builder.then_built(argument)),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::executes`].
    pub fn executes<F>(self, f: F) -> Self
    where
        F: Fn(&CommandContext<S, R>) -> R + Send + Sync + 'static,
    {
        match self {
            Self::Literal(builder) => Self::Literal(builder.executes(f)),
            Self::Required(builder) => Self::Required(builder.executes(f)),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::executes_result`].
    pub fn executes_result<F>(self, f: F) -> Self
    where
        F: Fn(&CommandContext<S, R>) -> Result<R, CommandSyntaxException> + Send + Sync + 'static,
    {
        match self {
            Self::Literal(builder) => Self::Literal(builder.executes_result(f)),
            Self::Required(builder) => Self::Required(builder.executes_result(f)),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::requires`].
    pub fn requires<F>(self, requirement: F) -> Self
    where
        F: Fn(&S) -> bool + Send + Sync + 'static,
    {
        match self {
            Self::Literal(builder) => Self::Literal(builder.requires(requirement)),
            Self::Required(builder) => Self::Required(builder.requires(requirement)),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::redirect`].
    pub fn redirect(self, target: Arc<RwLock<CommandNode<S, R>>>) -> Self {
        match self {
            Self::Literal(builder) => Self::Literal(builder.redirect(target)),
            Self::Required(builder) => Self::Required(builder.redirect(target)),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::redirect_with`].
    pub fn redirect_with(
        self,
        target: Arc<RwLock<CommandNode<S, R>>>,
        modifier: Arc<SingleRedirectModifier<S, R>>,
    ) -> Self
    where
        S: 'static,
        R: 'static,
    {
        match self {
            Self::Literal(builder) => Self::Literal(builder.redirect_with(target, modifier)),
            Self::Required(builder) => Self::Required(builder.redirect_with(target, modifier)),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::fork`].
    pub fn fork(
        self,
        target: Arc<RwLock<CommandNode<S, R>>>,
        modifier: Arc<RedirectModifier<S, R>>,
    ) -> Self {
        match self {
            Self::Literal(builder) => Self::Literal(builder.fork(target, modifier)),
            Self::Required(builder) => Self::Required(builder.fork(target, modifier)),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::forward`].
    pub fn forward(
        self,
        target: Arc<RwLock<CommandNode<S, R>>>,
        modifier: Option<Arc<RedirectModifier<S, R>>>,
        fork: bool,
    ) -> Self {
        match self {
            Self::Literal(builder) => Self::Literal(builder.forward(target, modifier, fork)),
            Self::Required(builder) => Self::Required(builder.forward(target, modifier, fork)),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::get_arguments`].
    pub fn get_arguments(&self) -> Vec<Arc<RwLock<CommandNode<S, R>>>> {
        match self {
            Self::Literal(builder) => builder.get_arguments(),
            Self::Required(builder) => builder.get_arguments(),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::get_command`].
    pub fn get_command(&self) -> &Command<S, R> {
        match self {
            Self::Literal(builder) => builder.get_command(),
            Self::Required(builder) => builder.get_command(),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::get_requirement`].
    pub fn get_requirement(&self) -> &Arc<dyn Fn(&S) -> bool + Send + Sync> {
        match self {
            Self::Literal(builder) => builder.get_requirement(),
            Self::Required(builder) => builder.get_requirement(),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::get_redirect`].
    pub fn get_redirect(&self) -> Option<Arc<RwLock<CommandNode<S, R>>>> {
        match self {
            Self::Literal(builder) => builder.get_redirect(),
            Self::Required(builder) => builder.get_redirect(),
        }
    }

    /// Delegates to the inner builder's
    /// [`ArgumentBuilder::get_redirect_modifier`].
    pub fn get_redirect_modifier(&self) -> Option<&Arc<RedirectModifier<S, R>>> {
        match self {
            Self::Literal(builder) => builder.get_redirect_modifier(),
            Self::Required(builder) => builder.get_redirect_modifier(),
        }
    }

    /// Delegates to the inner builder's [`ArgumentBuilder::is_fork`].
    pub fn is_fork(&self) -> bool {
        match self {
            Self::Literal(builder) => builder.is_fork(),
            Self::Required(builder) => builder.is_fork(),
        }
    }

    /// The literal keyword. Java: the `(LiteralArgumentBuilder<S, ?>) builder`
    /// downcast the covariant override makes compile-time safe.
    ///
    /// # Panics
    ///
    /// Panics when this is a required-argument builder — Java's downcast
    /// raising `ClassCastException`; pattern matching on the variants is the
    /// safe alternative.
    pub fn get_literal(&self) -> &str {
        match self {
            Self::Literal(builder) => builder.get_literal(),
            Self::Required(_) => {
                panic!("AnyArgumentBuilder::get_literal() called on a required-argument builder")
            }
        }
    }

    /// The argument name. Java: the `(RequiredArgumentBuilder<S, ?>) builder`
    /// downcast the covariant override makes compile-time safe.
    ///
    /// # Panics
    ///
    /// Panics when this is a literal builder — Java's downcast raising
    /// `ClassCastException`; pattern matching on the variants is the safe
    /// alternative.
    pub fn get_name(&self) -> &str {
        match self {
            Self::Required(builder) => builder.get_name(),
            Self::Literal(_) => {
                panic!("AnyArgumentBuilder::get_name() called on a literal builder")
            }
        }
    }

    /// The argument parser. Java: the `(RequiredArgumentBuilder<S, ?>)`
    /// builder downcast the covariant override makes compile-time safe.
    ///
    /// # Panics
    ///
    /// Panics when this is a literal builder — Java's downcast raising
    /// `ClassCastException`; pattern matching on the variants is the safe
    /// alternative.
    pub fn get_type(&self) -> &Arc<dyn crate::arguments::ArgumentType + Send + Sync> {
        match self {
            Self::Required(builder) => builder.get_type(),
            Self::Literal(_) => {
                panic!("AnyArgumentBuilder::get_type() called on a literal builder")
            }
        }
    }

    /// Build into the node the inner builder produces. Java: the abstract
    /// `build()` dispatching to the subclass override.
    pub fn build(self) -> CommandNode<S, R> {
        match self {
            Self::Literal(builder) => builder.build(),
            Self::Required(builder) => builder.build(),
        }
    }
}

impl<S, R> IntoCommandNode<S, R> for AnyArgumentBuilder<S, R> {
    fn into_node(self) -> Arc<RwLock<CommandNode<S, R>>> {
        match self {
            Self::Literal(builder) => builder.into_node(),
            Self::Required(builder) => builder.into_node(),
        }
    }
}
