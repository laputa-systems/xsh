# lsmem reference coverage

The reference is util-linux **v2.42.4**. The behavioral inventory comes from its
[implementation](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/lsmem.c),
[manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/lsmem.1.adoc),
and [native integration test](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/lsmem/lsmem).

The upstream test runs two sysfs dumps with nine invocations each. The native
tests preserve those argument lists and compare status, stdout, and stderr to
the pinned reference:

| Upstream case | Inputs |
|---|---|
| `--split=STATE,REMOVABLE` | `s390-zvm-6g`, `x86_64-16g` |
| `--output RANGE,SIZE --split none` | both dumps |
| `--output RANGE,SIZE,STATE --split STATE` | both dumps |
| `--all --output RANGE,SIZE,STATE,REMOVABLE,BLOCK,NODE,ZONES` | both dumps |
| `--raw --output RANGE,SIZE,STATE,REMOVABLE,BLOCK,NODE,ZONES --split RANGE,SIZE,STATE,REMOVABLE,BLOCK,NODE,ZONES` | both dumps |
| `--json --output RANGE,SIZE,STATE,REMOVABLE,BLOCK,NODE,ZONES --split RANGE,SIZE,STATE,REMOVABLE,BLOCK,NODE,ZONES` | both dumps |
| `-o +ZONES` | both dumps |
| `--output-all` | both dumps |
| no options | both dumps |

The imported archives are [`s390-zvm-6g.tar.bz2`](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/lsmem/dumps/s390-zvm-6g.tar.bz2)
and [`x86_64-16g.tar.bz2`](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/lsmem/dumps/x86_64-16g.tar.bz2), stored in
`core/tests/fixtures/util-linux/memory/upstream/`. They retain the official
sysfs content, including sparse memory IDs and node symlinks. The upstream
matrix is only a baseline; it does not cover the complete option surface.

## Option inventory

| Option | Reference behavior and native coverage |
|---|---|
| `-a`, `--all` | List each block; upstream matrix and sparse-root cases |
| `-b`, `--bytes` | Print byte sizes; sparse-root and configured-root cases |
| `-h`, `--help` | Print help and exit successfully; both forms are compared |
| `-J`, `--json` | JSON output; upstream, sparse-root, and configured-root cases |
| `-n`, `--noheadings` | Omit headings; sparse-root cases |
| `-o`, `--output LIST` | Select or extend columns; upstream and rooted cases |
| `--output-all` | Request every available column |
| `-P`, `--pairs` | Key/value output; sparse-root and configured-root cases |
| `-r`, `--raw` | Raw output; upstream, sparse-root, and configured-root cases |
| `-S`, `--split LIST` | Select merge attributes; upstream and state/node/zone/configured-root cases |
| `-s`, `--sysroot DIR` | Read under an alternate root; rooted cases cover separated arguments, and `test_lsmem_rooted_non_utf8_sysroot_arguments_match_reference` compares raw non-UTF-8 paths attached to `--sysroot=` and `-s`, plus separated raw and ordinary paths |
| `--summary[=when]` | `when` may be `never`, `always`, or `only`; all modes and the bare form use the sparse fixture |
| `-V`, `--version` | Print version; both forms use the separate identity assertion |

## Source behavior inventory

| Area | util-linux v2.42.4 contract |
|---|---|
| Discovery | `--sysroot` prefixes `/sys/devices/system/memory`; `block_size_bytes` is required. The command accepts `memory` directories with decimal numeric suffixes, sorted by version. `/sys/firmware/memory` and the global hotplug parameter are optional. |
| Attributes | The block size is hexadecimal. `state` maps `online`, `offline`, and `going-offline` to `online`, `offline`, and `on->off`; absent or unrecognized state remains unknown. `removable` is shown only for online blocks. Node membership comes from `nodeN` entries, including sysfs symlinks. Zones are read from `valid_zones`; an unrecognized zone prints as `Unknown`. |
| Configured memory | The presence of `sys/firmware/memory/memory0` enables configuration inventory. `config` and `memmap_on_memory` control `CONFIGURED` and `MEMMAP-ON-MEMORY`; an unconfigured block is offline. The summary may show the global `memmap_on_memory` value `Y`, `N`, or `force`. |
| Ranges and merging | A range end is inclusive. Adjacent blocks merge unless a selected split attribute differs. By default, output columns also select split attributes; `--split=none` ignores attributes, while `--all` keeps each block separate. |
| Columns | Supported columns are `RANGE`, `SIZE`, `STATE`, `REMOVABLE`, `BLOCK`, `NODE`, `ZONES`, `CONFIGURED`, and `MEMMAP-ON-MEMORY`. Default columns are `RANGE,SIZE,STATE,REMOVABLE,BLOCK`. `--output-all` requests every column; `+LIST` extends defaults; `LSMEM_COLUMNS` supplies the output list when `--output` is absent. |
| Formats and summaries | `--raw`, `--pairs`, and `--json` select mutually exclusive output formats and suppress summaries. `--bytes` changes size and summary values. `--noheadings` removes headings. `--summary=never`, `always`, or `only` controls summary output; bare `--summary` means only. |
| Option errors | Positional operands are rejected. `--all` conflicts with `--split`. `--json`, `--pairs`, and `--raw` conflict with each other. A format-only output conflicts with `--summary=only`. Invalid summary arguments and unknown output columns fail. |
| Identity | `--help` is compared to the pinned reference. `--version` uses the narrow identity assertion so the reference and XSH version banners are checked independently. |

## Rooted fixtures and native cases

Every applet comparison runs the reference and XSH candidate against the same
temporary scene. The `sparse` fixture uses a 4 KiB block size and an offline
first block, which conflict with the live host values. It includes a gap in
block numbers, numeric ordering (`memory2` before `memory10`), adjacent blocks
that differ by state, node, or zone, a missing state and removable attribute,
malformed state/removable/zone contents, an internal state symlink, and the
`nodeN` symlinks used by sysfs. The `malformed-block-size` fixture also
exercises invalid block-size text; `missing-block-size` exercises the
required-file error. An empty scene is passed as `--sysroot` to ensure a missing
sysfs tree cannot fall back to host memory.

The `configured` fixture has system and firmware memory entries, both configured
and unconfigured blocks, per-block memmap values, node and zone data, and a
global `force` parameter. The native test changes only the copied scene's global
parameter to also compare `Y` and `N` summary output. The `no-numeric-blocks`
fixture contains only a nonnumeric `memory` directory. For this fixture alone,
the oracle requires status 1, empty stdout, and the exact diagnostic prefix
through the rooted memory path; util-linux's zero-entry `scandir` error reports
stale `errno`, so only the suffix after that prefix may vary. Synthetic controls
reject status, path, prefix, argument, and stdout changes, while the general
util-linux comparator remains byte-exact. The `nonnumeric-entry`
fixture adds a valid numeric block beside a nonnumeric `memory` directory; its
native case verifies the latter does not hide the block.

`test_lsmem_rooted_non_utf8_sysroot_arguments_match_reference` copies the sparse
fixture under a path containing an undecodable byte and compares attached long
and short sysroot arguments plus separated raw and ordinary path arguments.
Terminal table output at 24 and 120 columns is compared by the shared
`core/tests/test-util-linux-contexts.xsh::test_util_linux_context_terminal_rooted_memory`
case.

| Native test | Coverage |
|---|---|
| `test_lsmem_util_linux_2_42_4_upstream_s390_zvm_6g` | Nine upstream invocations on the s390 dump |
| `test_lsmem_util_linux_2_42_4_upstream_x86_64_16g` | Nine upstream invocations on the x86 dump |
| `test_lsmem_util_linux_2_42_4_version_identity` | Independent exact version identities for `--version` and `-V` |
| `test_lsmem_util_linux_2_42_4_help_matches_reference` | Help output for `--help` and `-h` |
| `test_lsmem_rooted_sparse_memory_attributes_and_output_modes` | Fixture-backed range discovery, merging, columns, output formats, bytes, headings, and summary modes |
| `test_lsmem_rooted_columns_environment_and_explicit_output_precedence` | `LSMEM_COLUMNS`, invalid environment columns, explicit `--output`, and `--output-all` |
| `test_lsmem_rooted_short_option_aliases` | Short aliases for the supported output, byte, split, and sysroot options |
| `test_lsmem_rooted_non_utf8_sysroot_arguments_match_reference` | Raw non-UTF-8 sysroot values attached to `--sysroot=` and `-s`, with separated raw and ordinary path controls |
| `test_lsmem_rooted_configured_memory_and_memmap_summary` | Firmware configuration, per-block memmap, and global summary modes |
| `test_lsmem_rooted_malformed_block_attributes` | Invalid hexadecimal block size and malformed block attributes |
| `test_lsmem_rooted_numeric_and_state_parsing_matches_reference` | Block-size syntax, exact state matching, and removable-value parsing |
| `test_lsmem_rooted_zone_tokenization_and_invalid_bytes_match_reference` | Space-delimited zones, tabs, leading whitespace, and invalid UTF-8 |
| `test_lsmem_rooted_dangling_and_cyclic_attribute_symlinks_match_reference` | Unreadable state, removable, and zone symlink targets and directories |
| `test_lsmem_rooted_required_block_size_wrong_types_match_reference` | Required block-size directory and dangling-symlink errors |
| `test_lsmem_rooted_firmware_inventory_can_differ_from_sysfs_blocks` | Firmware-only memory entry across table, JSON, and summary output |
| `test_lsmem_rooted_firmware_integer_parsing_matches_reference` | Firmware config and memmap parsing with signs, suffixes, and malformed values |
| `test_lsmem_rooted_leading_zero_block_ids_follow_reference_order` | Numeric IDs with leading zeroes and version ordering |
| `test_lsmem_rooted_missing_block_size_is_an_error` | Missing required block-size attribute |
| `test_lsmem_rooted_nonnumeric_memory_entries_match_reference` | Zero numeric entries with exact status and stable diagnostic prefix; stale `errno` suffix variation is confined to this case |
| `test_lsmem_rooted_nonnumeric_memory_entry_does_not_hide_numeric_block` | Nonnumeric `memory` directory is ignored alongside a valid numeric block |
| `test_lsmem_rooted_missing_sysfs_tree_does_not_use_host_memory` | Missing rooted sysfs data fails without host fallback |
| `test_lsmem_rooted_option_errors_match_reference` | Format conflicts, invalid arguments, unknown columns/options, and positional operands |
| `core/tests/test-util-linux-contexts.xsh::test_util_linux_context_terminal_rooted_memory` | Exact reference/candidate table bytes at terminal widths 24 and 120 |

The upstream calls and added rooted cases are differential oracles, not recorded
expected transcripts. The cases compare each candidate invocation against the
pinned reference using the same scene and arguments. They cover deterministic
malformed files, wrong file types, and dangling or cyclic links. They do not
assert a result while sysfs changes concurrently: util-linux does not read a
coherent snapshot, so a sequential differential comparison cannot define that
behavior. Root descriptor race and confinement behavior is tested at the
filesystem boundary instead. Before feature optimization, review and resolve
every observed mismatch against the pinned reference; no host-derived output is
an oracle.
