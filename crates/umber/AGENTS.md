# Core MVP

Read the root guidance. `src/lib.rs` owns the driver and document state;
`token.rs` owns compact tokens, the command inventory, definition metadata, and
source descriptors; `names.rs` owns name lookup and immutable spelling bytes;
`values.rs` owns the shared token builder and backing chunks;
`value_headers.rs` owns stable value-header slabs and their shared owners;
`value_metadata.rs` owns typed metadata blocks and buffers, with focused tests in
`value_metadata_tests.rs`;
`values_tests.rs` and `memory_tests.rs` check reclamation and semantic ownership;
`input.rs` supplies lazy character input and token cursors;
`define.rs` builds macro definitions; `macros.rs` builds invocation packets and
captures ranges; `delimit.rs` scans grouped and delimited arguments; `scan.rs` scans
integers and primitive operands; `expression.rs` evaluates integer expressions;
`scalars.rs` stores integer tables and their group levels;
`lists.rs` scans raw/expanded lists into their destinations; `convert.rs` builds
conversion and case-change output; `expand.rs` dispatches expansion;
`condition.rs` handles conditional selection/skipping; `execute.rs` handles
assignments, groups, and deferred tokens; `tests.rs` and `extended_tests.rs`
contain semantic fixtures; `scan_tests.rs` checks operand cursor boundaries and
rollback; `names_tests.rs` checks spelling chunk publication and retention;
`main.rs` is the native file/message adapter. Keep
host access out of the portable library.

`examples/allocations.rs` is a separate native diagnostic adapter. Its allocator
counters cover engine construction and execution, excluding the host input buffer.

`lexing/`, built only with the `lexing-prototype` feature, is the whole-file lexer
prototype from `docs/fast-lexing.md`: `scan.rs` splits lines and runs the shared
state machine, `simd.rs` builds AVX2 masks and writes runs, `hash.rs` holds the
name hashes, `line.rs` lexes fallback lines, and `tests.rs` compares every lexer
with `FileInput::read`. The engine does not use it. `examples/lexing.rs` checks
and times the lexers on source files.
