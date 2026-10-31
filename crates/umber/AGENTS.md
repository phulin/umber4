# Core MVP

Read the root guidance. `src/lib.rs` separates the session stores from restorable
document state;
`checkpoint.rs` owns the non-consuming outer driver and explicit snapshot roots,
with replay and ownership tests in `checkpoint_tests.rs`;
`tables.rs` supplies pooled COW table pages with ownership tests in
`tables_tests.rs`; `messages.rs` retains immutable message prefixes with tests
in `messages_tests.rs`;
`token.rs` owns compact tokens, the command inventory, definition metadata, and
source descriptors; `names.rs` owns name lookup and immutable spelling bytes;
`values.rs` owns the shared token builder and backing chunks;
`value_headers.rs` owns stable value-header slabs and their shared owners;
`value_metadata.rs` owns typed metadata blocks and buffers, with focused tests in
`value_metadata_tests.rs`; `value_packets_tests.rs` checks packet and long-name
scratch ownership, reuse, and failure cleanup;
`values_tests.rs` and `memory_tests.rs` check reclamation and semantic ownership;
`input.rs` supplies lazy character input and token cursors;
`define.rs` builds macro definitions; `macros.rs` builds invocation packets and
captures ranges; `delimit.rs` scans grouped and delimited arguments; `scan.rs` scans
integers and primitive operands; `expression.rs` evaluates integer expressions;
`scalars.rs` stores integer tables and their group levels;
`numeric.rs` supplies scaled arithmetic/glue and `numeric_tests.rs` checks it;
`dimension_scan.rs` scans dimensions/glue, `quantities.rs` shares internal-value
formatting, `layout_values.rs` owns typed COW bindings, and `layout_assign.rs`
applies typed assignments/arithmetic, with operand and fixture tests in
`dimension_tests.rs`;
`nodes.rs` owns versioned chunked node storage and consuming list/node transfers,
with branch/reclamation tests in `nodes_tests.rs`; these node APIs precede actual
paragraph and page execution;
`tfm.rs` owns validated immutable metric resources and exact scaling, `fonts.rs`
owns restorable loaded-font descriptors/parameters and font assignments,
`text.rs` owns external modes and horizontal/vertical lists, and `ligatures.rs`
builds TFM ligatures/kerns with component ownership; `font_text_tests.rs` and
`tfm_tests.rs` use controlled metrics from `font_test_data.rs`, while the explicitly
invoked native adapter in `font_oracle_tests.rs` compares real and controlled TFMs;
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
