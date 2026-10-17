# Core command behavior

`iw` implements a bounded `nl80211` subset: device and PHY listing/info,
connection and station queries, scan dump/trigger/abort, device type/channel/
frequency/transmit-power changes, regulatory queries/updates/reload, and event
monitoring. Association, authentication, mesh, AP, and other upstream `iw`
commands are absent.

`nsenter` forks by default only when entering a PID or time namespace;
`--no-fork` disables that child. `unshare` forks only with `--fork`. Both relay
a command's terminating signal so the caller observes the same signal death.
`setpriv` applies settings in this order: capability bounding set, securebits,
`no_new_privs`, supplementary groups, GIDs, UIDs, inheritable capabilities,
ambient capabilities, parent-death signal, then ptracer.

| Applet | Byte and error behavior |
|---|---|
| `tee` | Copies input bytes to each output. Write failures are governed by `warn`, `warn-nopipe`, `exit`, or `exit-nopipe`; the default exits on pipe errors and reports other output errors. With `-p`, the default is `warn-nopipe`; `nopipe` modes exit once every output is a broken pipe. |
| `wc` | `-l` counts LF line endings (a CRLF pair counts once) and `-c` counts bytes. Character, word, and display-width counts interpret text; byte counts do not depend on UTF-8 decoding. Read failures are reported and return failure. |
| `more` | Paging requires terminal stdin, terminal stdout, and an accessible controlling terminal. Without that, it concatenates input; with no display transformations it preserves every byte, including an unterminated final line. |
| `tail` | Copies selected data as bytes. `--follow=name` cannot follow standard input; read and write errors are reported and contribute to a failing exit status even when other operands produce output. |
| `sort` | Keeps record contents as bytes; plain whole-record order is bytewise, and malformed UTF-8 payloads are not rewritten. Output uses newline separators by default and NUL with `-z`. File and output errors are reported as failures. |
| `mdev` | Leaves the caller's umask unchanged. Device and alias parent directories request `0755`, narrowed by the caller's umask; rule commands run in a child with umask `000`. |
| `cat` | With no display transforms, copies input bytes verbatim, including NUL and malformed UTF-8 across chunks. Numbering and show/squeeze options transform only the requested output. |
| `dircolors` | Accepts a non-UTF-8 database path as raw bytes, but the database contents must be valid UTF-8 or the command fails. Diagnostics quote the original path bytes. |
| `ls` | Retains entry names as raw bytes and sorts them bytewise. Quoting styles escape malformed UTF-8 bytes; literal output preserves them, while `-q` renders non-printable bytes as `?`. |
| `lslocks` | Reports locks from `/proc/locks`, resolves held OFD owners through `/proc/PID/fdinfo`, and leaves blocked OFD PIDs at `-1`. |
| `lsipc` | Reads System V IPC rows from `/proc/sysvipc`, named POSIX objects from `/dev/shm` and `/dev/mqueue`, and global limits from `/proc/sys`. |
| `nl` | `-d` appends `:` when its delimiter argument contains one character. C/POSIX locales count bytes; other locales count UTF-8 characters, with malformed bytes treated as separate units. |
