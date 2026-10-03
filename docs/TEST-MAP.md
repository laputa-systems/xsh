# Test Map

Callable aliases: `tests/xsh/callable-aliases.xsh` covers defaults, named and
spread arguments, alias chains, captured values, qualified exports, dynamic
module contracts, and erased/effect rejection boundaries.
`callable_alias_signatures_agree_in_full_and_compact_facts` pins authoritative
signature facts and annotation preservation. The xsht `callable_alias`
integration filter covers exact forwarder fixes, refusal, and convergence.
Run the native module, the focused checker/tooling filters, then ordinary
semantic and tooling integration suites.

Choose the narrowest useful command first, then run the broader gate for the
area touched. Agents do not run formatters or linters; leave gates that invoke
them to the owner and report that limit. Unfiltered `cargo test` includes
`runtime::coverage` cases and two `runtime::examples` cases that launch
`xsht fmt` or `xsht lint`, so agents use the filtered runtime gate below.

## Read-only lint performance gate

`make check` delegates to `cargo dev check lint`, which runs
`cargo test --release -p xsht --test integration lint_performance::
-- --test-threads=1 --nocapture`. The repository case invokes the release
`xsht lint` from the repository root with ordinary configured discovery,
requires a successful exit with no diagnostics, and enforces a 15-second wall
deadline. Debug builds do not measure it: the case is ignored under
`debug_assertions`.
Compilation finishes before timing begins; discovery, checking, linting, and
process startup count toward the deadline. It never supplies `--fix`.
The harness kills and reaps a timed-out child and captures both output streams
to files. Isolated cases verify imported diagnostics, configured fixture
exclusions, unchanged source bytes, and deadline cleanup.

The frontend worker stack boundary is covered by `cargo test -p xsht --test
integration cli_workers_check_and_lint_nested_schema -- --nocapture`. The
subprocess fixture checks and lints nested named schema constructors with
`RUST_MIN_STACK` absent, and verifies that both commands leave the source intact.

## Lint and format invariance

`cargo test -p xsht --test integration lint_format_invariance::` copies each
corpus to a temporary directory and requires `xsht lint` to report the same
diagnostics, keyed by file, code, and the spelling-independent class of the
anchored token in source order, before and after `xsht fmt`. It covers the
repository corpus, layout perturbations of it (redundant operand parentheses,
operator line breaks, doubled blank lines, joined bracketed lines; only edits
the formatter erases are kept), the pre-format corpus from `git archive
b1f984c8^` when history is present, and `../packages` (or
`XSH_PACKAGE_CORPUS`) when present.
`lint_fix_commutes_with_formatting_on_corpus_files` checks that `fmt` then
`lint --fix` and the reverse order agree up to blank lines. The cases launch
`xsht fmt` and `xsht lint --fix` only inside temporary copies; run them with
`--release` for a faster turnaround.

## IR coverage report tool

`target/debug/xsht check tools/xsh-ir-coverage.xsh` checks the maintained scanner.
`target/debug/xsht test --jobs 1 tests/xsh/ir-coverage-tool.xsh` checks its CLI,
concrete JSON report, source counts, fallback groups, and invalid-root errors.
The exact Rust test `runtime::coverage::ir_coverage_scans_multiline_top_level_regions_once`
retains multiline region coverage without invoking formatters or linters.

## Nullable lookup APIs

`target/debug/xsht test --jobs 1 tests/xsh/absence-lookups.xsh` covers byte
offsets, zero hits, invalid starts, nullable bytes, present-null collection
entries, removed overloads, eager snapshots, lazy fallback, and integer fast
paths. Run the native Str/Bytes/List/Map modules, checker integration tests,
indexed verifier tests, and `cargo test -p xsht --test integration absence_lookup`
for migration proof, refusal, rechecking, and convergence.
`target/debug/xsht test --jobs 1 tests/xsh/constant-key-projections.xsh`
covers visible record/module key selection, nullable values, keyword labels,
optional exports, and receiver evaluation. `cargo test -p xsh --test integration
sema::constant_key_projection` checks full/compact fact parity; `cargo test
-p xsht --test integration constant_key_projection` covers identity-only schema
require fixes and conversion/dynamic/no-fix boundaries.

## Boolean guards

`target/debug/xsht test --jobs 1 tests/xsh/boolean-guards.xsh` covers once-only
conditions, Status, success/failure refinements, rejected fallthrough and
parameters, mutation invalidation, lexical loop targets, and cleanup before
return. Run the syntax and checker gates, indexed runtime verifier tests, and
`cargo test -p xsht --test integration lint::` for the checked negative-if rewrite.

## Explicit native test declarations

`target/debug/xsht test --jobs 1 tests/xsh/test-declarations.xsh` covers checked
registration, immutable context headers, effects, and ordinary-script behavior.
`cargo test -p xsht --test integration native_test_declaration -- --test-threads=1`
covers discovery, stable file/name IDs, isolation, legacy migration diagnostics,
and rejection at the CLI boundary. Test bodies use `test NAME { ... }`, or
`test NAME [effects] { |ctx| ... }` for context helpers. Maintain exact old names
when migrating existing harness entrypoints; new declarations need no prefix.

## Bare lexical blocks

Bare lexical block consumption, scope cleanup, and lexical transfers are covered
by `target/debug/xsht test --jobs 1 tests/xsh/lexical-blocks.xsh`. The grammar and
formatting distinction between blocks and literals is covered by
`cargo test --test integration syntax::parser_and_formatter_preserve_bare_block_literal_distinctions`.

The native lexical-block module also owns safe lint prefix edits, preserved
comments, refusal cases, normal rechecking, and second-pass convergence. It also
covers lexical block grep/refactor, the public API snippet, dynamic module
permissions, integer exits, and resource escape versus deferred invalidation.

## List patterns

`target/debug/xsht test --jobs 2 tests/xsh/list-pattern.xsh` covers exact and
prefix lengths, nested captures, dynamic narrowing, rejected subjects, rest
value semantics, guards, and conservative exhaustiveness. The shared matcher
slot publication and invalid indexed rest boundary are covered by
`cargo test -p xsh --lib list_pattern -- --test-threads=1` and
`cargo test -p xsh --lib verifier_rejects_invalid_list_rest_patterns -- --test-threads=1`.
Focused tooling acceptance uses `cargo test -p xsht --test integration list_pattern`;
the core argument parsing migration uses
`target/debug/xsht test --jobs 2 core/tests/test-ip.xsh`.

## Pattern aliases and alternatives

`target/debug/xsht test --jobs 2 tests/xsh/pattern-aliases.xsh` covers nested
aliases, resolved capture types, alternative order, subject/guard evaluation,
conditional binding contexts, atomic capture publication, and syntax/type
rejections. Indexed slot boundaries use
`cargo test -p xsh --lib pattern_aliases -- --test-threads=1` and
`cargo test -p xsh --lib verifier_rejects_incompatible_alternative_and_alias_capture_slots -- --test-threads=1`.
Focused tooling acceptance uses
`cargo test -p xsht --test integration pattern_`; run the existing list and
pattern conditional native modules after changes to the shared matcher.
## Core assertions

`target/debug/xsht test --jobs 1 tests/xsh/assert.xsh` independently observes
subprocess status, stdout, and diagnostics for lazy context, operand evaluation,
short circuiting, bounded rendering, message propagation, retry capture, cleanup,
and rejected types/effects. Broader gates are the syntax/checker tests,
`cargo test -p xsh --lib runtime::eval::indexed::full::tests --features native-tests`,
and `cargo test -p xsht --test integration core_assert --features native-tests`.
`cargo build -p xsh --bin xsh --no-default-features` witnesses independence from
native-test support.

## Local Result capture

`target/debug/xsht test --jobs 1 tests/xsh/try-capture.xsh` covers nearest
propagation, nested Result data, nominal inference, Unit assertions, process
error identity, lexical exits, cleanup priority, producer suspension and
cancellation, and recursive calls through heap frames. Shared regressions also
use `tests/xsh/retry.xsh`, `tests/xsh/error-context-blocks.xsh`, and
`cargo test -p xsh --lib runtime::eval::indexed::full::tests --features native-tests`.
Focused tooling uses `cargo test -p xsht --test integration try_capture`.

## Lexical error contexts

`target/debug/xsht test --jobs 1 tests/xsh/error-context-blocks.xsh` covers
nested propagation, untouched error data, handled failures, label evaluation,
ordinary `ctx` names, value tails, deferred cleanup, and abort behavior.
Shared boundaries also use the capture/retry/deferred-block native modules,
`cargo test -p xsh --lib runtime::eval::indexed::full::tests`, and focused
formatter/lint/structural-tooling acceptance tests.

## Direct scalar iteration

`target/debug/xsht test --jobs 1 tests/xsh/scalar-iteration.xsh` covers Unicode
scalars, invalid UTF-8 bytes, snapshots, nested comprehensions, Result identity,
immutable bindings, source evaluation, suspension, and lexical cleanup.
`cargo test -p xsh --lib scalar_cursor` checks retained source representation
and incremental element construction. Tooling acceptance uses
`cargo test -p xsht --test integration scalar_iteration`; run the checker,
indexed verifier, collection, and deferred-block gates after cursor changes.

## Routine CI

`target/debug/xsht test --jobs 1 tests/xsh/stage-functions.xsh` owns statically
resolved unary stage calls, literal/aggregate defaults, named configuration,
qualified imports, Result data, effects, cleanup, short-circuiting, and exact
wrapper fixes. Related gates are native `stdlib/streams.xsh`, syntax and sema,
`cargo test -p xsht --test integration stage_callable_wrapper`, and
`cargo test -p xsh --lib runtime::eval::indexed::full::tests`.

`.github/workflows/lint.yml` and `.github/workflows/test.yml` run on pull requests
and pushes to `master` with read-only repository permission. Both run on
`ubuntu-24.04-arm` inside the image defined by `Dockerfile.test`, with the
`aarch64-unknown-linux-musl` target and `dev/targets.xsh::docker_test_env`
linker flags. Lint runs `cargo dev lint --fix` and fails on any resulting
`git diff`. Tests run `cargo dev internal test-linux-ci` with the `dev`
profile, including the privileged Linux features. The internal driver builds
all three products before testing and supplies the `target/debug` executable
paths used by native subprocess fixtures.

Locally, `make lint` delegates to `cargo dev lint --fix`, while `make test`
delegates to `dev/test_workflows.xsh::rust` (`cargo test` in the debug profile).
The manual release workflow retains the `dist` profile. Lint and its
formatter/autofix steps are owner-run unless the user explicitly requests them.

`Dockerfile.test` installs the reference utilities required by the current
system-report checker and its opt-in utility corroboration adapters. Alpine splits
`findmnt`, `lsblk`, and `lscpu` into separate packages, places `swapon` in
`util-linux-misc`, and places `ip` in
`iproute2-minimal`. The image also includes `kmod`, `pciutils`, `usbutils`,
`cpupower`, `lm-sensors`, and `dmidecode`. The image build checks the expected
executable paths. A manifest validation run only checks declared invocations;
live checks establish that the installed executables support their required
arguments.

The image uses Alpine edge packages for Clang 23 and `libclang`; the pinned
Rust nightly reports LLVM 23.1.1 and supplies `llvm-ar`, `llvm-strip`, and
`rust-lld` through `llvm-tools`. This replaces the separate prebuilt LLVM
tarballs in `Dockerfile.test`. Alpine edge packages move over time, so record
the built image digest and build date when comparing runs. An image build and
exact `xsh`, `xsht`, and `xshi` debug builds exercise the resulting C, linker,
and musl setup.

The release matrix in `.github/workflows/release.yml` matches the three
triples in `dev/targets.xsh::resolve`: x86_64 and aarch64 Linux musl, and
aarch64 Darwin. `dev/dist.xsh::native_dist` builds `xsh`, `xsht`, and `xshi`
for each target; `dev/release.xsh::validate_artifacts` requires all nine
binary artifacts and their checksum sidecars. The names and validation
boundary are covered by `dev/tests/test-targets.xsh`. Actual `dist` builds
and package smoke checks run in the manual release workflow.

`linux_priv_kill_all_signals_contained_new_session_process` reexecutes the exact
test under `unshare --pid --fork --mount-proc`. Its harness is namespace PID 1;
the matching proc mount prevents process-wide signals from targeting concurrent
tests or the Docker supervisor.

`tests/linux_priv.rs` is included only with `linux-priv-tests`. In the pinned
privileged image it runs under the CI `dev` profile with `net tools`; use
`--nocapture` to see capability or fixture skip reasons. Rust counts those
early-return cases as passed, so record whether a privileged case actually ran.
The Docker test driver passes `--init` so stopped jobs orphaned by an exiting
`xshi` are reaped by PID 1. Without an init process, a zombie can make
`runtime::interactive::forced_exit_terminates_a_stopped_job` report a live PID
after the shell terminated it.

## Common Gates

| Change | Narrow command | Broader gate |
|---|---|---|
| Error payload/cause ownership transfers and long shared resource reachability | `target/debug/xsht test --jobs 1 tests/xsh/error-resource-ownership.xsh` and `cargo test -p xsh --lib resource_reachable_values -j1 -- --test-threads=1` | Rebuild exact debug xsh/xsht binaries; `tests/xsh/stdlib/process.xsh` and `tests/xsh/typed-causes.xsh` |
| Typed causes, outer Result error inference, immutable aliases, context/process metadata, bounded diagnostics, and constructor frames | `target/debug/xsht test --jobs 1 tests/xsh/typed-causes.xsh` and `cargo test -p xsh --lib typed_cause -- --test-threads=1` | Syntax/checker gates, indexed verifier tests, `cargo test -p xsht --test integration typed_cause`; rebuild the exact debug xsh/xsht binaries before native tests |
| Rust compile only | `cargo build` | relevant filtered package tests; unfiltered `cargo test` is owner-run |
| Local empty collection and nullable inference, monomorphic aliases, static branch/loop contributions, and concrete indexed publication | `target/debug/xsht test --jobs 1 tests/xsh/local-inference.xsh` and `cargo test -p xsh --lib local_collection_inference_publishes_concrete_indexed_call_and_slot_types -- --test-threads=1` | Checker and syntax gates; shared constraint solver tests; `local_constraint_probe_does_not_copy_completed_body_binding_history` and `local_constraint_probe_does_not_copy_checked_expression_history`; `empty_map_fold_inference_publishes_concrete_accumulator_types`; focused annotation rewrite acceptance |
| Canonical builtin signature templates and collection call binding | `target/debug/xsht test --jobs 1 tests/xsh/builtin-templates.xsh` | Native collection and Map suites; checker, indexed verifier, registry signature, and API gates |
| Checked dynamic boundaries and removed check strict option | `target/debug/xsht test --jobs 1 tests/xsh/dynamic-boundaries.xsh` and `cargo test -p xsht --test integration check_dynamic_boundary` / `check_strict_option` | Checker full/compact facts, indexed verifier, native JSON/module/auth gates; checker and runners share `tests/fixtures/sema/invalid/unchecked-json-boundary.xsh`, and rejected annotation passes preserve source bytes |
| `Lexer::lex_compact`, `Parser::parse_source_arena_only`, or formatter | targeted `cargo test --test integration syntax::TEST_NAME` | `cargo test --test integration syntax::` |
| `Checker::check_compact_declarations` or lint | targeted `cargo test --test integration sema::TEST_NAME` for checker or `cargo test -p xsht --test integration lint::TEST_NAME` for lint | `cargo test --test integration sema::` for checker or `cargo test -p xsht --test integration` for lint |
| `Evaluator::prepare_compact_indexed_only`, `indexed_run`, or runtime behavior | targeted `cargo test --test integration runtime::TEST_NAME` | `cargo test --test integration runtime:: -- --skip runtime::coverage:: --skip runtime::examples::example_corpus_is_formatted --skip runtime::examples::example_corpus_lints_without_warnings --test-threads=1`; run relevant `runtime::coverage` tests by exact name only when they do not invoke formatters or linters |
| Computed-key Map literals and fresh initialization rewrites | `target/debug/xsht test --jobs 1 tests/xsh/stdlib/map.xsh` and `cargo test -p xsht --test integration map_literal` | Syntax/checker gates, full indexed verifier, registry/API tests; tooling acceptance covers comments, observations, checked conversions, grep/refactor, and rewrite convergence |
| List literal splicing and safe construction rewrites | `target/debug/xsht test --jobs 1 tests/xsh/collections.xsh` and `cargo test -p xsht --test integration list_splicing` | Syntax and checker gates plus indexed verifier and API registry tests; tooling acceptance tests check parse/check, no-fix boundaries, and formatter idempotence without invoking CLI formatting or linting |
| Stream yield delegation and chained cancellation | `xsht test --jobs 1 tests/xsh/yield-delegation.xsh` and `cargo test -p xsh --test integration small_stack_yield_delegation` | Native stream suite, indexed verifier tests, frontend-indexed producer lifecycle fixtures, syntax/checker gates, and xsht integration/API gates |
| Adjacent ordering chains and stable repeated operand rewrites | `target/debug/xsht test --jobs 1 tests/xsh/comparison-chain.xsh` and `cargo test -p xsht --test integration comparison_chain` | Syntax and checker gates plus `cargo test -p xsh --lib runtime::eval::indexed::full::tests --features native-tests`; bare assertion coverage requires checked statement classification |
| pattern-bound conditionals and loops, branch captures, value contexts, loop cleanup, safe lint fixes | `target/debug/xsht test --jobs 2 tests/xsh/pattern-conditionals.xsh` | native `basic.xsh`, `ergonomics.xsh`, `pattern-tests.xsh`; indexed verifier tests; xsht integration and API tests |
| non-binding pattern predicates, nominal resolution, Result inspection, narrowing, formatter and lint fixes | `target/debug/xsht test --jobs 1 tests/xsh/pattern-tests.xsh` | `cargo test -p xsh --lib runtime::eval::indexed::full::tests`; `cargo test -p xsht --test integration lint::`; `cargo test -p xsht --test api` |
| Result error fallback blocks, exact error binding, lazy success, lexical exits, retry and cleanup | `target/debug/xsht test --jobs 1 tests/xsh/fallback-blocks.xsh` | Syntax and checker gates; `cargo test -p xsht --test integration lint::`; `cargo test -p xsht --test api api_core_fallback`; `cargo test -p xsh --lib runtime::eval::indexed::full::tests` |
| Nested functional record updates, snapshots, typed replacements, failure propagation, and safe nested spread fixes | `target/debug/xsht test --jobs 1 tests/xsh/record-update.xsh`, `cargo test -p xsh --test integration nested_record_update`, and `cargo test -p xsht --test integration nested_record_update` | Syntax/checker gates, `cargo test -p xsh --lib record_update`, `cargo test -p xsht --test integration static_record_update`, and native collection/storage gates |

| Static named argument spreading, finite visible fields, occupancy, defaults, source order, native operations, and safe forwarding fixes | `target/debug/xsht test --jobs 1 tests/xsh/named-argument-spreading.xsh` and `cargo test -p xsht --test integration named_argument_spread` | Syntax/checker gates, indexed verifier tests, API registry tests, and `target/debug/xsht test --jobs 1 core/tests/test-fd.xsh` |
| Lowering of checker-accepted call and statement forms: registered calls consuming published `CheckedApiCall` slots, `env.PATH` views, user-module named and defaulted calls, top-level `guard let`, Map parameter defaults | `target/debug/xsht test --jobs 1 tests/xsh/lowering-coverage.xsh` | Full native suite; `cargo test -p xsh --lib runtime::eval -- --test-threads=1`; indexed verifier tests |
| Constructor inference, nested constraints, declared receiver slots, expected unused arguments, constants, and structural compatibility | `target/debug/xsht test --jobs 1 tests/xsh/generic-constructors.xsh` | `cargo test -p xsh --lib inferred_record_constructors_keep_concrete_facts_after_frontend_drop` and `cargo test -p xsht --test integration generic_record_constructor` |
| Parameterized record schemas, concrete aliases, universal defaults, exact specialization rejection, imported private dependencies, and safe constructor fixes | `target/debug/xsht test --jobs 1 tests/xsh/parametric-records.xsh` and `cargo test -p xsht --test integration parametric_record_constructor` | `cargo test -p xsh --test integration parametric_record_separate_module`, syntax/checker gates and `target/debug/xsht test --jobs 1 tests/xsh/record-constructors.xsh` |
| Typed record constructors, lexical literal defaults, aliases, source order, validation boundaries, and safe constructor fixes | `target/debug/xsht test --jobs 1 tests/xsh/record-constructors.xsh` and `cargo test -p xsht --test integration record_constructor` | Syntax and checker gates, `target/debug/xsht test --jobs 1 tests/xsh/collections.xsh`, and `cargo test -p xsht --test api api_core_records_demonstrates_schema_owned_defaults_and_constructor_puns` |
| Explicit nominal enum declarations, singleton payloads, imported constructors, aliases, and legacy migration | `target/debug/xsht test --jobs 1 tests/xsh/enum-declarations.xsh` | Syntax and checker gates; `cargo test -p xsht --test integration enum_`; imported module native tests |
| Checked Duration arithmetic, dimensions, operand order, quantization, constant defaults, and timeout inputs | `target/debug/xsht test --jobs 1 tests/xsh/duration-arithmetic.xsh` | Syntax and checker gates; `cargo test -p xsht --test integration duration_arithmetic`; indexed verifier tests; native `tests/xsh/stdlib/time.xsh` and `tests/xsh/retry.xsh` |
| native XSH module behavior | `target/debug/xsht test --exact --jobs 1 PATH::TEST_NAME` | `target/debug/xsht test --jobs 1 tests/xsh/stdlib` |
| development command `--target` override reaches the context | `xsht test --exact --jobs 1 dev/tests/test-targets.xsh::test_dev_main_target_override_reaches_context` | `xsht test --jobs 1 dev/tests/test-targets.xsh`; the pinned amd64 image passed all 17 cases after the environment value was evaluated as the requested triple |
| `xshi` long-listing parity across filesystems | `cargo test -p xsh --test integration runtime::interactive::parity::scenarios::l_ -- --test-threads=1` | Run the filtered runtime integration gate in `xsh-test` with Docker `--init`. The comparison masks directory link counts and size-column padding, which vary with the filesystem, while retaining file link counts, file sizes, modes, dates, names, and symlink targets. |
| system-report relationships, USB descriptor ownership and bounds, USB controller and device-class parent source states, OS release source precedence and escaping, namespace, network device, and driver link failure states and v1 replay compatibility, source parsers, CPU list parsing, section projection, JSON conversion/redaction, terminal-safe rendering, mount usage, cgroup accounting, firmware issues, and offline CLI replay | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_class_parent_retains_independent_fallback_and_link_failure`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_controller_link_distinguishes_directories_disappearance_and_failures`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_driver_link_distinguishes_unbound_and_unreadable_devices`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_network_device_links_keep_absence_separate_from_failures`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_identity_preserves_namespace_link_failures` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report full text renders cache sharing and block-layer index lists | `target/x86_64-unknown-linux-musl/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_full_text_renders_numeric_relationship_lists` in the pinned amd64 image | `xsht test --jobs 1 tests/xsh/system-report.xsh` in that image |
| system-report typed policy membership and live collection effects | `xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_checker_keeps_cpu_policy_members_typed` and `xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_checker_rejects_live_collection_in_pure_code` | The checker must reject a CPU policy membership list as `Str` and a live collector call from a pure function; both focused native tests pass in the pinned amd64 image. |
| system-report complete identity source reads, local os-release fallback, and exact uptime parsing | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_identity_rejects_truncated_source_prefixes` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_uptime_parser_requires_complete_two_column_decimal` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report `lscpu` online CPU JSON rejects non-boolean states while preserving sparse IDs | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lscpu_reference_parser_keeps_sparse_online_ids` | `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh` |
| system-report malformed, missing, or invalidly spelled local os-release ID retains valid fields without vendor fallback | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_os_release_value_parser_rejects_malformed_assignments`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_identity_withholds_malformed_os_release_values_without_vendor_fallback`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_identity_marks_os_release_without_id_partial` | The pure ID grammar test passes on the available debug `xsht`; in the pinned ARM64 image, run the focused rooted tests and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate |
| system-report ARM device-tree model and compatible values require complete NUL-terminated strings, and heterogeneous CPU parts survive without DMI | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_device_tree_strings_require_complete_terminated_values` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_arm_identity_preserves_heterogeneous_cpus_without_dmi` | In the pinned ARM64 image, run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate |
| system-report DMI placeholder text remains raw observed identity | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_identity_retains_dmi_placeholder_text_as_raw_values` | In the pinned ARM64 image, run the focused test and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate; the current debug binary stops at collector module loading |
| system-report independent DMI firmware identity, sensitive serial and UUID observations, and DMI plus device-tree source selection | `target/debug/xsht test --jobs 1 dmi_identity_reference_scores`, `target/debug/xsht test --jobs 1 dmi_identity_rooted_reference`, and `target/debug/xsht test --jobs 1 device_tree_reference_requires_exact` | In the pinned ARM64 image, run both full native gates and `xsh dev system-report-check --compare-identity --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Stable DMI attributes or a device tree-only reference score `identity.firmware`; a hybrid host keeps `source=dmi` while its device tree strings are still checked. The local rooted DMI fixture passes, but the live collector needs a current binary. |
| system-report private raw DMI identity capture, collector replay, and default redaction | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmi_identity_capture_validates_saved_sources_and_oracle`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmi_identity_capture_keeps_unavailable_sources_unscoreable`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmi_identity_reference_scores_raw_fields_and_sensitive_observations`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmi_identity_capture_replays_production_collector` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-dmi-identity-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-dmi-identity-bundle NEW_DIRECTORY`. The private bundle retains bounded raw vendor, product, board, BIOS, serial, and UUID class attributes, explicit absence, digests, and an independent value oracle. Validation and pure default-redaction comparison pass locally; production replay needs the pinned rebuild. Exact replay requires a stable complete capture with an observed vendor or product and redacted serial and UUID values in default output. |
| system-report private raw device-tree capture and collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_device_tree_capture_validates_saved_sources_and_oracle`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_device_tree_capture_keeps_unavailable_sources_unscoreable`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_device_tree_capture_replays_production_collector` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-device-tree-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-device-tree-bundle NEW_DIRECTORY`. The bundle stores bounded raw model and ordered compatible strings with source states, digests, and an independent decoder oracle. Validation passes locally; production replay needs the pinned rebuild. A compatible-only tree is scoreable; incomplete or malformed bytes are not. |
| system-report process-visible namespace and cgroup scope | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_scope_keeps_all_process_visible_namespace_identities` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_namespace_reference_requires_observed_exact_targets` | In the pinned ARM64 image, run the focused native test, `xsh dev system-report-check --compare-namespaces --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`, and the full native report gate; the old debug binary cannot load the updated collector |
| system-report PCI functions retain separate identities, optional PCIe link absence, unknown NUMA, malformed decimal widths, truncated link-speed prefixes, and failed relationship-link issues | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_pci_multifunction_keeps_optional_link_sources_distinct` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report-collect.xsh::test_system_report_pci_decimal_attribute_rejects_nondecimal_and_inexact_values` | In the pinned ARM64 image, run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate; the old debug binary lacks `fs.root_readlink_result` and cannot load the updated collector |
| system-report PCI collection reports a non-UTF-8 sysfs name and retains a valid neighboring function | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_pci_collection_reports_non_utf8_names_without_losing_valid_functions` | Run the rooted fixture and full report suite in the pinned ARM64 image; the local debug binary cannot load the report test module's newer network dump schema |
| system-report device-class names and attributes reject truncated reads | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_device_classes_reject_truncated_names_and_attributes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report sound and input classes remain complete when DRM is absent | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_device_classes_keep_sound_and_input_without_drm` | In the pinned ARM64 image, run the focused test followed by `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh`; the old debug binary cannot load the updated collector |
| system-report opt-in `lsusb` inventory, tree, and selected descriptor corroboration | `xsht test --jobs 1 dev/tests/test-system-report-lsusb-check.xsh` | In the pinned Linux image, run `xsh dev system-report-check --compare-lsusb --lsusb-bin /usr/bin/lsusb --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. The version-aware adapter brackets the sensitive USB report with stable `lsusb` device and tree views, compares each displayed child interface's upstream port and driver, and uses one bounded, independently selected `lsusb -v -s` capture for numeric descriptor fields. Saved-output tests pass; the live amd64 comparison matched seven devices, seven tree rows, and four selected descriptor fields. |
| system-report USB platform parents, repeated numeric IDs, and root hubs joined after children | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_controller_path_handles_pci_and_platform_roots` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_parent_join_handles_root_hubs_sorted_last` | In the pinned ARM64 image, run the focused tests and the full native report gate; the old debug binary stops at collector module loading |
| system-report independent USB device topology and stable bus, device, and speed values | `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_usb_topology_name_parser_keeps_root_hubs_and_sparse_ports`, `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_usb_topology_reference_scores_parent_links_and_stable_numbers`, and `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_usb_topology_rooted_reference_reads_devices_without_interfaces` | In the pinned ARM64 image, run the rooted reference test, the full checker gate, and `xsh dev system-report-check --compare-usb-topology --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Exact scoring requires stable device names, indexed parents, root hubs, port paths, bus and device numbers, and speeds. The available old debug binary cannot enumerate the rooted fixture directory. |
| system-report independent USB raw IDs, source labels, and failed identity reads | `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_usb_ids_reference_scores_raw_ids_and_observed_labels`, `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_usb_ids_rooted_reference_reads_fixed_width_values`, and `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_identity_read_failures_keep_field_issues` | In the pinned ARM64 image, run the rooted and collector tests, the full checker and report gates, and `xsh dev system-report-check --compare-usb-ids --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Exact scoring requires stable fixed-width raw IDs and source labels. The old local debug binary cannot enumerate the rooted fixture or load the current collector. |
| system-report unreadable USB power attributes remain field issues | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_power_read_failures_make_section_partial` | In the pinned ARM64 image, run the focused test and the full native report gate; the old debug binary stops at collector module loading |
| system-report truncated USB scalar sources cannot publish a complete-looking prefix | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_truncated_scalar_sources_do_not_publish_prefixes` | In the pinned ARM64 image, run the focused test and the full native report gate. The old local runner finds zero tests because it cannot check the current report schema, so local discovery is not verification. |
| system-report independent USB power controls, signed autosuspend delay, runtime status, and configuration | `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_usb_power_number_reference_keeps_signed_autosuspend_delay`, `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_usb_power_reference_scores_controls_and_brackets_runtime_state`, `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_usb_power_rooted_reference_reads_runtime_and_configuration`, and `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_runtime_status_replays_legacy_absence` | In the pinned ARM64 image, run the rooted reference and collector tests, both full native suites, and `xsh dev system-report-check --compare-usb-power --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Stable fields score exactly; changing runtime status or incomplete reads remain partial. The old local binary cannot enumerate the rooted fixture or load the current collector. |
| system-report independent USB active interfaces, driver links, and available descriptor settings | `target/debug/xsht test --jobs 1 usb_interface` covers descriptor ownership, comparison, and rooted reads | In the pinned ARM64 image, run the rooted case, the full checker and report gates, and `xsh dev system-report-check --compare-usb-interfaces --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Stable bindings, active attributes, and descriptor settings score exactly; incomplete or changed reads remain partial. The old local binary cannot enumerate the rooted fixture. |
| system-report CPU enumeration integrity, absent CPU zero, rooted symlink cycles and escapes, and truncated cgroup CPU sets | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_collection_does_not_invent_absent_cpu_zero`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_enumeration_requires_a_valid_present_list`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_present_symlinks_cannot_cycle_or_escape_the_source_root`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_effective_cpuset_rejects_a_truncated_source` | In the pinned ARM64 image, run the focused cases and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate; the local debug binary rejects the newer network dump schema while loading this module |
| system-report 128 present CPUs and absent CPUFreq/idle capability | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_collection_preserves_128_present_ids` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_absent_cpufreq_unavailable` | In the pinned ARM64 image, run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate |
| system-report CPUFreq policy membership, configured bounds, bracketed current gauges, EPP, and boost controls | `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpufreq_reference_scores_policy_membership_and_bounds`, `target/debug/xsht test --jobs 1 cpufreq_boost_reference_preserves_scope`, `target/debug/xsht test --jobs 1 cpufreq_rooted_reference_reads_every_policy`, `target/debug/xsht test --jobs 1 cpufreq_members_accept_kernel_space_separated_ids`, and `target/debug/xsht test --jobs 1 cpufreq_scaling_current_replays_legacy_requested_name` | In the pinned ARM64 image, run `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh`, the full checker gate, and `xsh dev system-report-check --compare-cpufreq --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Policy membership and exposed stable EPP/boost controls can score separately. `cpu.freq-bounds` scores only when configured bounds and current gauges are complete and stable around collection; changing or unreadable gauges remain partial. |
| system-report CPUIdle state identity, legacy replay, read only governor fallback, and independent bracketed counters | `target/debug/xsht test --jobs 1 idle_state_index_requires_canonical`, `target/debug/xsht test --jobs 1 cpuidle_reference_scores`, `target/debug/xsht test --jobs 1 cpuidle_rooted_reference_reads_each_present_cpu_state`, `target/debug/xsht test --jobs 1 idle_governor_uses_read_only_source`, and `target/debug/xsht test --jobs 1 v1_replay_keeps_unrecorded_idle_state_index_unknown` | In the pinned ARM64 image, run the focused rooted tests, the full report and checker suites, and `xsh dev system-report-check --compare-cpuidle --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Exact scoring requires stable indexed state identities and metadata with each exported usage/time counter inside its before/after bracket. |
| system-report opt-in `cpupower` frequency and idle corroboration | `xsht test --jobs 1 dev/tests/test-system-report-cpupower-check.xsh` | In the pinned Linux image, run `xsh dev system-report-check --compare-cpupower --cpupower-bin /usr/bin/cpupower --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. The version-aware parser checks only explicit CPU 0 `frequency-info --driver`, `--hwlimits`, and `idle-info` forms; it compares stable driver, hardware bounds, global idle identity, and state names at their indexes. The saved-output and synthetic process tests pass, and the live amd64 comparison matched all six fields. |
| system-report raw capture excludes simultaneous live comparison | `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_capture_rejects_simultaneous_live_comparison` | The CLI rejects CPU, pressure, uptime, DMI identity, thermal, SMBIOS, and mountinfo bundle capture plus a live comparison before creating the destination bundle. The process test runs against the available debug binary. |
| system-report CPU topology relationships against util-linux logical IDs | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lscpu_topology_scores_relationships_across_id_spaces` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_coverage_manifest_contract` | In the pinned ARM64 image, run `xsh dev system-report-check --compare-cpu-topology --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` and the full report suite. The live checker brackets one CPU report with `lscpu --json --extended=CPU,ONLINE,SOCKET,CORE,NODE --all`, then scores exact CPU identity, normalized package/core groups, sibling membership, and NUMA nodes only when all reference columns are present. |
| system-report CPU topology rejects truncated and inexact package, die, core, and sibling sources | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_topology_rejects_truncated_scalar_prefixes` | Run the focused test and the full native report gate in the pinned ARM64 image; the old local runner finds zero report tests before execution. |
| system-report shared cache identity remains one record across distinct NUMA node assignments | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset` | In the pinned ARM64 image, run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate |
| system-report CPU model enrichment rejects truncated cpuinfo prefixes | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpuinfo_rejects_a_truncated_complete_looking_prefix` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report CPU frequency policy source states, unreadable optional values, exact numeric bounds, and shared boost reads | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpufreq_policy_rejects_truncated_field_prefixes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report CPU idle controls, counters, unreadable optional values, labels, and affinity source states | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_idle_and_affinity_reject_truncated_prefixes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report CPU directory failure and disappearance issues | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_directory_failures_keep_source_issues` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report CPU cache size scaling, overflow, truncated level and type, and bounded line size and set counts | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_bounded_size_bytes_checks_scaled_json_range`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_cache_sizes_reject_scaled_overflow_and_truncation` | Run the focused tests and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate in the pinned ARM64 image; the old local runner finds zero report tests. |
| system-report bounded source reader withholds decoded text from incomplete reads | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report-collect.xsh::test_system_report_bounded_text_reader_withholds_truncated_prefix` | In the pinned ARM64 image, run the focused test and full `target/debug/xsht test --jobs 1 tests/xsh/system-report-collect.xsh` gate. The old runner loads this test but fails before execution at `full_ir_function_blocker`. |
| system-report cache sharing requires an unambiguous CPU list | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report-collect.xsh::test_system_report_cache_shared_cpu_list_rejects_ambiguous_membership` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_cache_rejects_ambiguous_shared_cpu_list` | In the pinned ARM64 image, run the focused rooted test and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate. The local stale binary cannot load the collector's newer rooted-readlink API, so its zero-test result is not evidence that the rooted regression passed. |
| system-report independent cacheinfo sharing comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cache_reference_scores_unique_instances_and_cpu_links`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cache_reference_parses_sizes_without_candidate_rules`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cache_rooted_reference_reads_shared_instance_sources` | In the pinned ARM64 image, run the rooted reference test, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_cache_keeps_distinct_kernel_ids_with_same_sharing`, and `xsh dev system-report-check --compare-cpu-cache --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. The current debug binary's `fs.root_children` failure prevents the rooted reference and live comparison from reaching candidate collection. Exact live scoring requires the kernel cache instance `id` source for every entry. |
| system-report block relationship enumeration and class-entry link failures | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_keeps_holder_and_slave_enumeration_failures` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report layered block identities retain reciprocal holder and slave indexes | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_links_layered_block_devices_by_identity` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_json_scores_devices_and_layering` | In the pinned ARM64 image, run both focused tests and the full native report gate; the old debug binary stops at collector module loading |
| system-report sparse partition numbers retain disk parent and exact byte sizes | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_keeps_sparse_partition_numbers_and_parent_links` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_json_preserves_sparse_partition_identity_and_parent_edges` | In the pinned ARM64 image, run both focused tests and the full native report gate; the amd64 musl report fixture and live `lsblk` comparison pass |

`lsblk` projects disk queue and removable/rotational values onto partition rows
even when the partition has no matching sysfs files. The report keeps those
partition fields absent. `compare_block_devices` checks the underlying disk and
the partition edge before accepting the projected reference values; a present
partition field still has to agree exactly. Linux partitions can also omit a
`slaves` directory; `collect_storage` treats that structural absence separately
from a directory read failure.
| system-report block identity, capacity, scheduler, unreadable queue attributes, model, firmware, and I/O stat layouts reject incomplete or unsafe source data | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_rejects_invalid_block_source_fields` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report block scheduler selection requires one unambiguous active choice | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_block_scheduler_requires_one_selected_choice` | In the pinned ARM64 image, run the rooted `test_system_report_storage_rejects_invalid_block_source_fields` and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate |
| system-report SMBIOS unknown record identity, type 16 short-form device count, and type 17 sentinel size bounds | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_smbios_unknown_type_keeps_record_identity`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_smbios_type16_reads_device_count_from_short_form`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_smbios_type16_reference_reads_short_form_device_count`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_smbios_sentinel_size_requires_complete_formatted_field` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` and `target/debug/xsht test --jobs 1 dev/tests/test-system-report-check.xsh` in the pinned ARM64 image |
| system-report mount inventory rejects a valid-looking truncated source prefix | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_mounts_reject_truncated_complete_looking_prefix` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report mount identities stay inside the exact JSON integer range | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_mount_rejects_json_unsafe_identity` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report mount usage skips shadowed targets and automount descendants | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_usage_skips_shadowed_and_automount_descendants` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report huge-page and NUMA directory failures | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_memory_directory_failures_keep_source_issues` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report global and NUMA huge-page size and count boundaries | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_huge_page_pools_reject_unsafe_sizes_and_partial_counts` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report independent global and NUMA huge-page pool reference | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_huge_page_reference_parses_complete_decimal_counters`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_huge_page_reference_scores_global_and_numa_pools`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_huge_page_reference_reads_visible_global_and_numa_sources` | In the pinned ARM64 image, `xsh dev system-report-check --compare-huge-pages --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive memory report with bounded rooted sysfs reads. Stable identities and counts must agree; changed counters remain unscored, and no exported pools are reference unavailable. The available old debug binary cannot enumerate the rooted fixture directory |
| system-report NUMA meminfo row identity, complete reads, and exact KiB conversion | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_numa_meminfo_requires_complete_rows_and_exact_bytes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report cgroup complete membership/mount sources, bounded values, and duplicate CPU or I/O counter identities | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cgroup_inventory_rejects_partial_membership_and_mounts`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cgroup_limits_keep_source_and_numeric_failures`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cgroup_cpu_and_io_counters_reject_partial_and_unsafe_values` | Run the focused cases and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate in the pinned ARM64 image |
| system-report independent cgroup2 limits, CPU and I/O counters, visible ancestors, and live differential scoring | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cgroup_v2_reference_parses_limits_and_named_counters`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cgroup_v2_rooted_reference_reads_all_visible_resource_families`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cgroup_v2_comparison_scores_stable_limits_and_bracketed_counters` | In the pinned ARM64 image, `xsh dev system-report-check --compare-cgroup-v2 --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets one sensitive memory report with bounded rooted cgroup2 reads. Stable limits must match exactly, monotonic counters must fall within the bracket, and changing gauges remain unscored. Device-only `io.stat` rows are valid and have no counter resources. |
| system-report hybrid cgroup v2 values with v1 limitation, including a hidden v1 mount | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cgroup_hybrid_keeps_v2_values_and_v1_limitation` | Run the focused test and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate in the pinned ARM64 image |
| system-report cgroup membership preserves pathname colons, rejects duplicate unified rows, identifies legacy membership with hidden mounts, selects the applicable cgroup v2 mount, and withholds values from malformed mountinfo rows | `target/debug/xsht test --jobs 1 tests/xsh/system-report-collect.xsh`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_memory_preserves_colons_in_cgroup_membership_path`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_effective_cpuset_rejects_a_truncated_source`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cgroup_hybrid_keeps_v2_values_and_v1_limitation` | Both pure source tests pass on the available debug binary. Run the rooted fixtures and full report suite in the pinned ARM64 image; the old debug binary cannot load the current network schema in the report test module |
| system-report pressure stall rows, exact totals, duplicate kinds, and unavailable sources | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_psi_average_parser_rejects_invalid_percentages`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_pressure_keeps_complete_rows_and_unavailable_sources_distinct` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report empty pressure source retains a malformed issue beside valid rows | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_empty_pressure_file_is_malformed_beside_valid_memory_rows` | Run the focused test and full report suite in the pinned ARM64 image; the old debug binary cannot load the current network schema in this test module |
| system-report independent pressure stall reference and live comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_psi_reference_parses_complete_rows_and_brackets_counters` | In the pinned ARM64 image, `xsh dev system-report-check --compare-pressure --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets one sensitive memory report with bounded raw `/bin/cat` reads of exported CPU, memory, and I/O pressure sources. Cumulative totals must fall inside the reference interval; averages score only when both raw observations agree. No exported sources are reference unavailable. |
| system-report raw bundle replay rejects capture metadata drift | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_bundle_replay_rejects_changed_capture_metadata` | The local focused test rejects a changed capture origin even when source bytes are unchanged. Every raw replay path in `dev/system_report_check.xsh` rereads exact `capture.json` bytes after production collection; run their full collector replay fixtures in the pinned ARM64 image. The available debug binary cannot load the current collector, so the helper test alone is not a production replay pass. |
| system-report raw pressure-stall capture and memory collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_pressure_capture_validates_saved_sources_and_oracle`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_pressure_capture_keeps_incomplete_sources_unscoreable`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_pressure_capture_replays_production_collector` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-pressure-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-pressure-bundle NEW_DIRECTORY`. The private bundle records bounded raw CPU, memory, and I/O pressure files, absent resources, digests, and an independently parsed snapshot. Capture validation passes locally; collector replay needs the pinned rebuild. Live pressure totals are dynamic, so bundle replay verifies the saved snapshot instead of requiring two live reads to agree. |
| system-report independent process affinity and visible cgroup CPU scope comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_status_affinity_reference_requires_one_valid_cpu_list`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_scope_affinity_comparison_requires_stable_reference`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_scope_reference_selects_visible_cgroup_mount`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_scope_reference_parses_exact_quota_period`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_scope_rooted_reference_reads_current_and_visible_ancestors`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_scope_cgroup_comparison_scores_visible_limits`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_coverage_manifest_contract` | In the pinned ARM64 image, `xsh dev system-report-check --compare-cpu-scope --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets one full sensitive report with bounded `/bin/cat /proc/self/status`, rooted cgroup membership/mountinfo, and current/visible-ancestor `cpu.max` and `cpuset.cpus.effective` reads. It scores only when both current-group controller files are available and all stable values agree. A v1-only host has no eligible cgroup2 scope. |
| system-report raw cgroup v2 membership, visible ancestors, and resource replay | `target/debug/xsht test --jobs 1 test_system_report_cgroup_v2_bundle` | In the pinned Linux image, run `xsh dev system-report-check --capture-cgroup2-bundle NEW_DIRECTORY` then `--replay-cgroup2-bundle NEW_DIRECTORY`. The private bundle stores bounded `/proc/self/cgroup`, mountinfo, and all exported files for each visible cgroup v2 ancestor, with source absence, digests, and independently decoded resources. Changed gauges and counters are recorded separately from static drift. A missing membership source or cgroup v2 mount is recorded as unscoreable. This amd64 image captured one ancestor and 19 resources with three changing source files; saved production replay matched all 19. |
| system-report repeated proc status affinity field remains malformed | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_affinity_rejects_duplicate_status_field` | Run the focused regression and the full native report gate in the pinned ARM64 image; the local debug test runner has an older report schema. |
| system-report complete transparent huge-page policy selection and unknown values | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_thp_policy_parser_keeps_unknown_selected_value`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_transparent_huge_page_policy_preserves_unknown_selection` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report independent transparent huge-page policy parsing and stable live comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_thp_reference_requires_one_selected_policy_and_stable_fields` | In the pinned ARM64 image, `xsh dev system-report-check --compare-thp --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive memory report with bounded `cat` observations for the available `enabled` and `defrag` policy files; no exported policy remains reference unavailable |
| system-report raw vulnerability capture and CPU collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_vulnerability_capture_validates_saved_files_and_oracle`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_vulnerability_capture_replays_production_cpu_collector`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_vulnerability_capture_preserves_absent_class_without_scoring` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-vulnerabilities-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-vulnerabilities-bundle NEW_DIRECTORY`. The private bundle retains bounded named sysfs files, source states, digests, and an independent description oracle. A complete nonempty stable set is required for an exact replay. The old local runner fails rooted directory enumeration before the focused capture test reaches validation. |
| system-report complete CPU vulnerability descriptions and source failures | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_vulnerability_reference_requires_complete_stable_named_values` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_vulnerability_read_failures_keep_named_issues` | In the pinned ARM64 image, `xsh dev system-report-check --compare-vulnerabilities --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets one sensitive CPU report with bounded raw reads of every visible vulnerability file; absence leaves the assertion unscored |
| system-report meminfo exact JSON bounds after KiB conversion and on unscaled counters | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_memory_reports_malformed_and_oversized_meminfo_fields` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report meminfo tab separators and duplicate-field withholding | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_memory_accepts_tabbed_values_and_withholds_duplicate_fields` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report truncated meminfo read rejects complete-looking prefix rows | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_memory_does_not_parse_truncated_meminfo_prefix` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report swap header, rows, escaped names, duplicate identities, exact byte bounds, impossible use counts, and incomplete source reads | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_swap_devices_keep_exact_bytes_and_reject_partial_sources` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_swap_devices_reject_duplicate_identity_and_impossible_usage` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report raw procfs swap capture, oracle, and collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_swaps_raw_reference_decodes_units_paths_and_empty_inventory`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_swaps_capture_validates_saved_source_and_oracle`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_swaps_capture_marks_absent_malformed_and_truncated_unscoreable`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_swaps_capture_replays_production_collector` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-swaps-bundle NEW_DIRECTORY` then `xsh dev system-report-check --replay-swaps-bundle NEW_DIRECTORY`. Validation of source bytes, digest, metadata, and independent KiB-to-byte oracle passes locally; production replay needs the pinned rebuild. Mandatory live scoring separately brackets `swapon --raw --bytes`. |
| system-report empty swap inventories require a requested section and source evidence | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_swap_comparison_requires_source_evidence_for_empty_set` | The comparator rejects an exact empty-set result when the memory section was not requested, source evidence is missing, or a swap source issue is present. Live scoring and collector replay still require the pinned ARM64 gate. |
| system-report kernel module inventory rejects truncated source prefixes and duplicate identities, retains valid rows beside malformed rows, and preserves unavailable use counts | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_modules_reject_truncated_source_prefix`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_modules_keep_valid_rows_with_malformed_neighbor`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_modules_preserve_unavailable_use_count`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_modules_reject_duplicate_identity_with_valid_neighbor` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report kernel command line retains source whitespace and redacts the whole payload | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_source_text_preserves_exact_whitespace_when_requested` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_command_line_preserves_source_whitespace` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report fixed kernel parameter inventory retains values and absent sources | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_parameter_allowlist_keeps_values_and_absence` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report sysctl permission failure under a real unprivileged reader | `cargo test --offline -p xsh --test linux_priv --features linux-priv-tests --target aarch64-unknown-linux-musl system_report_sysctl_denial_as_unprivileged_reader_creates_no_child -- --exact --test-threads=1` | Run in the pinned ARM64 image as root so the Rust host fixture can drop the XSH child and `strace` to UID/GID 65534; it requires `kernel.pid_max` to remain null with an EACCES issue, a readable neighboring sysctl to remain observed, and no child process or secondary exec. Manifest validation and a fake Cargo runner check do not prove this privilege test passed |
| system-report PCI identity without a label database or helper | `cargo test --offline -p xsh --test linux_priv --features linux-priv-tests --target aarch64-unknown-linux-musl system_report_pci_keeps_numeric_ids_without_a_label_database_or_helper -- --exact --test-threads=1` | Run in the pinned ARM64 image; the Rust host fixture gives the XSH collector only numeric rooted PCI attributes, an unusable `PATH`, and a process/file trace, then checks exact IDs, no secondary process, and no label-database access |
| system-report nested sensor, thermal, and power-cap directory failures | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_nested_sensor_and_power_directories_keep_issues` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report raw sensor units, sparse indexed thermal trips, partial battery fields, power-cap limits, class control-type filtering, and zone parent links | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_sensor_and_power_sources_keep_raw_units_and_partial_attributes` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_thermal_trip_indexes_round_trip_and_legacy_unknown` | Run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate in the pinned ARM64 image; the old local runner cannot load the newer report schema |
| system-report independent thermal-zone and indexed-trip comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_thermal_reference_preserves_sparse_trip_indexes_and_brackets_temperature` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_thermal_rooted_reference_reads_indexed_sources` | In the pinned ARM64 image with thermal zones, `xsh dev system-report-check --compare-thermal --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets one sensitive sensor report with bounded thermal sysfs reads. Stable zone and trip identities and values score exactly; changing temperatures remain partial. An unrelated hwmon issue does not invalidate thermal agreement, while a thermal field issue does. The old local debug binary's rooted directory enumeration failure prevents this live gate. |
| system-report power-cap class enumeration failure | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_powercap_enumeration_failure_is_partial` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report independent powercap zone hierarchy, indexed constraints, and bounded energy counters | `target/debug/xsht test --jobs 1 powercap_reference_scores_nested` and `target/debug/xsht test --jobs 1 powercap_rooted_reference` | In the pinned Linux image, run the rooted case, both full native gates, and `xsh dev system-report-check --compare-powercap --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Stable zones and constraints score exactly; wrapped counters and incomplete reads remain partial. On this host, a privileged amd64 run matched both exposed powercap zones exactly. |
| system-report device-class entry identity, duplicate labels, and PCI/USB parent links | `target/debug/xsht test --jobs 1 device_class_reference_scores_duplicate_labels`, `target/debug/xsht test --jobs 1 device_class_rooted_reference`, and `target/debug/xsht test --jobs 1 device_class_entry_identity_survives` | In the pinned ARM64 image, run the rooted and collector cases, both full native gates, and `xsh dev system-report-check --compare-device-classes --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Stable class entries, labels, and indexed physical parents score exactly; changed or incomplete sources remain partial. The old local binary cannot enumerate the rooted fixture or load the current collector. |
| system-report independent hwmon chip identity, raw channel units and values, thresholds, alarms, and PCI/USB parents | `target/debug/xsht test --jobs 1 hwmon_reference_scores_duplicate_chip_names`, `target/debug/xsht test --jobs 1 hwmon_rooted_reference`, and `target/debug/xsht test --jobs 1 hwmon_identity_separates_duplicate_chip_names` | In the pinned ARM64 image, run the rooted and collector cases, both full native gates, and `xsh dev system-report-check --compare-hwmon --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Stable source values and indexed parents score exactly; changing readings and incomplete attributes remain partial. The old local binary cannot enumerate the rooted fixture or load the current collector. |
| system-report raw hwmon class links, source bytes, and saved channel replay | `target/debug/xsht test --jobs 1 test_system_report_hwmon_capture` | In the pinned Linux image, run `xsh dev system-report-check --capture-hwmon-bundle NEW_DIRECTORY`, then `--replay-hwmon-bundle NEW_DIRECTORY`. The private bundle preserves class and device links, up to 8192 allowlisted source paths with 4096 bytes each, source absence and digests, and an independent raw-channel interpretation; metadata cannot exceed its 16 MiB replay bound. The amd64 image captured three chips and ten channels with stable static sources and two changing gauges; saved replay matched all ten channels with zero field mismatches. Eight PCI parent assertions remained partial because the hwmon bundle does not include PCI enumeration. |
| system-report opt-in unconfigured `sensors -j` corroboration | `target/debug/xsht test --jobs 1 dev/tests/test-system-report-sensors-check.xsh` | In the pinned Linux image or on a physical host, run `xsh dev system-report-check --compare-sensors-json --sensors-bin ABSOLUTE_SENSORS --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. The adapter brackets one live sensor report with `sensors -j -c /dev/null`, compares uniquely mapped equal input readings after unit conversion, and reports version, timing, and output hashes. Differing live readings remain partial even if the surrounding samples happen to match; duplicate chip names and unsupported channels do not enter the mandatory raw hwmon score. |
| system-report independent SMBIOS record identity, formatted raw fields, string bytes, and sentinel bounds | `target/debug/xsht test --jobs 1 smbios_raw_reference_scores_records` and `target/debug/xsht test --jobs 1 smbios_rooted_reference_reads_only_exported_table` | In the pinned ARM64 image, run both focused cases, the full checker and report gates, and `xsh dev system-report-check --compare-smbios --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Stable complete tables score exactly; changed or malformed tables remain partial. `dmidecode` interpretation on captured tables remains a separate corroboration gate. |
| system-report dmidecode corroboration rejects capture metadata changes during utility execution | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmidecode_corroboration_rejects_changed_capture_origin` | The synthetic executable changes a valid capture origin after the version probe. Corroboration must reject the bundle before writing a comparison; the local focused test passes. |
| system-report opt-in dmidecode saved-dump corroboration | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmidecode_dump_relocates_smbios3_entry_point`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmidecode_dump_relocates_smbios2_entry_point`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmidecode_hex_output_corroborates_raw_records_and_strings`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmidecode_corroboration_records_opt_in_utility_provenance`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmidecode_version_failure_keeps_reference_provenance`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_dmidecode_cli_runs_only_on_explicit_captured_bundle` | On a pinned or physical host with an installed executable, first capture `--capture-smbios-bundle NEW_DIRECTORY`, then run `xsh dev system-report-check --corroborate-smbios-bundle NEW_DIRECTORY --dmidecode-bin ABSOLUTE_DMIDECODE`. The optional adapter saves full version, argv, locale, UID, timing, status, output, and digests in the private bundle and compares bounded hex fields and string bytes. The local process fixture uses a labeled synthetic tool; no installed dmidecode reference has run on this worker. |
| system-report independent power-supply names, raw units, signed current, optional fields, and changing gauges | `target/debug/xsht test --jobs 1 power_supply_reference_scores_units` and `target/debug/xsht test --jobs 1 power_supply_rooted_reference` | In the pinned ARM64 image, run the rooted case, the full checker and report gates, and `xsh dev system-report-check --compare-power-supplies --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`. Exact scoring requires stable supply attributes; changed or unreadable fields remain partial. The old local binary cannot enumerate the rooted fixture. |
| system-report raw power supply class and attribute replay | `target/debug/xsht test --jobs 1 test_system_report_power_supply_bundle` | In the pinned Linux image, run `xsh dev system-report-check --capture-power-supply-bundle NEW_DIRECTORY` then, when scoreable, `--replay-power-supply-bundle NEW_DIRECTORY`. The private bundle retains class symlinks, all eleven bounded exported attributes, explicit source absence, SHA-256 byte digests, changing gauges, and an independent source decoder. A supply needs an observed `type` to be scoreable. The synthetic battery replays exact signed current and other raw units through the production power collector. This amd64 image has no power supplies, so its live capture is correctly unscoreable. |
| system-report truncated power-cap zone and constraint names | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_powercap_rejects_truncated_names` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report truncated power-supply text attributes | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_power_supply_rejects_truncated_text_attributes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report indexed power-cap constraints, text/JSON round trip, and legacy v1 replay | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_powercap_constraints_round_trip_and_render`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_v1_replay_restores_legacy_powercap_constraint` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report source-aware bounded integer parsing | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_bounded_number_respects_source_state_and_json_range` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report proc stat decimal and exact JSON identity parsing | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_process_stat_parser_preserves_start_identity` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report process collection rejects truncated stat identity and derived-field prefixes | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_process_collection_rejects_truncated_stat_and_field_prefixes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report process page scaling and private-source omission | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_process_collection_scales_pages_and_omits_private_sources` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_forbidden_process_read_trace_normalizes_source_paths` | In the pinned ARM64 image, run the rooted fixture, full report gate, and `xsh dev system-report-check --no-subprocess --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`; the pure trace check alone cannot establish a live collector trace |
| system-report statm byte overflow cannot fall back to an unrelated stat value | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_process_statm_overflow_does_not_publish_stat_fallback` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report statm requires the complete seven-field kernel row | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_statm_reference_uses_reported_page_size_and_exact_bytes` | In the pinned ARM64 image, run `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_process_statm_requires_all_kernel_fields` and the full report suite |
| system-report process cgroup requires one absolute unified path | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_cgroup_reference_requires_one_absolute_v2_path` | In the pinned ARM64 image, run `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_process_cgroup_requires_one_absolute_v2_path` and the full report suite |
| system-report real UID accepts tabbed status and rejects duplicate or incomplete rows | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_status_uid_reference_requires_one_numeric_row` | In the pinned ARM64 image, run `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_process_uid_requires_one_complete_numeric_status_row` and the full report suite |
| system-report resource comparison counts only stable per-process fields | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_process_resource_reference_scores_only_stable_fields` | In the pinned ARM64 image, run `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_process_resource_snapshot_requires_complete_per_pid_sources` and `xsh dev system-report-check --compare-processes --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` |
| system-report independent process identity reference parsing and PID reuse comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_stat_reference_preserves_identity_and_rejects_unsafe_fields`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_status_uid_reference_requires_one_numeric_row`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_process_identity_reference_excludes_pid_reuse_and_scores_stable_values` | In the pinned ARM64 image, run `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_process_identity_snapshot_keeps_complete_stable_sources` followed by `xsh dev system-report-check --compare-processes --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`; the available old debug binary cannot enumerate rooted `/proc` correctly |
| system-report raw procfs process identity and resource replay | `target/debug/xsht test --jobs 1 test_system_report_process_bundle` | In the pinned Linux image, run `xsh dev system-report-check --capture-process-bundle NEW_DIRECTORY` then `--replay-process-bundle NEW_DIRECTORY`. The private bundle stores bounded `stat`, `statm`, `status`, and `cgroup` bytes for PIDs whose start identity and thread count survive capture, plus digests, the captured page size, and one source state or race reason per skipped PID. Replay validates exact saved PID membership and raw bytes before the production process collector and independent reference comparisons. Absent `/proc` is unscoreable. This amd64 image captured two PIDs and replayed both identities and eight resource fields exactly. |
| system-report process exits and arrivals during the reference bracket | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_process_reference_excludes_exits_and_new_arrivals_from_static_scoring` | The pure bracket comparison scores only identities present and unchanged in both observations; the mapped rooted identity and resource snapshot tests still require the pinned ARM64 image because the old debug binary cannot enumerate rooted `/proc` correctly |
| system-report malformed sensor inputs and power-cap range failures | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_sensor_units_and_powercap_ranges_are_bounded` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report truncated hwmon and thermal names, thermal and battery numeric sources | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_thermal_and_battery_reads_reject_truncated_prefixes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report executable fixture mapping and exact one-test scoring | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_coverage_manifest_contract`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_fixture_definition_check_ignores_comments_and_partial_names`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_fixture_failure_shows_cargo_diagnostic` | Manifest schema v4 records `reference_commands` as complete argv vectors and keeps the macOS replay case separate from Linux fixtures. Empty command lists identify the explicitly validated rooted sysfs/procfs adapters; command adapters declare complete argv vectors. Validation requires each mapped test to have a named procedure or attributed Rust test in its owner source; exact one-test output remains the runtime evidence, and a failed Cargo invocation reports its diagnostic. The pinned amd64 run passed all 95 Linux cases. In the normal pinned ARM64 image, run `xsh dev system-report-check --run-fixtures --xsh-bin ABSOLUTE_XSH --xsht-bin ABSOLUTE_XSHT --cargo-bin ABSOLUTE_CARGO`. On macOS, run `xsh dev system-report-check --run-macos-fixtures --xsh-bin ABSOLUTE_XSH --xsht-bin ABSOLUTE_XSHT`; each gate reports the other platform's cases as unexercised |
| system-report traced live, minimal-PATH, malformed-replay, and offline-replay effects | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_traced_live_and_replay_paths_keep_host_effect_contract` | In the pinned ARM64 image with `strace`, the mapped test calls the production syscall audit for live, saved-report, and failure paths. The fixture runner executes a shared test once and reuses its result for each mapped scenario. The old local `xsh` cannot load the current collector, so this focused test fails before any live trace contract can pass |
| system-report syscall trace parser ignores syscall-like text inside file paths | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_trace_ignores_syscall_names_inside_file_paths` | The pure checker identifies the syscall before parsing arguments and checks actual open flags; the pinned production trace is still required for an effects result |
| system-report raw CPU-set capture and collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_set_capture_validates_saved_reference`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_set_capture_rejects_observed_error_metadata`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_set_capture_records_missing_source`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_set_capture_replays_raw_sources` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-cpu-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-cpu-bundle NEW_DIRECTORY`; the version-2 metadata binds each raw file to a SHA-256 digest, rejects contradictory observed/error states, and revalidates after collection. The directory is created with mode `0700` outside the tracked tree |
| system-report raw CPUFreq policy capture and collector replay | `xsht test --jobs 1 test_system_report_cpufreq_capture` | Run `xsh dev/main.xsh system-report-check --capture-cpufreq-bundle NEW_DIRECTORY` then `xsh dev/main.xsh system-report-check --replay-cpufreq-bundle NEW_DIRECTORY` in the pinned Linux image. The private bundle binds CPU sets and policy files to byte digests and an independently parsed policy reference. Static policy sources must remain stable across capture; changing current-frequency gauges are recorded separately and the saved snapshot is replayed through the production CPU collector. This host exposed 32 policies; seven gauges changed during capture, while saved CPU sets, policy membership, bounds, and controls replayed exactly. |
| system-report raw CPU topology capture and collector replay | `xsht test --jobs 1 test_system_report_cpu_topology_capture` | Run `xsh dev/main.xsh system-report-check --capture-cpu-topology-bundle NEW_DIRECTORY` then `xsh dev/main.xsh system-report-check --replay-cpu-topology-bundle NEW_DIRECTORY` in the pinned Linux image. The private bundle preserves CPU identity sets, bounded per CPU topology values, and checked NUMA links. It validates source bytes and independent topology references before and after production CPU collection. This host's 32 CPUs and 32 NUMA links captured with `stable=true` and replayed exactly. |
| system-report raw meminfo and THP capture with independent collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_memory_capture_validates_raw_sources_and_oracles`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_memory_capture_rejects_observed_error_metadata`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_memory_capture_replays_raw_sources` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-memory-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-memory-bundle NEW_DIRECTORY`; the bundle retains bounded `/proc/meminfo` and available THP policy files with source states, digests, origin, and a separately parsed oracle. Observed/error contradictions and changes during replay are rejected. A changing live meminfo file is recorded but its saved raw snapshot remains replayable |
| system-report raw os-release capture with local precedence and identity collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_os_release_capture_validates_selected_raw_source`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_os_release_capture_rejects_observed_error_metadata`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_os_release_capture_ignores_unselected_vendor_read_failure`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_os_release_capture_does_not_fallback_from_malformed_local_source`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_os_release_capture_replays_product_identity` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-os-release-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-os-release-bundle NEW_DIRECTORY`. The private bundle retains both bounded source reads, absence/error states, digests, origin, the selected path, and an independently parsed ID/version oracle. Replay rejects contradictory observed/error metadata, verifies saved bytes again after collector execution, and compares the production identity; an unselected vendor read failure does not invalidate a complete local source, and a malformed local source never falls back to vendor data. Capture validation passes locally; the available old runner cannot load the current collector, so product replay needs the pinned rebuild. |
| system-report raw thermal-zone capture and collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_thermal_capture_replays_raw_zone_and_rejects_tampering` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_thermal_capture_preserves_absent_class_without_scoring` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-thermal-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-thermal-bundle NEW_DIRECTORY`. The private bundle retains bounded zone and indexed trip bytes, source states, digests, origin, and an independent reference; replay records changing live gauges without excluding complete saved snapshots, rejects conflicting observed-source error metadata, and revalidates after production collection. The available old debug binary cannot enumerate the temporary rooted thermal directory, so replay needs the pinned rebuild. |
| system-report raw powercap capture and collector replay | `xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_powercap_capture_replays_nested_zones_and_rejects_tampering` | Run `xsh dev/main.xsh system-report-check --capture-powercap-bundle NEW_DIRECTORY` then `xsh dev/main.xsh system-report-check --replay-powercap-bundle NEW_DIRECTORY` in the pinned privileged Linux image. The private bundle preserves class links, nested zone identities, bounded raw values, absence, byte digests, and an independent reference. Replay validates the sources before and after production power collection. On this amd64 host, both captured zones replayed exactly. |
| system-report raw PCI capture and collector replay | `xsht test --jobs 1 test_system_report_pci_capture` | Run `xsh dev/main.xsh system-report-check --capture-pci-bundle NEW_DIRECTORY` then `xsh dev/main.xsh system-report-check --replay-pci-bundle NEW_DIRECTORY` in the pinned Linux image. The private bundle preserves bus entry and binding symlinks, bounded identity and PCIe link attributes, source absence, byte digests, and an independent numeric reference. Replay validates sources before and after production PCI collection; unexposed PCIe link fields are reported unavailable. This host's 33 functions captured with `stable=true` and replayed exactly for identity, bindings, and links. |
| system-report raw USB capture and collector replay | `xsht test --jobs 1 test_system_report_usb_capture` | Run `xsh dev/main.xsh system-report-check --capture-usb-bundle NEW_DIRECTORY` then `xsh dev/main.xsh system-report-check --replay-usb-bundle NEW_DIRECTORY` in the pinned Linux image. The private bundle preserves device and interface links, root hub interface names, bounded attributes, raw descriptors, source absence, byte digests, and independent references. Replay validates sources before and after production USB collection; unexposed power and interface fields are reported unavailable. This host's 14 bus entries, including seven devices, captured with `stable=true`; topology, IDs, power, and interfaces replayed exactly. |
| system-report raw SMBIOS capture and firmware collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_smbios_capture_validates_saved_raw_table_and_oracle`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_smbios_capture_scores_table_without_optional_entry_point`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_smbios_capture_preserves_absent_source_without_scoring`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_smbios_capture_replays_production_collector_from_raw_table` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-smbios-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-smbios-bundle NEW_DIRECTORY`. The private `0700` directory retains bounded DMI table and optional SMBIOS entry-point bytes, source states, digests, and an independently parsed oracle; replay validates the bundle before and after the firmware collector. Local capture and validation cases pass, including rejection of conflicting observed-source error metadata; replay needs the current collector binary. |
| system-report coverage denominator, CPU, swap, storage, and identity reference comparisons, process and host-effect trace parsing, and offline replay source-read detection | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_os_release_reference_decodes_data_and_requires_candidate_agreement`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_swapon_raw_parser_and_comparison_keep_swap_identity`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_swapon_raw_parser_rejects_ambiguous_or_unsafe_rows`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_json_scores_devices_and_layering`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_json_rejects_incomplete_or_unsafe_rows`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_json_preserves_shared_tree_edges` | `xsh dev system-report-check` validates the checked-in manifest; `xsh dev system-report-check --compare-cpu --compare-swaps --compare-storage --compare-modules --compare-identity --compare-namespaces --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets four sysfs CPU identity sets, byte-valued swap areas, an explicit-column all-device `lsblk` tree, loaded modules, uname release and architecture, `/proc/uptime`, OS release ID/version, and eight process-visible namespace links; `xsh dev system-report-check --no-subprocess --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` traces live JSON, replay, and failure paths |
| system-report raw meminfo name, unit, stable byte, and named host-field comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_meminfo_reference_preserves_units_and_rejects_ambiguous_rows` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_meminfo_comparison_scores_stable_fields_and_host_projection` | In the pinned ARM64 image, `xsh dev system-report-check --compare-meminfo --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets one sensitive memory report with bounded `cat /proc/meminfo` observations; changed gauges remain unscored |
| system-report os-release production and independent reference parsers reject malformed assignment and unquoted-value syntax while preserving later valid keys | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_os_release_value_parser_rejects_malformed_assignments`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_os_release_reference_decodes_data_and_requires_candidate_agreement` | In the pinned ARM64 image, `xsh dev system-report-check --compare-identity --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` compares decoded `ID` and `VERSION_ID` from the selected local source |
| system-report VLAN parent links, IPv6 addresses, and policy routing retain prefix, lifetime, nexthop, table, mark/mask, and interface fields | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_assembles_network_links_addresses_routes_and_rules` | In the pinned ARM64 image, run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate; full live address, route, and rule differential evidence remains outstanding |
| system-report independent link identity and kernel raw field comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_link_reference_scores_stable_identity_and_state`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_link_reference_checks_master_relationship`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_link_reference_checks_lower_link_relationship`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_network_link_raw_reference_scores_flags_type_and_stable_counters`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_network_link_raw_reference_reads_bounded_sysfs_attributes` | In the pinned ARM64 image with iproute2, `xsh dev system-report-check --compare-network-links --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive network report with `ip -json -details link show` and bounded `/sys/class/net` reads. `compare_ip_links` checks identity, MTU, administrative and operational state, kind, and master/lower links; `compare_network_link_raw` checks hardware type, the IFF bits common to sysfs and rtnetlink, internal consistency of the candidate raw flag word and named flags, and RX/TX byte counters within monotone bracketing samples. It scores `network.links` only when both references are complete and link identity, type, and flags are stable. The current amd64 musl image passed the rooted fixture and live link gate. |

For network links, `/sys/class/net/*/flags` exposes `net_device.flags`;
rtnetlink also reports operational and group-managed bits. Treating these as
identical falsely failed both links on the amd64 test host. The independently
observed common bits remain exact in
`dev/system_report_check.xsh::compare_network_link_raw`, and the candidate's
raw word must agree with its named flags. The kernel source defines the sysfs
read in `net/core/net-sysfs.c::NETDEVICE_SHOW_RW(flags)` and the volatile flag
set in `include/uapi/linux/if.h::IFF_VOLATILE`. A focused regression covers
the differing but valid words.
| system-report oversized route-netlink link counters retain the link, report an exact-range field issue, and keep enumeration success separate from field completeness and malformed entities | `cargo test -p xsh --lib oversized_link_counters_keep_the_link_and_report_field_failures`, `cargo test -p xsh --lib network_dump_keeps_enumeration_success_separate_from_field_issues`, `cargo test -p xsh --lib malformed_entity_prevents_successful_enumeration_after_all_dumps_finish`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_assembles_network_links_addresses_routes_and_rules` | Run the Rust tests and native report gate in the pinned ARM64 image; the local builder exposes only amd64 and the current debug XSH binaries are stale |
| route-netlink dump status framing and kernel error signs | `cargo test -p xsh --lib dump_accumulator_rejects_truncated_status_and_positive_error_codes` | Run in the pinned ARM64 image with the full filtered netlink decoder test module; the local builder exposes only amd64 |
| route-netlink raw multipart framing, serialization, and bundle integrity | `cargo test -p xsh --lib raw_multipart_dumps_replay_through_the_network_snapshot_decoder` and `cargo test -p xsh --lib raw_reply_bundle_replays_validated_datagrams_and_rejects_tampering` | The pinned amd64 image ran these cases in the 203-test Rust library gate. Synthetic link, address, route, and rule datagrams pass through the production accumulator and entity decoders; bounded bundle replay rejects changed bytes. The repository's ARM64 CI target remains outside this task. |
| opt-in route-netlink reply capture and later replay | In the pinned image, set `XSH_NETLINK_CAPTURE_DIR=NEW_PRIVATE_DIRECTORY` outside the tracked tree and run `cargo test -p xsh --lib --target x86_64-unknown-linux-musl live_raw_reply_capture_or_replay_uses_the_network_decoder -- --ignored --nocapture`. Later set `XSH_NETLINK_REPLAY_DIR=SAVED_DIRECTORY` and run the same command. Set `XSH_NETLINK_XSH_BIN=ABSOLUTE_XSH` to verify XSH report assembly after either capture or replay. | The ignored Rust test saves four validated raw dumps in a newly created `0700` directory with a `0600` file. Its digest, native byte order, architecture, origin, and capture time are checked on replay. `XSH_NETLINK_CAPTURE_ORIGIN=physical_live` marks an explicitly authorized physical-host capture; the pinned container defaults to `container_live`. The Alpine edge amd64 capture and replay passed with two links, three addresses, eight routes, and seven rules. With `XSH_NETLINK_XSH_BIN`, the replay also passed through the production XSH assembler and preserved enumeration success, entity counts, link indices, and per-link address counts. |
| system-report independent iproute2 address JSON parsing and bracketed IPv4/IPv6 comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_address_reference_preserves_ipv6_identity_and_link_membership` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_address_lifetimes_score_bracketed_countdowns` | In the pinned ARM64 image with iproute2, `xsh dev system-report-check --compare-network-addresses --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive network report with bounded `ip -json address show` observations. `compare_ip_addresses` checks interface-scoped identity, prefix, scope, and broadcast; `compare_ip_address_lifetimes` requires valid and preferred lifetimes to fall within monotone before/after samples. An absent lifetime or a renewed lifetime leaves this assertion partial. The amd64 live comparison scored exact for three addresses and their bracketed lifetimes. |
| system-report independent iproute2 policy-rule JSON parsing and static IPv4/IPv6 selector comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_rule_reference_keeps_family_and_static_selectors`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_rule_goto_target_is_part_of_rule_identity`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_rule_full_score_requires_representable_selectors`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_rule_reference_preserves_unknown_numeric_action`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_rule_reference_normalizes_named_kernel_actions`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_rule_reference_preserves_prefix_without_address_attribute` | In the pinned ARM64 image with iproute2, `xsh dev system-report-check --compare-network-rules --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive network report with bounded `ip -json -family inet rule show` and `ip -json -family inet6 rule show` observations. It compares family, priority, source and destination prefixes, mark/mask, table, action, interface names, and a `goto` priority decoded from the candidate's raw `FRA_GOTO` attribute. Numeric and named kernel actions keep the product's explicit labels; an absent address attribute with a nonzero prefix length retains that prefix; iproute2's NAT metadata is recognized but unscored. The assertion scores only when every reference field and candidate rule attribute is represented, each candidate priority is present, and rule flags are zero; unknown fields, attributes, or flag bits leave it partial. The amd64 live comparison matched all five IPv4/IPv6 rules while recording two other address-family rules outside the reference scope; its mandatory score remains partial because kernel default rules did not expose every priority attribute. |
| system-report independent iproute2 route JSON parsing and static IPv4/IPv6 route comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_route_reference_keeps_family_table_and_link_identity`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_route_reference_scores_source_and_multipath_hops`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_route_full_score_requires_representable_fields`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_route_reference_preserves_unknown_numeric_enums` | In the pinned ARM64 image with iproute2, `xsh dev system-report-check --compare-network-routes --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive network report with bounded IPv4 and IPv6 route JSON observations. The comparison covers destination, source selector and prefix, preferred source, table, type, scope, protocol, metric, gateway, output link, known route flags, and multipath hops. Numeric unknown type, scope, and protocol values keep the product's explicit labels. The full `network.routes` assertion scores only when every reference key and candidate attribute is represented, including supported multipath gateway attributes; unrepresented reference fields and candidate attributes or flag bits keep it partial. An unknown iproute2 flag name is a reference parse error. The live gate remains pending the pinned rebuild. |
| system-report independent numeric PCI function comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lspci_vmm_numeric_identity_keeps_repeated_ids_distinct` | In the pinned ARM64 image with pciutils, `xsh dev system-report-check --compare-pci --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets one sensitive PCI report with bounded `lspci -D -vmm -n -k` observations. It compares BDF-scoped numeric IDs, class and program interface when exposed, revision, subsystem IDs, driver, NUMA node, and IOMMU group when exposed. This adapter cannot establish parent links or absent optional numeric values, so its PCI identity comparison remains partial. The current worker has no `lspci` binary. |
| system-report independent PCI binding comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_pci_binding_reference_resolves_parent_indexes_and_absent_links`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_pci_binding_parent_reference_requires_own_bdf_and_keeps_bridge`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_pci_binding_rooted_reference_reads_links_and_unknown_numa` | In the pinned ARM64 image with PCI functions, `xsh dev system-report-check --compare-pci-bindings --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive report with bounded PCI sysfs link, directory, and NUMA observations. It compares driver, parent BDF, NUMA, and IOMMU group for each function; changed references cannot score. The old debug binary's rooted directory reads prevent the rooted and live gates here. |
| system-report independent PCIe link comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_pci_link_reference_scores_all_functions_and_stable_link_fields` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_pci_link_rooted_reference_reads_bounded_attributes` | In the pinned ARM64 image on a host with PCIe link attributes, `xsh dev system-report-check --compare-pci-links --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets one sensitive PCI report with bounded `/sys/bus/pci/devices` readings. It scores all exposed current and maximum speeds and widths by BDF when the reference is stable; a host without exported link attributes reports the assertion unavailable. The local debug binary's rooted `fs.root_children` failure prevents this live gate. |
| system-report trace audit resolves descriptor-relative source paths before privacy and replay checks | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_forbidden_process_read_trace_normalizes_source_paths`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_replay_trace_rejects_normalized_live_source_paths`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_trace_resolves_annotated_directory_descriptors` | In the pinned ARM64 image, run `xsh dev system-report-check --no-subprocess --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`; `strace -yy` supplies descriptor paths so relative reads can be checked against their actual directory. The focused parser tests do not establish the production trace result. |
| system-report uptime reference requires exactly two decimal columns and JSON-safe whole seconds, then brackets changing readings | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_uptime_reference_requires_a_bracketed_integer_second` | In the pinned ARM64 image, `xsh dev system-report-check --compare-identity --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` captures `/proc/uptime` before and after one candidate identity report |
| system-report raw uptime capture and identity collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_uptime_capture_validates_saved_source_and_oracle`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_uptime_capture_keeps_absent_malformed_and_truncated_unscoreable`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_uptime_capture_replays_production_identity` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-uptime-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-uptime-bundle NEW_DIRECTORY`. The private bundle keeps one bounded raw source, digest, and independent exact-second oracle. Raw validation passes locally; product replay needs the pinned rebuild. Live uptime can advance between reads, so replay checks the saved snapshot rather than requiring a stable live source. |
| system-report independent device-tree reference reads bounded od bytes, enforces NUL-terminated model and compatible values, and scores their order | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_device_tree_od_reference_reads_bounded_raw_source` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_device_tree_reference_requires_exact_terminated_bytes_and_order` | In the pinned ARM64 image with device-tree identity files, `xsh dev system-report-check --compare-identity --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` runs bounded `/usr/bin/od` reads before and after the identity report; absent files remain unscored and changed bytes cannot be scored |
| system-report kernel module reference joins and exact report comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsmod_reference_scores_module_values_and_state`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsmod_reference_rejects_conflicting_or_unsafe_rows`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_modules_accept_taint_flags_and_reject_extra_columns` | In the pinned ARM64 image, `xsh dev system-report-check --compare-modules --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets the candidate with `lsmod` and `/proc/modules` snapshots. Both parsers accept the optional seventh taint word and reject further columns. Exact comparison requires successful module enumeration and no module source issue, including when both module lists are empty. A changed set or source disagreement cannot be scored. |
| system-report private raw kernel module capture and collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_modules_raw_reference_requires_complete_unique_rows`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_proc_modules_preserves_unavailable_use_count`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_modules_capture_validates_raw_source_and_oracle`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_modules_capture_marks_absent_and_malformed_unscoreable`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_modules_capture_replays_production_collector` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-kernel-modules-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-kernel-modules-bundle NEW_DIRECTORY`. The private bundle retains bounded `/proc/modules` bytes, source state, digest, origin, and an independent parsed module oracle, including unavailable use counts. An absent, malformed, or changing source remains unscoreable. Raw validation passes locally; production replay needs the pinned rebuild. The mandatory live assertion also requires an agreeing `lsmod` reference with nonnegative counts. |
| system-report raw kernel command-line bytes and default redaction | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_command_line_reference_preserves_bytes_and_redaction` | In the pinned ARM64 image, `xsh dev system-report-check --compare-command-line --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets sensitive and default candidate reports with bounded `cat /proc/cmdline` references |
| system-report private raw kernel command-line capture and collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_command_line_capture_validates_raw_bytes`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_command_line_capture_records_absence_without_scoring`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_command_line_capture_replays_redaction` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-kernel-command-line-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-kernel-command-line-bundle NEW_DIRECTORY`. The private bundle retains bounded raw bytes, source state, origin, digest, and exact byte reference. An absent source is recorded but cannot score. Replay verifies the saved source and reruns the production kernel collector in sensitive and default modes, requiring exact bytes or base64 in the former and no payload in the latter. Raw validation, including rejection of conflicting observed-source error metadata, passes locally; product replay needs the pinned rebuild because the old runner cannot load the current collector. |
| system-report fixed sysctl and module-parameter values and absent states | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_parameter_reference_scores_values_and_absence` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_parameter_reference_rejects_duplicate_or_unexpected_names` | In the pinned ARM64 image, `xsh dev system-report-check --compare-parameters --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets sensitive kernel output with named `sysctl -n` and bounded module-parameter file readings |
| system-report private raw kernel parameter capture and collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_parameter_capture_validates_observed_absent_and_tampered_sources`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_parameter_capture_preserves_malformed_utf8_and_rejects_truncation`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_parameter_capture_replays_production_collector` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-kernel-parameters-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-kernel-parameters-bundle NEW_DIRECTORY`. The bundle retains only the six fixed sysctl and three fixed module-parameter paths, source states, digests, and an independent nine-key oracle. A changed or incomplete source remains unscoreable. The first two validation tests pass locally; collector replay needs the pinned rebuild. The mandatory live assertion separately requires `sysctl -n` and module-file references. |
| system-report flat findmnt mount identities, relationships, options, and redaction | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_findmnt_json_scores_repeated_targets_and_redaction` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_findmnt_json_rejects_ambiguous_rows` | In the pinned ARM64 image, `xsh dev system-report-check --compare-mounts --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets the candidate with two all-mount `findmnt` observations using explicit columns; repeated targets remain separate by mount ID |
| system-report private raw mountinfo capture and storage collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_mountinfo_raw_reference_preserves_ids_escapes_and_options`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_mountinfo_capture_validates_saved_bytes_and_oracle`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_mountinfo_capture_preserves_absent_source_without_scoring`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_mountinfo_capture_replays_production_storage_collector` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-mountinfo-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-mountinfo-bundle NEW_DIRECTORY`. Bundle schema v2 retains at most 4 MiB of raw `/proc/self/mountinfo` bytes, source state, digest, and an independent decoded oracle. Replay checks exact saved metadata and compares production mount IDs, relationships, paths, options, propagation class, and sanitized group IDs without capacity path traversal. Unknown optional fields remain redacted entries. The parser and saved-source fixtures pass locally; production replay needs the pinned rebuild. |
| system-report independently selected safe mount capacity and explicit skips | `target/debug/xsht test --exact --jobs 1 tests/xsh/stdlib/fs.xsh::test_fs_root_operations_reject_traversal`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_findmnt_usage_scores_safe_mounts_and_explicit_skips`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_findmnt_usage_rejects_ambiguous_or_unsafe_rows`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_findmnt_usage_keeps_unavailable_capacity_explicit` | In the pinned Linux image, `xsh dev system-report-check --compare-mount-usage --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets the candidate with ID-filtered `findmnt --df --bytes` observations for independently eligible local mounts, including regular file bind mounts |
| system-report queue reference fields and bounded firmware/stat snapshots | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_queue_json_scores_supported_fields`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_queue_json_rejects_duplicate_and_unsafe_rows`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_block_queue_raw_sources_bracket_counters_and_firmware`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_block_queue_raw_sources_reject_incomplete_and_unsafe_stats` | In the pinned ARM64 image, `xsh dev system-report-check --compare-queue --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets explicit `lsblk` type and static queue fields with bounded sysfs firmware/stat observations; partition rows may project parent queue values while the report retains absent partition sources; changing in-flight I/O is unscored rather than counted as an exact match. The amd64 live check matched all 28 devices and scored exact when the in-flight gauge remained stable |
| system-report raw block class, partition, layer, queue, and counter replay | `target/debug/xsht test --jobs 1 test_system_report_block_bundle`, `target/debug/xsht test --jobs 1 test_system_report_block_raw_reference_checks_each_layer_direction` | In the pinned Linux image, run `xsh dev system-report-check --capture-block-bundle NEW_DIRECTORY` then `--replay-block-bundle NEW_DIRECTORY`. The private bundle preserves class links and relation links to the listed peer device paths, source absence, SHA-256 byte digests, up to 8192 paths at 4096 bytes each, and independent sysfs interpretations. It checks holder and slave directions separately. `dev/system_report_check.xsh::BlockRawReference` and `BlockRawDevice` are exported construction contracts for independent references; fixtures annotate these domains before invariant collection checking. Static fields must be stable; changed `stat` sources are recorded separately. The amd64 image captured 28 devices, three graph edges, and two changing stat files; production replay matched identity, optional partition queue fields, firmware, and saved counters exactly. Mountinfo has its own bundle. |
| Lossless formatted Path and compound process interpolation | `cargo test -p xsh --test integration path_interpolation` and `target/debug/xsht test --jobs 1 test_path_interpolation` | Native path/process gates and both indexed routes. The raw filesystem fixture explicitly reports macOS `EILSEQ` at filename creation; native argv coverage still runs on that host |
| Raw host path, argv, environment bytes, bounded rooted reads, rooted directory enumeration, typed rooted symlink observations, and rooted filesystem counters | `target/debug/xsht test --exact --jobs 1 tests/xsh/stdlib/fs_root_children.xsh::test_fs_root_children_reads_newly_created_directory`, `cargo test -p xsh-root --test security readable_directory_open_rejects_files_fifos_and_escapes -- --exact`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/stdlib/fs_root_readlink_result.xsh::test_fs_root_readlink_result_distinguishes_link_absence_and_read_failure` | Native stdlib gate plus the filtered runtime gate; the filesystem-name case skips on macOS because the host returns `EILSEQ` |
| Privileged Linux mount and `switch_root` behavior | In the pinned privileged `xsh-test` image: `cargo test -p xsh --target aarch64-unknown-linux-musl --test linux_priv --features linux-priv-tests linux_priv_mount_and_switch_root_fail_within_private_namespace -- --exact --nocapture` | Run the full `linux_priv` test binary in the same image with the target and feature flags |
| Privileged Linux loop lifecycle | In the pinned privileged `xsh-test` image: `cargo test -p xsh --target aarch64-unknown-linux-musl --test linux_priv --features linux-priv-tests linux_priv_loop_attach_list_and_detach_release_device -- --exact --nocapture` | Run the full `linux_priv` test binary and `xsht test tests/xsh/stdlib/linux.xsh` in that image. On this amd64 host, the focused case, all seven privileged tests, and all 13 native Linux module tests passed after excluding the `/dev/loop` directory from numbered-device enumeration. |
| Linux live process descriptor lifecycle | In the pinned ARM64 `xsh-test` image: `target/debug/xsht test --exact tests/xsh/stdlib/linux.xsh::test_linux_open_files_tracks_a_live_child_descriptor` | The native Linux stdlib gate in the same image |
| One runtime fixture | `cargo test --test integration runtime::TEST_NAME` | the filtered runtime gate above |
| Process, network job, and stream-worker cancellation | `cargo test --test integration --features net runtime::process::sigterm_drains_process_net_job_and_parallel_workers_with_trace_parentage -- --exact` | Run the filtered runtime gate above with `--features net` on macOS and in the pinned Linux image |
| `xsht::cli::CliOutput`, `xsht::grep::find_matches_in_program`, or CLI/tooling | targeted `cargo test -p xsht --test integration cli::TEST_NAME` or `cargo test -p xsht --test integration grep::TEST_NAME` | `cargo test -p xsht --test integration` is owner-run: even the `cli::` group invokes `fmt` and `lint` |
| Copied `xsht` formatter/linter parity on script-backed calls in static and loaded modules | owner-run `cargo test -p xsht --test integration cli::copied_xsht_formats_and_lints_script_backed_calls_in_static_and_loaded_modules -- --exact` | The runnable-corpus gate below and the existing copied-product check/run test |
| Migrated API parity across `xsh` profiles | `cargo test -p xsht --test profile_parity -- --nocapture` after building the four debug/release and default/no-default products | Run the same test and builds in the pinned `Dockerfile.test` ARM64 musl image using `dev/targets.xsh::docker_test_env`; missing products print their build commands |
| Copied product and packaged core smoke, including the `system-report` entrypoint from a separate working directory | `target/debug/xsht test --jobs 1 dev/tests/test-targets.xsh`, `python3 -m unittest discover -s tools -p 'test_copied_product_smoke.py'`, and `tools/copied-product-smoke.py` with all three debug binaries and a `dev/release.xsh::package_core` archive | Repeat with the pinned Linux ARM64 musl debug products and `--linux`; use `--linux-platform linux/amd64` with the amd64 image and products for this task. The 16-case release target gate covers command and library paths, modes, test exclusion, extraction, checksums, and rejecting another compressed artifact before writing a new archive; the Python verifier rejects duplicate, non-file, or special-mode members before extraction and accepts the regular-file mode encoding emitted by this tree's tar writer; the current-tree fixture checks extracted `core/system-report` bytes, mode, and library presence; the path test requires command installation to remove only the final `.xsh` suffix, even when an earlier `.xsh` appears in the filename; the smoke checks the packaged `system-report` schema-version command from outside the package working directory, collects a live identity-only v1 JSON report there, then renders that saved report through `--from` from the same separate directory. `bench/stdlib-port/README.md` gives the commands |
| Benchmark workload | `cargo bench -p xshi --bench bench --features benchmark BENCHMARK -- --sample-count 1 --sample-size 1` | `cargo dev bench --fast` (memory/regression) or `cargo dev bench` (latency) |
| `xshi` editing, input decoding, line buffer, prompt, listing, aliases, config, denv | `cargo test -p xshi --lib interactive::ported_tests` (ports of `ish`'s tests) or the module's own `tests` | `cargo test -p xshi` |
| `xshi` terminal geometry and repaint | `cargo test -p xshi --lib interactive::render::tests` | `cargo test -p xshi` and `cargo test --test integration runtime::interactive:: -- --test-threads=1` |
| `xshi` completion candidates and remote completion | `cargo test -p xshi --lib interactive::complete::tests` and `cargo test -p xshi --lib remote_completion` | `cargo test -p xshi` |
| `xshi` history storage: log, cache, lock, sync, reset, recovery | `cargo test -p xshi --lib interactive::history::tests` (real files; includes concurrent-shell and 200,000-line cases) | `cargo test -p xshi` and the interactive gate for cross-shell scenarios |
| `xshi` shell semantics (statuses, redirection order, globs, substitution, variables) | `cargo test --test integration runtime::interactive::NAME -- --exact` (piped session tests) | `cargo test --test integration runtime::interactive:: -- --test-threads=1` |
| `xshi` screen behavior against the reference shell | `cargo test --test integration runtime::interactive::parity::scenarios::NAME -- --exact` (or `::extended::NAME`) | `cargo test --test integration runtime::interactive::parity` |
| `xshi` PTY terminal lifecycle and descriptor hygiene | `cargo test --test integration runtime::interactive::xshi_pty_master_does_not_survive_exec -- --exact` | `cargo test --test integration runtime::interactive:: -- --test-threads=1` |
| `xshi` job control (`Ctrl-Z`, `fg` continuing lists, forced exit) | `cargo test --test integration runtime::interactive::fg_resumes_the_rest_of_an_and_or_list -- --exact` and `cargo test -p xshi --lib stopped_background_job_resumes_then_foregrounds` | `cargo test -p xshi` and the interactive gate |
| `ProcessRedirection::ChildDup` ordering | `cargo test --lib runtime::process::tests::child_dup_follows_the_childs_own_redirections` | `cargo test --test integration runtime::interactive::redirections_apply_left_to_right` |
| Non-Tokio archive/network dependency update | `cargo tree -i tokio` and `cargo tree -p xsh-net -e features` | focused archive or network runtime gate |
| Arena or indexed-IR layout | `scripts/ir-layout.py` (or `--only TYPE` for a focused report) | focused rustybench workload plus the applicable behavior tests |
| Frontend retained/peak accounting | `cargo test -p xsh --lib frontend_stats::tests` and `cargo run --bin xsh-frontend-stats -- --json tests/fixtures/frontend-indexed` | `cargo dev bench --fast` after the applicable syntax/checker gate |
| Frozen indexed fixtures and lexical shadowing | `target/debug/xsht test --jobs 1 tests/xsh/frontend-indexed.xsh` | The same native module plus `cargo test --test integration runtime::frontend_indexed:: -- --test-threads=1` for producer lifecycle fixtures |
| Runtime controller/worker allocation accounting | `cargo test -p xsh --lib runtime_stats::tests` and `cargo build -p xsh --bin xsh-runtime-stats` | `xsh-runtime-stats --json REPORT SCRIPT [-- ARGS...]` with matching output fingerprint, scoped worker attribution, and paired release host RSS |
| `FullBuilder::build_compact`, `FullVerifier::verify`, or executable IR | targeted `cargo test -p xsh --lib runtime::eval::indexed::full::tests::` | `cargo test -p xsh runtime::eval::indexed::full::tests --lib --features native-tests` |
| Explicit execution frames | targeted `cargo test --test integration runtime::stack_depth -- --test-threads=1` | `cargo test -p xsh runner::tests --lib --features native-tests` plus the runtime gate |
| `guard let` else control flow within an enclosing loop | `target/x86_64-unknown-linux-musl/debug/xsht test --exact --jobs 1 tests/xsh/basic.xsh::test_guard_failure_controls_enclosing_loop` in the pinned amd64 image | `xsht test --jobs 1 tests/xsh/basic.xsh` in that image plus the filtered runtime gate |
| Production executable runtime | targeted `cargo test --test integration runtime::TEST_NAME` | `cargo test -p xsh --test integration runtime:: --features native-tests -- --skip runtime::coverage:: --skip runtime::examples::example_corpus_is_formatted --skip runtime::examples::example_corpus_lints_without_warnings --test-threads=1` plus `cargo test -p xsh --test integration runtime::coverage::xsh_native_tests --features native-tests -- --exact` and `cargo dev bench --fast` |
| Syscall diagnostics | benchmark smoke test on the host | `cargo dev bench --syscalls` on Linux/Docker |
| LLVM IR size | `tools/llvm-lines-repeat-offenders.xsh` over an existing capture | fresh `cargo llvm-lines` capture plus the applicable behavior/benchmark gate |
| API registry/reference/examples | see `API Gate` below | same |
| Broad cross-cutting work | closest targeted tests | relevant filtered package tests; unfiltered `cargo test` is owner-run |
| Coordinated language ergonomics and statement/value contexts | `target/debug/xsht test --jobs 1 tests/xsh/ergonomics.xsh`, `target/debug/xsht test --jobs 1 tests/xsh/ergonomics-extended.xsh`, and `target/debug/xsht test --jobs 1 tests/xsh/statement-value-contracts.xsh` (statement, value, Result, default, and producer decision matrix) plus the nearest feature test module | syntax and checker integration suites, isolated lint acceptance tests, the filtered runtime gate, and native stdlib tests |
| Ambient filesystem authority policy | `cargo test --test ambient_fs_policy` | relevant filtered tests; unfiltered `cargo test --tests` is owner-run |

The coordinated syntax smoke fixtures are
`tests/fixtures/runtime/ergonomics-coordinated.xsh` and
`tests/fixtures/runtime/ergonomics-extended.xsh`. Execute both with the ordinary
`xsh` binary built using `--no-default-features --features net,tools` to check
that the language features do not depend on native-test support.

Network link, address, route, and rule reference adapters score the fields they
can compare when all four route-netlink dumps and entity decodes finished,
including a partial network section with a separate field issue. An incomplete
enumeration keeps those comparisons unscored. The partial field comparisons do not count as
complete mandatory assertions.

`dev/system-report-coverage.json` maps raw route-netlink multipart, interrupted,
unknown-attribute, and truncated-address scenarios to exact Rust decoder tests
in `src/modules/linux/real/netlink.rs`. In the pinned Linux image,
`xsh dev system-report-check --run-fixtures --xsh-bin ABSOLUTE_XSH --xsht-bin ABSOLUTE_XSHT --cargo-bin ABSOLUTE_CARGO`
runs those tests with `cargo test --offline -p xsh --lib --target
TARGET` and an exact one-test filter, where `TARGET` is the selected context
triple (`aarch64-unknown-linux-musl` by default or the explicit amd64 override
for this task). Missing Cargo,
zero-test runs, and failed tests fail the mapped case; manifest validation alone
only proves the test definitions exist.

`dev/tests/test-system-report-check.xsh::test_system_report_coverage_manifest_contract`
checks that the system-report trace audit rejects mixed route-netlink sends
when any decoded message type is outside the collector's four query types. It
also rejects attempted local socket connections, sends to local helper
sockets, query-shaped sends without a proved netlink target, and sends on
inherited sockets with no visible destination. It permits stdout/stderr writes
and rejects writes to other descriptors or positioned writes; only decoded
route-netlink `sendto` queries addressed to kernel port zero with no multicast
groups are expected on the collector path. Socket creation is limited to raw
or datagram `NETLINK_ROUTE`; external or local socket families, other netlink protocols,
socket pairs, listeners, and accepts fail the trace audit even if their syscalls
return errors. The checker parses syscall arguments so a netlink marker
inside a payload or quoted socket path cannot justify a send or bind. Other
send forms and connections without an allowed target fail closed. The
trace selector explicitly includes file timestamp and allocation changes,
descriptor-to-descriptor copies, credential and namespace changes, and system
clock changes; attempted calls fail even when the kernel denies them. It also
traces `ioctl` and allows only read-only terminal, interface, block, filesystem,
clock, and loop queries recognized by the host adapters. Unknown or mutating
requests fail even when denied. The trace
audit rejects attempted reads of per-process environments, command lines, memory,
and open-path sources while allowing `stat`, `status`, and the kernel command
line needed for the report. It checks the syscall's source-path argument and
normalizes `.` and `..` components and repeated separators from the fixed `/`
working directory, so a readlink result or a differently spelled procfs path
cannot hide or invent a process-source read. `strace -yy` adds directory
descriptor paths; the checker resolves relative paths against those paths,
rejects a relative source read when its directory descriptor has no absolute
annotation, and allows annotated stdout and stderr descriptors. Saved-report replay applies the
same normalization before rejecting live `/proc`, `/sys`, and `/etc` source
paths. The audit first traces the same applet's `--version` startup path and
subtracts only identical syscall and normalized-path occurrences from replay.
This accounts for runtime allocator probes; any additional live source read
still fails. The production `strace` invocation prints up to 4096 characters per
string so paths remain visible to these checks. An unfinished, resumed, or
truncated file-open record makes the trace audit inconclusive and fails the
gate instead of being treated as a read-only operation.
The production trace gate also runs missing, malformed, unsupported-schema,
and invalid-UTF-8 replay inputs. Each successful or failing replay trace must
avoid additional live `/proc`, `/sys`, and `/etc` file and metadata reads
beyond startup. The production audit passed in the pinned `Dockerfile.test`
image on x86_64 musl; denied or malformed live-source environments remain
separate fixture evidence.

`dev/system_report_check.xsh::compare_live_processes` reads bounded raw
`/proc/[pid]/stat`, `status`, `statm`, and `cgroup` sources before and after one sensitive process
report. Its parser is separate from `core/lib/system_report_live.xsh` and uses
the same kernel ABI and rooted file API, so it checks parsing and report
normalization rather than independent kernel instrumentation. PID plus start
ticks defines a comparable identity. PIDs that disappear, are reused, or change
static fields across the bracket are reported as unstable; skipped references
are counted. The live
path checks PID, parent PID, real numeric UID, start ticks, and command for the
stable subset. Invalid unused `stat` memory counters do not remove a valid
identity or thread observation. Threads, RSS bytes, virtual bytes, and the
unified cgroup path
are compared only when their before and after values agree for the same
PID/start identity. Changed resource fields and process state remain unscored.
This partial check does not increment the mandatory live assertion count for
either process manifest entry.

For successful JSON paths, `dev/system_report_check.xsh::forbidden_process_field_violations`
also rejects environment, command-line, memory, credential, and open-path
fields on serialized process records. These parser checks do not replace a
trace of the rebuilt applet on the pinned Linux target.

The PTY harness (`laputa-ptytest`) keeps a master descriptor open while it
spawns `xshi` and other runtime tests may spawn children in parallel. Its master
and duplicated slave descriptors must have close-on-exec set at creation;
`runtime::interactive::xshi_pty_master_does_not_survive_exec` checks the
inherited-descriptor boundary in a child process. A forked child also holds any
`flock` descriptor its parent had open until it execs, so tests that take a
history lock in one thread while another spawns processes must wait for the
lock rather than expect it to be free.

### Interactive differential gate

`tests/runtime/interactive/parity/` replays each scenario against the real
`xshi` in an isolated `HOME` and compares the transcript with a golden recorded
from `ish` (`tests/fixtures/interactive-parity/<os>/`, one set per operating
system because the programs scenarios run word their diagnostics differently on
macOS and Linux). The gate needs no `ish` install. Related environment variables:

| Variable | Effect |
| --- | --- |
| `XSHI_PARITY_ISH_BIN=/path/to/ish` | also run each scenario against `ish`; require `ish` == golden == `xshi` |
| `XSHI_PARITY_RECORD=1` | with the above, rewrite the goldens from `ish` |
| `XSHI_PARITY_FULL=1` | print whole transcripts on a mismatch |

The scenarios pin every input that changes a screen: the terminal locale
(ASCII on Linux, whose images have no `locale` tool to list UTF-8 locales),
`/etc/profile` (`XSHI_PROFILE_PATH=/dev/null`), and the prompt's host name.
Narrow-terminal scenarios wrap the prompt, so the host name's length decides
what lands on each row; `xshi` is started with `XSHI_HOSTNAME=sentry` and `ish`,
which cannot be told, must run where its short name is also six characters
(the macOS host, or `docker run --hostname sentry`). Linux goldens are recorded
by building `ish` for `aarch64-unknown-linux-musl` (its sibling checkouts
mounted read-only, its target directory on a volume) and running the gate in the
`Dockerfile.test` image with `XSHI_PARITY_ISH_BIN` pointing at that binary.
`Dockerfile.test` gives each musl target an empty `libutil.a` because the PTY
harness links `-lutil`.

Goldens record terminal text, styles, cursor state, and persisted effects, not
implementation details. Record on the platform being tested only when the output
is platform-independent; scenarios that print machine data pin it in the
fixture (fixed mtimes, fixed terminal width) or normalize it in the harness.
`ish` itself is only needed to add or refresh a scenario; scenarios for
behavior `ish` lacks (`$` completion, `fg` continuing a list, forced exit) are
`xshi`-only tests in `tests/runtime/interactive.rs`.

On 2026-09-29 the sibling `../../rustybench` crate compiled with the pinned
nightly and `cargo dev bench --fast` ran all ten `xshi` workloads in the
Alpine edge amd64 image. Set `CARGO_BUILD_TARGET=x86_64-unknown-linux-musl`
for this image so Cargo keeps target linker flags off host proc-macro crates.
The fast mode uses one measured run with one sample per workload; without a
previous baseline, its output establishes an initial observation, not a
regression verdict. Retain the generated baseline under ignored `target/`
unless the owner explicitly chooses to publish it.

## Native XSH Test Rule

Retired record string contracts: `tests/xsh/record-contract-removal.xsh` covers
the removed diagnostic, nested schema validation, extra fields, aliases,
optional absence versus present null, and retained CLI descriptor strings.
Run `target/debug/xsht test --jobs 1 tests/xsh/record-contract-removal.xsh`,
the existing `tests/xsh/stdlib/record.xsh` module, and focused
`cargo test -p xsh --test integration removed_record_require` and
`cargo test -p xsht --test integration removed_record_require` gates.
The tooling witnesses use isolated fixture files for CST edits, Unicode,
comments, ordinary rechecking, refusal, and convergence. API inventory removal
is covered by the registry and aggregate API gates.

Language behavior **must** be specified in the native XSH corpus, normally in
`tests/xsh/stdlib/` or the nearest `tests/xsh/*.xsh` module. Do not embed new
XSH source strings in Rust tests merely to exercise language behavior.

Rust integration tests are reserved for boundaries that native XSH cannot own:
fixture servers, exact process/byte lifecycles, PTYs, privileges, and platform
behavior. A Rust-owned boundary harness must invoke the relevant disk-backed
native XSH test and provide only its fixture inputs (for example URLs,
certificate paths, or temporary roots). Keep the assertions about the language
contract in XSH. Any exception requires an adjacent comment explaining why a
disk-backed native test cannot express the behavior.

Nested `test.run_script` stderr includes the run's temporary script path, which
embeds the runner PID and a nanosecond timestamp. Assert on rendered message
fragments such as `exited with status 127`, never on bare numbers: a bare
`"127" not in stderr` check made `tests/xsh/run.xsh::test_whole_script_run_error_diagnostics`
fail intermittently.

## Executed Test Accounting

`docs/TEST-EXECUTION.json` records the tests that actually ran, along with each
skip reason and test ID. `tools/test-execution-report.py` checks that parsed
case lines agree with each completed gate's summary before writing the JSON.
The 2026-09-24 snapshot covers the full configured native `xsht test --jobs 1`
suite and the focused Rust `runtime::interactive` gate on macOS ARM64 and the
pinned Linux ARM64 musl image. It does not claim Rust suite-wide coverage.

The source inventory snapshot had 23 PTY `#[ignore]` attributes in
`tests/runtime/interactive.rs`, one intentionally ignored cold-start diagnostic
in `src/stdlib.rs`, and 35 `test.skip` call sites in tracked XSH tests. The
commented-out stress probe in `tests/runtime/os.rs` is not a registered test.
The interactive gate has since been replaced: `runtime::interactive::` now
holds the differential scenarios and `xshi`-only tests with no `#[ignore]`, so
that part of the snapshot is stale until the report is regenerated. The
cold-start probe is a manual measurement. Native skips are conditional on
platform, installed paths, and network fixtures, so the JSON records observed
counts separately for each host.

For the Linux native gate, bind the container-built
`target/aarch64-unknown-linux-musl/debug/xsh` over `/work/target/debug/xsh`.
The `dev/tests/test-lifecycle.xsh` fake tools use that path as a shebang;
without this bind, the shared macOS target directory supplies a Mach-O binary
and six test cases fail before reaching their assertions. Use the pinned
`Dockerfile.test` image for this gate.
The image does not install `make`; the Makefile facade test in
`dev/tests/test-lifecycle.xsh` reports an explicit skip there and runs on hosts
with `make` available.

## API Gate

```sh
cargo build -p xsh -p xshi -p xsht --bin xsh --bin xshi --bin xsht
cargo metadata --no-deps --format-version 1
cargo test --test integration libxsh_api
cargo test -p xsh-registry --lib
cargo test -p xsh-registry --test api_reference
cargo test -p xsh --lib modules::signature
cargo test -p xsht --test api
target/debug/xsht api
target/debug/xsht api summary --format jsonl
target/debug/xsht check docs/snippets/api
cargo dev check
git diff --check
```

Run the relevant language or runtime test gate when the API contract or an
example exposes behavior that changed outside the registry and renderer. The
snippet directory check scans only that explicit directory, even when the
repository config has additional `include` roots.

## XSH Corpus Gate

Use the runnable-corpus integration test after changing core applets, native
tests, showcases, tools, benchmark scripts, or repository automation scripts.
It checks formatting and linting without rewriting files; intentional parser,
formatter, and runtime fixtures under `tests/fixtures/` are excluded.
Documentation fragments under `docs/snippets/` are excluded as well because
they may contain illustrative placeholders rather than complete programs.
This is an owner-run gate under the agent workflow rule above.

```sh
cargo test --test integration runtime::coverage::runnable_xsh_corpus_is_formatted_and_lints_without_warnings
```

Membership and boolean statement contracts are covered by
`tests/xsh/assertions.xsh`, including caller-owned membership-named callable
fields accessed through `Any`; the independent CLI failure witness is
`runtime::run::assertion_failure_is_an_unsuccessful_cli_exit`. Migration
coverage belongs in `crates/xsht/tests/lint.rs`;
`linter_migrates_package_nested_assertions_and_multiline_match_membership`
covers nested fixes and inline match syntax, while
`linter_match_membership_snapshots_preserve_effects_and_failure` checks snapshot
order, skipped arms, custom messages, and assertion failure through `xsht trace`.
On Linux x86-64, run these
in the amd64 `Dockerfile.test` image with the
`x86_64-unknown-linux-musl` flags from `dev/targets.xsh::docker_test_env`.
`runtime::eval::lowered_ops::assertion_detail_tests` covers bounded rendering
of the indexed record representation's interned field names.

## Runtime Test Modules

Language assertions and dry-run module contracts live in the nearest
`tests/xsh/` module. Rust runtime tests retain CLI, PTY, host fixtures,
process and signal lifecycles, raw bytes, allocation accounting, and small
stack boundaries.

| Area | File |
|---|---|
| half-open List/Unicode scalar Str/Bytes slicing and evaluation order | `tests/xsh/slicing.xsh`, `tests/syntax.rs`, `tests/sema.rs`, `crates/xsht/tests/lint.rs` |
| list concatenation, compound updates, collection aliasing and allocation traffic | `tests/xsh/stdlib/methods.xsh`, `tests/xsh/stdlib/map.xsh`, `tests/runtime/collections.rs` |
| coverage, lint, grep-adjacent tooling | `tests/runtime/coverage.rs` |
| frontend indexed fixtures | `tests/xsh/frontend-indexed.xsh`, `tests/runtime/frontend_indexed.rs` |
| `fs.walk`/`fs.files` options and walk value consumption | `tests/xsh/stdlib/fs.xsh` |
| cataloged examples | `tests/runtime/examples.rs` |
| `core/pstree.xsh` process-tree output | `core/tests/test-pstree.xsh`, `tests/runtime/unix.rs` |
| interactive behavior | `tests/runtime/interactive.rs` |
| Linux-specific behavior | `tests/xsh/stdlib/linux.xsh`, `tests/runtime/linux.rs` |
| standard modules | `tests/xsh/stdlib/module.xsh`, `tests/runtime/modules.rs` |
| embedded standard-module linkage and copied checker/runner binaries | `tests/stdlib_port.rs`, `tests/runtime/run.rs::copied_products_check_and_run_script_backed_calls_in_static_and_loaded_modules` |
| OS-facing runtime behavior | `tests/xsh/stdlib/unix.xsh`, `tests/runtime/os.rs`, `tests/runtime/unix.rs`, `tests/runtime/linux.rs` |
| Explicit accepted exit policies, actual Status, signal/setup/decode failures, invalid pre-spawn options, Command/waits, per-segment pipelines, and late stream failures | `tests/xsh/run-accept.xsh`, `tests/xsh/run-accept-stream-capture.xsh`, `tests/runtime/process.rs::accepted_process_*`, `tests/runtime/process.rs::accept_policy_expression_runs_once_before_child_spawn`, `tests/syntax.rs::parser_and_formatter_preserve_accept_policy_expressions`, `tests/sema.rs::checker_accept_policy_requires_bounded_int_codes_on_every_plan_route`, `crates/xsht/tests/lint.rs::explicit_accept_policy_keeps_propagation_and_custom_status_handlers` |
| Exit policy composition with cwd/environment scopes, typed causes, named stages, and checked callable aliases | `tests/xsh/run-accept-cross-feature.xsh`; `target/debug/xsht test --jobs 1 tests/xsh/run-accept` |
| Bytes stdin redirection, concurrent capture delivery, early closure, Command/stream/pipeline routes, and owned spawn cleanup | `tests/runtime/process.rs::bytes_stdin_*`, `tests/xsh/stdlib/process.xsh::test_bytes_stdin_*`; `cargo test -p xsh --test integration bytes_stdin` |
| `run_capture`, `spawn_managed`, and process execution | `tests/xsh/run.xsh`, `tests/xsh/stdlib/process.xsh`, `tests/runtime/process.rs`, `tests/runtime/run.rs` |
| local Result capture, nested Result data, lexical exits, assertion and cleanup failures | `tests/xsh/try-capture.xsh` |
| retry blocks, selective nominal/facet patterns, error identity, cleanup and selection trace | `tests/xsh/retry.xsh` |
| stack depth and explicit lowered frames | `tests/runtime/stack_depth.rs` (including Result projections inside native arguments, operand order, and error cleanup); `tests/xsh/expression-continuations.xsh` pins ordinary and traced native argument order and dispatch boundaries |
| structured stream behavior | `tests/xsh/stdlib/streams.xsh` |
| Owned and sliced Str field equality in where predicates | `tests/xsh/stdlib/streams.xsh::test_stream_where_string_views_match_ordinary_record_comparison` |
| Structured stage named configuration, spreads, modes, entry timing, and migration | `tests/xsh/stream-options.xsh`, `tests/syntax.rs::stream_stage_flags_are_fatal_migration_diagnostics_with_exact_fixes`, and xsht CLI migration tests |
| stream argv and signal process boundaries | `tests/runtime/streams.rs` |

## Fixture Locations

| Fixture | Purpose |
|---|---|
| `tests/fixtures/syntax` | parser and formatter fixture sources |
| `tests/fixtures/fmt` | annotated disk-backed formatter fixture and golden used by `tests/xsh/formatter.xsh` |
| `tests/fixtures/sema` | checker fixture sources |
| `tests/fixtures/runtime` | executable runtime fixture scripts |
| `tests/fixtures/frontend-indexed` | frozen indexed-execution and indexed-method fixtures |
| `examples` | standalone example scripts cataloged in `examples/catalog.json` |
| `showcase` and `showcase/tests` | larger standalone scripts and native tests |

## Commands To Avoid

- `cargo dev lint`, `cargo fmt`, `cargo clippy`, `xsht fmt`, and `xsht lint`
  are owner-run commands for agent work, including their fix variants.
- The `dist` profile is reserved for release packaging, not local agent
  verification.
- Benchmark commands intentionally use release code generation.

Nested and renamed record binding behavior is covered by
`tests/xsh/record_binding.xsh`. Parser spans and checker rejection cases live in
`parser_retains_nested_renamed_record_binding_targets_and_spans`,
`checker_rejects_nested_record_binding_contract_violations`, and
`checker_keeps_selected_nested_record_field_types`. The
`lint.prefer-record-destructuring` rule combines adjacent unannotated field
bindings from one checked record identifier; annotations, comments, effectful
receivers, dynamic schemas, and retained intermediate bindings require manual
review. Its focused tooling tests verify formatter preservation and convergence.

Guarded value control uses `tests/xsh/guarded-control.xsh` for condition-first
payload laziness, selected-branch narrowing, stream yields, and deferred cleanup.
`tests/xsh/guarded-control-proofs.xsh` checks full-check and runtime agreement for
complementary return/loop proofs, Bool aliases, exiting mutations, resumable
yields, and invalid control targets.
`tests/sema.rs::checker_guarded_control_proofs_agree_on_full_and_compact_routes`
pins the same retained receiver types on both checker routes.
`tests/syntax.rs::guarded_value_controls_round_trip_without_absorbing_guard_into_run_argv`
and `guarded_value_control_keeps_payload_and_condition_source_spans` cover the
parser/formatter boundary; `tests/sema.rs::checker_guarded_value_control_*` retain
ordinary target/type/effect rejection. `crates/xsht/tests/lint.rs::linter_prefer_guard_*`
cover safe fixes, refusal cases, grouping, and convergence.

Optional postfix evaluation, layer preservation, and strict receiver boundaries
are covered by `tests/xsh/optional-postfix.xsh`. Run
`xsht test --jobs 1 tests/xsh/optional-postfix.xsh` and the focused xsht
`optional_postfix`/`guarded_postfix` integration filters before the ordinary
syntax, semantic, tooling, and native stdlib gates. The null-branch migration
rule is `lint.prefer-optional-postfix`; it refuses mutation, lost comments,
and results whose null value would change fallback behavior.
`test_optional_record_return_field_alias_preserves_receiver_type` pins named
callable return schemas through unannotated field aliases. The rooted storage
regression gate is `xsht test --jobs 1 test_system_report_storage`.

Direct typed map iteration is covered in `tests/xsh/stdlib/map.xsh` by the
`test_map_iteration_*` procedures. These pin entry types/order, snapshot value
semantics, nested destructuring and qualifiers, nominal Result propagation,
break/continue, and lexical restoration. `lint.prefer-map-entry-iteration` has
focused fix, refusal, Unicode-span, formatter, and convergence coverage in
`crates/xsht/tests/lint.rs`; mutable sources, annotations, comments on the
lookup, and nonstandard receivers remain manual transformations.

`tests/xsh/defer-blocks.xsh` owns deferred-block registration, mutable reads and
snapshots, LIFO order, local control, statement failures, secondary diagnostics,
and stream cancellation. `tests/syntax.rs::deferred_block_parses_and_formats_as_statement_body`
checks comment/span preservation and formatter convergence. The
`linter_defer_block_helper_*` tests in `crates/xsht/tests/lint.rs` cover the narrow
`lint.prefer-defer-block` fix and refusal boundaries.

Prepared regex literals: `tests/xsh/regex-literals.xsh` covers raw syntax,
existing operations, Unicode byte offsets, defaults, repeated calls, and dynamic
compile errors. `src/modules/regex.rs` checks source spans, unreachable invalid
patterns, and shared preparation across frontend passes. The indexed-store
`prepared_regex_pool_survives_frontend_and_evaluator_reuse` test verifies engine
identity and pool bounds. `crates/xsht/tests/lint.rs` covers exact re-encoding,
conservative exclusions, comment retention, and convergence; `crates/xsht/tests/cli.rs`
checks invalid unreachable literals without script execution.

Private proc effect inference is exercised by
`tests/xsh/private-proc-effects.xsh` (transitive calls, recursion, explicit bounds,
references, mutation, assertions, capture, host requirements, and unknown call
chains). `tests/sema.rs::private_proc_effects_*` checks full/compact facts and
separately parsed module identity. `crates/xsht/tests/lint.rs::private_proc_effects_*`
checks inference convergence and conservative annotation removal. Run
`target/debug/xsht test --jobs 1 tests/xsh/private-proc-effects.xsh`, then
`CARGO_BUILD_JOBS=1 cargo test -p xsh --test integration private_proc_effects -- --test-threads=1`
and `CARGO_BUILD_JOBS=1 cargo test -p xsht --test integration private_proc_effects -- --test-threads=1`.

Private pure return inference is exercised by
`tests/xsh/private-pure-inference.xsh` (values, dependency order, captures,
Result boundaries, pattern/fallback capture shadowing, imported tag variants,
and rejection paths). `tests/sema.rs::private_pure_inference_*`
checks published full/compact signature facts; `crates/xsht/tests/lint.rs::private_pure_return_*`
and the matching CLI tests cover exact opt-in annotation removal, refusal,
convergence, and `--annotate=returns` preservation. Run the native module, then
`cargo test -p xsh --test integration sema::` and
`cargo test -p xsht --test integration private_pure_return`.

Field label semantics: `tests/xsh/field-labels.xsh` covers keyword schemas,
constructors/error payload patterns, access and mutation, renamed destructuring,
serialization parity, all keyword spellings, validation, duplicates, and illegal
bindings/puns. Syntax coverage pins brace disambiguation, lossless key spelling,
formatter round trips, and quoted dotted keys. Focused tooling acceptance uses
`cargo test -p xsht --test integration field_label`; bare label rewrites
recheck and converge. Known get calls retain their Result behavior, including
comments, consumer conversions, and error contexts.
List element and nested assignment behavior lives in `tests/xsh/list-assignment.xsh`.
It pins existing record/Map evaluation policy, same-root selector/RHS mutation,
strict bounds, contextual schemas, alias independence, and cleanup after failure.
`assignment_path_reuses_unique_storage_and_preserves_aliases` owns internal
allocation identity; `list_assignment_verifies_paths_and_executes_both_routes`
checks malformed indexed paths and both runtime routes. The
`linter_list_element_assignment_*` tooling tests cover exact-bound fixes,
refusal, Unicode/comments, normal rechecking, and convergence.
Block header unification is covered by `tests/xsh/block-parameters.xsh`:
nominal with/guard handler types, sequential initializer short-circuiting,
lexical scope, cleanup, invalid headers, and rejected legacy syntax. Its
isolated tooling fixtures verify comment preservation, source rechecking,
and migration convergence. `tests/xsh/basic.xsh` retains guard loop transfers.

Structured pipeline fact parity is covered by
`tests/sema.rs::compact_stream_stage_result_matrix_matches_canonical_checked_facts`
and `canonical_stream_stage_facts_distinguish_equal_spans_in_modules`.
`tests/xsh/stdlib/streams.xsh` retains keyed-count downstream methods and
procedure-local terminal/callback contracts; `tests/xsh/stdlib/json.xsh`
retains JSON adapter lists consumed through validated record fields.
Run the checker integration suite and both native modules for these boundaries.

Explicit value pipeline holes are covered by `tests/xsh/value-pipeline-holes.xsh`
for positional/named placement, input and argument order, optional laziness,
Result boundaries, and rejected contexts. `tests/sema.rs::value_pipeline_holes_*`
pins full/compact facts and record presence refinement. `tests/syntax.rs::parser_value_pipeline_holes_*`
checks arena spans and formatter convergence; the matching xsht grep/refactor
and lint tests check structural retention, safe migration, refusal, and a stable
second fix pass, including the isolated CLI convergence/execution test. Run the
native module, syntax/checker integration suites, and
focused xsht integration tests before the full relevant gates.

Block strings: `tests/xsh/block-strings.xsh` pins exact margins, empty/shared
breaks, blank lines, raw/formatted text, interpolation order, unaffected literal
domains, rejection, CRLF/tabs, and isolated formatter/lint execution parity.
`src/syntax/literal.rs::block_string_tests` covers byte-level layout and untouched
interpolation source slices; `tests/syntax.rs` covers original diagnostic spans
and formatter round trips. Run the native module with `xsht test --jobs 1`,
`cargo test -p xsh --lib block_string`, and the focused xsht `block_string` tests
before the ordinary syntax/tooling gates.

## Prepared constants

`target/debug/xsht test --jobs 1 tests/xsh/constants.xsh` covers lexical and
qualified references, forward dependencies, concrete empty containers, schema
and tag construction, immutable aliases, and rejected runtime initialization.
`cargo test -p xsh --lib constant_scope_index_tests -- --test-threads=1`
checks bounded containment lookup work, equal-span and overlapping block
identity, shared workspace source isolation, and preparation of unused bodies.
`cargo test -p xsh --lib prepared_constant_pool --features native-tests -- --test-threads=1`
checks pool reuse, checkpoint rewind, and verifier rejection. Tooling acceptance
uses `cargo test -p xsht --test integration prepared_constant_fix`; it checks
keyword preservation, inert migration boundaries, comments, and convergence
through library APIs. Broaden with the syntax/checker integration gates and the
indexed verifier suite; formatting and lint CLI gates remain owner-run.


Typed scalar Map domains, numeric and byte order, receiver-bound methods, aliases,
empty contexts, absent versus null, nested COW updates, and JSON rejection:
`target/debug/xsht test --jobs 1 tests/xsh/typed-map-keys.xsh`; broaden with native
stdlib map/collections, syntax and semantics Rust gates, indexed verifier tests,
and xsht formatter/lint/grep integration coverage.

## Str-backed enum boundaries

`target/debug/xsht test --jobs 1 tests/xsh/wire-enums.xsh` covers constant
mappings, nested JSON conversion, atomic rejection, nominal imports, ordinary
enum rejection, and constructor-only defaults. Run the indexed verifier gate
and `cargo test -p xsht --test api api_core_enums` for pool validity, both runtime
routes after frontend drop, and API example lookup. System report codec changes
also require the JSON golden and rejection cases in `tests/xsh/system-report.xsh`.

### FsRoot receiver methods

Nearest gate: `target/debug/xsht test --jobs 1 tests/xsh/stdlib/fs_root_methods.xsh`.
The native cases cover all receiver methods, parent/child close independence,
bounded observations, raw byte names, symlink confinement, named argument order,
forged records, removed aliases, lazy optional receivers, method trace IDs, and
filesystem effects. The opaque owner identity is covered by
`cargo test -p xsh --lib opaque_fs_root_identity --features native-tests`; both
indexed execution routes and missing default slots are covered by
`cargo test -p xsh --lib fs_root_methods_keep_opaque_identity --features native-tests`.
Broaden with
`target/debug/xsht test --jobs 1 tests/xsh/stdlib/fs.xsh`,
`tests/xsh/stdlib/fs_root_children.xsh`, and
`tests/xsh/stdlib/fs_root_readlink_result.xsh`. Tool boundaries use
`cargo test -p xsht --test integration fs_root_receiver --features native-tests`
and `cargo test -p xsht --test api api_fs_root --features native-tests`.
## Signature CLI entries

`target/debug/xsht test --jobs 1 tests/xsh/signature-cli.xsh` covers typed
positionals, options, repeated defaults, rest operands, aliases, prepared
constants, help, and declaration rejection. The process and module initializer
boundary uses `cargo test -p xsh --test integration signature_cli -- --test-threads=1`:
help exits 0, invalid argv exits 2, and filesystem markers remain absent before
successful dispatch. `cargo test -p xsht --test integration signature_cli`
covers literal-schema migration, comment preservation, policy rejection, and
fix convergence. Ordinary explicit CLI parsing remains covered by
`tests/xsh/stdlib/cli.xsh`.
## Constant CLI descriptors

`tests/xsh/stdlib/cli_commands_constants.xsh` covers common command shapes,
options, rootless/fallback selection, imported composition, named spreads,
dynamic validation, and rejection of unreachable invalid descriptors.
`checker_cli_command_descriptors` pins full/compact parity;
`cli_command_descriptor_plans` pins shared prepared plans on both indexed routes.

`target/debug/xsht test --jobs 1 tests/xsh/stdlib/cli_constants.xsh` covers
constant, imported, projected, and composed descriptors, defaults, aliases,
repeated fields, explicit optional positionals, applet duplicate policy,
`parse_full.values`, named argument evaluation order, dynamic validation, and
declaration-time rejection with imported source provenance.
Broaden with `target/debug/xsht test --jobs 1 tests/xsh/stdlib/cli.xsh`.
`cargo test --test integration checker_cli_constant_descriptor` checks full
and compact type parity and declaration spans. The indexed plan pool, verifier
boundaries, and both execution routes use
`cargo test -p xsh --lib cli_descriptor_plans --features native-tests`; broaden
with the filtered indexed runtime gate. `prepared_cli_plan_pool_rewinds_with_builder_checkpoint`
covers speculative plan pool cleanup.
Record projection and Boolean alias provenance is owned by
`src/sema/check/proof.rs::BindingProof` and `ConditionNarrowings`. Full and compact
checkers share subject identities, bounded mutation stamps, path overlap rules,
and continuation intersections. Immutable aliases retain shared proof sets;
`condition_proofs` records when predicates were checked so later mutations cannot
revive stale evidence. Both routes publish precise expression types and proved
Optional fallback receivers. Indexed lowering reads those facts and inserts no
casts or runtime proof checks.

## Record proof provenance

`target/debug/xsht test --jobs 1 tests/xsh/proof-provenance.xsh` covers nested
projections, bounded alias DAGs, short circuit scopes, assertion success,
continuation intersections, snapshots, sibling updates, mutation, shadowing,
capture invalidation, recovery joins, and unreachable fallback calls.
`cargo test -p xsh --test integration checker_record_proof_types` checks exact
full/compact facts. The indexed verifier gate includes
`record_proof_precise_types_and_unreachable_fallback_survive_frontend_drop` for
both executor routes without retained frontend state. `cargo test -p xsht --test integration record_proof_fallback_fix`
uses tooling library APIs to check proof-only fixes, refusal, and convergence.
Broaden with semantic integration tests, Boolean guard and pattern native modules,
and indexed verifier tests.
Compatibility vocabulary removal is covered by
`target/debug/xsht test --jobs 1 tests/xsh/compatibility-vocabulary.xsh`:
preparation before effects, byte/character distinction, direct-child ordering and
missing-path errors, every run result mode, exact comment/argv preservation,
shorthand wire keys, shadowing, invalid arguments, rechecking and idempotence.
The migrated syntax and checker fixtures remain under `tests/syntax.rs` and
`tests/sema.rs`; public inventory is checked through native API queries.
## Cwd and environment value scopes

`target/debug/xsht test --jobs 1 tests/xsh/context-scopes.xsh` covers tail values,
nested Results, entry failure, source order, lexical transfers, transparent
propagation, nested restoration, defer timing, and producer escape rejection.
`test_scope_command_capture_tails_keep_values_and_restore_context` covers text,
bytes, capture records, nested command Results, discarded captures, and failed
commands. `context_scope_command_capture_tails_execute_after_frontend_drop_on_both_routes`
checks their ordinary indexed execution, scoped cleanup, and restoration after
frontend disposal without requiring the native test harness.
It also checks suspended and delegated producer isolation and cancellation cleanup.
Error payloads and shared nominal or process error causes are covered by
`test_scope_rejects_producers_hidden_in_error_causes`,
`test_scope_rejects_producers_hidden_in_process_error_causes`, and
`test_scope_preserves_scalar_error_causes_as_data`.
Run `tests/xsh/stdlib/env.xsh`, the syntax and sema integration targets, indexed
verifier unit tests, and xsht scope tooling acceptance tests for the full gate.
`cargo test -p xsh --lib context_scope_` checks native environment bytes,
fatal and forced cleanup, and dynamic outer assignment on both evaluator routes.
Default parameter declarations are covered by `tests/xsh/default-parameters.xsh`:
constant/import projections, already permitted calls, lexical shadowing, lazy
omission, supplied argument order, cleanup and error propagation. Native
coverage also pairs explicit and inferred lazy producer defaults, including
unconsumed producers and eager supplied arguments. Focused host
checks use `cargo test --test integration default_parameter` and
`cargo test -p xsht --test integration default_parameter`; broaden to syntax,
checker, lint acceptance tests and indexed verifier tests after those pass.

Mixed enum and removed-record migrations across imports are covered by
`cargo test -p xsht --test integration cli::mixed_enum_and_record_require`.
The isolated CLI fixtures pin normal graph rechecking, exact comment and Unicode
retention, execution after repair, staged convergence, and refusal without partial writes
for unrelated errors in the entry, an import, or a removed-result consumer.

`target/debug/xsht test --jobs 1 tests/xsh/inferred-require.xsh` covers
independently anchored validation targets, unchanged validation failures,
and rejected unanchored contexts. `cargo test -p xsht --test integration
inferred_require` covers argument-only source fixes and convergence;
`cargo test -p xsh --lib inferred_require` covers prepared schemas after
frontend disposal on both indexed execution routes.

UInt mutation boundaries: `tests/xsh/uint-mutation.xsh` pins scalar, record,
List, Map, whole replacement, compound arithmetic, selector order, RHS effects,
failure atomicity, call arguments/defaults/returns, constructor payloads, functional
method operands, and reached producer items. Broaden with native list-assignment
and stdlib Map gates,
and indexed verifier execution on both routes.

Closed helper capture and command scope migration are covered by
`cargo test -p xsht --lib lint_try_capture` and
`cargo test -p xsht --lib context_scope`. The helper suite checks unchanged
Result/statement/effect facts and success/failure observations; the scope suite
checks placeholder visibility, process results, nested restoration, and cleanup
refusals. `cargo test -p xsht --lib literal_migration_tests` pins inert Path and
Duration eligibility, literal-origin byte bounds, exported keyword spans, and
one preparation per changed source during convergence.
