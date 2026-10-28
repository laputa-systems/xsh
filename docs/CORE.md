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

`dhcpc INTERFACE` is the supervised DHCPv4 client: one foreground process per
interface, run under a supervisor such as `xinit`, never in PID 1. It owns the
exact addresses and routes it configured, recorded in a versioned lease file
under `--lease-dir` (default `/var/lib/dhcpc`), and never flushes anything
else. It reads replies from a packet socket, because a reply to an unaddressed
client is dropped by reverse-path filtering before a UDP socket sees it, and
sends unaddressed messages as raw frames. Renewal at T1 is unicast, rebinding
at T2 is broadcast, expiry withdraws only owned state, classless static routes
(option 121, RFC 3442) replace the router option and are owned and withdrawn
route by route (a route whose gateway the kernel could not reach is dropped,
and an on-link route has link scope), and a remembered lease
is validated with INIT-REBOOT after a restart or boot-identity change. The
resolver is shared through one coordinator: per-interface contributions under
`--run-dir` are merged with the operator's static file (`--static-file`) into
`--resolv-dir/resolv.conf`, and a `resolv.conf` that the coordinator did not
write, or a symlink, is reported in the status and never replaced. The status
is `RUN_DIR/INTERFACE.status` (JSON, with counters and the last `trouble`), and
`RUN_DIR/INTERFACE.control` takes `down`, `up` and `stop`.

`ifup` and `ifdown` drive a running `dhcpc` through that control socket and
wait up to `--wait SECONDS`. Without a running client, `ifup` runs the shared
client once (`--once` semantics): it acquires and configures a lease and then
exits, so nothing renews it; a supervised `dhcpc` is the way to keep one.
`ifdown` releases the lease by unicast RELEASE, withdraws owned state and the
interface's resolver contribution, and leaves the link up. Static interfaces
behave as before.

| Applet | Byte and error behavior |
|---|---|
| `tee` | Copies input bytes to each output. Write failures are governed by `warn`, `warn-nopipe`, `exit`, or `exit-nopipe`; the default exits on pipe errors and reports other output errors. With `-p`, the default is `warn-nopipe`; `nopipe` modes exit once every output is a broken pipe. |
| `wc` | `-l` counts LF line endings (a CRLF pair counts once) and `-c` counts bytes. Character, word, and display-width counts interpret text; byte counts do not depend on UTF-8 decoding. Read failures are reported and return failure. |
| `more` | Paging requires terminal stdin, terminal stdout, and an accessible controlling terminal. Without that, it concatenates input; with no display transformations it preserves every byte, including an unterminated final line. |
| `lsmem` | Reads memory block data from `/sys` or the tree named by `--sysroot`; sysroot operands preserve raw Unix path bytes, and a missing memory sysfs tree is an error. With no numeric `memoryN` entries, it exits 1 without stdout and reports `lsmem: Failed to read <root>/sys/devices/system/memory: no numeric memory entries`; parity allows only util-linux's stale-errno suffix to vary after the rooted path prefix. Adjacent blocks merge unless `--all` is used or a selected `--split` attribute differs. Supports table, raw, pairs, JSON, and summary output. |
| `tail` | Copies selected data as bytes. `--follow=name` cannot follow standard input; read and write errors are reported and contribute to a failing exit status even when other operands produce output. |
| `sort` | Keeps record contents as bytes; plain whole-record order is bytewise, and malformed UTF-8 payloads are not rewritten. Output uses newline separators by default and NUL with `-z`. File and output errors are reported as failures. |
| `syslogd` | Receives datagrams on a Unix datagram socket (default `/dev/log`) and writes one escaped line per accepted message to stdout; it has no other sink and never writes to a syslog socket. Exits nonzero without touching the path when another receiver is live or the path is a symlink, directory or other file; replaces only a dead receiver's socket; removes only the socket it created. A record carries the kernel-reported pid, uid and gid apart from the tag, host and time the sender claims, and no wall-clock time. Payloads are quoted and escaped, so one datagram is one line; a payload that is not valid UTF-8, has more than 64 characters to escape or escapes past 3000 bytes is written as base64 in a `payload_base64` field, and `cut=N` counts bytes beyond 2048. Datagrams over 8192 bytes, with lost control data or with passed descriptors (closed on receipt), and senders over their rate are dropped and counted by reason. A stdout that is not writable drops and counts the record; a failed write is counted and reported on stderr at most once a second. A removed log file stops the service so the supervisor can reopen it. Delivery is not lossless: the kernel drops datagrams from a full socket queue (ten datagrams here, `net.unix.max_dgram_qlen`, not raised by `SO_RCVBUF`) without telling the receiver, and the summary says `kernel_loss=unobservable`. |
| `mdev` | Leaves the caller's umask unchanged. Device and alias parent directories request `0755`, narrowed by the caller's umask; rule commands run in a child with umask `000`. |
| `cat` | With no display transforms, copies input bytes verbatim, including NUL and malformed UTF-8 across chunks. Numbering and show/squeeze options transform only the requested output. |
| `dircolors` | Accepts a non-UTF-8 database path as raw bytes, but the database contents must be valid UTF-8 or the command fails. Diagnostics quote the original path bytes. |
| `ls` | Retains entry names as raw bytes and sorts them bytewise. Quoting styles escape malformed UTF-8 bytes; literal output preserves them, while `-q` renders non-printable bytes as `?`. |
| `lslocks` | Reads lock rows from `/proc/locks` and resolves held OFD owners and descriptor holders through `/proc/PID/fdinfo`. `--output`, `--output-all`, and `LSLOCKS_COLUMNS` select from 13 columns; `--pid` and `--filter` select rows. `--json` emits typed JSON, while `--raw` removes table alignment. If collection yields no rows, no output is written; `--pid` or `--filter` can still produce an empty JSON container after collecting rows. On a terminal, `--notruncate` keeps long `PATH` values. `HOLDERS` lists `PID,COMMAND,FD` for each associated descriptor. A blocked OFD row keeps PID `-1` and reports its owner in `BLOCKER`. |
| `lsipc` | Reads System V queues, shared memory, and semaphore sets from `/proc/sysvipc`. It reads POSIX message queues from `/dev/mqueue`, and POSIX shared memory and semaphores from `/dev/shm`. `--global` reports System V counts and limits from `/proc/sysvipc` and `/proc/sys/kernel`. It reports POSIX message-queue counts and limits from `/dev/mqueue` and `/proc/sys/fs/mqueue`. `--id` selects a System V object. `--name` selects a POSIX object. `--creator` and `--time` add applicable columns. `--output` selects columns. Selecting one ID or name uses pretty output by default. `--list`, `--raw`, `--export`, `--newline`, and `--json` choose other formats. |
| `nl` | `-d` appends `:` when its delimiter argument contains one character. C/POSIX locales count bytes; other locales count UTF-8 characters, with malformed bytes treated as separate units. |
