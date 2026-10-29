# brigaxsir

A Rust port of Mojang's [Brigadier](https://github.com/Mojang/brigadier) command
parser & dispatcher, originally designed for Minecraft: Java Edition and freely
usable elsewhere under the MIT license.

`brigaxsir` follows the Java sources (`com.mojang.brigadier` 1.3) as the source
of truth: observable behaviour, error texts, cursor positions, parse ordering
and usage rendering match the original. The surface is idiomatic Rust —
`snake_case` names, `Result` instead of thrown exceptions, `Option` instead of
`null` — while staying safe: no `unsafe` anywhere in the crate.

# Installation

```toml
[dependencies]
brigaxsir = "0.1"
```

Full API reference: [docs.rs/brigaxsir](https://docs.rs/brigaxsir).

# Usage

At the heart of the library you need a
[`CommandDispatcher<S>`](crate::command_dispatcher::CommandDispatcher), where
`S` is any custom object you choose to identify a "command source".

A command dispatcher holds a "command tree", a series of
[`CommandNode`](crate::tree::CommandNode)s that represent the various possible
syntax options that form a valid command.

## Registering a new command

Before we can start parsing and dispatching commands, we need to build up our
command tree. Every registration is an append operation, so you can freely
extend existing commands in a project without needing access to the source
code that created them.

A "command" is a fairly loose term, but typically it means an exit point of
the command tree. Every node can have an `executes` closure attached to it,
which signifies that if the input stops here then this closure will be called
with the context so far.

Consider the following example:

```rust
use brigaxsir::prelude::*;
use std::sync::Arc;

#[derive(Debug, PartialEq)]
struct CommandSource {}

let mut dispatcher = CommandDispatcher::new();

dispatcher.register(
    literal("foo")
        .then(
            argument("bar", integer()).executes(|ctx| {
                println!("Bar is {}", get_integer(ctx, "bar"));
                1
            }),
        )
        .executes(|_| {
            println!("Called foo with no arguments");
            1
        }),
);

// `foo` and `foo <bar>` are both valid commands now.
assert_eq!(dispatcher.execute("foo", Arc::new(CommandSource {})).unwrap(), 1);
assert_eq!(dispatcher.execute("foo 123", Arc::new(CommandSource {})).unwrap(), 1);

// A failed parse surfaces as an error instead of an exception.
assert!(dispatcher.execute("foo abc", Arc::new(CommandSource {})).is_err());
```

This registers two "commands": `foo` and `foo <bar>`. It is also common to
refer to the `<bar>` as a "subcommand" of `foo`, as it's a child node.

At the start of the tree is a "root node", and it **must** have literal nodes
as children. Here we register one command under the root: `literal("foo")`,
which means "the user must type the literal string 'foo'".

Under that are two extra definitions: a child node for possible further
evaluation, or an `executes` closure if the user input stops here.

The child node works exactly the same way, but is no longer limited to
literals. The other type of node that is now allowed is an argument node,
which takes in a name and an argument type
([`argument`](crate::builder::argument)).

Arguments can be anything, and you are encouraged to build your own for
seamless integration into your own product
([`ArgumentType`](crate::arguments::ArgumentType)). There are some standard
arguments included in the library, such as
[`integer()`](crate::arguments::integer_argument_type::integer).

Argument types are asked to parse input as much as they can, and then store
the "result" of that argument however they see fit or fail with a relevant
error if they can't parse. For example, an integer argument parses `"123"` and
stores it as `123` (`i32`), but fails if the input were `onetwothree`.

When a command is actually run, it can access these arguments in the context
provided to the registered closure:
[`CommandContext::get_argument`](crate::context::CommandContext::get_argument)
(Java's `getArgument(name, Class)`) with the built-in types, and the fallible
[`try_get_argument`](crate::context::CommandContext::try_get_argument) where
Java would throw.

## Parsing user input

So, we've registered some commands and now we're ready to take in user input.
If you're in a rush, you can just call
[`dispatcher.execute("foo 123", source)`](crate::command_dispatcher::CommandDispatcher::execute)
and call it a day.

The result of `execute` is the value returned from the evaluated command. Its
meaning depends on the command — by convention (and like Java) it is an
`i32`, but the dispatcher is generic over the result type `R`.

The `source` is an object of `S`, your own custom type to track
users/players/etc. It will be provided to the command so that it has some
context on what's happening.

If the command failed or could not parse, a
[`CommandSyntaxException`](crate::errors::CommandSyntaxException) is returned
as `Err` instead of being thrown.

If you wish to have more control over the parsing & executing of commands, or
wish to cache the parse results so you can execute them multiple times, you
can split it up into two steps:

```rust
# use brigaxsir::prelude::*;
# use std::sync::Arc;
# #[derive(Debug, PartialEq)]
# struct CommandSource {}
# let mut dispatcher = CommandDispatcher::new();
# dispatcher.register(
#     literal("foo")
#         .then(argument("bar", integer()).executes(|ctx| {
#             let _ = get_integer(ctx, "bar");
#             1
#         }))
#         .executes(|_| 1),
# );
let source = Arc::new(CommandSource {});
let parse = dispatcher.parse("foo 123", source);

// execute takes the results by value; the results are never mutated by it,
// so clones let you run the same parse more than once.
assert_eq!(dispatcher.execute_parsed(parse.clone()).unwrap(), 1);
assert_eq!(dispatcher.execute_parsed(parse).unwrap(), 1);
```

This is highly recommended, as the parse step is the most expensive, and may
be easily cached depending on your application.

You can also use this to do further introspection on a command, before (or
without) actually running it.

## Inspecting a command

If you [`parse`](crate::command_dispatcher::CommandDispatcher::parse) some
input, you can find out what it will perform (if anything) and provide hints
to the user safely and immediately.

The parse never fails: the
[`ParseResults`](crate::parse_results::ParseResults) it returns contain a
*possible* context that a command may be called with (and from that, you can
inspect which nodes the user entered, complete with start/end positions in
the input string). It also contains a map of parse errors for each command
node it encountered. If it couldn't build a valid context, the reason why is
inside this error map.

## Displaying usage info

There are two forms of "usage strings" provided by the library, both taking a
target node.

[`get_all_usage`](crate::command_dispatcher::CommandDispatcher::get_all_usage)
returns a list of all possible commands (executable end-points) under the
target node and their human readable path. If `restricted`, it will ignore
commands that `source` does not have access to
([`requires`](crate::builder::argument_builder::ArgumentBuilder::requires)).

[`get_smart_usage`](crate::command_dispatcher::CommandDispatcher::get_smart_usage)
returns one "smart usage" string per child node. This tries to squash
future-nodes together and show optional & typed information.

```rust
# use brigaxsir::prelude::*;
# use std::sync::Arc;
# #[derive(Debug, PartialEq)]
# struct CommandSource {}
# let mut dispatcher = CommandDispatcher::new();
# dispatcher.register(
#     literal("foo")
#         .then(argument("bar", integer()).executes(|ctx| {
#             let _ = get_integer(ctx, "bar");
#             1
#         }))
#         .executes(|_| 1),
# );
let source = Arc::new(CommandSource {});
let root = dispatcher.get_root();

assert_eq!(
    dispatcher.get_all_usage(&root.read(), &source, true),
    ["foo", "foo <bar>"]
);

let smart = dispatcher.get_smart_usage(&root.read(), &source);
assert_eq!(smart.len(), 1);
assert_eq!(smart[0].1, "foo [<bar>]");
```

## Completion suggestions

The dispatcher also computes tab completions:
[`get_completion_suggestions`](crate::command_dispatcher::CommandDispatcher::get_completion_suggestions)
(and its `_with_cursor` variant) parse-aware walk the tree and merge the
suggestions of every reachable node — literal prefixes and argument types
alike, ordered exactly like Java's `compareToIgnoreCase` ordering. Custom
suggestions are provided through
[`SuggestionsBuilder`](crate::suggestion::SuggestionsBuilder) and
[`SuggestionProvider`](crate::suggestion::SuggestionProvider); see the
`custom_argument` example below.

One thing to know before writing your own argument completions: the builder
does **not** filter candidates by the typed prefix — Java's doesn't either.
Literal nodes check the prefix themselves (case-insensitively, via
[`remaining_lowercase`](crate::suggestion::SuggestionsBuilder::remaining_lowercase));
a custom argument type should do the same inside its `list_suggestions`, or
every candidate shows up regardless of what the user has typed so far. The
`realistic` example below does exactly that.

# Features

- **Java 1.3 parity** — the whole `com.mojang.brigadier` surface: dispatcher,
  tree, builders, argument types, context family, error model, suggestions,
  reader.
- **Byte-exact behaviour** — error messages, context snippets
  (`<--[HERE]`), usage rendering, suggestion ordering (down to Java's
  `HashSet` iteration order and TimSort small-path emulation), and
  `Double.toString`/`Float.toString`-style number formatting.
- **Safe & thread-safe** — `#![deny(unsafe_code)]`; the dispatcher and tree
  are `Send + Sync` unconditionally, the context family when `S` is.
- **Typed argument reads** — parsed values are type-erased in the tree but
  read back through `get_argument::<V>` / `try_get_argument::<V>` with
  Java-exact error messages.
- **Async suggestions** — providers return a `Send` boxed future (Java's
  `CompletableFuture` analogue), with synchronous helpers for the simple
  cases.
- **Zero mandatory runtime** — no executor, no async runtime; two small
  dependencies (`parking_lot`, `indexmap`).

# Performance

Numbers from the criterion benchmark suite (the Java JMH benchmarks ported
1:1; medians on the maintainer's Windows-gnu machine, release profile —
treat them as order-of-magnitude, not absolute):

| operation | median |
|---|---|
| parse a simple command (`parse_c`) | ~1.4 µs |
| parse a deep literal tree (`parse_a1i`) | ~3.9 µs |
| parse through a redirect (`parse_k1i`) | ~3.9 µs |
| execute a simple command | ~1.6 µs |
| execute through a single redirect | ~2.8 µs |
| root-level completion suggestions | ~36 µs |

The parse path is fast in absolute terms — parsing a thousand-level
command chain and executing it takes ~17 ms in release, against ~28 ms for
the Java original under the same probe (Java 21, both default builds).
Two structural notes for anyone pushing the boundaries:

- **Recursion depth** — `parse`, redirects, usage, paths and ambiguity
  scans recurse once per tree level, like Java. Both implementations hit
  the platform stack limit on pathologically deep trees: with a 2 MiB stack,
  release builds handle ~1000-deep chains and Java 21 (with `-Xss2m`)
  handles ~4000 — same order of magnitude, both far beyond any realistic
  command tree. Debug builds consume larger frames (~300-500 levels on
  2 MiB). If your trees can be unusually deep, run parsing on a thread with
  a bigger stack (the Rust equivalent of Java's `-Xss`).
- **Huge inputs are linear** — 1 MB+ strings through the greedy/word/quoted
  string modes parse in milliseconds (measured linear up to 8 MB), with
  bounded error-context snippets and no quadratic blowups.

# How it maps to Java

| Java (`com.mojang.brigadier.*`) | Rust |
|---|---|
| `CommandDispatcher<S>` | [`command_dispatcher::CommandDispatcher<S, R = i32>`](crate::command_dispatcher::CommandDispatcher) |
| `builder.LiteralArgumentBuilder`, `literal(...)` | [`builder::literal`](crate::builder::literal) |
| `builder.RequiredArgumentBuilder`, `argument(...)` | [`builder::argument`](crate::builder::argument) |
| `arguments.ArgumentType<T>` | [`arguments::ArgumentType`](crate::arguments::ArgumentType) |
| `arguments.{Integer,Long,Double,Float,String,Bool}ArgumentType` | the factories `integer()`, `long()`, `double()`, `float()`, `string()`/`word()`/`greedy_string()`, `bool()` |
| `context.{CommandContext,CommandContextBuilder,ContextChain}` | [`context`](crate::context) |
| `exceptions.*` | [`errors`](crate::errors) |
| `suggestion.*` | [`suggestion`](crate::suggestion) |
| `tree.{CommandNode,LiteralCommandNode,ArgumentCommandNode,RootCommandNode}` | [`tree::CommandNode`](crate::tree::CommandNode) + `CommandNodeKind` |
| `StringReader`, `ImmutableStringReader` | [`string_reader`](crate::string_reader) |
| `Message`, `LiteralMessage` | [`message`](crate::message) |
| `ParseResults` | [`parse_results::ParseResults`](crate::parse_results::ParseResults) |
| `ResultConsumer`, `RedirectModifier` | [`result_consumer`](crate::result_consumer), [`modifier`](crate::modifier) |

Naming follows one convention: Java-primary getters keep `get_*`
(`get_argument`, `get_root`, ...), simple value accessors use bare
`snake_case` (`text()`, `remaining()`), and Java method overloads become
arity-suffixed functions (`integer`/`integer_min`/`integer_range`,
`redirect`/`redirect_with`).

# Differences from Java

The port is behavioural-parity first; the differences are the Rust-idiom
conversions, each documented at its code site:

- **Exceptions are `Result`.** Commands, modifiers and providers return
  `Result<_, CommandSyntaxException>` instead of throwing;
  `IllegalArgumentException`-style programmer errors (like a missing or
  mistyped `get_argument`) panic with Java's exact messages, and
  [`try_get_argument`](crate::context::CommandContext::try_get_argument)
  offers them as data.
- **The command result type is generic (`R`, default `i32`).** Java's
  commands always return `int`; here `R` is a type parameter so richer
  results are possible, with `i32` the drop-in default.
- **Suggestions are boxed `Send` futures, not `CompletableFuture`.** The
  error rides inside the future; a failing provider's error propagates out of
  the merged future instead of hanging it, as Java's does.
- **`ArgumentType` is not generic over its value type.** The trait is
  object-safe; parsed values are stored type-erased and read back with
  `get_argument::<V>`. Argument types needing the command source downcast it
  in `parse_with_source`.
- **Cursors count characters, not UTF-16 code units.** Identical for ASCII
  and BMP input; astral characters (emoji) count one instead of two.
- **The reader saturates at the input bounds** where Java would throw
  `StringIndexOutOfBoundsException` (only `peek`/`peek_offset`/`read` panic,
  matching Java's throwing spots).
- **Overload naming** — Java's overloads become distinctly named functions
  (`integer`/`integer_min`/`integer_range`), and Java `Collection` views
  become snapshots of shared `Arc` handles.
- **`ContextChain` stages are separate types.** Java's one open TODO
  ("ideally those two would have separate types") is closed: modifier and
  executable chain stages are `ModifierStage`/`ExecutableStage`, and an
  executable stage can only be built for a command-bearing context — the
  runtime `IllegalArgumentException` check becomes a constructor that
  returns `Option`.

# Examples

Runnable examples live in the `examples` directory:

- `basic` — the quickstart above: register, execute, parse/execute split,
  usage strings.
- `redirect_fork` — redirects, single-source redirect modifiers and forking
  over multiple sources.
- `custom_argument` — a custom `ArgumentType` end to end: parsing, a
  Java-style dynamic error type, completion suggestions and examples.
- `realistic` — a small game-style command system: permission-gated
  commands, custom player/item arguments with tooltips, an optional
  argument, forking a chat message to every other player, and help/usage
  output.

Run them with `cargo run --example basic` (and likewise for the others).

# License

MIT. See `LICENSE` and `NOTICE` for attribution.
