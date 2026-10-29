# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-10-25

### Added

Initial release: a full, safe, idiomatic Rust port of Mojang Brigadier 1.3
(`com.mojang.brigadier`), behaviour-verified against the Java original.

- `CommandDispatcher`: registration, parsing and execution, redirects (plain,
  single-source modifier, fork), context chains, smart/full usage rendering,
  ambiguity detection and completion suggestions — observable behaviour
  verified line-by-line against Java 1.3.
- The complete Java error model: `Message`/`LiteralMessage`,
  `CommandExceptionType`, `Simple`/`Dynamic{,2,3,4,N}` exception types, and
  `CommandSyntaxException` with Java-exact message strings — including
  `Double.toString`/`Float.toString` number rendering, differentially verified
  against a local JDK 21 oracle.
- Suggestions: boxed `Send` async futures (the `CompletableFuture` analog),
  Java-exact result ordering (HashSet iteration order plus TimSort
  small-array path emulation), tooltips, `IntegerSuggestion`, and contextual
  suggestion providers.
- All seven built-in argument types at Java parity: bounded constructors,
  getters, `Display` matching Java `toString`, and hash-code formulas pinned
  against JDK oracle values.
- The full builder family (`literal`/`argument` + fluent surface), the
  `CommandContext`/`CommandContextBuilder`/`ContextChain`/`ParsedArgument`
  context family with typed `get_argument::<V>` reads (Java-exact panic
  messages) plus a `try_get_argument` extension, and the `CommandNode` tree
  with insertion-ordered children and `Arc`-handle identity.
- UTF-8-correct `StringReader`: char-based cursors, safe slicing, non-ASCII
  coverage (Cyrillic, emoji) and no panics on malformed input.
- Thread safety: the dispatcher and tree are `Send + Sync` unconditionally,
  contexts when the source is; a dedicated concurrency test suite.
- 403 tests — the Java test corpus ported 1:1 plus Rust-specific
  property-based, concurrency and robustness suites and an 85-scenario
  differential oracle battery against the live Java library — with 17
  doctests, criterion benchmarks ported from Java's JMH files, and four
  runnable examples.
- Java's one open TODO (separate types for `ContextChain` modifier and
  executable stages) is closed type-safely: `ModifierStage`/`ExecutableStage`
  wrap the full context, and an executable stage can only exist for a
  command-bearing context.
- The README doubles as the crate-level docs (`#![doc = include_str!]`), so
  the docs.rs landing page and the repository README are one artifact.
- Robustness verified: deep (500-level) literal chains, redirect chains,
  usage/path/ambiguity walks and completion suggestions run green in debug
  and release on a sized stack, with the recursion-vs-stack-limit parity to
  Java documented (both implementations overflow on pathologically deep
  trees); 1 MB+ inputs through every string mode stay linear and panic-free.

### Divergences from Java (deliberate, documented at each code site)

- Commands return `Result<R, CommandSyntaxException>` instead of throwing
  exceptions; `get_argument` keeps Java's panic behaviour and
  `try_get_argument` is the `Result`-returning extension.
- Reader cursors count Unicode scalar values (chars), not UTF-16 units, and
  saturate at input bounds instead of throwing
  `StringIndexOutOfBoundsException`.
- Argument values are type-erased (`Arc<dyn ParsedValue>`) with typed reads.
- Suggestion futures are boxed and `Send`; a failing provider propagates its
  error instead of leaving Java's never-completing future behind.
- The full list lives in the README ("Differences from Java") and in the
  per-item documentation.
