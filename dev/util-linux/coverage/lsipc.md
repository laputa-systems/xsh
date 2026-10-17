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
features as the pinned image. The coordinator has run the keeper's six-class
reference visibility, unchanged-state, explicit-release, and cancellation
selectors in the guarded image. Candidate parity cases remain open until the
reference and XSH command run on the same owned fixture and their bytes,
statuses, and fixture state are compared.

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

With no resource selector, `lsipc` enables global mode and all six resource
flags. The global collectors emit summary rows for System V queues, shared
memory, semaphores, and POSIX message queues; default output does not list the
registered objects. POSIX shared memory and POSIX semaphores have no global
collector. The help text documents `--global` with `--shmems`, `--queues`, and
`--semaphores`; the 2.42.4 implementation also runs the POSIX message-queue
global collector for `--global --posix-mqueues`. Combining global mode with
POSIX shared memory or POSIX semaphores emits no resource rows.

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

For `--posix-mqueues`, `--posix-shmems`, and `--posix-semaphores`, default
output includes `USEDBYTES,MSGS`, `SIZE`, and `SVAL` respectively, but
explicit `--output` validation rejects those columns: the accepted POSIX range
stops at `MTIME`. The explicit all-column cases therefore cover POSIX common
columns, while error cases preserve this 2.42.4 behavior. Global mode resets
the explicit-column range to generic and summary columns, even when combined
with a resource selector.

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

The output-column parser accepts one trailing comma, but rejects an empty
column list, a leading comma, and repeated commas. Negative IDs below `-1`
retain the reference's enumerate-all behavior for System V queues and
semaphores.

The manual says the default table output can change. Native comparisons pin
the exact 2.42.4 output using the owned fixture. Tests that need a stable schema
use `--output` explicitly.

## Native case map

`core/tests/test-lsipc.xsh` uses these selectors. The source line ranges
identify the upstream behavior each test covers. `ipcutils.c` lines 241–509
enumerate POSIX shared-memory and semaphore names and metadata; lines 625–714
enumerate POSIX message queues. System V collectors read the kernel IPC
tables and query each selected ID.

| Native selector | Behavior under test | Upstream origin | Disposition |
|---|---|---|---|
| `test_lsipc_default_and_global_summary_outputs_match_reference` | Default global-summary selection, summary resource rows, explicit summary columns, and all explicit global selector combinations | `lsipc.c:703-724,1044-1084,1348-1369,1565-1579,1627-1659` | Open. Fixture required. |
| `test_lsipc_default_and_explicit_columns_cover_all_six_resource_modes` | Default columns and every explicitly accepted column for all resource classes; trailing comma acceptance | `lsipc.c:157-203,533-1370`; `ipcutils.c:131-714` | Open. Fixture required. |
| `test_lsipc_id_and_name_select_registered_objects` | Exact ID and name selection, POSIX message-queue slash spelling, and negative-ID enumeration for queues and semaphores | `lsipc.c:559-563,647-648,823-827,958-965,1094-1097,1265-1272`; `ipcutils.c:283-289,492-498,645-700` | Open. Fixture required. |
| `test_lsipc_missing_ids_and_names_preserve_reference_status_and_diagnostics` | Missing IDs and names, successful exit status, and warning diagnostics for all six resource modes | `lsipc.c:559-563,738-741,823-827,958-961,1094-1097,1265-1268,1661-1666` | Open. Capture exact oracle output. |
| `test_lsipc_rejects_id_and_name_without_matching_modes` | Selector prerequisites and option conflicts | `lsipc.c:1415-1420,1565-1570` | Open. Capture exact oracle output. |
| `test_lsipc_creator_and_time_options_report_live_metadata` | Creator/owner identities, operation times, and all supported time formats | `lsipc.c:1591-1617,273-290,457-481` plus the per-mode renderers | Open. Fixture required. |
| `test_lsipc_bytes_and_numeric_permissions_match_reference` | Byte counts and numeric permission rendering in resource and summary output | `lsipc.c:483-526,584-591,906-912,1170-1176,1299-1306,1324-1330` | Open. Fixture required. |
| `test_lsipc_output_modes_and_table_controls_match_reference` | Requested column order and duplicates, list/raw/export/JSON/newline modes, shell names, headings, truncation, and summary rendering | `lsipc.c:128-147,223-243,363-453,1620-1622`; manual output-format sections | Open. Fixture required. |
| `test_lsipc_help_lists_resource_modes_and_columns` | Help output through `--help` and `-h`, option names, and all documented columns | `lsipc.c:292-360,1381-1421` | Open. Stable input. |
| `test_lsipc_version_preserves_xsh_provenance` | XSH identity and exact XSH version banner through `--version` and `-V` | `lsipc.c:1557-1562`; campaign `version-provenance` decision | Open. Assert XSH banner independently. |
| `test_lsipc_short_option_aliases_match_long_options` | Short and long aliases for global mode, IDs, names, output, rendering, and value options | `lsipc.c:1381-1421` | Open. Compare exact output for each spelling. |
| `test_lsipc_rejects_unknown_and_inapplicable_columns` | Unknown and inapplicable columns, default-only POSIX columns, global summary range, and empty/leading/repeated output separators | `lsipc.c:223-243,1526-1535` | Open. Stable input. |
| `test_lsipc_rejects_unknown_time_format` | Invalid time formats, status, and diagnostics | `lsipc.c:273-290` | Open. Stable input. |
| `test_lsipc_rejects_invalid_options_and_missing_arguments` | Unknown options, missing option operands, and malformed IDs | `lsipc.c:1381-1455` | Open. Compare exact diagnostics and status. |
| `test_lsipc_rejects_each_exclusive_option_pair` | Every pair in the four source-defined exclusivity groups | `lsipc.c:1415-1420` | Open. Stable input. |
| `test_lsipc_visibility_matches_for_owner_and_unprivileged_observer` | Per-class visibility for two real UIDs with capabilities dropped | `ipcutils.c:131-240,249-300,342-510,521-575,633-713` | Open. Requires the positive two-UID fixture. |
| `test_lsipc_collectors_leave_registered_resources_unchanged` | IDs, names, permissions, sizes, values, counters, and timestamps remain unchanged after each collector | `lsipc.c:1370-1666`; all six collectors | Open. Keeper state snapshots required. |
| `test_lsipc_keeper_reference_observes_all_classes_and_release_removes_them` | The reference sees each registered ID or reported name, leaves every seeded state unchanged, and sees no record after successful release | `lsipc.c:559-563,823-827,958-965,1094-1097,1265-1272,1661-1666`; all six collectors | Guarded reference-only proof passed; candidate comparison remains open. |
| `test_lsipc_keeper_cancellation_removes_registered_resources` | `SIGTERM` cleanup removes only the keeper's six registered resource classes | fixture cleanup handlers; `ipcutils.c:131-714` | Guarded reference-only cancellation proof passed; candidate comparison remains open. |

Permission visibility follows each collector: System V collectors read procfs
records, POSIX shared memory reads directory metadata, and POSIX semaphore and
message-queue collectors attempt read access to each object. Exercise both
UIDs and compare against the reference rather than assuming one rule applies
to every resource class.

Use `LC_ALL=C` and `TZ=UTC` for case runs. Preserve exact stdout and stderr
bytes. Keep JSON structural assertions beside raw comparisons. Do not sort
rows or normalize spacing, nulls, numbers, escaping, or diagnostics. Record
only observed PID or ID variation in the comparison rules.

The fixture snapshot also checks the seeded kernel state before any parity
case: queue message counts, byte use, sender and receiver PIDs and times;
shared-memory size, attachment count, creator and last-operation PIDs and
times; semaphore count, values and operation time; POSIX queue count and times;
POSIX shared-memory owner, mode, size and times; and POSIX semaphore owner,
mode, value and times. The shared-memory command-column case checks the live
keeper PID and path.

## Fixture boundary

The foundation-owned runner enters private IPC, mount, and PID namespaces. It
mounts fresh procfs, private `/dev/shm` backing, and a private mqueue filesystem
before it creates POSIX resources. Each comparison runs the reference and
candidate against the same fixture and namespace.

The fixture must keep a registry of returned System V IDs and exact POSIX
names. It must never find resources by scanning a host table or path. Give one
owner exclusive cleanup responsibility for that registry. The owner removes
only registered IDs and names after success, setup failure, assertion failure,
timeout, and cancellation.

Keep at most 16 objects per resource class and at most 1 MiB per object. Track
each exact System V ID returned by the keeper and each unique run-scoped POSIX
name. The permission cases need two actual non-root UIDs, no capabilities, and
objects with distinct owners and modes. Running only as root does not execute
these cases.

The coordinator owns private namespaces, mounts, and the shared keeper process
API. This lane's `keeper.py` uses Python ctypes to create the six resource
classes, seed activity, set ownership and modes, hold the creator alive, and
remove only registered IDs and names. It performs permission-checked
operations, snapshots, and removal under each resource owner's effective UID.
It reads each System V status under the creator UID before `IPC_SET`, preserving
fields such as the queue byte limit while it changes owner and mode. Native
tests use the shared scene API to run the keeper and reference. Parity cases
also run the XSH command in that namespace.
