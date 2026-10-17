# `lslocks` behavior inventory

Reference: util-linux 2.42.4, as installed in the pinned `xsh-test` image.
The command is absent from the campaign base. The XSH banner keeps the
project's existing provenance and is the sole approved output difference.

## Reference sources

| Source | URL | Use |
|---|---|---|
| Command implementation | <https://raw.githubusercontent.com/util-linux/util-linux/v2.42.4/misc-utils/lslocks.c> | Options, data collection, fallback paths, filter types, JSON schema, and columns. |
| Manual | <https://raw.githubusercontent.com/util-linux/util-linux/v2.42.4/misc-utils/lslocks.8.adoc> | Public options, columns, environment, lock identity notes, and documented behavior. |
| Upstream regression test | <https://raw.githubusercontent.com/util-linux/util-linux/v2.42.4/tests/ts/lslocks/lslocks> | The 15 named subcases below. |
| Upstream fixture helper | <https://raw.githubusercontent.com/util-linux/util-linux/v2.42.4/tests/helpers/test_mkfds.c> | Exact lock ranges, file sizes, descriptor duplication, and lock acquisition. |
| Filter grammar | <https://raw.githubusercontent.com/util-linux/util-linux/v2.42.4/libsmartcols/scols-filter.5.adoc> | Filter syntax, coercion, and unsigned numeric limits. |
| Size option text | <https://raw.githubusercontent.com/util-linux/util-linux/v2.42.4/man-common/in-bytes.adoc> | Human-readable base-1024 size suffixes. |
| Help and version text | <https://raw.githubusercontent.com/util-linux/util-linux/v2.42.4/man-common/help-version.adoc> | `-h` and `-V`. |
| Smartcols environment | <https://raw.githubusercontent.com/util-linux/util-linux/v2.42.4/man-common/env-smartcols.adoc> | `LIBSMARTCOLS_JSON`. |

The Git tag is fixed at `v2.42.4`. Do not use the moving `master` manual as the
oracle. In particular, `--pager`, `--nopager`, `PAGER`, and `PAGER_ENABLE` do
not exist in this tag's `lslocks.c` or manual.

## Upstream cases

The upstream script creates one regular file and asks `test_mkfds` to hold it.
It checks six raw, heading-free columns and the `PATH` column for nine lock
forms. It then duplicates the descriptor and checks `HOLDERS` for six forms.
The test controls readiness by reading the holder PID before invoking
`lslocks`; its cleanup sends `DONE` and waits for the helper.

| Upstream subcase ID | Lock and covered output | Native selector |
|---|---|---|
| `flock-sh` | Shared `FLOCK`; `COMMAND,TYPE,SIZE,MODE,START,END`; `PATH`. | `test_lslocks_upstream_flock_sh` |
| `flock-ex` | Exclusive `FLOCK`; same columns and `PATH`. | `test_lslocks_upstream_flock_ex` |
| `posix-r-` | POSIX read lock; same columns and `PATH`. | `test_lslocks_upstream_posix_read` |
| `posix--w` | POSIX write lock; same columns and `PATH`. | `test_lslocks_upstream_posix_write` |
| `posix-rw` | POSIX read lock on bytes `[0,1)` and write lock on `[2,3)`; 3-byte file. | `test_lslocks_upstream_posix_read_write` |
| `ofd-r-` | OFD read lock; same columns and `PATH`. | `test_lslocks_upstream_ofd_read` |
| `ofd--w` | OFD write lock; same columns and `PATH`. | `test_lslocks_upstream_ofd_write` |
| `ofd-rw` | OFD read lock on bytes `[0,1)` and write lock on `[2,3)`; 3-byte file. | `test_lslocks_upstream_ofd_read_write` |
| `lease-w` | Write lease; same columns and `PATH`. | `test_lslocks_upstream_lease_write` |
| `flock-sh+HOLDERS` | Duplicated shared `FLOCK`; includes both descriptor holders. | `test_lslocks_upstream_flock_sh_holders` |
| `flock-ex+HOLDERS` | Duplicated exclusive `FLOCK`; includes both descriptor holders. | `test_lslocks_upstream_flock_ex_holders` |
| `ofd-r-+HOLDERS` | Duplicated OFD read lock; includes both descriptor holders. | `test_lslocks_upstream_ofd_read_holders` |
| `ofd--w+HOLDERS` | Duplicated OFD write lock; includes both descriptor holders. | `test_lslocks_upstream_ofd_write_holders` |
| `ofd-rw+HOLDERS` | Duplicated OFD read/write lock request; includes both descriptor holders. | `test_lslocks_upstream_ofd_read_write_holders` |
| `lease-w+HOLDERS` | Duplicated write lease; includes both descriptor holders. | `test_lslocks_upstream_lease_write_holders` |

The upstream suite does not cover POSIX `HOLDERS`, blocked requests, byte-range
boundaries, JSON, filters, output selection, permission failures, deleted
paths, owner disappearance, or concurrent host locks. The added native corpus
must cover those behaviors. A test that is not run remains open.

## Command surface

| Option | Reference behavior to assert |
|---|---|
| `-b`, `--bytes` | Render `SIZE` as bytes. JSON `SIZE` becomes a number. Without it, sizes use powers of 1024 and one-letter suffixes; JSON `SIZE` is a string. |
| `-H`, `--list-columns` | List all 13 selectable columns; `--json` and `--raw` select machine-readable list modes. `SIZE` advertises `<string|number>`. |
| `-i`, `--noinaccessible` | Drop a lock row when the command cannot discover a path for it. |
| `-J`, `--json` | Emit a smartcols object named `locks`; JSON types follow the column schema below. |
| `-n`, `--noheadings` | Omit headings in table output. |
| `-o`, `--output LIST` | Select a comma-separated, case-insensitive column list. `+LIST` extends the default columns. Unknown and repeated columns need exact status and diagnostic assertions. |
| `--output-all` | Select all 13 columns in their declared order. |
| `-p`, `--pid PID` | Keep rows whose reported PID equals `PID`, including blocked rows whose PID is the waiter. Under `-Q`, OFD PID `-1` cannot be selected because filter numbers are unsigned. |
| `-Q`, `--filter EXPR` | Apply an experimental libsmartcols filter. Cover string and numeric comparisons, regex, logical, unknown-column, malformed-expression, and unselected-column cases. Numeric filter values are non-negative; OFD PID `-1` is not filterable. |
| `-r`, `--raw` | Emit raw table output. `-r` and `-J` are mutually exclusive and must fail together. |
| `-u`, `--notruncate` | Disable text-column truncation, especially for `PATH`. |
| `-h`, `--help` | Write help to stdout and exit successfully. Keep the exact pinned help as reference output. |
| `-V`, `--version` | Exit successfully. Preserve util-linux identity on the reference and the exact `gnu.version_text("lslocks") + "\n"` identity on XSH. |

The command has no documented operands. The reference uses `getopt_long` and
does not inspect remaining operands; record the exact behavior for stray words.
The parser uses the process locale. Table width, truncation, padding, and
`HOLDERS` wrapping also depend on terminal dimensions and must be compared in
the same captured-terminal context. Raw and JSON cases use the same bytes on
both sides without terminal normalization.

## Columns and values

The default columns are `COMMAND,PID,TYPE,SIZE,MODE,M,START,END,PATH`.
`--output-all` selects all 13 columns:

| Column | Reference value and JSON type |
|---|---|
| `COMMAND` | Process command name; JSON string. |
| `PID` | Holder or waiter PID, with `-1` possible for OFD locks; JSON number. |
| `TYPE` | `LEASE`, `FLOCK`, `POSIX`, or `OFDLCK`; JSON string. |
| `SIZE` | File size when a descriptor path is found; absent data is blank in a table and null in JSON. JSON uses a string by default and a number with `--bytes`. |
| `INODE` | Inode number; JSON number. |
| `MAJ:MIN` | Device major and minor number; JSON string. Table output includes smartcols padding. |
| `MODE` | `READ` or `WRITE`; append `*` for a blocked waiter. JSON string. |
| `M` | Mandatory-lock flag; JSON boolean. |
| `START` | Relative start byte; `0` for kernel `EOF`; JSON number. |
| `END` | Inclusive ending byte; `0` for kernel `EOF`; JSON number. |
| `PATH` | Full path when found. If path lookup fails, use the matching mountpoint plus `...` when available. JSON string. |
| `BLOCKER` | PID of the holder that blocks a queued request; JSON number when present and null when absent. |
| `HOLDERS` | Matching `(PID,COMMAND,FD)` rows separated by newlines in table output; JSON array of strings. |

`HOLDERS` groups lock records by device, inode, type, mode, range, mandatory
state, and blocked state. A single open file description may appear through
more than one descriptor or process. `COMMAND` and `PID` show one reported
holder, so do not infer unique ownership for `FLOCK`, OFD, or lease locks.

## Behavior case map

Each row is required native coverage with a stable selector. A selector is not
covered until the test exists and passes in its qualified environment. The
reference and candidate use the same private fixture. Compare stdout bytes,
stderr bytes, exit status, and fixture state. Do not normalize output ordering,
whitespace, JSON types, nulls, or paths. Normalize a PID, inode, or device only
if the test uses different fixture objects and records an identity mapping for
that field.

| Native selector | Behavior |
|---|---|
| `test_lslocks_defaults_and_lslocks_columns` | Default columns; `LSLOCKS_COLUMNS` override; `-o` replacement and `+` extension; CLI selection precedence over environment. |
| `test_lslocks_all_columns_types_and_json` | Every column, all-row identity, JSON top-level key, exact JSON types, nulls, and row order. |
| `test_lslocks_bytes_and_human_size` | Human suffixes, byte output, and JSON `SIZE` type with and without `-b`. |
| `test_lslocks_list_columns_modes` | `-H`, `-H -r`, `-H -J`, and the advertised `SIZE` type. |
| `test_lslocks_raw_noheadings_and_json` | Raw and heading-free output; JSON/raw conflict status and diagnostics. |
| `test_lslocks_output_column_names_and_errors` | Case-insensitive names, custom ordering, `+LIST`, repeated names, empty names, and unknown-column failure. |
| `test_lslocks_pid_filters_holders_and_waiters` | Exact PID filtering for holders and queued waiters; absent PID; `-1` OFD row behavior. |
| `test_lslocks_filter_values_and_hidden_columns` | Numeric and string comparisons, regular-expression and logical filters, including filter columns absent from output. |
| `test_lslocks_filter_errors` | Invalid filter syntax and unknown filter columns fail with reference status and stderr. |
| `test_lslocks_default_and_selected_lock_rows` | All rows in a private PID namespace; selected rows in the shared namespace; outside holder stays invisible in both commands. |
| `test_lslocks_ranges_and_eof` | Finite POSIX/OFD byte ranges, zero-length-to-EOF locks, exact `START` and `END`, and non-overlapping ranges. |
| `test_lslocks_blocker_flock`, `test_lslocks_blocker_posix`, `test_lslocks_blocker_ofd` | Synchronized queued requests have `MODE` suffixed with `*`, the exact `BLOCKER` PID, and release cleanly after the owner exits. |
| `test_lslocks_posix_holders` | `HOLDERS` behavior for POSIX locks, including the process-wide descriptor association. |
| `test_lslocks_mandatory_flag` | `M` for advisory locks; qualify a mandatory-lock positive case only in a filesystem/kernel setup that enables it. |
| `test_lslocks_inaccessible_and_noinaccessible` | Cross-UID path/command access, fallback mountpoint text, and `-i` row removal under the qualified permission context. |
| `test_lslocks_deleted_path_and_owner_exit` | A deleted locked file shows the reference path text while the descriptor remains open; owner exit removes its row. |
| `test_lslocks_disappearing_owner_during_collection` | Process exit during collection does not invent an owner, corrupt another row, or silently turn unexpected I/O failure into success. |
| `test_lslocks_notruncate_and_terminal_width` | Long UTF-8 and non-UTF-8 paths, normal truncation, `-u`, and matched terminal widths. |
| `test_lslocks_version_provenance` | Both commands succeed and identify `lslocks`; assert each exact version banner separately. |
| `test_lslocks_help_and_usage_errors` | Exact help output; missing option values, invalid PID, unknown option, operands, and `-J -r`. |
| `test_lslocks_smartcols_json_environment` | `LIBSMARTCOLS_JSON=compact`, `lines`, unknown, and unset values. |
| `test_lslocks_locale_and_table_order` | `LC_ALL=C` is the qualified baseline; compare a second installed locale only when the pinned image has it. Preserve row order and localized diagnostics. |

## Fixture requirements

The fixture helper must create only files below the native test's private
directory. It must report the holder PID and descriptor numbers after the
requested lock has been acquired. A waiter reports readiness only after its
blocked `->` lock record is visible. Release every holder and waiter through
an owned control path, kill it on timeout or assertion failure, and wait for
its exit before the test ends. A single test owns each process handle and each
fixture path.

Cover shared and exclusive `FLOCK`, POSIX read/write locks, OFD read/write
locks, and a write lease. Keep duplicated descriptors open for the upstream
`HOLDERS` cases. Use a finite data file for ranged locks. Use a private PID
namespace with procfs freshly mounted for that namespace for default and
all-row comparisons. Run permission cases as distinct real UIDs with
capabilities dropped. A conditional positive case stays open until it executes
in its qualified context.

The current `linux.open_files()` collector establishes process and descriptor
identity, path, inode, optional device, and access mode. It does not report
lock class, range, blocker, or holders. XSH `/proc/locks` parsing can establish
class, mode, wait state, PID, device, inode, and range. The lane fixture helper
is prepared; its native lifecycle adapter and the product-side owner join are
still open.

## Requests for shared test support

`support.util_linux` supplies the reference, exact comparator, version-banner,
and namespace checks used by the ordinary native cases. Its keeper lifecycle
adapter starts `keeper.py` with owned input/output FIFOs and bounds readiness,
release, and cleanup. The namespace guard also checks that PID 1 and the test
process resolve to the same PID namespace through `/proc`, which proves the
private PID namespace has a matching procfs mount. The default-row case still
needs a lock owner outside that namespace, with the runner supplying its exact
identity and path. The inaccessible-path case needs a live fixture owner at
UID 10002 while running each command as UID 10001 with capabilities dropped.
The current helper API does not provide a captured PTY with a selected width;
terminal truncation remains open until that seam exists. Owner-disappearance-
during-collection also needs a deterministic synchronization point before it
can be a passing parity case.
