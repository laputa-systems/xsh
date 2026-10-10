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

Prefer typed standard-module APIs over shelling out or parsing command text.
Use `cli.parse` for ordinary option records, including short aliases and
clusters, and reserve `cli.tokens` for applets whose option grammar is itself
the feature. Keep usage errors local and explicit unless the applet follows
GNU diagnostics (see above), preserve Unix-compatible stdout shapes, and cover
behavior through `core/tests/*.xsh`.

`unshare` and `nsenter` run their command through `linux.run_in_namespaces`,
which makes every namespace change in a forked child (a user namespace needs a
single-threaded process, and scripts do not run in one), so the command is
always a child of the applet and the applet reports its status or repeats its
signal death. `lsns` formats `linux.namespaces`. `unshare` refuses the
persistent `--mount=FILE` forms, which need a process outside the new
namespaces to bind the namespace file; `nsenter` adds
`--preserve-credentials` for entering a user namespace whose `setgroups` is
denied, the case every unprivileged `unshare --map-root-user` creates.

`system-report` is documented in `core/SYSTEM-REPORT.md`.

`core/wc.xsh` reads raw bytes for line and byte counts, so `-l` and `-c`
accept non-UTF-8 input. Word counts still use `Str.count_words()` and require
valid UTF-8.
`core/cat.xsh` and `core/tee.xsh` preserve file and stdin bytes through stdout.
`tee` keeps output descriptors open, streams bounded chunks, and appends in
place. Its output-error modes isolate failed outputs and keep healthy outputs
receiving data; no-pipe modes observe broken outputs while input is idle.
