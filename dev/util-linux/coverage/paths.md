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
| `mountpoint` `test_mountpoint_show_on_a_real_mount_and_non_mount_path` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.1.adoc) | `--show` on a discovered mount root and an ordinary path. Reference support depends on the util-linux build's statmount support. |
| `mountpoint` `test_mountpoint_rejects_bad_operand_counts_and_options` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.1.adoc) | Missing and extra operands, an unknown option, and `-h` / `--help`, including usage status and diagnostics. |
| `mountpoint` `test_mountpoint_path_arguments_preserve_non_utf8_names` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c) | Raw path bytes through native `Path` argv transport. |
| `mountpoint` `test_mountpoint_private_same_device_bind_mount` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.1.adoc) | Same-device bind target is a mountpoint; checks `--fs-devno`, `--show`, and the restored hidden target after unmount in the required private namespace. |
| `mountpoint` `test_mountpoint_version_provenance` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/mountpoint.c) | Asserts the reference's exact `mountpoint from util-linux 2.42.4` banner for `-V` / `--version` and the XSH `gnu.version_text` banner separately. |
| `namei` `test_namei_upstream_logic_plain_relative_paths` | [upstream `logic`](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/namei/logic#L23-L30) | The two upstream plain relative file paths. |
| `namei` `test_namei_each_output_option_and_long_combination` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | `-x` / `--mountpoints`, `-m` / `--modes`, `-o` / `--owners`, `-l` / `--long`, `-n` / `--nosymlinks`, `-v` / `--vertical`, `-Z` / `--context`, and option combinations. SELinux context support depends on `HAVE_LIBSELINUX`. |
| `namei` `test_namei_component_walk_follows_relative_and_absolute_symlinks` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | Directory, regular, FIFO, character-device, and symlink components; relative and absolute symlink targets; default traversal; and `--nosymlinks`. |
| `namei` `test_namei_committed_path_fixture_links_and_components` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | Read-only committed fixtures for regular, nested, relative-link, dangling-link, loop, and root-link paths. |
| `namei` `test_namei_mountpoint_markers_follow_a_real_mount_boundary` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | `-x` mount markers on the active mount root and a path below it. |
| `namei` `test_namei_private_same_device_bind_mount_marker` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | `-x` marker for a same-device bind mount and its visible child in the required private namespace. |
| `namei` `test_namei_dangling_missing_and_looping_components` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | Dangling links, missing intermediate components, symlink loops, and `--nosymlinks` on a loop. |
| `namei` `test_namei_multiple_operands_and_usage_errors` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c), [manual](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.1.adoc) | No operand, unknown option, multiple operands, `-h` / `--help`, continued reporting, and aggregate failure status. |
| `namei` `test_namei_path_arguments_preserve_non_utf8_components` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | Undecodable path components through native `Path` argv transport. |
| `namei` `test_namei_permission_denied_from_unprivileged_runner` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | Search-permission failure when the test process has a real unprivileged identity; the test skips under UID 0. |
| `namei` `test_namei_version_provenance` | [source](https://github.com/util-linux/util-linux/blob/v2.42.4/misc-utils/namei.c) | Asserts the reference's exact `namei from util-linux 2.42.4` banner for `-V` / `--version` and the XSH `gnu.version_text` banner separately. |

The bind fixture is prepared at
[`core/tests/fixtures/util-linux/paths/bind`](../../../core/tests/fixtures/util-linux/paths/bind).
Its source and target are ordinary directories on the same checked-out
filesystem; `source/entry` and `target/sentinel` distinguish the visible trees.
No mount is active in the repository. Same-device bind coverage requires the
coordinator's capability-qualified private mount namespace runner. The native
tests call `ul.require_private_namespace()`, bind the fixture only inside that
namespace, compare reference and candidate there, and unmount before asserting
the original target is visible again. The reference and candidate therefore
see the same mount identity. A host mount, disk, or device is never used as a
fixture.

Permission-denied coverage is likewise unqualified when the native harness runs
as UID 0. The corpus does not create another user or change credentials; a
root-only result cannot stand in for a denied search permission.
