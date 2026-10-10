# Core Applet Style

Each `core/*.xsh` applet is a standalone command script by default. Duplicate
small helpers locally when that keeps an applet self-contained and easy to
inspect. Shared libraries under `core/lib/` are reserved for audited command
families whose behavior must stay consistent across multiple applets, such as
auth account parsing and shadow-file updates. `lib/text_input.xsh` preserves
the file and stdin operand order shared by the line-oriented applets.
`lib/gnu.xsh` owns the GNU-compatibility surface the compat applets must agree
on exactly: the invoked name (`process.script_path()`, never symlink-resolved),
`PROG: message` diagnostics with GNU name quoting and `strerror` text, usage
errors with the `Try 'PHRASE --help'` hint and a caller-chosen exit status,
`--help` and `--version` first lines, stdout write failures, and byte-stream
operand and stdout helpers. The audited command families also share
`lib/perm.xsh` for permission and ownership policy, `lib/file_publish.xsh` for
move/link/install publication, `lib/fs_misc.xsh` for path and size policy,
`lib/text_a2.xsh` for byte records and display columns, `lib/bytes_enc*.xsh` for
binary encodings and conversion, `lib/checksums.xsh` for checksum formats and
verification, and `lib/proc_launch.xsh` for executable lookup and exit status.

An applet never spells a `/proc` or `/sys` path itself
(`dev/compat/check_kernel_reads.py` fails the build when one does); it asks the
owner of that kernel interface. Beyond the native modules, those owners are
`lib/selinux.xsh` (whether the kernel lists or mounts selinuxfs, shared by
`mkdir`, `mknod`, `mv`, `install` and `ss`), `lib/proc_target.xsh` (the
namespace, root and working-directory files of a target pid, for `nsenter`),
`lib/efivars.xsh` (the efivarfs directory and whether the platform has one),
and `lib/net_sockets.xsh` (the socket counters, local port range and cgroup ids
`ss` prints, and the raw-socket table it falls back to when the kernel has no
raw_diag handler). Process task ids come from `process.threads`, the
supplementary group list from `unix.id().supplementary`, the file-descriptor
limit ceiling from `linux.sysctl_get`, and the inherited environment as bytes
from `env.entries()`.

Shared semantic libraries remain XSH: `lib/awk.xsh` owns its lexer, parser and
interpreter, `lib/sed.xsh` owns addressing and execution, the `lib/fat*.xsh`
family owns geometry and checking, and `lib/date_parse.xsh` owns human date
grammar. Native operations supply reusable byte, numeric, codec and host
boundaries. The search, hardware, procps, kmod, storage, compression and inode
attribute families also share typed XSH helpers under `lib/`.

`dev/release.xsh::package_core` installs applets without the `.xsh` suffix as
executable commands. It keeps that suffix for `core/lib/` modules so adjacent
`use lib.auth` imports resolve after extraction; modules install with mode
`0644`.

The `core/` audit found one stable repeated host boundary: reading text files
and `-` stdin operands in order. `fold`, `rev`, `shuf`, and `tr` now share
`lib/text_input.xsh` with `cut`, `head`, `tail`, `uniq`, and `sort`. Applet usage
errors remain local because their messages and accepted operand shapes differ;
`cli.applet` and `cli.parse` already own their option parsing. The one
exception is an applet that reproduces a GNU utility: it reports usage
errors, operating-system errors, and quoted names through `lib/gnu.xsh`, so
wording and exit statuses cannot drift, and it chooses its own status (1 by
default, 2 for `ls`, `cmp`, `diff`, and `grep`, 125 for `env`, `nice`,
`nohup`, `timeout`, `stdbuf`, and `chroot`).

`taskset`, `chrt`, `ionice`, and `prlimit` reproduce the util-linux utilities
over the typed `process.affinity`, `scheduler`, `io_priority`, and `rlimit`
families. They keep util-linux's own diagnostics (`failed to set pid N's
affinity: ...`) through `lib/gnu.xsh`, parse options in `getopt_long` order
with the glibc wording, and exit 1 on failure and 126 or 127 when the command
cannot be executed. The listing `prlimit` prints before running a command is
written in full, where util-linux loses its unflushed rows at `exec`.

Prefer typed standard-module APIs over shelling out or parsing command text.
Use `cli.parse` for ordinary option records, including short aliases and
clusters, and reserve `cli.tokens` for applets whose option grammar is itself
the feature. Keep usage errors local and explicit unless the applet follows
GNU diagnostics (see above), preserve Unix-compatible stdout shapes, and cover
behavior through `core/tests/*.xsh`.

`core/setpriv.xsh` parses its own command line instead of using `cli.applet`:
util-linux reports a duplicate or conflicting option in command-line order and
names the earlier one, and the first operand starts the command, neither of
which a parsed option record keeps. Privilege state is read and changed only
through `linux.privileges()` and the `linux` and `unix` setters, in the order
the script header documents. Where util-linux silently skips a request the
kernel refused (an ambient capability that is not inheritable, adding back a
capability already dropped from the bounding set), the applet fails with the
kernel's wording; `+all` stays tolerant and covers only what can still apply.
`unshare` and `nsenter` run their command through `linux.run_in_namespaces`,
which makes every namespace change in a forked child (a user namespace needs a
single-threaded process, and scripts do not run in one), so the command is
always a child of the applet and the applet reports its status or repeats its
signal death. `lsns` formats `linux.namespaces`. `unshare` refuses the
persistent `--mount=FILE` forms, which need a process outside the new
namespaces to bind the namespace file; `nsenter` adds
`--preserve-credentials` for entering a user namespace whose `setgroups` is
denied, the case every unprivileged `unshare --map-root-user` creates.

`iw` reproduces the wireless configuration tool over the generic-netlink
primitives (`linux.genl_family_id`, `netlink_open`, `netlink_request`,
`recvfrom`); `lib/nl80211.xsh` owns the nl80211 attribute codec, the
decoders, and the report formats, and `iw.xsh` owns the command line and the
order of requests. The surface is `dev`, `dev DEV info|link|scan [dump|trigger|
abort]|station dump|get|set type|channel|freq|txpower`, `phy`, `list`,
`phy PHY info|reg get|set channel|freq|txpower`, `reg get|set|reload`, `event
[-t|-T|-r]`, `help`, and `--version`. Association, authentication, access
point, mesh, wowlan, and the other commands are not implemented and are not
listed in the help, so they fail as unknown commands. Command parsing, usage
text, `command failed: ... (-N)` errors and exit statuses follow iw 6.17,
whose output formats were checked against the strings of that binary; the
`reg get` report was compared byte for byte with it on a host without
wireless hardware. Report parts the decoders do not implement are omitted
rather than approximated: VHT, HE, and EHT capability blocks, the HT
operation and capability elements of a BSS, the vendor-specific elements
other than WPA, TID and TXQ statistics, MLO links, and DMG capability names.
`--debug`, `event -f`, and `station dump -v` are not accepted. Tests run the
applet under `test.linux_fake` with a `netlink_fixture` of recorded
exchanges (SPEC section 17), so no test reaches a wireless device.

`system-report` is documented in `core/SYSTEM-REPORT.md`.

`core/wc.xsh` reads raw bytes for line and byte counts, so `-l` and `-c`
accept non-UTF-8 input. Word counts still use `Str.count_words()` and require
valid UTF-8.
`core/cat.xsh` and `core/tee.xsh` preserve file and stdin bytes through stdout.
`tee` keeps output descriptors open, streams bounded chunks, and appends in
place. Its output-error modes isolate failed outputs and keep healthy outputs
receiving data; no-pipe modes observe broken outputs while input is idle.
