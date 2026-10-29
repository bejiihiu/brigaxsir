//! The command tree: nodes that make up the parseable command graph.
//! Mirrors Java's `com.mojang.brigadier.tree` package (`CommandNode`,
//! `LiteralCommandNode`, `ArgumentCommandNode`, `RootCommandNode`).
//!
//! Java models literals/arguments/root as `CommandNode` subclasses; the Rust
//! port keeps a single [`CommandNode`] type whose [`CommandNodeKind`] payload
//! says which kind it is (Java's `instanceof` checks become `is_literal` /
//! `is_argument` / `is_root`, and the subclass getters live on the payload
//! structs). Node identity follows Java's object identity: everything that
//! hands nodes around passes the same `Arc` handle, and the identity-needing
//! operations (`parse`, `find_ambiguities`) take that handle explicitly
//! where Java would use `this`.

use std::{
    cmp::Ordering,
    collections::HashSet,
    fmt::{self, Debug},
    sync::Arc,
};

use indexmap::IndexMap;
use parking_lot::RwLock;

use crate::{
    ambiguity_consumer::AmbiguityConsumer,
    builder::argument_builder::{AnyArgumentBuilder, ArgumentBuilder},
    context::{CommandContext, CommandContextBuilder, ParsedArgument, StringRange},
    errors::{BuiltInExceptionProvider, CommandSyntaxException, built_in_exceptions},
    modifier::RedirectModifier,
    string_reader::StringReader,
    suggestion::{Suggestions, SuggestionsBuilder, SuggestionsFuture},
};

mod argument_command_node;
mod literal_command_node;
mod root_command_node;

pub use argument_command_node::ArgumentCommandNode;
pub use literal_command_node::LiteralCommandNode;
pub use root_command_node::RootCommandNode;

/// A command closure: `Some` when the node is executable. Java's `Command<S>`
/// functional interface, with `Result` instead of thrown exceptions.
pub type Command<S, R> =
    Option<Arc<dyn Fn(&CommandContext<S, R>) -> Result<R, CommandSyntaxException> + Send + Sync>>;

/// The conventional success result a command returns: `1`. Java:
/// `Command.SINGLE_SUCCESS` (`int SINGLE_SUCCESS = 1`).
///
/// The constant is `i32`, the crate's default command result type
/// ([`CommandNodeKind`]'s `R` default). A command whose result type is not
/// `i32` writes its own `1` — Java has no analogue of other result types, so
/// there is nothing further to mirror.
pub const SINGLE_SUCCESS: i32 = 1;

/// Which kind of node this is — Java's `CommandNode` subclasses as variants.
/// The payloads hold the per-kind data (`LiteralCommandNode` /
/// `ArgumentCommandNode`); the shared node state (children, command,
/// redirect, ...) lives on [`CommandNode`] itself.
pub enum CommandNodeKind<S, R = i32> {
    /// A literal keyword, e.g. `foo` in `foo bar`. Java:
    /// `LiteralCommandNode<S>`.
    Literal(LiteralCommandNode),
    /// A required argument, e.g. `<name>`. Java: `ArgumentCommandNode<S, T>`.
    Argument(ArgumentCommandNode<S, R>),
    /// The tree root. Java: `RootCommandNode<S>`.
    Root(RootCommandNode),
}

impl<S, R> Clone for CommandNodeKind<S, R> {
    fn clone(&self) -> Self {
        match self {
            Self::Literal(literal) => Self::Literal(literal.clone()),
            Self::Argument(argument) => Self::Argument(argument.clone()),
            Self::Root(root) => Self::Root(root.clone()),
        }
    }
}

// manual: the derive would demand `S: Debug`/`R: Debug` even though neither
// generic parameter appears in the payloads' debug output
impl<S, R> Debug for CommandNodeKind<S, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(literal) => Debug::fmt(literal, f),
            Self::Argument(argument) => Debug::fmt(argument, f),
            Self::Root(root) => Debug::fmt(root, f),
        }
    }
}

impl<S, R> From<LiteralCommandNode> for CommandNodeKind<S, R> {
    fn from(literal: LiteralCommandNode) -> Self {
        Self::Literal(literal)
    }
}

impl<S, R> From<ArgumentCommandNode<S, R>> for CommandNodeKind<S, R> {
    fn from(argument: ArgumentCommandNode<S, R>) -> Self {
        Self::Argument(argument)
    }
}

impl<S, R> PartialEq for CommandNodeKind<S, R> {
    /// Java: the subclass `equals` overrides — a node of one kind never
    /// equals a node of another; within a kind the payload fields compare
    /// (literal keyword / argument name + argument type).
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(a), Self::Literal(b)) => a == b,
            (Self::Argument(a), Self::Argument(b)) => a == b,
            (Self::Root(_), Self::Root(_)) => true,
            _ => false,
        }
    }
}

/// A node in the command tree: a literal keyword, a required argument, or
/// the root. Java: `CommandNode<S>` (plus its three subclasses folded into
/// [`CommandNodeKind`]).
///
/// Nodes are cheap clone handles over the shared state: the tree, the parse
/// contexts and every API surface deal in `Arc<RwLock<CommandNode>>` handles
/// of the same underlying node, so `Arc::ptr_eq` is Java's `==`.
///
/// Child maps are insertion-ordered (Java's `LinkedHashMap`), which is what
/// parse candidate order, usage strings and ambiguity scans observe.
pub struct CommandNode<S, R = i32> {
    kind: CommandNodeKind<S, R>,
    children: IndexMap<String, Arc<RwLock<CommandNode<S, R>>>>,
    literals: IndexMap<String, Arc<RwLock<CommandNode<S, R>>>>,
    arguments: IndexMap<String, Arc<RwLock<CommandNode<S, R>>>>,

    command: Command<S, R>,
    requirement: Arc<dyn Fn(&S) -> bool + Send + Sync>,
    redirect: Option<Arc<RwLock<CommandNode<S, R>>>>,
    forks: bool,
    modifier: Option<Arc<RedirectModifier<S, R>>>,
}

impl<S, R> Clone for CommandNode<S, R> {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind.clone(),
            children: self.children.clone(),
            literals: self.literals.clone(),
            arguments: self.arguments.clone(),
            command: self.command.clone(),
            requirement: self.requirement.clone(),
            redirect: self.redirect.clone(),
            forks: self.forks,
            modifier: self.modifier.clone(),
        }
    }
}

impl<S, R> CommandNode<S, R> {
    /// A node of the given kind with the shared state Java's protected
    /// `CommandNode` constructor takes. Java: `CommandNode(command,
    /// requirement, redirect, modifier, forks)`.
    pub fn new(
        kind: CommandNodeKind<S, R>,
        command: Command<S, R>,
        requirement: Arc<dyn Fn(&S) -> bool + Send + Sync>,
        redirect: Option<Arc<RwLock<CommandNode<S, R>>>>,
        modifier: Option<Arc<RedirectModifier<S, R>>>,
        forks: bool,
    ) -> Self {
        Self {
            kind,
            children: IndexMap::new(),
            literals: IndexMap::new(),
            arguments: IndexMap::new(),
            command,
            requirement,
            redirect,
            forks,
            modifier,
        }
    }

    /// A fresh root node: always usable, no command, and the identity
    /// redirect modifier (a redirect to the root runs the command once for
    /// the original source). Java: `new RootCommandNode<>()`.
    pub fn root() -> Self {
        Self::new(
            CommandNodeKind::Root(RootCommandNode),
            None,
            Arc::new(|_| true),
            None,
            Some(Arc::new(|context: &CommandContext<S, R>| {
                Ok(vec![context.get_source_arc()])
            })),
            false,
        )
    }

    /// Which kind of node this is; match on it for the full per-kind payload
    /// (Java: `instanceof` + downcast).
    pub fn kind(&self) -> &CommandNodeKind<S, R> {
        &self.kind
    }

    /// Whether this is a literal node. Java: `node instanceof
    /// LiteralCommandNode`.
    pub fn is_literal(&self) -> bool {
        matches!(self.kind, CommandNodeKind::Literal(_))
    }

    /// Whether this is a required-argument node. Java: `node instanceof
    /// ArgumentCommandNode`.
    pub fn is_argument(&self) -> bool {
        matches!(self.kind, CommandNodeKind::Argument(_))
    }

    /// Whether this is the root node. Java: `node instanceof RootCommandNode`.
    pub fn is_root(&self) -> bool {
        matches!(self.kind, CommandNodeKind::Root(_))
    }

    /// The literal payload, when this is a literal node. Java: the
    /// `(LiteralCommandNode) node` cast.
    pub fn as_literal(&self) -> Option<&LiteralCommandNode> {
        match &self.kind {
            CommandNodeKind::Literal(literal) => Some(literal),
            _ => None,
        }
    }

    /// The argument payload, when this is an argument node. Java: the
    /// `(ArgumentCommandNode) node` cast.
    pub fn as_argument(&self) -> Option<&ArgumentCommandNode<S, R>> {
        match &self.kind {
            CommandNodeKind::Argument(argument) => Some(argument),
            _ => None,
        }
    }

    /// The literal keyword. Java: `LiteralCommandNode#getLiteral`.
    ///
    /// # Panics
    ///
    /// Panics when this node is not a literal — Java's `(LiteralCommandNode)
    /// node` cast raising `ClassCastException`; prefer [`Self::is_literal`]
    /// or [`Self::as_literal`] when the kind is not known.
    pub fn get_literal(&self) -> &str {
        self.as_literal()
            .map(|literal| literal.get_literal())
            .expect("CommandNode::get_literal() called on a non-literal node")
    }

    /// The argument parser. Java: `ArgumentCommandNode#getType`.
    ///
    /// # Panics
    ///
    /// Panics when this node is not an argument — Java's
    /// `(ArgumentCommandNode) node` cast; prefer [`Self::is_argument`] or
    /// [`Self::as_argument`] when the kind is not known.
    pub fn get_type(&self) -> &Arc<dyn crate::arguments::ArgumentType + Send + Sync> {
        self.as_argument()
            .map(|argument| argument.get_type())
            .expect("CommandNode::get_type() called on a non-argument node")
    }

    /// The node's name: the literal keyword, the argument name, or `""` for
    /// the root. Java: `getName`.
    pub fn get_name(&self) -> &str {
        match &self.kind {
            CommandNodeKind::Literal(literal) => literal.get_literal(),
            CommandNodeKind::Argument(argument) => argument.get_name(),
            CommandNodeKind::Root(_) => "",
        }
    }

    /// How this node renders in usage strings: literals print bare,
    /// arguments print as `<name>`, the root prints as `""`. Java:
    /// `getUsageText`.
    pub fn get_usage_text(&self) -> String {
        match &self.kind {
            CommandNodeKind::Argument(argument) => format!("<{}>", argument.get_name()),
            CommandNodeKind::Literal(literal) => literal.get_literal().to_owned(),
            CommandNodeKind::Root(_) => String::new(),
        }
    }

    /// The command to execute when this node is reached, if any. Java:
    /// `getCommand`.
    pub fn get_command(&self) -> &Command<S, R> {
        &self.command
    }

    /// Every child in insertion order (Java's `LinkedHashMap` values). Java:
    /// `getChildren`.
    pub fn get_children(&self) -> Vec<Arc<RwLock<CommandNode<S, R>>>> {
        self.children.values().cloned().collect()
    }

    /// Look up a child by name. Java: `getChild(name)`.
    pub fn get_child(&self, name: &str) -> Option<Arc<RwLock<CommandNode<S, R>>>> {
        self.children.get(name).cloned()
    }

    /// The redirect target, if any. Java: `getRedirect`.
    pub fn get_redirect(&self) -> Option<Arc<RwLock<CommandNode<S, R>>>> {
        self.redirect.clone()
    }

    /// The redirect modifier, if any. Java: `getRedirectModifier`.
    pub fn get_redirect_modifier(&self) -> Option<&Arc<RedirectModifier<S, R>>> {
        self.modifier.as_ref()
    }

    /// The permission requirement. Java: `getRequirement`.
    pub fn get_requirement(&self) -> &Arc<dyn Fn(&S) -> bool + Send + Sync> {
        &self.requirement
    }

    /// Whether execution forks across modified sources. Java: `isFork`.
    pub fn is_fork(&self) -> bool {
        self.forks
    }

    /// Whether `source` may use this node (the `requires` predicate). Java:
    /// `canUse(S)`.
    pub fn can_use(&self, source: &S) -> bool {
        (self.requirement)(source)
    }

    /// Insert `node` as a child, merging when a child with the same name
    /// already exists: the existing child keeps its position, gains the new
    /// node's command (if any), and absorbs the new node's grandchildren.
    /// Java: `addChild` with the same merge semantics.
    ///
    /// # Panics
    ///
    /// Panics when `node` is a root node, with Java's exact message — Java
    /// throws `UnsupportedOperationException` there. This is a programmer
    /// error (the root is the tree's top, never a child), never triggered by
    /// user input, so a documented panic mirrors the Java throw rather than
    /// threading a `Result` through every registration path.
    pub fn add_child(&mut self, node: &Arc<RwLock<CommandNode<S, R>>>) {
        if node.read().is_root() {
            panic!("Cannot add a RootCommandNode as a child to any other CommandNode");
        }

        let name = node.read().get_name().to_owned();
        if let Some(child) = self.children.get(&name).cloned() {
            // We've found something to merge onto
            if let Some(command) = node.read().get_command().clone() {
                child.write().command = Some(command);
            }
            for grandchild in node.read().get_children() {
                child.write().add_child(&grandchild);
            }
        } else {
            self.children.insert(name, node.clone());
            match &node.read().kind {
                CommandNodeKind::Literal(literal) => {
                    self.literals
                        .insert(literal.get_literal().to_owned(), node.clone());
                }
                CommandNodeKind::Argument(argument) => {
                    self.arguments
                        .insert(argument.get_name().to_owned(), node.clone());
                }
                // rejected by the root check above
                CommandNodeKind::Root(_) => unreachable!("root nodes cannot become children"),
            }
        }
    }

    /// Candidate child nodes worth trying for the input ahead: if the next
    /// word matches a literal exactly, just that literal (Java returns a
    /// `Collections.singleton` — a one-element vector here); otherwise every
    /// argument node. Java: `getRelevantNodes` (private there, public here).
    ///
    /// Java finds the word by scanning the reader forward and taking a
    /// `substring`, then rewinds the cursor — a net-zero reader mutation and
    /// one `String` allocation per call. The word is exactly the
    /// space-delimited prefix of [`StringReader::remaining`], so this reads
    /// it straight off the borrowed input: no allocation, no scan, and the
    /// reader is never touched at all.
    pub fn get_relevant_nodes(
        &self,
        input: &mut StringReader,
    ) -> Vec<Arc<RwLock<CommandNode<S, R>>>> {
        if !self.literals.is_empty() {
            // `' '` is ASCII, so every `split_once` split point is a char
            // boundary — the slice is always valid UTF-8 slicing. An empty
            // remaining input yields the empty word, same as Java's scan
            // (which stops immediately and substrings nothing).
            let text = input
                .remaining()
                .split_once(' ')
                .map_or(input.remaining(), |(word, _)| word);
            if let Some(literal) = self.literals.get(text) {
                return vec![literal.clone()];
            }
        }
        let mut relevant = Vec::with_capacity(self.arguments.len());
        relevant.extend(self.arguments.values().cloned());
        relevant
    }

    /// Consume the input this node matches, recording the result in
    /// `context_builder`. Literals must match exactly or produce a
    /// `LiteralIncorrect` error; arguments delegate to their parser (the
    /// source-aware overload, like Java) and store a `ParsedArgument` plus
    /// the node itself; the root consumes nothing. Java:
    /// `parse(StringReader, CommandContextBuilder)`.
    ///
    /// The `node` handle is Java's `this`: it is the exact `Arc` the tree
    /// stores, so the recorded parse node is the tree's node (identity), not
    /// a clone.
    pub fn parse_with_context(
        node: &Arc<RwLock<CommandNode<S, R>>>,
        reader: &mut StringReader,
        context_builder: &mut CommandContextBuilder<S, R>,
    ) -> Result<(), CommandSyntaxException>
    where
        S: Send + Sync + 'static,
    {
        let this = node.read();
        match &this.kind {
            CommandNodeKind::Argument(argument) => {
                let start = reader.cursor();
                let result = argument.parse_with_source(reader, context_builder.get_source())?;
                let parsed = ParsedArgument::new_erased(start, reader.cursor(), result);

                context_builder.with_argument(argument.get_name(), parsed.clone());
                context_builder.with_node(node.clone(), parsed.get_range());

                Ok(())
            }
            CommandNodeKind::Literal(literal) => {
                let start = reader.cursor();
                if let Some(end) = literal.parse(reader) {
                    context_builder.with_node(node.clone(), StringRange::between(start, end));
                    return Ok(());
                }

                Err(built_in_exceptions()
                    .literal_incorrect()
                    .create_with_context(&*reader, literal.get_literal().to_owned()))
            }
            // Java: RootCommandNode#parse is an empty method
            CommandNodeKind::Root(_) => Ok(()),
        }
    }

    /// Suggest completions for this node: literals offer their own keyword
    /// when it matches the remaining input case-insensitively, arguments
    /// delegate to their custom provider or parser, the root offers nothing.
    /// Java: `listSuggestions`.
    pub fn list_suggestions(
        &self,
        context: CommandContext<S, R>,
        builder: SuggestionsBuilder,
    ) -> SuggestionsFuture {
        match &self.kind {
            CommandNodeKind::Literal(literal) => {
                if literal
                    .literal_lower_case()
                    .starts_with(builder.remaining_lowercase())
                {
                    builder.suggest(literal.get_literal()).build_future()
                } else {
                    Suggestions::empty()
                }
            }
            CommandNodeKind::Argument(argument) => argument.list_suggestions(context, builder),
            CommandNodeKind::Root(_) => Suggestions::empty(),
        }
    }

    /// Whether `input` could be consumed by this node, used by the ambiguity
    /// scan. Java: `isValidInput` (protected there).
    pub fn is_valid_input(&self, input: &str) -> bool {
        match &self.kind {
            CommandNodeKind::Literal(literal) => literal.is_valid_input(input),
            CommandNodeKind::Argument(argument) => argument.is_valid_input(input),
            CommandNodeKind::Root(_) => false,
        }
    }

    /// Example inputs this node accepts, feeding the ambiguity scan. Java:
    /// `getExamples`.
    pub fn get_examples(&self) -> Vec<String> {
        match &self.kind {
            CommandNodeKind::Literal(literal) => literal.examples(),
            CommandNodeKind::Argument(argument) => argument.examples(),
            CommandNodeKind::Root(_) => Vec::new(),
        }
    }

    /// Turn this node back into a builder carrying the same command,
    /// requirement, redirect and fork configuration. Java: `createBuilder` —
    /// the covariant override returns the concrete builder subclass; a single
    /// Rust method returns the [`AnyArgumentBuilder`] enum over the two
    /// built-in kinds.
    ///
    /// # Panics
    ///
    /// Panics on the root node with Java's exact `IllegalStateException`
    /// message (`Cannot convert root into a builder`) — a programmer error,
    /// mirroring the Java throw.
    pub fn create_builder(&self) -> AnyArgumentBuilder<S, R> {
        let command = self.command.clone();
        let requirement = self.requirement.clone();
        let target = self.redirect.clone();
        let modifier = self.modifier.clone();
        let forks = self.forks;
        match &self.kind {
            CommandNodeKind::Literal(literal) => {
                AnyArgumentBuilder::Literal(ArgumentBuilder::from_parts(
                    LiteralCommandNode::new(literal.get_literal()),
                    command,
                    requirement,
                    target,
                    modifier,
                    forks,
                ))
            }
            CommandNodeKind::Argument(argument) => {
                AnyArgumentBuilder::Required(ArgumentBuilder::from_parts(
                    ArgumentCommandNode::new(
                        argument.get_name(),
                        argument.get_type().clone(),
                        argument.get_custom_suggestions().cloned(),
                    ),
                    command,
                    requirement,
                    target,
                    modifier,
                    forks,
                ))
            }
            CommandNodeKind::Root(_) => panic!("Cannot convert root into a builder"),
        }
    }

    /// Sort key for the Comparable ordering. Java: `getSortedKey` (protected
    /// there).
    fn get_sorted_key(&self) -> &str {
        match &self.kind {
            CommandNodeKind::Literal(literal) => literal.sorted_key(),
            CommandNodeKind::Argument(argument) => argument.sorted_key(),
            CommandNodeKind::Root(_) => "",
        }
    }

    /// Scan this node's subtree for ambiguous siblings: whenever a child's
    /// examples are all valid inputs of a sibling, the consumer is notified.
    /// Java: `findAmbiguities(AmbiguityConsumer)`.
    ///
    /// The `node` handle is Java's `this` — the node reported to the
    /// consumer as the ambiguous pair's parent.
    pub fn find_ambiguities(
        node: &Arc<RwLock<CommandNode<S, R>>>,
        consumer: &dyn AmbiguityConsumer<S, R>,
    ) {
        let mut matches: HashSet<String> = HashSet::new();
        let children = node.read().get_children();

        for child in &children {
            for sibling in &children {
                if Arc::ptr_eq(child, sibling) {
                    continue;
                }

                for input in child.read().get_examples() {
                    if sibling.read().is_valid_input(&input) {
                        matches.insert(input);
                    }
                }

                if !matches.is_empty() {
                    consumer.ambiguous(node, child, sibling, &matches);
                    matches = HashSet::new();
                }
            }

            Self::find_ambiguities(child, consumer);
        }
    }
}

impl<S, R> Default for CommandNode<S, R> {
    /// The default node is a root node — the only node ever created without
    /// an explicit kind (the dispatcher's tree root). Java: `new
    /// RootCommandNode<>()`.
    fn default() -> Self {
        Self::root()
    }
}

impl<S, R> Ord for CommandNode<S, R> {
    /// Java: `Comparable<CommandNode>#compareTo` — nodes of the same kind
    /// class (both literals, or neither) order by their sort key, and
    /// literals order before everything else.
    ///
    /// The key compare is Java's `String.compareTo`: UTF-16 code unit by
    /// unit. Plain `str::cmp` would disagree with Java for astral keys
    /// (a surrogate first unit 0xD800..0xDBFF sorts *below* U+E000..U+FFFF,
    /// while UTF-8 byte order sorts astral code points above the BMP).
    fn cmp(&self, other: &Self) -> Ordering {
        if self.is_literal() == other.is_literal() {
            return self
                .get_sorted_key()
                .encode_utf16()
                .cmp(other.get_sorted_key().encode_utf16());
        }
        if other.is_literal() {
            Ordering::Greater
        } else {
            Ordering::Less
        }
    }
}

impl<S, R> PartialOrd for CommandNode<S, R> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<S, R> PartialEq for CommandNode<S, R> {
    /// Java: `CommandNode#equals` plus the subclass overrides — the kinds
    /// must match, the children compare by value (recursively, order
    /// blind like Java's map equality), and the command compares by
    /// reference (closures have no value equality; a shared `Arc` is the
    /// Java "same command object" case). Redirect, requirement and fork
    /// state are deliberately not compared, matching Java.
    fn eq(&self, other: &Self) -> bool {
        if self.kind != other.kind {
            return false;
        }

        if self.children.len() != other.children.len() {
            return false;
        }
        for (name, child) in &self.children {
            match other.children.get(name) {
                Some(other_child) => {
                    if *child.read() != *other_child.read() {
                        return false;
                    }
                }
                None => return false,
            }
        }

        match (&self.command, &other.command) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}
impl<S, R> Eq for CommandNode<S, R> {}

impl<S, R> Debug for CommandNode<S, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandNode")
            .field("name", &self.get_name())
            .field("children", &self.children.len())
            .field("command", &self.command.is_some())
            .field("redirect", &self.redirect)
            .field("forks", &self.forks)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arguments::{ArgumentType, integer_argument_type::integer};

    type Node = CommandNode<String, i32>;
    type NodeHandle = Arc<RwLock<Node>>;

    #[test]
    fn single_success_is_javas_command_constant() {
        assert_eq!(SINGLE_SUCCESS, 1);
    }

    fn literal_node(literal: &str) -> NodeHandle {
        Arc::new(RwLock::new(CommandNode::new(
            CommandNodeKind::Literal(LiteralCommandNode::new(literal)),
            None,
            Arc::new(|_| true),
            None,
            None,
            false,
        )))
    }

    fn argument_node(name: &str) -> NodeHandle {
        Arc::new(RwLock::new(CommandNode::new(
            CommandNodeKind::Argument(ArgumentCommandNode::new(name, Arc::new(integer()), None)),
            None,
            Arc::new(|_| true),
            None,
            None,
            false,
        )))
    }

    #[test]
    fn children_keep_insertion_order() {
        let node = literal_node("parent");
        for name in ["echo", "alpha", "midway", "beta"] {
            node.write().add_child(&literal_node(name));
        }
        let names: Vec<String> = node
            .read()
            .get_children()
            .iter()
            .map(|child| child.read().get_name().to_owned())
            .collect();
        // insertion order, not lexicographic
        assert_eq!(names, ["echo", "alpha", "midway", "beta"]);
    }

    #[test]
    fn literals_and_arguments_maps_track_children() {
        let node = literal_node("parent");
        let literal_child = literal_node("l");
        let argument_child = argument_node("a");
        node.write().add_child(&literal_child);
        node.write().add_child(&argument_child);

        assert_eq!(node.read().get_children().len(), 2);
        // the sub-maps hold the same handles as the children map
        assert!(Arc::ptr_eq(
            &literal_child,
            &node.read().literals.get("l").unwrap().clone()
        ));
        assert!(Arc::ptr_eq(
            &argument_child,
            &node.read().arguments.get("a").unwrap().clone()
        ));
    }

    #[test]
    #[should_panic(expected = "Cannot add a RootCommandNode as a child to any other CommandNode")]
    fn add_child_rejects_root() {
        let node = literal_node("parent");
        let root: NodeHandle = Arc::new(RwLock::new(Node::root()));
        node.write().add_child(&root);
    }

    #[test]
    fn comparable_orders_literals_before_arguments_then_by_key() {
        let literal_a = literal_node("a");
        let literal_b = literal_node("b");
        let argument_a = argument_node("a");

        assert_eq!(literal_a.read().cmp(&literal_b.read()), Ordering::Less);
        // literals come first regardless of sort key
        assert_eq!(literal_b.read().cmp(&argument_a.read()), Ordering::Less);
        assert_eq!(argument_a.read().cmp(&literal_b.read()), Ordering::Greater);
        // two non-literals compare by sort key
        assert_eq!(argument_a.read().cmp(&literal_a.read()), Ordering::Greater);
    }

    #[test]
    fn comparable_key_orders_like_java_string_compare_to() {
        // Java's String.compareTo is UTF-16-unit order: an astral key's
        // leading surrogate (0xD800..0xDBFF) sorts BELOW U+E000..U+FFFF,
        // while UTF-8 byte order would put the astral code point on top.
        let bmp_high = literal_node("\u{FFFF}");
        let astral = literal_node("\u{10000}");
        assert_eq!(astral.read().cmp(&bmp_high.read()), Ordering::Less);
        // plain ASCII keys are unaffected by the unit model
        assert_eq!(
            literal_node("a").read().cmp(&literal_node("b").read()),
            Ordering::Less
        );
    }

    #[test]
    fn structural_equality_is_java_exact() {
        // same shape, separately built objects: Java's map-value equality
        let a = literal_node("foo");
        let b = literal_node("foo");
        assert_eq!(*a.read(), *b.read());

        // children compare by value, not pointer
        let with_child = |name: &str| {
            let node = literal_node(name);
            node.write().add_child(&literal_node("bar"));
            node
        };
        assert_eq!(*with_child("foo").read(), *with_child("foo").read());

        // different kind: literal vs argument with the same name
        assert_ne!(*a.read(), *argument_node("foo").read());

        // root vs literal: Java's RootCommandNode.equals rejects other kinds
        assert_ne!(*a.read(), Node::root());
    }

    #[test]
    fn root_has_java_defaults() {
        let root = Node::root();
        assert_eq!(root.get_name(), "");
        assert_eq!(root.get_usage_text(), "");
        assert!(root.is_root());
        assert!(root.can_use(&"any source".to_owned()));
        // Java's RootCommandNode constructor wires the identity redirect
        // modifier
        assert!(root.get_redirect_modifier().is_some());
        assert!(!root.is_fork());
        assert!(root.get_command().is_none());
        assert!(root.get_examples().is_empty());
        assert!(!root.is_valid_input("anything"));
    }

    #[test]
    fn is_valid_input_and_examples_per_kind() {
        let literal = literal_node("foo");
        assert!(literal.read().is_valid_input("foo"));
        assert!(literal.read().is_valid_input("foo "));
        assert!(!literal.read().is_valid_input("foobar"));
        assert_eq!(literal.read().get_examples(), ["foo".to_owned()]);

        let argument_child = argument_node("num");
        assert!(argument_child.read().is_valid_input("42"));
        // a trailing space is fine (Java's boundary check), a glued-on
        // character is not
        assert!(argument_child.read().is_valid_input("42 42"));
        assert!(!argument_child.read().is_valid_input("42x"));
        assert!(!argument_child.read().is_valid_input("not a number"));
        assert_eq!(
            argument_child.read().get_examples(),
            ArgumentType::examples(&integer())
        );
    }

    struct Collector {
        seen: std::sync::Mutex<Vec<(String, String, Vec<String>)>>,
    }
    impl AmbiguityConsumer<String, i32> for Collector {
        fn ambiguous(
            &self,
            _parent: &NodeHandle,
            child: &NodeHandle,
            sibling: &NodeHandle,
            inputs: &HashSet<String>,
        ) {
            self.seen.lock().unwrap().push((
                child.read().get_name().to_owned(),
                sibling.read().get_name().to_owned(),
                inputs.iter().cloned().collect(),
            ));
        }
    }

    #[test]
    fn find_ambiguities_reports_sibling_overlap() {
        // two argument children whose example sets overlap: every integer
        // example of the first is a valid input of the second
        let mut parent = CommandNode::new(
            CommandNodeKind::Literal(LiteralCommandNode::new("parent")),
            None,
            Arc::new(|_| true),
            None,
            None,
            false,
        );
        let first = argument_node("first");
        let second = argument_node("second");
        parent.add_child(&first);
        parent.add_child(&second);

        let collector = Collector {
            seen: std::sync::Mutex::new(Vec::new()),
        };
        let parent_arc: NodeHandle = Arc::new(RwLock::new(parent));
        Node::find_ambiguities(&parent_arc, &collector);

        let seen = collector.seen.into_inner().unwrap();
        // both sibling orders are reported, like Java's double loop
        assert_eq!(seen.len(), 2);
        assert!(seen.iter().all(|(child, sibling, _)| matches!(
            (child.as_str(), sibling.as_str()),
            ("first", "second") | ("second", "first")
        )));
        // the ambiguous inputs are the shared examples of the integer type
        // (["0", "123", "-123"]) — every one of them is a valid input of
        // both siblings
        assert!(
            seen.iter()
                .all(|(_, _, inputs)| inputs.contains(&"123".to_owned()))
        );
    }

    #[test]
    fn find_ambiguities_walks_into_children() {
        let collector = Collector {
            seen: std::sync::Mutex::new(Vec::new()),
        };
        // a nested subtree one level down must also be scanned
        let mut parent = CommandNode::new(
            CommandNodeKind::Literal(LiteralCommandNode::new("parent")),
            None,
            Arc::new(|_| true),
            None,
            None,
            false,
        );
        let child = literal_node("child");
        child.write().add_child(&argument_node("first"));
        child.write().add_child(&argument_node("second"));
        parent.add_child(&child);

        let parent_arc: NodeHandle = Arc::new(RwLock::new(parent));
        Node::find_ambiguities(&parent_arc, &collector);

        let seen = collector.seen.into_inner().unwrap();
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn create_builder_round_trips_literal() {
        let literal = literal_node("foo");
        let builder = literal.read().create_builder();
        assert_eq!(builder.get_literal(), "foo");
        assert!(builder.get_command().is_none());
        assert!(Arc::ptr_eq(
            builder.get_requirement(),
            literal.read().get_requirement()
        ));
        // the built node equals the original: same keyword, no command
        let rebuilt = Arc::new(RwLock::new(builder.build()));
        assert_eq!(*literal.read(), *rebuilt.read());
    }

    #[test]
    fn create_builder_round_trips_argument() {
        let argument = argument_node("num");
        let builder = argument.read().create_builder();
        assert_eq!(builder.get_name(), "num");
        assert!(Arc::ptr_eq(builder.get_type(), argument.read().get_type()));
        let rebuilt = Arc::new(RwLock::new(builder.build()));
        assert_eq!(*argument.read(), *rebuilt.read());
    }

    #[test]
    #[should_panic(expected = "Cannot convert root into a builder")]
    fn create_builder_rejects_root() {
        let _ = Node::root().create_builder();
    }

    #[test]
    fn get_relevant_nodes_literal_singleton_beats_arguments() {
        let mut node = CommandNode::new(
            CommandNodeKind::Literal(LiteralCommandNode::new("parent")),
            None,
            Arc::new(|_| true),
            None,
            None,
            false,
        );
        node.add_child(&literal_node("foo"));
        node.add_child(&argument_node("bar"));

        // the word at the cursor is a known literal: only that literal comes
        // back (Java returns Collections.singleton)
        let mut reader = StringReader::from("foo 42");
        let relevant = node.get_relevant_nodes(&mut reader);
        assert_eq!(relevant.len(), 1);
        assert!(Arc::ptr_eq(
            &relevant[0],
            &node.literals.get("foo").unwrap().clone()
        ));
        // the reader cursor is restored
        assert_eq!(reader.cursor(), 0);

        // a non-literal word at the cursor: every argument is a candidate,
        // in insertion order
        let mut reader = StringReader::from("quux");
        let relevant = node.get_relevant_nodes(&mut reader);
        assert_eq!(relevant.len(), 1);
        assert!(Arc::ptr_eq(
            &relevant[0],
            &node.arguments.get("bar").unwrap().clone()
        ));

        // no literals at all: the arguments map is the candidate list
        let mut arguments_only = CommandNode::new(
            CommandNodeKind::Literal(LiteralCommandNode::new("plain")),
            None,
            Arc::new(|_| true),
            None,
            None,
            false,
        );
        let first = argument_node("first");
        let second = argument_node("second");
        arguments_only.add_child(&first);
        arguments_only.add_child(&second);
        let relevant = arguments_only.get_relevant_nodes(&mut StringReader::from(""));
        assert_eq!(relevant.len(), 2);
        assert!(Arc::ptr_eq(&relevant[0], &first));
        assert!(Arc::ptr_eq(&relevant[1], &second));
    }
}
