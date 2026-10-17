# lsipc behavior map

## Reference

Use util-linux 2.42.4 from the pinned `xsh-test` image as the executable
oracle. The upstream release has no dedicated `tests/ts/lsipc` suite, so this
lane adds the native cases below instead of importing upstream test IDs.

The behavior source is [`sys-utils/lsipc.c`](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/lsipc.c).
It defines the options and columns at lines 1381–1421 and 1433–1666. The
resource collectors live in [`sys-utils/ipcutils.c`](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/ipcutils.c).
The command contract is in [`sys-utils/lsipc.1.adoc`](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/lsipc.1.adoc).
`sys-utils/Makemodule.am` links `lsipc.c` with `ipcutils.c` and
`ipcutils.h`.

The oracle build must report util-linux 2.42.4 and the same enabled POSIX IPC
features as the pinned image. No command comparison has run from this lane.
Every case remains open until the reference and XSH command run on the same
owned fixture and their bytes, statuses, and fixture state are compared.

## Resource modes and default columns

One resource option selects one mode. Without a resource option, `lsipc`
selects all six modes and shows global summaries. An explicit `--global` may
combine with System V modes. `--global` without a resource option covers the
reference's default resource set.

| Selector | Resource | Default columns |
|---|---|---|
| `--queues` | System V message queues | `KEY,ID,PERMS,OWNER,USEDBYTES,MSGS,LSPID,LRPID` |
| `--shmems` | System V shared memory | `KEY,ID,PERMS,OWNER,SIZE,NATTCH,STATUS,CTIME,CPID,LPID,COMMAND` |
| `--semaphores` | System V semaphore sets | `KEY,ID,PERMS,OWNER,NSEMS` |
| `--posix-mqueues` | POSIX message queues | `NAME,PERMS,OWNER,MTIME,USEDBYTES,MSGS` |
| `--posix-shmems` | POSIX shared memory objects | `NAME,PERMS,OWNER,SIZE,MTIME` |
| `--posix-semaphores` | POSIX named semaphores | `NAME,PERMS,OWNER,MTIME,SVAL` |
| `--global` | Summary rows | `RESOURCE,DESCRIPTION,LIMIT,USED,USE%` |

The default selection includes all six resource modes, but global collectors
emit rows only for System V queues, shared memory, semaphores, and POSIX message
queues. POSIX shared memory and POSIX semaphore modes have no global collector.
Explicit `--global` can be combined with a resource selector; the same four
collectors determine which rows appear.

The column source defines these complete ranges:

| Range | Columns |
|---|---|
| System V common | `KEY,ID,OWNER,PERMS,CUID,CUSER,CGID,CGROUP,UID,USER,GID,GROUP,CTIME` |
| POSIX common | `NAME,OWNER,PERMS,CUID,CUSER,CGID,CGROUP,MTIME` |
| System V message queue | `USEDBYTES,MSGS,SEND,RECV,LSPID,LRPID` |
| System V shared memory | `SIZE,NATTCH,STATUS,ATTACH,DETACH,COMMAND,CPID,LPID` |
| System V semaphore | `NSEMS,OTIME` |
| Summary | `RESOURCE,DESCRIPTION,LIMIT,USED,USE%` |
| POSIX semaphore | `SVAL` |

`--creator` adds `CUID,CGID` and, for System V resources, `UID,GID`.
`--time` adds System V queue send/receive/create times, shared-memory attach
and detach times, or semaphore operation/create times. `--time-format` accepts
`short`, `full`, and `iso`. Without an explicit time format, output uses the
full format for implicit pretty detail output selected by `--id` or `--name`,
and the short format otherwise.

## Options

The reference accepts these options:

| Family | Options |
|---|---|
| Resource selection | `-m, --shmems`; `-M, --posix-shmems`; `-q, --queues`; `-Q, --posix-mqueues`; `-s, --semaphores`; `-S, --posix-semaphores`; `-g, --global` |
| Object selection | `-i, --id ID`; `-N, --name NAME` |
| Columns | `-o, --output LIST`; `--noheadings`; `--notruncate` |
| Rendering | `-e, --export`; `-J, --json`; `-l, --list`; `-n, --newline`; `-r, --raw`; `-y, --shell` |
| Values | `-b, --bytes`; `-c, --creator`; `-P, --numeric-perms`; `-t, --time`; `--time-format TYPE` |
| Generic | `-h, --help`; `-V, --version` |

The reference rejects more than one output mode from `--json`, `--export`,
`--list`, `--newline`, and `--raw`. It rejects conflicts among `--name`,
`--global`, and `--id`; among `--creator`, `--output`, and `--time`; and among
the six resource selectors. These are source-defined option groups, not
assumed last-option-wins behavior.

The manual says the default table output can change. The native corpus will
pin the exact 2.42.4 output after the reference fixture can run. Tests that need
a stable schema use `--output` explicitly.

## Native case map

`core/tests/test-lsipc.xsh` will use these selectors. The source line ranges
identify the upstream behavior each test covers. `ipcutils.c` lines 241–509
enumerate POSIX shared-memory and semaphore names and metadata; lines 625–714
enumerate POSIX message queues. System V collectors read the kernel IPC
tables and query each selected ID.

| Native selector | Behavior under test | Upstream origin | Disposition |
|---|---|---|---|
| `test_lsipc_default_selects_all_six_modes` | Default resource selection and global summary rows | `lsipc.c:1565-1579,1627-1661` | Open. Fixture required. |
| `test_lsipc_lists_system_v_message_queue_columns` | Queue defaults and every applicable explicit column | `lsipc.c:176-182,785-944` | Open. Fixture required. |
| `test_lsipc_lists_system_v_shared_memory_columns` | Shared-memory defaults, status, attachment counts, PIDs, command, and every applicable column | `lsipc.c:183-191,1086-1252`; `ipcutils.c:131-240` | Open. Fixture required. |
| `test_lsipc_lists_system_v_semaphore_columns` | Semaphore defaults, every applicable column, and per-element detail rows | `lsipc.c:192-194,533-700` | Open. Fixture required. |
| `test_lsipc_lists_posix_message_queue_columns` | Queue names, size, message count, ownership, permissions, and times | `lsipc.c:945-1043`; `ipcutils.c:625-714` | Open. Fixture required. |
| `test_lsipc_lists_posix_shared_memory_columns` | Shared-memory names, size, ownership, permissions, and modification time | `lsipc.c:1253-1347,ipcutils.c:241-301` | Open. Fixture required. |
| `test_lsipc_lists_posix_semaphore_columns` | Semaphore names, values, ownership, permissions, and modification time | `lsipc.c:725-814,1526-1534`; `ipcutils.c:445-510` | Open. Fixture required. |
| `test_lsipc_id_selects_only_the_requested_system_v_object` | Exact ID selection for each System V mode | `lsipc.c:559-563,823-827,1094-1097,1439-1443,1629-1655` | Open. Fixture required. |
| `test_lsipc_name_selects_only_the_requested_posix_object` | Exact name selection, including the leading slash required for POSIX message queues | `lsipc.c:945-965,1253-1272`; `ipcutils.c:283-289,492-498,645-648,694-700` | Open. Fixture required. |
| `test_lsipc_missing_id_and_name_report_reference_errors` | Missing IDs and names, status, and diagnostics | `lsipc.c:559-563,738-741,823-827,958-961,1094-1097,1265-1268` | Open. Capture exact oracle output. |
| `test_lsipc_rejects_id_and_name_without_a_resource_mode` | Selector prerequisites and option conflicts | `lsipc.c:1415-1420,1565-1570` | Open. Capture exact oracle output. |
| `test_lsipc_global_reports_limits_and_usage` | Global limits, used values, percentages, and output types | `lsipc.c:483-531,703-724,1044-1084,1348-1369,1565-1579,1627-1661` | Open. Fixture and kernel limits required. |
| `test_lsipc_creator_columns_distinguish_owner_and_creator` | Creator UID/GID and current owner UID/GID for each applicable mode | `lsipc.c:1591-1597` plus the per-mode renderers | Open. Fixture required. |
| `test_lsipc_time_columns_cover_each_operation` | Creation, message send/receive, shared-memory attach/detach, and semaphore operation times | `lsipc.c:1599-1617` plus the per-mode renderers | Open. Fixture required. |
| `test_lsipc_time_format_accepts_short_full_and_iso` | Exact date formatting for all three supported values | `lsipc.c:273-290,457-481`; manual `--time-format` | Open. Fixture required. |
| `test_lsipc_bytes_switches_size_columns_to_integer_bytes` | Byte counts in resource and summary output | `lsipc.c:483-526,906-912,1170-1176,1324-1330` | Open. Fixture required. |
| `test_lsipc_numeric_permissions_formats_mode_bits` | Numeric `PERMS` output versus symbolic mode output | `lsipc.c:584-591,1299-1306` plus other mode renderers | Open. Fixture required. |
| `test_lsipc_output_columns_preserve_requested_order` | Column-name matching, custom order, duplicate columns, and mode-specific rejection | `lsipc.c:223-243,1620-1622` | Open. Fixture required. |
| `test_lsipc_output_formats_preserve_types_and_bytes` | List, raw, export, JSON, and newline formats; exact headers, escaping, nulls, and JSON value types | `lsipc.c:128-147,363-453`; manual output-format sections | Open. Fixture required. |
| `test_lsipc_noheadings_and_notruncate_change_only_table_shape` | Heading suppression and long-value truncation | `lsipc.c:363-411`; manual output-format sections | Open. Fixture required. |
| `test_lsipc_shell_names_export_columns` | `--shell` column identifiers with export output | `lsipc.c:139-147,363-391`; manual `--shell` | Open. Fixture required. |
| `test_lsipc_help_lists_resource_modes_and_columns` | Help output, option names, and all documented columns | `lsipc.c:292-360` | Open. Stable input. |
| `test_lsipc_version_preserves_xsh_provenance` | XSH identity and exact XSH version banner | `lsipc.c:1557-1562`; campaign `version-provenance` decision | Open. Assert XSH banner independently. |
| `test_lsipc_rejects_unknown_columns_and_time_formats` | Unknown and inapplicable columns, invalid time formats, statuses, and diagnostics | `lsipc.c:223-243,273-290` | Open. Stable input. |
| `test_lsipc_rejects_each_exclusive_option_pair` | Every pair in the four source-defined exclusivity groups | `lsipc.c:1415-1420` | Open. Stable input. |
| `test_lsipc_matches_reference_visibility_for_owner_and_non_owner` | Per-class visibility for two real UIDs with capabilities dropped | `ipcutils.c:131-240,249-300,342-510,521-575,633-713` | Open. Requires the positive two-UID fixture. |
| `test_lsipc_does_not_change_owned_resources` | Fixture IDs, names, permissions, sizes, values, and timestamps remain unchanged after listing | `lsipc.c:1370-1666`; all six collectors | Open. Fixture snapshot required. |

Permission visibility follows each collector: System V collectors read procfs
records, POSIX shared memory reads directory metadata, and POSIX semaphore and
message-queue collectors attempt read access to each object. Exercise both
UIDs and compare against the reference rather than assuming one rule applies
to every resource class.

Use `LC_ALL=C` and `TZ=UTC` for case runs. Preserve exact stdout and stderr
bytes. Keep JSON structural assertions beside raw comparisons. Do not sort
rows or normalize spacing, nulls, numbers, escaping, or diagnostics. Record
only observed PID or ID variation in the comparison rules.

## Fixture boundary

The parent-owned fixture must enter private IPC and mount namespaces. It must
mount fresh procfs, private `/dev/shm` backing, and a private mqueue filesystem
before it creates POSIX resources. The pinned util-linux 2.42.4 `ipcmk` can
create each of the six resource classes, and `ipcrm` can remove an exact
recorded ID or name. Each comparison runs the reference and candidate against
the same fixture and namespace.

The fixture must keep a registry of returned System V IDs and exact POSIX
names. It must never find resources by scanning a host table or path. Give one
owner exclusive cleanup responsibility for that registry. The owner removes
only registered IDs and names after success, setup failure, assertion failure,
timeout, and cancellation.

Keep at most 16 objects per resource class and at most 1 MiB per object. Track
the exact System V ID returned by `ipcmk`; its source creates those objects
with generated keys. Give each POSIX object a unique run-scoped name. The
permission cases need two actual non-root UIDs, no
capabilities, and objects with distinct owners and modes. Running only as root
does not execute these cases.

Foundation owns private namespaces, mount setup, and keeper lifecycle. This
lane's `keeper.py` uses Python ctypes in the pinned test image to create the
six classes, seed stats and ownership, hold the creator alive, and remove only
registered IDs and names. Native tests use the shared scene API to run the
keeper, reference, and candidate in the same namespace.
