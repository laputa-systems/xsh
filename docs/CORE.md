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
| `lsmem` | Reads memory block data from `/sys` or the tree named by `--sysroot`; a missing memory sysfs tree is an error. Adjacent blocks merge unless `--all` is used or a selected `--split` attribute differs. Supports table, raw, pairs, JSON, and summary output. |
| `tail` | Copies selected data as bytes. `--follow=name` cannot follow standard input; read and write errors are reported and contribute to a failing exit status even when other operands produce output. |
| `sort` | Keeps record contents as bytes; plain whole-record order is bytewise, and malformed UTF-8 payloads are not rewritten. Output uses newline separators by default and NUL with `-z`. File and output errors are reported as failures. |
| `mdev` | Leaves the caller's umask unchanged. Device and alias parent directories request `0755`, narrowed by the caller's umask; rule commands run in a child with umask `000`. |
| `cat` | With no display transforms, copies input bytes verbatim, including NUL and malformed UTF-8 across chunks. Numbering and show/squeeze options transform only the requested output. |
| `dircolors` | Accepts a non-UTF-8 database path as raw bytes, but the database contents must be valid UTF-8 or the command fails. Diagnostics quote the original path bytes. |
| `ls` | Retains entry names as raw bytes and sorts them bytewise. Quoting styles escape malformed UTF-8 bytes; literal output preserves them, while `-q` renders non-printable bytes as `?`. |
| `lslocks` | Reads lock rows from `/proc/locks` and resolves held OFD owners and descriptor holders through `/proc/PID/fdinfo`. `--output`, `--output-all`, and `LSLOCKS_COLUMNS` select from 13 columns; `--pid` and `--filter` select rows. `--json` emits typed JSON, while `--raw` removes table alignment. If collection yields no rows, no output is written; `--pid` or `--filter` can still produce an empty JSON container after collecting rows. On a terminal, `--notruncate` keeps long `PATH` values. `HOLDERS` lists `PID,COMMAND,FD` for each associated descriptor. A blocked OFD row keeps PID `-1` and reports its owner in `BLOCKER`. |
| `lsipc` | Reads System V queues, shared memory, and semaphore sets from `/proc/sysvipc`. It reads POSIX message queues from `/dev/mqueue`, and POSIX shared memory and semaphores from `/dev/shm`. `--global` reports System V counts and limits from `/proc/sysvipc` and `/proc/sys/kernel`. It reports POSIX message-queue counts and limits from `/dev/mqueue` and `/proc/sys/fs/mqueue`. `--id` selects a System V object. `--name` selects a POSIX object. `--creator` and `--time` add applicable columns. `--output` selects columns. Selecting one ID or name uses pretty output by default. `--list`, `--raw`, `--export`, `--newline`, and `--json` choose other formats. |
| `nl` | `-d` appends `:` when its delimiter argument contains one character. C/POSIX locales count bytes; other locales count UTF-8 characters, with malformed bytes treated as separate units. |
