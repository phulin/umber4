# Core MVP

Read the root guidance. `src/lib.rs` owns the driver and document state;
`token.rs` owns compact tokens, immutable definition storage, names, and source
descriptors; `input.rs` supplies lazy character input and token cursors;
`macros.rs` builds definitions and argument packets; `scan.rs` scans integers;
`expand.rs` implements expansion and conditionals; `execute.rs` handles
assignments and groups; `tests.rs` contains semantic fixtures; `main.rs` is the
native file/message adapter. Keep host access out of the portable library.
