# Test Map

Choose the narrowest useful command first, then run the broader gate for the
area touched. Agents do not run formatters or linters; leave gates that invoke
them to the owner and report that limit. Unfiltered `cargo test` includes
`runtime::coverage` cases and two `runtime::examples` cases that launch
`xsht fmt` or `xsht lint`, so agents use the filtered runtime gate below.

## Routine CI

`.github/workflows/verify.yml` runs on pull requests and pushes to `master`
with read-only repository permission. Both jobs use `DIST_PROFILE=dev` and
dispatch through `cargo dev test`, which calls `dev/main.xsh`:

| Runner | Target | Features selected by the XSH workflow |
| --- | --- | --- |
| macOS ARM64 | `aarch64-apple-darwin` | `net tools` in `dev/test_workflows.xsh::macos_ci`, plus default features |
| pinned Linux ARM64 musl image | `aarch64-unknown-linux-musl` | `linux-priv-tests net tools` in `dev/internal.xsh::linux_ci_test`, plus default features |

`dev/docker.xsh::internal_argv` forwards the selected target and profile into
the pinned container. The manual release workflow retains the `dist` profile;
ordinary CI uses debug products. These full CI tests include formatter and
linter checks and are owner-run under the agent workflow rule.

The release matrix in `.github/workflows/release.yml` matches the three
triples in `dev/targets.xsh::resolve`: x86_64 and aarch64 Linux musl, and
aarch64 Darwin. `dev/dist.xsh::native_dist` builds `xsh`, `xsht`, and `xshi`
for each target; `dev/release.xsh::validate_artifacts` requires all nine
binary artifacts and their checksum sidecars. The names and validation
boundary are covered by `dev/tests/test-targets.xsh`. Actual `dist` builds
and package smoke checks run in the manual release workflow.

`tests/linux_priv.rs` is included only with `linux-priv-tests`. In the pinned
privileged image it runs under the CI `dev` profile with `net tools`; use
`--nocapture` to see capability or fixture skip reasons. Rust counts those
early-return cases as passed, so record whether a privileged case actually ran.

## Common Gates

| Change | Narrow command | Broader gate |
|---|---|---|
| Rust compile only | `cargo build` | relevant filtered package tests; unfiltered `cargo test` is owner-run |
| `Lexer::lex_compact`, `Parser::parse_source_arena_only`, or formatter | targeted `cargo test --test integration syntax::TEST_NAME` | `cargo test --test integration syntax::` |
| `Checker::check_compact_declarations`, `Checker::probe_compact_bodies`, or lint | targeted `cargo test --test integration sema::TEST_NAME` for checker or `cargo test -p xsht --test integration lint::TEST_NAME` for lint | `cargo test --test integration sema::` for checker or `cargo test -p xsht --test integration` for lint |
| `Evaluator::prepare_compact_indexed_only`, `indexed_run`, or runtime behavior | targeted `cargo test --test integration runtime::TEST_NAME` | `cargo test --test integration runtime:: -- --skip runtime::coverage:: --skip runtime::examples::example_corpus_is_formatted --skip runtime::examples::example_corpus_lints_without_warnings --test-threads=1`; run relevant `runtime::coverage` tests by exact name only when they do not invoke formatters or linters |
| native XSH module behavior | `target/debug/xsht test --exact --jobs 1 PATH::TEST_NAME` | `target/debug/xsht test --jobs 1 tests/xsh/stdlib` |
| system-report relationships, USB descriptor ownership and bounds, USB controller and device-class parent source states, OS release source precedence and escaping, namespace, network device, and driver link failure states and v1 replay compatibility, source parsers, CPU list parsing, section projection, JSON conversion/redaction, terminal-safe rendering, mount usage, cgroup accounting, firmware issues, and offline CLI replay | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_class_parent_retains_independent_fallback_and_link_failure`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_controller_link_distinguishes_directories_disappearance_and_failures`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_driver_link_distinguishes_unbound_and_unreadable_devices`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_network_device_links_keep_absence_separate_from_failures`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_identity_preserves_namespace_link_failures` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report complete identity source reads, local os-release fallback, and exact uptime parsing | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_identity_rejects_truncated_source_prefixes` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_uptime_parser_requires_complete_two_column_decimal` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report malformed, missing, or invalidly spelled local os-release ID retains valid fields without vendor fallback | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_os_release_value_parser_rejects_malformed_assignments`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_identity_withholds_malformed_os_release_values_without_vendor_fallback`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_identity_marks_os_release_without_id_partial` | The pure ID grammar test passes on the available debug `xsht`; in the pinned ARM64 image, run the focused rooted tests and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate |
| system-report ARM device-tree model and compatible values require complete NUL-terminated strings, and heterogeneous CPU parts survive without DMI | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_device_tree_strings_require_complete_terminated_values` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_arm_identity_preserves_heterogeneous_cpus_without_dmi` | In the pinned ARM64 image, run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate |
| system-report DMI placeholder text remains raw observed identity | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_identity_retains_dmi_placeholder_text_as_raw_values` | In the pinned ARM64 image, run the focused test and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate; the current debug binary stops at collector module loading |
| system-report process-visible namespace and cgroup scope | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_scope_keeps_all_process_visible_namespace_identities` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_namespace_reference_requires_observed_exact_targets` | In the pinned ARM64 image, run the focused native test, `xsh dev system-report-check --compare-namespaces --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`, and the full native report gate; the old debug binary cannot load the updated collector |
| system-report PCI functions retain separate function identities, optional PCIe link absence, and malformed width issues | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_pci_multifunction_keeps_optional_link_sources_distinct` | In the pinned ARM64 image, run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate; the old debug binary's `fs.root_children` failure prevents this rooted fixture from reaching the collector |
| system-report PCI collection reports a non-UTF-8 sysfs name and retains a valid neighboring function | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_pci_collection_reports_non_utf8_names_without_losing_valid_functions` | Run the rooted fixture and full report suite in the pinned ARM64 image; the local debug binary cannot load the report test module's newer network dump schema |
| system-report device-class names and attributes reject truncated reads | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_device_classes_reject_truncated_names_and_attributes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report sound and input classes remain complete when DRM is absent | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_device_classes_keep_sound_and_input_without_drm` | In the pinned ARM64 image, run the focused test followed by `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh`; the old debug binary cannot load the updated collector |
| system-report USB platform parents, repeated numeric IDs, and root hubs joined after children | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_controller_path_handles_pci_and_platform_roots` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_parent_join_handles_root_hubs_sorted_last` | In the pinned ARM64 image, run the focused tests and the full native report gate; the old debug binary stops at collector module loading |
| system-report unreadable USB power attributes remain field issues | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_usb_power_read_failures_make_section_partial` | In the pinned ARM64 image, run the focused test and the full native report gate; the old debug binary stops at collector module loading |
| system-report CPU enumeration integrity, absent CPU zero, rooted symlink cycles and escapes, and truncated cgroup CPU sets | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_collection_does_not_invent_absent_cpu_zero`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_enumeration_requires_a_valid_present_list`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_present_symlinks_cannot_cycle_or_escape_the_source_root`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_effective_cpuset_rejects_a_truncated_source` | In the pinned ARM64 image, run the focused cases and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate; the local debug binary rejects the newer network dump schema while loading this module |
| system-report 128 present CPUs and absent CPUFreq/idle capability | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_collection_preserves_128_present_ids` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_absent_cpufreq_unavailable` | In the pinned ARM64 image, run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate |
| system-report shared cache identity remains one record across distinct NUMA node assignments | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset` | In the pinned ARM64 image, run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate |
| system-report CPU model enrichment rejects truncated cpuinfo prefixes | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpuinfo_rejects_a_truncated_complete_looking_prefix` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report CPU frequency policy source states, unreadable optional values, exact numeric bounds, and shared boost reads | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpufreq_policy_rejects_truncated_field_prefixes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report CPU idle controls, counters, unreadable optional values, labels, and affinity source states | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_idle_and_affinity_reject_truncated_prefixes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report CPU directory failure and disappearance issues | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_directory_failures_keep_source_issues` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report CPU cache size scaling, overflow, and truncated reads | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_bounded_size_bytes_checks_scaled_json_range`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cpu_cache_sizes_reject_scaled_overflow_and_truncation` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report block relationship enumeration and class-entry link failures | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_keeps_holder_and_slave_enumeration_failures` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report layered block identities retain reciprocal holder and slave indexes | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_links_layered_block_devices_by_identity` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_json_scores_devices_and_layering` | In the pinned ARM64 image, run both focused tests and the full native report gate; the old debug binary stops at collector module loading |
| system-report sparse partition numbers retain disk parent and exact byte sizes | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_keeps_sparse_partition_numbers_and_parent_links` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_json_preserves_sparse_partition_identity_and_parent_edges` | In the pinned ARM64 image, run both focused tests and the full native report gate; the old debug binary stops at collector module loading |
| system-report block identity, capacity, scheduler, unreadable queue attributes, model, firmware, and I/O stat layouts reject incomplete or unsafe source data | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_rejects_invalid_block_source_fields` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report block scheduler selection requires one unambiguous active choice | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_block_scheduler_requires_one_selected_choice` | In the pinned ARM64 image, run the rooted `test_system_report_storage_rejects_invalid_block_source_fields` and full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate |
| system-report SMBIOS unknown record identity and type 17 sentinel size bounds | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_smbios_unknown_type_keeps_record_identity`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_smbios_sentinel_size_requires_complete_formatted_field` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report mount inventory rejects a valid-looking truncated source prefix | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_mounts_reject_truncated_complete_looking_prefix` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report mount identities stay inside the exact JSON integer range | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_mount_rejects_json_unsafe_identity` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report mount usage skips shadowed targets and automount descendants | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_storage_usage_skips_shadowed_and_automount_descendants` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report huge-page and NUMA directory failures | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_memory_directory_failures_keep_source_issues` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report global and NUMA huge-page size and count boundaries | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_huge_page_pools_reject_unsafe_sizes_and_partial_counts` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report NUMA meminfo row identity, complete reads, and exact KiB conversion | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_numa_meminfo_requires_complete_rows_and_exact_bytes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report cgroup complete membership/mount sources and bounded memory, CPU, IO, and PID values | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cgroup_inventory_rejects_partial_membership_and_mounts`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cgroup_limits_keep_source_and_numeric_failures`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cgroup_cpu_and_io_counters_reject_partial_and_unsafe_values` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report hybrid cgroup v2 values with v1 limitation | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_cgroup_hybrid_keeps_v2_values_and_v1_limitation` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report pressure stall rows, exact totals, duplicate kinds, and unavailable sources | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_psi_average_parser_rejects_invalid_percentages`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_pressure_keeps_complete_rows_and_unavailable_sources_distinct` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report complete transparent huge-page policy selection and unknown values | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_thp_policy_parser_keeps_unknown_selected_value`, `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_transparent_huge_page_policy_preserves_unknown_selection` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report independent transparent huge-page policy parsing and stable live comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_thp_reference_requires_one_selected_policy_and_stable_fields` | In the pinned ARM64 image, `xsh dev system-report-check --compare-thp --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive memory report with bounded `cat` observations for the available `enabled` and `defrag` policy files; no exported policy remains reference unavailable |
| system-report meminfo exact JSON bounds after KiB conversion and on unscaled counters | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_memory_reports_malformed_and_oversized_meminfo_fields` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report meminfo tab separators and duplicate-field withholding | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_memory_accepts_tabbed_values_and_withholds_duplicate_fields` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report truncated meminfo read rejects complete-looking prefix rows | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_memory_does_not_parse_truncated_meminfo_prefix` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report swap header, rows, escaped names, exact byte bounds, and incomplete source reads | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_swap_devices_keep_exact_bytes_and_reject_partial_sources` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report kernel module inventory rejects truncated source prefixes and retains valid rows beside malformed rows | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_modules_reject_truncated_source_prefix` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_modules_keep_valid_rows_with_malformed_neighbor` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report kernel command line retains source whitespace and redacts the whole payload | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_source_text_preserves_exact_whitespace_when_requested` and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_command_line_preserves_source_whitespace` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report fixed kernel parameter inventory retains values and absent sources | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_kernel_parameter_allowlist_keeps_values_and_absence` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report sysctl permission failure under a real unprivileged reader | `cargo test --offline -p xsh --test linux_priv --features linux-priv-tests --target aarch64-unknown-linux-musl system_report_sysctl_denial_as_unprivileged_reader_creates_no_child -- --exact --test-threads=1` | Run in the pinned ARM64 image as root so the Rust host fixture can drop the XSH child and `strace` to UID/GID 65534; it requires `kernel.pid_max` to remain null with an EACCES issue, a readable neighboring sysctl to remain observed, and no child process or secondary exec. Manifest validation and a fake Cargo runner check do not prove this privilege test passed |
| system-report PCI identity without a label database or helper | `cargo test --offline -p xsh --test linux_priv --features linux-priv-tests --target aarch64-unknown-linux-musl system_report_pci_keeps_numeric_ids_without_a_label_database_or_helper -- --exact --test-threads=1` | Run in the pinned ARM64 image; the Rust host fixture gives the XSH collector only numeric rooted PCI attributes, an unusable `PATH`, and a process/file trace, then checks exact IDs, no secondary process, and no label-database access |
| system-report nested sensor, thermal, and power-cap directory failures | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_nested_sensor_and_power_directories_keep_issues` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report raw sensor units, thermal trips, partial battery fields, power-cap limits, class control-type filtering, and zone parent links | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_sensor_and_power_sources_keep_raw_units_and_partial_attributes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report power-cap class enumeration failure | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_powercap_enumeration_failure_is_partial` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
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
| system-report process exits and arrivals during the reference bracket | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_process_reference_excludes_exits_and_new_arrivals_from_static_scoring` | The pure bracket comparison scores only identities present and unchanged in both observations; the mapped rooted identity and resource snapshot tests still require the pinned ARM64 image because the old debug binary cannot enumerate rooted `/proc` correctly |
| system-report malformed sensor inputs and power-cap range failures | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_sensor_units_and_powercap_ranges_are_bounded` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report truncated hwmon and thermal names, thermal and battery numeric sources | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_thermal_and_battery_reads_reject_truncated_prefixes` | `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` |
| system-report executable fixture mapping and exact one-test scoring | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_coverage_manifest_contract` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_fixture_definition_check_ignores_comments_and_partial_names` | Manifest schema v4 records `reference_commands` as complete argv vectors and keeps the macOS replay case separate from Linux fixtures. Empty command lists are reserved for rooted process references, and PCI, route, rule, and trace assertions must declare the commands the checker executes. Validation requires each mapped test to have a named procedure or attributed Rust test in its owner source; exact one-test output remains the runtime evidence. In the pinned ARM64 image, run `xsh dev system-report-check --run-fixtures --xsh-bin ABSOLUTE_XSH --xsht-bin ABSOLUTE_XSHT --cargo-bin ABSOLUTE_CARGO`. On macOS, run `xsh dev system-report-check --run-macos-fixtures --xsh-bin ABSOLUTE_XSH --xsht-bin ABSOLUTE_XSHT`; each gate reports the other platform's cases as unexercised |
| system-report traced live, minimal-PATH, malformed-replay, and offline-replay effects | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_traced_live_and_replay_paths_keep_host_effect_contract` | In the pinned ARM64 image with `strace`, the mapped test calls the production syscall audit for live, saved-report, and failure paths. The fixture runner executes a shared test once and reuses its result for each mapped scenario. The old local `xsh` cannot load the current collector, so this focused test fails before any live trace contract can pass |
| system-report syscall trace parser ignores syscall-like text inside file paths | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_trace_ignores_syscall_names_inside_file_paths` | The pure checker identifies the syscall before parsing arguments and checks actual open flags; the pinned production trace is still required for an effects result |
| system-report raw CPU-set capture and collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_set_capture_validates_saved_reference`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_set_capture_records_missing_source`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_cpu_set_capture_replays_raw_sources` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-cpu-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-cpu-bundle NEW_DIRECTORY`; the version-2 metadata binds each raw file to a SHA-256 digest, and the directory is created with mode `0700` outside the tracked tree |
| system-report raw meminfo and THP capture with independent collector replay | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_memory_capture_validates_raw_sources_and_oracles` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_memory_capture_replays_raw_sources` | In the pinned ARM64 image, run `xsh dev system-report-check --capture-memory-bundle NEW_DIRECTORY` followed by `xsh dev system-report-check --replay-memory-bundle NEW_DIRECTORY`; the bundle retains bounded `/proc/meminfo` and available THP policy files with source states, digests, origin, and a separately parsed oracle. A changing live meminfo file is recorded but its saved raw snapshot remains replayable |
| system-report coverage denominator, CPU, swap, storage, and identity reference comparisons, process and host-effect trace parsing, and offline replay source-read detection | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_os_release_reference_decodes_data_and_requires_candidate_agreement`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_swapon_raw_parser_and_comparison_keep_swap_identity`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_swapon_raw_parser_rejects_ambiguous_or_unsafe_rows`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_json_scores_devices_and_layering`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_json_rejects_incomplete_or_unsafe_rows`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_json_preserves_shared_tree_edges` | `xsh dev system-report-check` validates the checked-in manifest; `xsh dev system-report-check --compare-cpu --compare-swaps --compare-storage --compare-modules --compare-identity --compare-namespaces --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets four sysfs CPU identity sets, byte-valued swap areas, an explicit-column all-device `lsblk` tree, loaded modules, uname release and architecture, `/proc/uptime`, OS release ID/version, and eight process-visible namespace links; `xsh dev system-report-check --no-subprocess --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` traces live JSON, replay, and failure paths |
| system-report raw meminfo name, unit, stable byte, and named host-field comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_meminfo_reference_preserves_units_and_rejects_ambiguous_rows` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_meminfo_comparison_scores_stable_fields_and_host_projection` | In the pinned ARM64 image, `xsh dev system-report-check --compare-meminfo --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets one sensitive memory report with bounded `cat /proc/meminfo` observations; changed gauges remain unscored |
| system-report os-release production and independent reference parsers reject malformed assignment and unquoted-value syntax while preserving later valid keys | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_os_release_value_parser_rejects_malformed_assignments`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_os_release_reference_decodes_data_and_requires_candidate_agreement` | In the pinned ARM64 image, `xsh dev system-report-check --compare-identity --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` compares decoded `ID` and `VERSION_ID` from the selected local source |
| system-report VLAN parent links, IPv6 addresses, and policy routing retain prefix, lifetime, nexthop, table, mark/mask, and interface fields | `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_assembles_network_links_addresses_routes_and_rules` | In the pinned ARM64 image, run the full `target/debug/xsht test --jobs 1 tests/xsh/system-report.xsh` gate; full live address, route, and rule differential evidence remains outstanding |
| system-report independent iproute2 link JSON parsing and static interface comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_link_reference_scores_stable_identity_and_state`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_link_reference_checks_master_relationship`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_link_reference_checks_lower_link_relationship` | In the pinned ARM64 image with iproute2, `xsh dev system-report-check --compare-network-links --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive network report with bounded `ip -json -details link show` observations; it compares interface identity, MTU, administrative and operational state, `linkinfo.info_kind`, and master/lower links by interface identity. Flags, hardware type, and counters remain outside this partial comparison, so the full mandatory link assertion is unscored |
| system-report oversized route-netlink link counters retain the link, report an exact-range field issue, and keep enumeration success separate from field completeness and malformed entities | `cargo test -p xsh --lib oversized_link_counters_keep_the_link_and_report_field_failures`, `cargo test -p xsh --lib network_dump_keeps_enumeration_success_separate_from_field_issues`, `cargo test -p xsh --lib malformed_entity_prevents_successful_enumeration_after_all_dumps_finish`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/system-report.xsh::test_system_report_assembles_network_links_addresses_routes_and_rules` | Run the Rust tests and native report gate in the pinned ARM64 image; the local builder exposes only amd64 and the current debug XSH binaries are stale |
| route-netlink dump status framing and kernel error signs | `cargo test -p xsh --lib dump_accumulator_rejects_truncated_status_and_positive_error_codes` | Run in the pinned ARM64 image with the full filtered netlink decoder test module; the local builder exposes only amd64 |
| system-report independent iproute2 address JSON parsing and static IPv4/IPv6 comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_address_reference_preserves_ipv6_identity_and_link_membership` | In the pinned ARM64 image with iproute2, `xsh dev system-report-check --compare-network-addresses --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive network report with bounded `ip -json address show` observations; address identity, interface index, prefix, scope, and broadcast are compared, while changing lifetimes remain unscored and the mandatory address assertion is not yet counted |
| system-report independent iproute2 policy-rule JSON parsing and static IPv4/IPv6 selector comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_rule_reference_keeps_family_and_static_selectors` | In the pinned ARM64 image with iproute2, `xsh dev system-report-check --compare-network-rules --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive network report with bounded `ip -json -family inet rule show` and `ip -json -family inet6 rule show` observations; family, priority, source and destination prefixes, mark/mask, table, action, and interface names are compared. Other selectors and rule attributes remain unscored, so the mandatory rule assertion is not yet counted |
| system-report independent iproute2 route JSON parsing and static IPv4/IPv6 route comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_route_reference_keeps_family_table_and_link_identity` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_ip_route_reference_scores_source_and_multipath_hops` | In the pinned ARM64 image with iproute2, `xsh dev system-report-check --compare-network-routes --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets a sensitive network report with bounded `ip -json -family inet route show table all` and `ip -json -family inet6 route show table all` observations. The comparison covers destination, source selector and prefix, preferred source, table, type, scope, protocol, metric, gateway, output link, known route flags, and multipath link, gateway, weight, and known hop flags. Unknown flag bits prevent an exact candidate score. Other route attributes remain unscored, so the mandatory route assertion is not yet counted |
| system-report independent numeric PCI function comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lspci_vmm_numeric_identity_keeps_repeated_ids_distinct` | In the pinned ARM64 image with pciutils, `xsh dev system-report-check --compare-pci --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets one sensitive PCI report with bounded `lspci -D -vmm -n -k` observations. It compares BDF-scoped numeric IDs, class and program interface when exposed, revision, subsystem IDs, driver, NUMA node, and IOMMU group when exposed. The reference does not establish PCI parent links, optional PCIe link data, or absent optional numeric values, so the mandatory PCI assertions remain unscored. The current worker has no `lspci` binary. |
| system-report trace audit normalizes source paths before privacy and replay checks | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_forbidden_process_read_trace_normalizes_source_paths` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_replay_trace_rejects_normalized_live_source_paths` | In the pinned ARM64 image, run `xsh dev system-report-check --no-subprocess --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT`; the focused parser tests do not establish the production trace result |
| system-report uptime reference brackets a changing whole-second reading and excludes values outside the bracket | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_uptime_reference_requires_a_bracketed_integer_second` | In the pinned ARM64 image, `xsh dev system-report-check --compare-identity --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` captures `/proc/uptime` before and after one candidate identity report |
| system-report independent device-tree reference reads bounded od bytes, enforces NUL-terminated model and compatible values, and scores their order | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_device_tree_od_reference_reads_bounded_raw_source` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_device_tree_reference_requires_exact_terminated_bytes_and_order` | In the pinned ARM64 image with device-tree identity files, `xsh dev system-report-check --compare-identity --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` runs bounded `/usr/bin/od` reads before and after the identity report; absent files remain unscored and changed bytes cannot be scored |
| system-report kernel module reference joins and exact report comparison | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsmod_reference_scores_module_values_and_state` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsmod_reference_rejects_conflicting_or_unsafe_rows` | In the pinned ARM64 image, `xsh dev system-report-check --compare-modules --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets the candidate with `lsmod` and `/proc/modules` snapshots; a changed set or source disagreement cannot be scored |
| system-report raw kernel command-line bytes and default redaction | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_command_line_reference_preserves_bytes_and_redaction` | In the pinned ARM64 image, `xsh dev system-report-check --compare-command-line --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets sensitive and default candidate reports with bounded `cat /proc/cmdline` references |
| system-report fixed sysctl and module-parameter values and absent states | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_parameter_reference_scores_values_and_absence` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_kernel_parameter_reference_rejects_duplicate_or_unexpected_names` | In the pinned ARM64 image, `xsh dev system-report-check --compare-parameters --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets sensitive kernel output with named `sysctl -n` and bounded module-parameter file readings |
| system-report flat findmnt mount identities, relationships, options, and redaction | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_findmnt_json_scores_repeated_targets_and_redaction` and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_findmnt_json_rejects_ambiguous_rows` | In the pinned ARM64 image, `xsh dev system-report-check --compare-mounts --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets the candidate with two all-mount `findmnt` observations using explicit columns; repeated targets remain separate by mount ID |
| system-report independently selected safe mount capacity and explicit skips | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_findmnt_usage_scores_safe_mounts_and_explicit_skips`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_findmnt_usage_rejects_ambiguous_or_unsafe_rows`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_findmnt_usage_keeps_unavailable_capacity_explicit` | In the pinned ARM64 image, `xsh dev system-report-check --compare-mount-usage --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets the candidate with ID-filtered `findmnt --df --bytes` observations for independently eligible local mounts |
| system-report queue reference fields and bounded firmware/stat snapshots | `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_queue_json_scores_supported_fields`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_lsblk_queue_json_rejects_duplicate_and_unsafe_rows`, `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_block_queue_raw_sources_bracket_counters_and_firmware`, and `target/debug/xsht test --exact --jobs 1 dev/tests/test-system-report-check.xsh::test_system_report_block_queue_raw_sources_reject_incomplete_and_unsafe_stats` | In the pinned ARM64 image, `xsh dev system-report-check --compare-queue --xsh-bin ABSOLUTE_XSH --script ABSOLUTE_SYSTEM_REPORT` brackets static `lsblk` fields and bounded sysfs firmware/stat observations; changing in-flight I/O is unscored rather than counted as an exact match |
| Raw host path, argv, environment bytes, bounded rooted reads, rooted directory enumeration, typed rooted symlink observations, and rooted filesystem counters | `target/debug/xsht test --exact --jobs 1 tests/xsh/stdlib/fs_root_children.xsh::test_fs_root_children_reads_newly_created_directory`, `cargo test -p xsh-root --test security readable_directory_open_rejects_files_fifos_and_escapes -- --exact`, and `target/debug/xsht test --exact --jobs 1 tests/xsh/stdlib/fs_root_readlink_result.xsh::test_fs_root_readlink_result_distinguishes_link_absence_and_read_failure` | Native stdlib gate plus the filtered runtime gate; the filesystem-name case skips on macOS because the host returns `EILSEQ` |
| Privileged Linux mount and `switch_root` behavior | In the pinned privileged `xsh-test` image: `cargo test -p xsh --target aarch64-unknown-linux-musl --test linux_priv --features linux-priv-tests linux_priv_mount_and_switch_root_fail_within_private_namespace -- --exact --nocapture` | Run the full `linux_priv` test binary in the same image with the target and feature flags |
| Privileged Linux loop lifecycle | In the pinned privileged `xsh-test` image: `cargo test -p xsh --target aarch64-unknown-linux-musl --test linux_priv --features linux-priv-tests linux_priv_loop_attach_list_and_detach_release_device -- --exact --nocapture` | Run the full `linux_priv` test binary and `xsht test tests/xsh/stdlib/linux.xsh` in that image |
| Linux live process descriptor lifecycle | In the pinned ARM64 `xsh-test` image: `target/debug/xsht test --exact tests/xsh/stdlib/linux.xsh::test_linux_open_files_tracks_a_live_child_descriptor` | The native Linux stdlib gate in the same image |
| One runtime fixture | `cargo test --test integration runtime::TEST_NAME` | the filtered runtime gate above |
| Process, network job, and stream-worker cancellation | `cargo test --test integration --features net runtime::process::sigterm_drains_process_net_job_and_parallel_workers_with_trace_parentage -- --exact` | Run the filtered runtime gate above with `--features net` on macOS and in the pinned Linux image |
| `xsht::cli::CliOutput`, `xsht::grep::find_matches_in_program`, or CLI/tooling | targeted `cargo test -p xsht --test integration cli::TEST_NAME` or `cargo test -p xsht --test integration grep::TEST_NAME` | `cargo test -p xsht --test integration` is owner-run: even the `cli::` group invokes `fmt` and `lint` |
| Copied `xsht` formatter/linter parity on script-backed calls in static and loaded modules | owner-run `cargo test -p xsht --test integration cli::copied_xsht_formats_and_lints_script_backed_calls_in_static_and_loaded_modules -- --exact` | The runnable-corpus gate below and the existing copied-product check/run test |
| Migrated API parity across `xsh` profiles | `cargo test -p xsht --test profile_parity -- --nocapture` after building the four debug/release and default/no-default products | Run the same test and builds in the pinned `Dockerfile.test` ARM64 musl image using `dev/targets.xsh::docker_test_env`; missing products print their build commands |
| Copied product and packaged core smoke | `tools/copied-product-smoke.py` with all three debug binaries and a `dev/release.xsh::package_core` archive | Repeat with the pinned Linux ARM64 musl debug products and `--linux`; `bench/stdlib-port/README.md` gives the commands |
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
| Production executable runtime | targeted `cargo test --test integration runtime::TEST_NAME` | `cargo test -p xsh --test integration runtime:: --features native-tests -- --skip runtime::coverage:: --skip runtime::examples::example_corpus_is_formatted --skip runtime::examples::example_corpus_lints_without_warnings --test-threads=1` plus `cargo test -p xsh --test integration runtime::coverage::xsh_native_tests --features native-tests -- --exact` and `cargo dev bench --fast` |
| Syscall diagnostics | benchmark smoke test on the host | `cargo dev bench --syscalls` on Linux/Docker |
| LLVM IR size | `tools/llvm-lines-repeat-offenders.xsh` over an existing capture | fresh `cargo llvm-lines` capture plus the applicable behavior/benchmark gate |
| API registry/reference/examples | see `API Gate` below | same |
| Broad cross-cutting work | closest targeted tests | relevant filtered package tests; unfiltered `cargo test` is owner-run |
| Ambient filesystem authority policy | `cargo test --test ambient_fs_policy` | relevant filtered tests; unfiltered `cargo test --tests` is owner-run |

Network link, address, route, and rule reference adapters score the fields they
can compare when all four route-netlink dumps and entity decodes finished,
including a partial network section with a separate field issue. An incomplete
enumeration keeps those comparisons unscored. The partial field comparisons do not count as
complete mandatory assertions.

`dev/system-report-coverage.json` maps raw route-netlink multipart, interrupted,
unknown-attribute, and truncated-address scenarios to exact Rust decoder tests
in `src/modules/linux/real/netlink.rs`. In the pinned ARM64 image,
`xsh dev system-report-check --run-fixtures --xsh-bin ABSOLUTE_XSH --xsht-bin ABSOLUTE_XSHT --cargo-bin ABSOLUTE_CARGO`
runs those tests with `cargo test --offline -p xsh --lib --target
aarch64-unknown-linux-musl` and an exact one-test filter. Missing Cargo,
zero-test runs, and failed tests fail the mapped case; manifest validation alone
only proves the test definitions exist.

`dev/tests/test-system-report-check.xsh::test_system_report_coverage_manifest_contract`
checks that the system-report trace audit rejects mixed route-netlink sends
when any decoded message type is outside the collector's four query types. It
also rejects attempted local socket connections, sends to local helper
sockets, query-shaped sends without a proved netlink target, and sends on
inherited sockets with no visible destination. It permits stdout/stderr writes
and rejects writes to other descriptors or positioned writes; only decoded
route-netlink `sendto` queries with an explicit netlink destination are expected
on the collector path. Socket creation is limited to raw or datagram
`NETLINK_ROUTE`; external or local socket families, other netlink protocols,
socket pairs, listeners, and accepts fail the trace audit even if their syscalls
return errors. The checker parses syscall arguments so a netlink marker
inside a payload or quoted socket path cannot justify a send or bind. Other
send forms and connections without an allowed target fail closed. The same
trace audit
rejects attempted reads of per-process environments, command lines, memory,
and open-path sources while allowing `stat`, `status`, and the kernel command
line needed for the report. It checks the syscall's source-path argument and
normalizes `.` and `..` components and repeated separators from the fixed `/`
working directory, so a readlink result or a differently spelled procfs path
cannot hide or invent a process-source read. Saved-report replay applies the
same normalization before rejecting live `/proc`, `/sys`, and `/etc` source
paths. The production `strace` invocation prints up to 4096 characters per
string so paths remain visible to these checks.
The production trace gate also runs missing, malformed, unsupported-schema,
and invalid-UTF-8 replay inputs. Each successful or failing replay trace must
avoid live `/proc`, `/sys`, and `/etc` file and metadata reads. It still needs
the rebuilt collector and controlled denied or malformed live-source
environments before those live failure paths can be counted as verified.

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

As of 2026-09-25, `cargo dev bench --fast` stops while compiling the sibling
`../../rustybench` crate: its `allocator_api` use lacks the feature gate on the
pinned nightly toolchain. The command does not reach XSH's benchmark cases.
Until that sibling build is repaired, record paired workload samples and the
applicable behavior gates directly; a failed benchmark invocation is not a
performance pass.

## Native XSH Test Rule

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

## Runtime Test Modules

Language assertions and dry-run module contracts live in the nearest
`tests/xsh/` module. Rust runtime tests retain CLI, PTY, host fixtures,
process and signal lifecycles, raw bytes, allocation accounting, and small
stack boundaries.

| Area | File |
|---|---|
| collection aliasing and allocation traffic | `tests/xsh/stdlib/methods.xsh`, `tests/xsh/stdlib/map.xsh`, `tests/runtime/collections.rs` |
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
| `run_capture`, `spawn_managed`, and process execution | `tests/xsh/run.xsh`, `tests/xsh/stdlib/process.xsh`, `tests/runtime/process.rs`, `tests/runtime/run.rs` |
| retry blocks | `tests/xsh/retry.xsh` |
| stack depth and explicit lowered frames | `tests/runtime/stack_depth.rs` |
| structured stream behavior | `tests/xsh/stdlib/streams.xsh` |
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
