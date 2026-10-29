//! The command dispatcher: register a tree of commands, parse input into
//! contexts, and execute them. Mirrors Java's
//! `com.mojang.brigadier.CommandDispatcher`.

#![allow(clippy::type_complexity)]

use std::{cmp::Ordering, collections::HashSet, sync::Arc};

use indexmap::IndexMap;
use parking_lot::RwLock;

use crate::{
    ambiguity_consumer::AmbiguityConsumer,
    builder::literal_argument_builder::LiteralArgumentBuilder,
    context::{CommandContextBuilder, ContextChain},
    errors::{
        BuiltInExceptionProvider, CommandResultTrait, CommandSyntaxException, built_in_exceptions,
    },
    parse_results::ParseResults,
    result_consumer::{DefaultResultConsumer, ResultConsumer},
    string_reader::{StringReader, char_to_byte},
    suggestion::{SuggestionsBuilder, SuggestionsFuture, join_all_and_merge},
    tree::CommandNode,
};

/// The entry point of the library: holds the command tree (`root`), parses
/// input against it, and runs the matching commands. Java:
/// `CommandDispatcher<S>`.
///
/// Java's `execute` also documents a `RuntimeException` escape hatch — a
/// command throwing anything unchecked bubbles straight out. Rust commands
/// report failures as `Result`; a panicking command is a programmer error and
/// its panic propagates like any other panic (no `catch_unwind` layer is
/// interposed, see [`Self::parse`]).
///
/// # Thread safety
///
/// `CommandDispatcher<S, R>` is `Send + Sync`: the tree lives behind
/// `Arc<RwLock<..>>` and every source/result type only shows up inside
/// `Send + Sync` closures, so one dispatcher can be shared between threads
/// (`Arc<CommandDispatcher<S, R>>`) with `parse`, `execute`,
/// `execute_parsed` and the usage/suggestion surfaces callable concurrently.
/// Commands and modifiers always run sequentially *within* one
/// `execute_parsed` call, like Java; nothing in the library spawns threads
/// or holds locks while running user code. The parsed [`ParseResults`] value
/// is `Send + Sync` when `S: Send + Sync`, so the Java-shaped
/// "parse in one thread, execute in another" flow works by moving the value
/// across threads.
///
/// ```
/// # use brigaxsir::prelude::*;
/// # struct CommandSource;
/// let mut subject = CommandDispatcher::<CommandSource>::new();
/// ```
pub struct CommandDispatcher<S, R = i32>
where
    Self: Sync + Send,
{
    /// The root of the command tree; register into it (or nest under it).
    /// Private like Java's `root` field — read it back through
    /// [`Self::get_root`].
    root: Arc<RwLock<CommandNode<S, R>>>,
    consumer: Box<dyn ResultConsumer<S, R> + Send + Sync>,
}

impl<S: Send + Sync + 'static, R: CommandResultTrait> CommandDispatcher<S, R> {
    /// The string required to separate individual arguments in an input
    /// string. Java: `CommandDispatcher.ARGUMENT_SEPARATOR`.
    ///
    /// ```
    /// # use brigaxsir::command_dispatcher::CommandDispatcher;
    /// assert_eq!(CommandDispatcher::<()>::ARGUMENT_SEPARATOR, " ");
    /// ```
    pub const ARGUMENT_SEPARATOR: &'static str = " ";

    /// The char required to separate individual arguments in an input
    /// string. Java: `CommandDispatcher.ARGUMENT_SEPARATOR_CHAR`.
    ///
    /// ```
    /// # use brigaxsir::command_dispatcher::CommandDispatcher;
    /// assert_eq!(CommandDispatcher::<()>::ARGUMENT_SEPARATOR_CHAR, ' ');
    /// ```
    pub const ARGUMENT_SEPARATOR_CHAR: char = ' ';

    const USAGE_OPTIONAL_OPEN: &'static str = "[";
    const USAGE_OPTIONAL_CLOSE: &'static str = "]";
    const USAGE_REQUIRED_OPEN: &'static str = "(";
    const USAGE_REQUIRED_CLOSE: &'static str = ")";
    const USAGE_OR: &'static str = "|";

    /// A dispatcher with an empty root and the default (no-op) result
    /// consumer. Java: the no-argument `CommandDispatcher()` constructor.
    pub fn new() -> Self {
        Self::with_root(Arc::new(RwLock::new(CommandNode::root())))
    }

    /// A dispatcher over an existing `root` node — useful to copy or share a
    /// pre-defined command tree. Java: the
    /// `CommandDispatcher(RootCommandNode<S>)` constructor.
    ///
    /// # Panics
    ///
    /// Panics when `root` is not a root node. Java's parameter type
    /// (`RootCommandNode<S>`) makes that unrepresentable; the check keeps
    /// this a documented programmer error instead of a silently misbehaving
    /// tree (the redirect-to-root usage rendering and `get_path` compare
    /// against this very node).
    pub fn with_root(root: Arc<RwLock<CommandNode<S, R>>>) -> Self {
        assert!(
            root.read().is_root(),
            "CommandDispatcher::with_root expects a RootCommandNode (build one with CommandNode::root())"
        );
        Self {
            root,
            consumer: Box::new(DefaultResultConsumer),
        }
    }

    /// The root of this command tree — a redirect target, a usage target, or
    /// the way to clone the tree into a fresh dispatcher with
    /// [`Self::with_root`]. Java: `getRoot()`.
    pub fn get_root(&self) -> Arc<RwLock<CommandNode<S, R>>> {
        self.root.clone()
    }

    /// Set the callback informed of the result of every command. Java:
    /// `setConsumer(ResultConsumer<S>)` — replacing the previous consumer
    /// (the default is a no-op, Java's empty lambda).
    pub fn set_consumer(&mut self, consumer: impl ResultConsumer<S, R> + Send + Sync + 'static) {
        self.consumer = Box::new(consumer);
    }

    /// Build `node` and add it to the root. Returns the built node so it can
    /// be redirected to later. Java: `register(LiteralArgumentBuilder)` —
    /// top-level commands are always literals; the argument builder has no
    /// `register` route (Java: no overload either).
    ///
    /// ```
    /// # use brigaxsir::prelude::*;
    /// # let mut subject = CommandDispatcher::<()>::new();
    /// subject.register(literal("foo").executes(|_| 42));
    /// ```
    pub fn register(
        &mut self,
        node: LiteralArgumentBuilder<S, R>,
    ) -> Arc<RwLock<CommandNode<S, R>>> {
        let build = Arc::new(RwLock::new(node.build()));
        self.root.write().add_child(&build);
        build
    }

    /// Parse `command` against the tree for `source` without executing.
    /// Java: `parse(String, S)` / `parse(StringReader, S)`. Never fails:
    /// candidate errors are collected into the returned [`ParseResults`] and
    /// only surface on execution.
    ///
    /// Java also wraps a `RuntimeException` thrown by a node's parse into a
    /// `dispatcherParseException` here. The port needs no such wrapper:
    /// built-in argument types report malformed input as `Result` errors
    /// (never panics — that is the project-wide input policy), so every
    /// parse failure already travels as the original
    /// [`CommandSyntaxException`]. A panic from a custom argument type is a
    /// bug in that type and propagates as a panic — interposing
    /// `catch_unwind` would silently convert bugs into parse failures and
    /// misbehave under `panic = "abort"`, so the Java path is deliberately
    /// not reproduced (the `dispatcher_parse_exception` provider method
    /// remains available for user code that wants the Java-shaped error).
    pub fn parse(&self, command: impl Into<StringReader>, source: S) -> ParseResults<'_, S, R> {
        let command = command.into();
        let source = Arc::new(source);

        let context = CommandContextBuilder::new(self, source, self.root.clone(), command.cursor());
        self.parse_nodes(&self.root, &command, context)
    }

    /// Walk the tree from `node`, trying every relevant child and tracking
    /// the best (shortest-remaining, fewest-errors) parse. Mirrors Java's
    /// `parseNodes` including redirect handling: a redirect immediately
    /// continues parsing into the target and short-circuits alternatives.
    fn parse_nodes<'a>(
        &'a self,
        node: &Arc<RwLock<CommandNode<S, R>>>,
        original_reader: &StringReader,
        context_so_far: CommandContextBuilder<'a, S, R>,
    ) -> ParseResults<'a, S, R> {
        // Java: `final S source = contextSoFar.getSource();` — the source
        // handle is shared with every candidate child builder
        let source = context_so_far.get_source_arc();
        // Java: `Map<CommandNode<S>, CommandSyntaxException> errors` — a
        // LinkedHashMap keyed by the candidate node, so insertion order is
        // observable (execute's single-exception pick reads the first entry).
        // Candidates all come from one parent's child map where names are
        // unique, so a name-keyed `IndexMap` is the same observable map:
        // same size, same exceptions, same insertion order, without hashing
        // a lock-protected recursive structure.
        let mut errors = IndexMap::<String, CommandSyntaxException>::new();
        let mut potentials: Vec<ParseResults<S, R>> = vec![];

        for child in node.read().get_relevant_nodes(&mut original_reader.clone()) {
            // One read guard for every cheap field read this frame does on
            // the candidate (Java re-reads the node for each one). The guard
            // is dropped before the recursive calls below so the lock
            // nesting stays shallow — `parse_with_context` takes its own
            // read.
            let (command, redirect) = {
                let child_data = child.read();
                if !child_data.can_use(&source) {
                    continue;
                }
                (child_data.get_command().clone(), child_data.get_redirect())
            };
            let mut context = context_so_far.copy();
            let mut reader = original_reader.clone();

            // Java catches CommandSyntaxException from child.parse and
            // stores it in the errors map *as-is*; only a RuntimeException
            // is wrapped in dispatcherParseException, which has no Rust
            // analogue here (argument errors travel as `Result`, panics stay
            // panics — see the `parse` docs).
            if let Err(ex) = CommandNode::parse_with_context(&child, &mut reader, &mut context) {
                errors.insert(child.read().get_name().to_owned(), ex);
                // no cursor rewind needed: `reader` is a fresh clone per
                // child, so Java's `reader.setCursor(cursor)` before its
                // `continue` is dead code there too
                continue;
            }
            if reader.can_read() && reader.peek() != Self::ARGUMENT_SEPARATOR_CHAR {
                errors.insert(
                    child.read().get_name().to_owned(),
                    built_in_exceptions()
                        .dispatcher_expected_argument_separator()
                        .create_with_context(&reader),
                );
                continue;
            }

            context.with_command(&command);
            if reader.can_read_length(if redirect.is_none() { 2 } else { 1 }) {
                reader.skip();
                match redirect {
                    Some(redirect) => {
                        let child_context = CommandContextBuilder::new(
                            self,
                            source,
                            redirect.clone(),
                            reader.cursor(),
                        );
                        let parse = self.parse_nodes(&redirect, &reader, child_context);
                        context.with_child(Arc::new(parse.context));
                        return ParseResults {
                            context,
                            reader: parse.reader,
                            exceptions: parse.exceptions,
                        };
                    }
                    None => {
                        potentials.push(self.parse_nodes(&child, &reader, context));
                    }
                }
            } else {
                potentials.push(ParseResults {
                    context,
                    reader,
                    exceptions: IndexMap::new(),
                });
            }
        }

        if !potentials.is_empty() {
            if potentials.len() > 1 {
                potentials.sort_by(|a, b| {
                    if !a.reader.can_read() && b.reader.can_read() {
                        return Ordering::Less;
                    };
                    if a.reader.can_read() && !b.reader.can_read() {
                        return Ordering::Greater;
                    };
                    if a.exceptions.is_empty() && !b.exceptions.is_empty() {
                        return Ordering::Less;
                    };
                    if !a.exceptions.is_empty() && b.exceptions.is_empty() {
                        return Ordering::Greater;
                    };
                    Ordering::Equal
                });
            }
            let best_potential = potentials.into_iter().next().unwrap();
            return best_potential;
        }

        ParseResults {
            context: context_so_far,
            reader: original_reader.clone(),
            exceptions: errors,
        }
    }

    /// Parse and execute the command using the given input and context.
    ///
    /// The number returned depends on the command, and may not be of
    /// significance. Java's `execute(String, S)` / `execute(StringReader, S)`
    /// shortcut; the impl-Into parameter covers both overloads.
    ///
    /// This is a shortcut for `Self::parse` and `Self::execute_parsed`.
    pub fn execute(
        &self,
        input: impl Into<StringReader>,
        source: S,
    ) -> Result<R, CommandSyntaxException> {
        let input = input.into();

        let parse = self.parse(input, source);
        self.execute_parsed(parse)
    }

    /// Every root-to-leaf path in the tree rooted at `node`, as node lists.
    /// Java: the private `addPaths(node, result, parents)` helper behind
    /// `getPath` — private here too, matching Java's surface.
    fn add_paths(
        node: Arc<RwLock<CommandNode<S, R>>>,
        result: &mut Vec<Vec<Arc<RwLock<CommandNode<S, R>>>>>,
        parents: Vec<Arc<RwLock<CommandNode<S, R>>>>,
    ) {
        let mut current = parents;
        current.push(node.clone());
        result.push(current.clone());

        for child in node.read().get_children() {
            Self::add_paths(child.clone(), result, current.clone());
        }
    }

    /// The path from the root to `target` as node names, or empty when the
    /// node is not in the tree. Compares by node identity — Java:
    /// `list.get(list.size() - 1) == target` — so `target` must be the exact
    /// handle the tree holds (e.g. from `register` or `then_built`), not a
    /// structurally equal copy.
    ///
    /// # Panics
    ///
    /// Unreachable from any caller input: the trailing `expect` relies on the
    /// internal invariant that every path list is non-empty (each one is
    /// seeded with the root before recursion).
    pub fn get_path(&self, target: &Arc<RwLock<CommandNode<S, R>>>) -> Vec<String> {
        let mut nodes: Vec<Vec<Arc<RwLock<CommandNode<S, R>>>>> = Vec::new();
        Self::add_paths(self.root.clone(), &mut nodes, vec![]);

        for list in nodes {
            if Arc::ptr_eq(list.last().expect("Nothing in list"), target) {
                let mut result: Vec<String> = Vec::with_capacity(list.len());
                for node in list {
                    if !Arc::ptr_eq(&node, &self.root) {
                        result.push(node.read().get_name().to_owned());
                    }
                }
                return result;
            }
        }
        vec![]
    }

    /// Follow `path` of names down from the root, returning the node it
    /// reaches, or `None` if any name is missing. Java: `findNode(path)`.
    pub fn find_node(&self, path: &[&str]) -> Option<Arc<RwLock<CommandNode<S, R>>>> {
        let mut node = self.root.clone();
        for name in path {
            match node.clone().read().get_child(name) {
                Some(child) => {
                    node = child;
                }
                None => {
                    return None;
                }
            };
        }
        Some(node)
    }

    /// Execute a previously parsed command. If input is left over, the error
    /// mirrors Java: the single parse exception when there is exactly one
    /// (the first inserted, Java's `LinkedHashMap` first value), otherwise
    /// unknown-command (nothing parsed) or unknown-argument. A failed
    /// flatten notifies the consumer with `(original, false, 0)` first, like
    /// Java. Java: `execute(ParseResults)`.
    ///
    /// # Panics
    ///
    /// Unreachable from any caller input: the exception extraction is guarded
    /// by an exact `len() == 1` check on the parse errors map.
    pub fn execute_parsed(&self, parse: ParseResults<S, R>) -> Result<R, CommandSyntaxException> {
        if parse.reader.can_read() {
            return Err(if parse.exceptions.len() == 1 {
                parse.exceptions.values().next().unwrap().clone()
            } else if parse.context.get_range().is_empty() {
                built_in_exceptions()
                    .dispatcher_unknown_command()
                    .create_with_context(&parse.reader)
            } else {
                built_in_exceptions()
                    .dispatcher_unknown_argument()
                    .create_with_context(&parse.reader)
            });
        }

        let command = parse.reader.string();
        let original = Arc::new(parse.context.build(command));
        let flat_context = ContextChain::try_flatten(original.clone());
        let Some(flat_context) = flat_context else {
            self.consumer.on_command_complete(original, false, 0);
            return Err(built_in_exceptions()
                .dispatcher_unknown_command()
                .create_with_context(&parse.reader));
        };

        flat_context.execute_all(original.get_source_arc(), self.consumer.as_ref())
    }

    /// Every usage string for every path under `node`, optionally filtered by
    /// source permissions. Java: `getAllUsage(node, source, restricted)`.
    pub fn get_all_usage(
        &self,
        node: &CommandNode<S, R>,
        source: &S,
        restricted: bool,
    ) -> Vec<String> {
        let mut result = vec![];
        self.get_all_usage_recursive(node, source, &mut result, "", restricted);
        result
    }

    fn get_all_usage_recursive(
        &self,
        node: &CommandNode<S, R>,
        source: &S,
        result: &mut Vec<String>,
        prefix: &str,
        restricted: bool,
    ) {
        if restricted && !node.can_use(source) {
            return;
        }
        if node.get_command().is_some() {
            result.push(prefix.to_owned());
        }
        match node.get_redirect() {
            Some(redirect) => {
                let redirect = if Arc::ptr_eq(&redirect, &self.root) {
                    "...".to_owned()
                } else {
                    format!("-> {}", redirect.read().get_usage_text())
                };
                if prefix.is_empty() {
                    result.push(format!(
                        "{}{}{}",
                        node.get_usage_text(),
                        Self::ARGUMENT_SEPARATOR,
                        redirect
                    ));
                } else {
                    result.push(format!("{prefix}{}{redirect}", Self::ARGUMENT_SEPARATOR));
                }
            }
            None => {
                for child in node.get_children() {
                    let child = child.read();
                    self.get_all_usage_recursive(
                        &child,
                        source,
                        result,
                        if prefix.is_empty() {
                            child.get_usage_text()
                        } else {
                            format!(
                                "{prefix}{}{}",
                                Self::ARGUMENT_SEPARATOR,
                                child.get_usage_text()
                            )
                        }
                        .as_str(),
                        restricted,
                    );
                }
            }
        }
    }

    /// Gets the possible executable commands from a specified node.
    ///
    /// You may use [`Self::get_root`] as a target to get usage data for the
    /// entire command tree.
    /// Compact usage strings: children with identical shapes collapse, and
    /// alternatives render as `(a|b)` / `[a|b]`. One entry per child whose
    /// usage could be built, in child insertion order — Java returns a
    /// `LinkedHashMap<CommandNode<S>, String>`, the pair vector keeps that
    /// order with the node handle as the key (compare via `Arc::ptr_eq`).
    /// Java: `getSmartUsage`.
    pub fn get_smart_usage(
        &self,
        node: &CommandNode<S, R>,
        source: &S,
    ) -> Vec<(Arc<RwLock<CommandNode<S, R>>>, String)> {
        let mut result = Vec::new();

        let optional = node.get_command().is_some();
        for child in node.get_children() {
            let usage = self.get_smart_usage_recursive(&child.read(), source, optional, false);
            if let Some(usage) = usage {
                result.push((child, usage));
            }
        }

        result
    }

    fn get_smart_usage_recursive(
        &self,
        node: &CommandNode<S, R>,
        source: &S,
        optional: bool,
        deep: bool,
    ) -> Option<String> {
        if !node.can_use(source) {
            return None;
        }

        let this = if optional {
            format!(
                "{}{}{}",
                Self::USAGE_OPTIONAL_OPEN,
                node.get_usage_text(),
                Self::USAGE_OPTIONAL_CLOSE
            )
        } else {
            node.get_usage_text()
        };
        let child_optional = node.get_command().is_some();
        let open = if child_optional {
            Self::USAGE_OPTIONAL_OPEN
        } else {
            Self::USAGE_REQUIRED_OPEN
        };
        let close = if child_optional {
            Self::USAGE_OPTIONAL_CLOSE
        } else {
            Self::USAGE_REQUIRED_CLOSE
        };

        if deep {
            return Some(this);
        }

        if let Some(redirect) = node.get_redirect() {
            let redirect = if Arc::ptr_eq(&redirect, &self.root) {
                "...".to_owned()
            } else {
                format!("-> {}", redirect.read().get_usage_text())
            };
            return Some(format!("{this}{}{redirect}", Self::ARGUMENT_SEPARATOR));
        }

        let children = node
            .get_children()
            .into_iter()
            .filter(|child| child.read().can_use(source))
            .collect::<Vec<_>>();
        match children.len().cmp(&1) {
            Ordering::Less => {}
            Ordering::Equal => {
                let usage = self.get_smart_usage_recursive(
                    &children[0].read(),
                    source,
                    child_optional,
                    child_optional,
                );
                if let Some(usage) = usage {
                    return Some(format!("{this}{}{usage}", Self::ARGUMENT_SEPARATOR));
                }
            }
            Ordering::Greater => {
                let mut child_usage = HashSet::new();
                for child in &children {
                    let usage =
                        self.get_smart_usage_recursive(&child.read(), source, child_optional, true);
                    if let Some(usage) = usage {
                        child_usage.insert(usage);
                    }
                }
                match child_usage.len().cmp(&1) {
                    Ordering::Less => {}
                    Ordering::Equal => {
                        let usage = child_usage.into_iter().next().unwrap();
                        let usage = if child_optional {
                            format!(
                                "{}{usage}{}",
                                Self::USAGE_OPTIONAL_OPEN,
                                Self::USAGE_OPTIONAL_CLOSE
                            )
                        } else {
                            usage
                        };
                        return Some(format!("{this}{}{usage}", Self::ARGUMENT_SEPARATOR));
                    }
                    Ordering::Greater => {
                        let mut builder = String::new();
                        builder.push_str(open);
                        let mut count = 0;
                        for child in children {
                            if count > 0 {
                                builder.push_str(Self::USAGE_OR);
                            }
                            builder.push_str(&child.read().get_usage_text());
                            count += 1;
                        }
                        if count > 0 {
                            builder.push_str(close);
                            return Some(format!("{this}{}{builder}", Self::ARGUMENT_SEPARATOR));
                        }
                    }
                }
            }
        }

        Some(this)
    }

    /// Suggestions for the end of a parsed command (Java:
    /// `getCompletionSuggestions(ParseResults)`).
    ///
    /// Anchors at the full length of the parsed input and delegates to
    /// [`Self::get_completion_suggestions_with_cursor`], like Java's
    /// one-argument overload calling the two-argument one with
    /// `parse.getReader().getTotalLength()`.
    pub fn get_completion_suggestions(parse: ParseResults<S, R>) -> SuggestionsFuture {
        let cursor = parse.reader.total_length();
        Self::get_completion_suggestions_with_cursor(parse, cursor)
    }

    /// Same as [`Self::get_completion_suggestions`] but anchored at `cursor`
    /// instead of end-of-input (Java: the two-argument overload). The shape of
    /// the computation mirrors Java step for step:
    ///
    /// 1. `findSuggestionContext(cursor)` on the parse's context builder —
    ///    same guards, child chain, last-node/root fallbacks and the same
    ///    panic for a cursor before the parsed range.
    /// 2. `start = min(startPos, cursor)`.
    /// 3. The input is truncated at `cursor` characters and lowercased with
    ///    full Unicode case conversion — byte-for-byte what Java's
    ///    `toLowerCase(Locale.ROOT)` produces (verified against a Java 21
    ///    oracle, including the Greek final-sigma rule Σ → ς in word-final
    ///    position and the İ → i + U+0307 expansion), so prefix matching
    ///    agrees with Java beyond plain ASCII too.
    /// 4. Every child of the node before the cursor gets one
    ///    `listSuggestions` call with a freshly built context (Java builds
    ///    inside the loop as well) and a pre-lowercased builder.
    /// 5. All child futures are polled concurrently and merged against the
    ///    full input, like Java's `allOf(...).thenRun(merge)`.
    ///
    /// Divergences from Java, all deliberate:
    ///
    /// - Java catches a `CommandSyntaxException` thrown *while obtaining* a
    ///   child's future and lets that node contribute nothing. A Rust
    ///   producer returns a boxed future and cannot fail at call time, so
    ///   there is no sync-throw path to catch; a provider that fails inside
    ///   its future propagates the error out of the merged future. Java's
    ///   own async-failure case leaves the merged future incomplete forever
    ///   (its `allOf` fails, the `thenRun` merge is skipped, nobody observes
    ///   the failed stage — the caller hangs); see [`SuggestionsFuture`].
    ///   A provider wanting Java's swallow-on-failure behaviour returns an
    ///   empty future instead of an error.
    /// - A `cursor` past the end of the input saturates the truncation here;
    ///   Java's `substring` would throw an uncaught
    ///   `StringIndexOutOfBoundsException` (cursor positions are characters,
    ///   see [`StringReader`]).
    pub fn get_completion_suggestions_with_cursor(
        parse: ParseResults<S, R>,
        cursor: usize,
    ) -> SuggestionsFuture {
        let context = parse.context;

        let node_before_cursor = context.find_suggestion_context(cursor);
        let parent = node_before_cursor.parent;
        let start = usize::min(node_before_cursor.start_pos, cursor);

        let full_input = parse.reader.string().to_string();
        // cursor (and every position here) is a character index — bridge to
        // bytes for the slice
        let truncated_input = full_input[..char_to_byte(&full_input, cursor)].to_string();
        let truncated_input_lowercase = truncated_input.to_lowercase();

        let mut futures = Vec::new();
        for node in parent.read().get_children() {
            let future = node.read().list_suggestions(
                node_before_cursor.context.build(&truncated_input),
                SuggestionsBuilder::new_with_lowercase(
                    &truncated_input,
                    &truncated_input_lowercase,
                    start,
                ),
            );
            futures.push(future);
        }

        join_all_and_merge(futures, full_input)
    }

    /// Scans the whole command tree for ambiguous commands. Java:
    /// `findAmbiguities(AmbiguityConsumer)` — the shortcut over
    /// `CommandNode#findAmbiguities` on the root.
    pub fn find_ambiguities(&self, consumer: &dyn AmbiguityConsumer<S, R>) {
        CommandNode::find_ambiguities(&self.root, consumer);
    }
}

impl<S: Send + Sync + 'static, R: CommandResultTrait> Default for CommandDispatcher<S, R> {
    fn default() -> Self {
        Self::new()
    }
}
