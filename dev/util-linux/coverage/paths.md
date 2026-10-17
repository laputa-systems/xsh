# Path command behavior inventory

This inventory covers util-linux 2.42.4 `mountpoint` and `namei`. Native tests
compare the reference and XSH byte streams and statuses without normalizing
whitespace, diagnostics, path escaping, or output order. The only intentional
difference is `version-provenance`: each command keeps its own version banner.

| Command and case selector | Reference evidence | Behavior covered |
|---|---|---|
| `mountpoint` `test_mountpoint_upstream_default_follows_mountpoint_symlink` | [upstream `default`](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/mountpoint/mountpoint#L18-L21), [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c) | Default final-symlink following for a symlink to a mount root discovered from the current mount table. |
| `mountpoint` `test_mountpoint_upstream_nofollow_does_not_follow_mountpoint_symlink` | [upstream `nofollow`](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/mountpoint/mountpoint#L22-L25), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.1.adoc) | `--nofollow` (long option only), including the symlink's non-mountpoint result. |
| `mountpoint` `test_mountpoint_upstream_mutually_exclusive_devno_nofollow` | [upstream `mutually-exclusive`](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/mountpoint/mountpoint#L27-L30) | Rejects `--devno` combined with `--nofollow`. |
| `mountpoint` `test_mountpoint_quiet_for_files_directories_and_dangling_symlinks` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.1.adoc) | `-q` / `--quiet` for regular files, directories, file symlinks, dangling symlinks, and missing paths. |
| `mountpoint` `test_mountpoint_filesystem_device_numbers_cover_mount_and_regular_paths` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c) | `-d` / `--fs-devno` for a mount root and a regular path that is not itself a mountpoint. |
| `mountpoint` `test_mountpoint_device_number_requires_a_block_device` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.1.adoc) | `-x` / `--devno` on a character device and a directory; no block device is created or changed. |
| `mountpoint` `test_mountpoint_devno_reports_owned_block_device` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.1.adoc) | `--devno` and `-x` on a disposable block-device node created in the owned scene. |
| `mountpoint` `test_mountpoint_show_on_a_real_mount_and_non_mount_path` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.1.adoc) | `--show` on `/proc` returns exactly the bytes `/proc\n`; a discovered mount root, symlink, ordinary path, and paths ending in `//.` cover `--quiet`, `--fs-devno`, and `--nofollow` interactions. |
| `mountpoint` `test_mountpoint_rejects_bad_operand_counts_and_options` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.1.adoc) | Missing and extra operands, an unknown option, and `-h` / `--help`, including usage status and diagnostics. |
| `mountpoint` `test_mountpoint_path_arguments_preserve_non_utf8_names` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c) | Raw path bytes through native `Path` argv transport under `C` and `C.UTF-8`. |
| `mountpoint` `test_mountpoint_inaccessible_path_from_unprivileged_runner` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c) | Search-permission errors for a regular path and quiet mode from UIDs 10001 and 10002 with capabilities dropped. |
| `mountpoint` `test_mountpoint_private_same_device_bind_mount` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.1.adoc) | Same-device bind target is a mountpoint; checks `--fs-devno`, `--show`, and the restored hidden target after unmount in the required private namespace. |
| `mountpoint` `test_util_linux_context_terminal_reference_width_and_escaping` | [shared terminal context](../../../core/tests/test-util-linux-contexts.xsh) | Exact reference/candidate stdout and stderr through PTYs at 32 and 120 columns with a path containing tab and control bytes. |
| `mountpoint` `test_mountpoint_version_provenance` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c) | Asserts the reference's exact `mountpoint from util-linux 2.42.4` banner for `-V` / `--version` and the XSH `gnu.version_text` banner separately. |
| `namei` `test_namei_upstream_logic_plain_relative_paths` | [upstream `logic`](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/namei/logic#L23-L30) | The two upstream plain relative file paths. |
| `namei` `test_namei_root_and_repeated_path_separators` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | `/`, repeated leading and interior separators, and a trailing separator. |
| `namei` `test_namei_each_output_option_and_long_combination` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | `-x` / `--mountpoints`, `-m` / `--modes`, `-o` / `--owners`, `-l` / `--long`, `-n` / `--nosymlinks`, `-v` / `--vertical`, and option combinations. Upstream guards `-Z` / `--context` with `HAVE_LIBSELINUX`; the pinned reference build lacks that support, so the test asserts both exact unsupported-option diagnostics and status 1. |
| `namei` `test_namei_component_walk_follows_relative_and_absolute_symlinks` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | Directory, regular, FIFO, character-device, and symlink components; relative and absolute symlink targets; default traversal; and `--nosymlinks`. |
| `namei` `test_namei_component_walk_reports_socket_and_block_device` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | Socket and disposable block-device node type output in `--modes` format. |
| `namei` `test_namei_committed_path_fixture_links_and_components` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | Read-only committed fixtures for regular, nested, relative-link, dangling-link, loop, and root-link paths. |
| `namei` `test_namei_mountpoint_markers_follow_a_real_mount_boundary` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | `-x` marks the `/proc` mount boundary on `/proc/1`; `--mountpoints` reports the same mounted filesystem. |
| `namei` `test_namei_private_same_device_bind_mount_matches_reference` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | `-x` matches the reference on a same-device bind mount in the required private namespace; the target's final component remains `d`, because util-linux 2.42.4's parent device/inode comparison does not mark this bind boundary. |
| `namei` `test_namei_dangling_missing_and_looping_components` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | Dangling links, missing intermediate components, symlink loops, and `--nosymlinks` on a loop. |
| `namei` `test_namei_multiple_operands_and_usage_errors` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | No operand, unknown option, multiple operands, `-h` / `--help`, continued reporting, and aggregate failure status. |
| `namei` `test_namei_path_arguments_preserve_non_utf8_components` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | Undecodable path components through native `Path` argv transport under `C` and `C.UTF-8`. |
| `namei` `test_namei_permission_denied_from_unprivileged_runner` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | Search-permission failure for long and `--nosymlinks` output from UIDs 10001 and 10002 with capabilities dropped. |
| `namei` `test_namei_modes_and_owner_columns_cover_special_bits` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | Setuid, setgid, and sticky mode markers, plus numeric owner and group output for IDs without names. |
| `namei` `test_util_linux_context_terminal_reference_width_and_escaping` | [shared terminal context](../../../core/tests/test-util-linux-contexts.xsh) | Exact reference/candidate stdout and stderr through PTYs at 32 and 120 columns with a path containing tab and control bytes. |
| `namei` `test_namei_version_provenance` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | Asserts the reference's exact `namei from util-linux 2.42.4` banner for `-V` / `--version` and the XSH `gnu.version_text` banner separately. |

The bind fixture is prepared at
[`core/tests/fixtures/util-linux/paths/bind`](../../../core/tests/fixtures/util-linux/paths/bind).
Its source and target are ordinary directories on the same checked-out
filesystem; `source/entry` and `target/sentinel` distinguish the visible trees.
The native tests copy those bytes to an owned writable scene; no mount is active
in the repository. Same-device bind coverage requires the
coordinator's capability-qualified private mount namespace runner. The native
tests call `ul.require_private_namespace()`, bind the fixture only inside that
namespace, compare reference and candidate there, and unmount before asserting
the original target is visible again. The reference and candidate therefore
see the same mount identity. A host mount, disk, or device is never used as a
fixture.

The mountpoint and namei permission cases make only their owned scene root
traversable and set the owned child directory to mode 000. The native support
helper runs the reference and candidate with UIDs 10001 and 10002 and
capabilities dropped, then the tests verify the denied directory mode and its
file contents are unchanged.

The path cases use stable missing, dangling, looped, and permission-denied
fixtures. They do not attempt a component lookup race. Neither command exposes
a barrier that can make the reference and candidate inspect a path during the
same controlled mutation.
