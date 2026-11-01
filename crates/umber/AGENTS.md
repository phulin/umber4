# Core MVP

Read the root guidance. `src/lib.rs` separates the session stores from restorable
document state;
`checkpoint.rs` owns the non-consuming outer driver and explicit snapshot roots,
with replay and ownership tests in `checkpoint_tests.rs`;
`tables.rs` supplies pooled COW table pages with ownership tests in
`tables_tests.rs`; `messages.rs` retains immutable chronological diagnostic prefixes with tests
in `messages_tests.rs`;
`token.rs` owns compact tokens, the command inventory, definition metadata, and
source descriptors; `names.rs` owns name lookup and immutable spelling bytes;
`values.rs` owns the shared token builder and backing chunks;
`value_headers.rs` owns stable value-header slabs and their shared owners;
`value_metadata.rs` owns typed metadata blocks and buffers, with focused tests in
`value_metadata_tests.rs`; `value_packets_tests.rs` checks packet and long-name
scratch ownership, reuse, and failure cleanup;
`values_tests.rs` and `memory_tests.rs` check reclamation and semantic ownership;
`input.rs` supplies lazy character input; `input_tokens.rs` owns ordered token
and file-boundary cursors; `input_files.rs` supplies borrowed host resolution and
filename scanning, with native, cleanup, and replay tests in `input_files_tests.rs`;
`define.rs` builds macro definitions; `macros.rs` builds invocation packets and
captures ranges; `delimit.rs` scans grouped and delimited arguments; `scan.rs` scans
integers and primitive operands; `expression.rs` evaluates integer expressions;
`scanner.rs` scopes transient scanner contexts and outer validity, with
replay, error, and lifetime coverage in `scanner_tests.rs`;
`scalars.rs` stores integer tables and their group levels;
`math_code_tests.rs` checks math-code initialization, scoped assignments,
math-character meanings, checkpoints, and the explicit native state oracle;
`math.rs` owns affine temporary math lists and a reusable typed arena;
`math_convert.rs` owns iterative conversion work and `math_lists.rs` its two passes;
`math_clean.rs`/`math_boxing.rs` own clean/shared box work; `math_atoms.rs` owns
ordinary ligatures/nuclei and `math_scripts.rs` scripts/operator limits;
`math_fraction.rs`, `math_forms.rs`, and `math_delimiters.rs` own fractions,
decorations/radicals/vcenter, and paired/extensible delimiters; `math_accents.rs`
owns accent work and character-script transfer.
`math_tests.rs`/`math_forms_tests.rs` check ownership, reuse, failures, checkpoints,
and deep traversal; `math_oracle_tests.rs`, `math_real_oracle_tests.rs`, and
`math_forms_oracle_tests.rs` compare typed ASTs with native geometry and DVI;
`math_accent_tests.rs` checks accent ownership/reuse/failure boundaries, and
`math_accent_oracle_tests.rs` compares controlled and CM accent geometry;
`math_glue_tests.rs` checks math-glue quantity boundaries, native arithmetic and
formatting, scoped bindings, and checkpoint replay;
`parameter_tests.rs` covers initialization parameters and newline rendering across
messages, writes, groups, snapshots, and native comparisons;
`numeric.rs` supplies scaled arithmetic/glue and `numeric_tests.rs` checks it;
`dimension_scan.rs` scans dimensions/glue, `quantities.rs` shares internal-value
formatting, `layout_values.rs` owns typed COW bindings, and `layout_assign.rs`
applies typed assignments/arithmetic, with operand and fixture tests in
`dimension_tests.rs`;
`nodes.rs` owns versioned chunked node storage and consuming list/node transfers;
`node_boxes.rs` owns its dense reusable COW box payload slots,
with branch/reclamation tests in `nodes_tests.rs`;
`node_insertions.rs` owns compact COW insertion payload slots and
`node_insertion_tests.rs` covers their copy, failure, and reclamation behavior;
`node_transfer.rs` owns consuming header/list transfers; `node_copy.rs` owns iterative
semantic list copies with ownership/failure tests in `node_copy_tests.rs`; page construction uses
the same nodes;
`tfm.rs` owns validated immutable metric resources and exact scaling, `fonts.rs`
owns restorable loaded-font descriptors/parameters, font assignments, and scoped
immutable metric readers,
`boxes.rs` owns horizontal/internal vertical construction frames, affine box registers,
scoped transfers, and box geometry/status primitives; `boxes_tests.rs` checks
native ordering, ownership, replay, failure cleanup, and executable packing;
`insertions.rs` captures insertion/adjustment lists through those construction frames;
`insertion_tests.rs` checks native page/packing behavior, failure boundaries, and replay;
`box_transfer.rs` owns unboxing, last-box extraction, and axis-specific shifts;
`vertical_box_tests.rs` covers their native fixtures, internal paragraphs, and replay;
`text.rs` owns external/restricted modes and horizontal/vertical lists, `paragraph.rs` owns
paragraph parameters, pooled shape roots, and reusable search storage,
`hyphenation.rs` owns shared pattern/exception roots and bounded lookup;
`hyphenation_scan.rs` reads expanded pattern/exception assignments, with native,
lookup, ownership, and replay coverage in `hyphenation_tests.rs` and
`hyphenation_oracle_tests.rs`; `hyphenation_render.rs` finds eligible words at
visited glue and synchronizes discretionary branches, while
`hyphenation_reconstitute.rs` rebuilds bounded cut prefixes with reusable ligature
scratch. `hyphenation_render_tests.rs` checks failure cleanup, version ownership
and replay, and `hyphenation_render_oracle_tests.rs` compares controlled and real
English paragraph lists; `language.rs` captures paragraph language/minima
and emits zero-width language nodes, with timing/copy tests in `language_tests.rs`;
`line_break.rs` traverses legal breaks and passes, `line_search.rs` implements
active/delta/passive search, `line_construct.rs` transfers selected ranges into
lines, and `packing.rs` packs horizontal/vertical lists and appends baseline glue;
`packing_tests.rs` checks packing specifications against fixed geometry and native boxes;
`output_state_tests.rs` covers scoped output-token storage;
`output_routine_tests.rs` checks direct shipout, output groups, native fixtures,
returned material, counters, and checkpoint replay;
`page_insertions.rs` accounts unsplit insertion classes and transfers selected child lists;
`page.rs` owns incremental
page state, default/custom output, and its suspended frame; `vertical.rs` executes
vertical contributions and page quantities, `page_output.rs` lowers owned nodes
with reusable traversal scratch, and `output.rs` retains detached page/box/glyph/
rule records in immutable prefixes; `output_tests.rs` checks positioned run
iteration, aggregate capacity, and nested movement boundaries. `page_tests.rs` covers page/replay/resource
ownership; `page_oracle_tests.rs` compares native boxes and placements with the
small test-only DVI reader in `page_dvi_tests.rs`; `page_offset_tests.rs` checks
signed offsets, scoped replay, and huge-page rejection. Scaled font headers use safe
once-initialized chunk handles in `tfm.rs`, retaining the sealed resource set;
`paragraph_tests.rs` covers ownership/replay and `paragraph_oracle_tests.rs`
compares packed structures with native INITEX; `math_boundary_tests.rs` and
`math_boundary_oracle_tests.rs` cover signed math boundaries, paragraph breaks,
hyphenation suppression, scoped math-surround state, and detached output.
`font_state_tests.rs` covers frozen
font identifiers, family bindings, and scoped replay against native TeX. `ligatures.rs`
builds TFM ligatures/kerns with component ownership; `font_text_tests.rs` and
`tfm_tests.rs` use controlled metrics from `font_test_data.rs`;
`ligature_entry_tests.rs` checks prepared program entries, while the explicitly
invoked native adapter in `font_oracle_tests.rs` compares real and controlled TFMs;
`write.rs` executes bounded immediate diagnostic writes, with native, checkpoint,
and lifetime coverage in `write_tests.rs`;
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
`examples/layout.rs` inspects detached prose pages and times unchanged-source
checkpoint replay; `examples/layout_allocations.rs` separately counts system
allocation and retention phases. They share `examples/layout_support/mod.rs`.

`lexing/`, built only with the `lexing-prototype` feature, is the whole-file lexer
prototype from `docs/fast-lexing.md`: `scan.rs` splits lines and runs the shared
state machine, `simd.rs` builds AVX2 masks and writes runs, `hash.rs` holds the
name hashes, `line.rs` lexes fallback lines, and `tests.rs` compares every lexer
with `FileInput::read`. The engine does not use it. `examples/lexing.rs` checks
and times the lexers on source files.
