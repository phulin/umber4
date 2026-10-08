# Core MVP

Read the root guidance. `src/lib.rs` owns the driver and document state;
`token.rs` owns compact tokens, immutable definition storage, names, and source
descriptors; `names.rs` owns the immutable spelling-byte allocator;
`input.rs` supplies lazy character input and token cursors;
`define.rs` builds macro definitions; `macros.rs` scans grouped arguments and builds
invocation packets; `delimit.rs` matches delimited arguments; `scan.rs` scans
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
