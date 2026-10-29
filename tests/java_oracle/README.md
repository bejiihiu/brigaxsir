# Java differential oracle

This directory holds the golden-oracle harness used by
`tests/differential_oracle_test.rs` (task 20): the **real Java 1.3 Brigadier
library** is compiled alongside `OracleHarness.java`, and the Rust test feeds
identical scenario batteries to both implementations and compares the results
field by field.

The compiled `.class` files are **never committed** — the test compiles the
harness on demand into `target/java-oracle/classes` (recompiling whenever any
`.java` source is newer than the `.compiled` stamp). The committed artifacts
are this harness source, `build.cmd`, and this README.

## Requirements

- Java 21 (or any JDK with `java`/`javac` on PATH). Gradle is NOT used — the
  library is compiled with plain `javac` (gradle 4.10 does not run on Java 21).
- The Java library sources must sit next to the crate at
  `../brigadier/src/main/java` (relative to the crate root).

## Wire protocol

Line-based, UTF-8 both ways. Rust sends the request, Java prints the
response; lines are `\n`-terminated, Java answers with CRLF (the Rust side
strips it).

Request (sent by the test):

```
BRIGAXSIR-ORACLE v1        # handshake; harness acks "OK BRIGAXSIR-ORACLE v1"
S <scenario-id>            # begin a scenario
N <path> [k=v ...]         # register a node (see below)
O <op ...>                 # one operation
E                          # end of scenario
DONE                       # finish
```

- `<path>` = `/`-separated segments; a segment is a literal name or
  `@<type>:<name>[:<min>:<max>]` with `-` for an absent bound
  (`@int:value:0:10`, `@int:value:-100:-`, `@word:w`, `@bool:v`,
  `@string:s`, `@greedy:t`, `@long:v`, `@float:x`, `@double:x`).
- Node flags (applied to the **last** segment of the path):
  - `exec=<n>` — command returning the constant `n`
  - `exec=int:<name>` / `exec=long:<name>` — command returning the parsed
    argument (the long form is cast to int, Java- and Rust-exactly)
  - `exec=strlen:<name>` / `exec=bool:<name>` — length of a string argument /
    `true` → 1, `false` → 0
  - `req=true|false|eq:<n>` — requirement: constant or "source equals n"
  - `red=<target-path-or-.>` — redirect to a previously registered node
    (paths are keyed by every prefix) or to the root (`.`)
  - `mod=identity|dup:<n>|empty` — redirect modifier producing
    `[source]` / `[source..source+n-1]` / `[]`
  - `fork=1` — the fork flag
  - `sug=list:<w[=tooltip]|...>` — custom suggestion provider suggesting the
    listed words (with optional tooltips)
- Ops:
  - `parse <source> <input>` — input is the rest of the line (may contain
    spaces)
  - `exec <source> <input>`
  - `sug <source> <cursor> <input>`
  - `usage <0|1 restricted> <source> <node-path-or-.>`
  - `smart <source> <node-path-or-.>`
  - `path <node-path-or-.>`
  - `find <comma-joined-names-or-->` (`-` = empty path = the root)

Response (printed by the harness, in op order):

```
S <scenario-id>
X <count>                        # parse: exception map size
Xe <node-name> <cursor-or--1>    # per error, insertion order
Xc <context-snippet-or-->        # getContext() or "-"
Xm <raw-message>                 # getRawMessage().getString()
Xr <reader-cursor> <0|1>         # reader cursor + canRead
Xg <start> <end>                 # context range
R <int>                          # exec: result
Rerr <cursor-or--1>              # exec: exception
Rc <context-snippet-or-->
Rm <raw-message>
Y <start> <end>                  # suggestions range
Ys <text>                        # per suggestion, final order
Yt <tooltip-or-->                # its tooltip
U <count>                        # getAllUsage size
Us <string>
M <count>                        # getSmartUsage size
Ms <child-spec-path> <string>
P <comma-joined-or-->            # getPath
F <0|1>                          # findNode found
E
DONE
```

Any harness-side problem prints `ERR <message>` and exits 1.

## Manual regeneration / debugging

`build.cmd` compiles the library + harness into
`target\java-oracle\classes` (the same cache the test uses). Then:

```
java -cp target\java-oracle\classes OracleHarness < scenarios.txt
```

## Comparison policy

- BMP-only inputs (ASCII and Cyrillic alike): exact comparison, including
  cursors and ranges — char cursors equal UTF-16 cursors for BMP text.
- Astral input (emoji scenarios): semantic comparison only — messages, node
  names and results must match, cursors/ranges/context snippets are skipped
  (Rust counts characters, Java counts UTF-16 units; documented divergence).
