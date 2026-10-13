# Core Applet Contracts

`core/lib/gnu.xsh` centralizes GNU-compatible diagnostics, usage hints, help and
version lines, stdout errors, and byte-stream helpers. It uses the invoked
script name without resolving symlinks. `tr` emits its ambiguous-octal warning
before later set-parse errors.
`base32`, `base64`, and `basenc` preserve a decoded prefix before reporting
invalid input, use GNU option-argument diagnostics, and report stdout failures
with the `write error:` prefix.
`comm` checks ordering as each input advances, checks file 1 first after a
shared row, and activates default checks only after an unpairable row; EOF
rechecks the final pair of records from that input. `expand` counts input
bytes as columns in the C and POSIX locales and uses
Unicode display columns in UTF-8 locales. It preserves input bytes; backspace
moves one column left and newline resets the tab position.
`nl` appends the default second section-delimiter character after a single
byte in C/POSIX or a single UTF-8 character in a UTF-8 locale; malformed bytes
remain separate characters. Section matching preserves the delimiter bytes.
`tail --follow=name` can reopen a disappeared, previously opened name while
another operand remains live. Without `--retry`, it exits when no named
operand remains open. Applets read kernel interfaces through their owning
module under `core/lib/`; `dev/checks/check_kernel_reads.py` rejects direct
`/proc` and `/sys` paths in applet scripts.

`dev/release.xsh::package_core` installs applets as executable commands without
the `.xsh` suffix, while keeping that suffix on `core/lib/` modules so adjacent
`use lib.auth` imports resolve after extraction. Library modules install with
mode `0644`.

`taskset`, `chrt`, `ionice`, and `prlimit` reproduce the util-linux utilities
over the typed `process.affinity`, `scheduler`, `io_priority`, and `rlimit`
families. They keep util-linux's own diagnostics (`failed to set pid N's
affinity: ...`) through `core/lib/gnu.xsh`, parse options in `getopt_long` order
with the glibc wording, and exit 1 on failure and 126 or 127 when the command
cannot be executed. The listing `prlimit` prints before running a command is
written in full, where util-linux loses its unflushed rows at `exec`.

`nvme` preserves the host libc wording for ioctl errors, including JSON error
output, because nvme-cli's diagnostics differ from GNU coreutils. It removes
the Rust errno suffix without changing that wording.

`core/setpriv.xsh` parses its own command line instead of using `cli.applet`:
util-linux reports a duplicate or conflicting option in command-line order and
names the earlier one, and the first operand starts the command, neither of
which a parsed option record keeps. Privilege state is read and changed only
through `linux.privileges()` and the `linux` and `unix` setters. Changes apply
in this order: bounding set, securebits, `no_new_privs`, supplementary groups,
GIDs, UIDs, inheritable and ambient capabilities, then the parent-death signal
and ptracer. The kernel clears the parent-death signal when credentials
change, so it is applied last. Where util-linux silently skips a request the
kernel refused (an ambient capability that is not inheritable or a capability
already dropped from the bounding set), the applet fails with the kernel's
wording; `+all` stays tolerant and covers only what can still apply.
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
`recvfrom`); `core/lib/nl80211.xsh` owns the nl80211 attribute codec, the
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
`--debug`, `event -f`, and `station dump -v` are not accepted.
`core/tests/test-iw.xsh` runs the applet under `test.linux_fake` with recorded
`netlink_fixture` exchanges, so tests do not reach a wireless device.

`system-report` is documented in `core/SYSTEM-REPORT.md`.

`core/wc.xsh` reads raw bytes for line and byte counts, so `-l` and `-c`
accept non-UTF-8 input. Word counts still use `Str.count_words()` and require
valid UTF-8.
`core/cat.xsh` and `core/tee.xsh` preserve file and stdin bytes through stdout.
`tee` keeps output descriptors open, streams bounded chunks, and appends in
place. Its output-error modes isolate failed outputs and keep healthy outputs
receiving data; no-pipe modes observe broken outputs while input is idle.

## Applet behavior

`core/more.xsh` concatenates bytes without file banners when stdout is redirected
or no controlling terminal is available. Inherited terminal descriptors alone
do not enable paging. Missing files and directory read errors produce status 1
while later operands are still read; no operands reads stdin through EOF. Short
files exit at EOF; `-e` also exits after a completely filled final screen. The
BusyBox 1.36.1 baseline accepts `-d`, `-e`, `-f`, `-l`, `-s`, and `-u` without
their display effects. XSH implements those effects, their long aliases, and
the extensions `-c`/`-p` and `-n`/`-F`/`-P`. Coverage is in
`core/tests/test-more.xsh` and `tests/core_compat_extra/more.rs`.

`core/lib/date_dircolors.xsh::DATABASE` preserves the GNU coreutils 9.12
built-in database, including terminal filters and suffix order. Shell
assignments and `--print-ls-colors` use the same `TERM` and `COLORTERM`
selection; listing color codes does not enable unmatched sections.

`core/ls.xsh` preserves operand and directory-name bytes through parsing and
name ordering. Its fixed-width byte key keeps distinct malformed UTF-8 names
distinct and ordered; argument restoration uses each original argv index.

`core/sort.xsh` retains each sort record as its original `Bytes`, with a valid
UTF-8 view only when decoding succeeds. Record boundaries and key/range offsets
stay byte-based, and in-memory sorting, spill runs, merge readers, and output
carry the original bytes so malformed UTF-8 survives unchanged. Native Linux
sort uses bytewise collation. Numeric conventions and month names can use
locale metadata, but the runtime has no libc `strcoll` binding for
`LC_COLLATE`; GNU non-C collation is outside the native sort contract.
