# XSH-53: Supervised DHCPv4 client and local syslog in XSH

Status: draft

## Summary

Two foreground services, a local syslog receiver and a per-interface DHCPv4
lease client, are written as ordinary XSH programs over typed host operations,
and `xinit` (Laputa) supervises them as separate processes. The runtime gains
only the operations the two programs cannot build from today's primitives. No
Rust protocol code, daemon, async runtime or service framework is added, and
the legacy `linux.dhcp_*` primitives are removed once the client replaces them.

This RFC records the verified baseline, the substrate inventory, the ordered
increments, the two-lane split with file ownership, the `xinit` companion
changes and the test environment. [XSH-53](../../TODO.md) is the syslog lane
(it also owns the shared substrate) and [XSH-54](../../TODO.md) is the DHCP lane.

## Baseline

Revisions: XSH `c2bb3981c72fa71f40e6ef09abb25952ef4f878a`, Laputa
`94859888444a216b3c8cb0d812e280bcf303d294` (clean). Probes ran on the
`target/debug/xsh` built from that XSH revision, in scratch under `.work/`.

What exists and works:

- Generic sockets: `linux.socket` (always close-on-exec; `SOCK_NONBLOCK`),
  `bind`, `connect`, `sendto`, `recvfrom`, `setsockopt_*`, `getsockopt_*`,
  `set_socket_timeout`, `unix.poll_fd`, `unix.read_fd`/`write_fd`/`close_fd`,
  and `linux.net_constants()` (includes `SO_PASSCRED`, `SO_BINDTODEVICE`,
  `SO_BINDTOIFINDEX`, `SO_RXQ_OVFL`, `IP_PKTINFO`, `MSG_TRUNC`, `MSG_CTRUNC`,
  `RTMGRP_*`). Probed: a nonblocking `AF_UNIX`/`SOCK_DGRAM` server binds a path,
  `recvfrom` on an empty queue fails `EAGAIN`, an empty datagram returns empty
  `data` with flags clear, a short buffer sets `MSG_TRUNC` in `flags`, and
  `SO_PASSCRED` yields one control message (level 1, type 2, 12 bytes
  pid/uid/gid). `AF_PACKET` sockets open, and a UDP socket binds port 68 with
  `SO_BINDTODEVICE`, inside `unshare -Urn`.
- `linux.network_dump()`: links, addresses, routes and rules with ifindex,
  table, protocol, scope, type, priority, nexthops, lifetimes,
  `enumeration_succeeded`, `issues`, and detection of `NLM_F_DUMP_INTR`,
  `NLMSG_OVERRUN`, sequence/port mismatch and a message limit.
- Mutation by hand-encoded route netlink from XSH (`core/route.xsh`,
  `core/ifconfig.xsh`) through `linux.netlink_request`.
- `fs.lock` (flock, close-on-exec, nonblocking contention fails `EWOULDBLOCK`),
  `FsRoot.write_atomic`, `Path.write_atomic`, `fs.rename_noreplace`,
  `fs.rename_exchange`, `fs.fsync`, `io.flush_stdout` (probed: `ENOSPC` and
  `EPIPE` are visible at flush), `/proc/uptime`, `/proc/sys/kernel/random/boot_id`.
- Test-only injection: `test.linux_fake`/`test.unix_fake` (harness-installed, no
  environment switch) and typed procs in records. Probed: a record of
  `proc(...)` fields is checked and callable, so production passes live procs and
  tests pass fakes with no ambient redirection.
- Namespace fixtures: `lib.nettools_fixture` builds dummy/veth links, addresses
  and routes in a private user+net namespace; native tests skip with a named
  reason when the kernel refuses.

Defects and limits found (each verified by probe or source reading):

1. A blocked `unix.poll_fd(fd, ["readable"], -1)` ignores `SIGTERM`: the probe
   survived 2 s and needed `SIGKILL`. `poll_fd`, `recvfrom`, `sendto`,
   `accept` and the netlink wait retry `EINTR` with no cancellation checkpoint;
   the SPEC states that blocking host calls observe signals at the next
   checkpoint, which never comes while idle. A foreground service cannot be
   stopped by `SIGTERM` while idle.
2. `time.sleep` polls for cancellation every 10 ms, so a sleeping service wakes
   about 100 times a second. `unix.wait_pid1_event` polls at 100 ms.
3. `linux.network_dump` routes omit `rtm_tos` (`parse_route` skips header byte
   3), so two routes that differ only in TOS are identical records. Laputa
   LAP-32 is blocked on the same gap. `linux.netlink_request` drops the closing
   `NLMSG_DONE`, so its flags are not visible to the caller.
4. `recvfrom` reports `MSG_TRUNC` but never the real datagram length (passing
   `MSG_TRUNC` still returns only the buffered prefix). Received `SCM_RIGHTS`
   descriptors are left open for the script to close from a fixed 1024-byte
   control buffer; a control message with a malformed length ends parsing and
   leaks the remaining descriptors. No test covers descriptor exhaustion.
5. No multi-descriptor poll, timer or signal wait exists; `unix.signal_setup`
   and `take_signals` are daemon-worker-only, polled, and exclusive with
   `on SIG` hooks.
6. `time.monotonic()` is a process-local `Instant`. There is no `CLOCK_BOOTTIME`
   operation; `unix.uptime_seconds` parses `/proc/uptime` into whole seconds.
7. `Path.write_atomic` and `FsRoot.write_atomic` rename over whatever is at the
   destination, including a symlink, and do not fsync the directory. A `rename`
   onto a bind-mounted file fails `EBUSY`.
8. `core/ifup.xsh` DHCP: fixed 5 x 3000 ms retries without backoff or jitter,
   `xid` from `time.now()`, a 1500-byte receive buffer that truncates silently,
   no cookie, chaddr or server check, no NAK handling, no lease time, T1, T2 or
   server-id retention, broadcast-only requests, and a malformed option length
   aborts the parse with `Err` (the client dies on one bad packet). It writes
   `/etc/resolv.conf` directly with `Path.write`, following symlinks and
   replacing whatever exists. `core/ifdown.xsh` `teardown_dhcp` flushes every
   IPv4 address on the interface and deletes every default route through it, and
   never releases the lease: `linux.dhcp_send_release` has no caller, and
   `test_ifdown_dhcp_sends_release` asserts only `link_down`, the flush and the
   route delete. `ifup` runs `stanza.up`/`pre-up` strings through `/bin/sh -c`.
9. The Rust `linux::real::net` module contains a DHCP packet builder
   (`build_dhcp_release`) and broadcast-only socket helpers. That is protocol
   code in Rust and is retired by D7.
10. `lib.nettools_fixture.link_add_veth` encodes `VETH_INFO_PEER` as type 2
    (header value 1), so the kernel auto-names the peer.
11. An `AF_UNIX` datagram socket whose queue is full gives the sender `EAGAIN`
    and gives the receiver nothing: a probe sent 200 datagrams into a 2 KiB
    queue, saw 189 `EAGAIN` at the sender, and `SO_MEMINFO` drops stayed 0. Kernel
    losses on the syslog socket are unobservable at the receiver and must be
    documented as such, not counted.
12. This host already has `/dev/log`. Every syslog test needs a private mount
    namespace; probed: `unshare -Urm` mounts a tmpfs over `/dev`.

Unverified, to be settled by the first real-client test: musl `syslog(3)` opens
its socket nonblocking, so a full queue drops silently; glibc's does not, so a
stalled receiver blocks glibc programs. The receive loop must therefore drain
first and never wait on the sink while the socket has data.

`xinit` facts (Laputa `xinit/`):

- Service stdout and stderr are one `O_APPEND` file, mode 0600, opened by
  `unix.spawn_process_group_log` before spawn; rotation is copy-truncate at start
  and on each scanner pass (default 1 MiB, 3 copies). A removed file keeps
  accepting writes to an unlinked inode without error.
- `readiness: "notify"` gives a pipe in `NOTIFY_FD`; the scanner polls it every
  100 ms and never blocks. Without `notify`, `spawn_service` runs `ready()` in a
  synchronous loop of up to `ready_timeout_ms` (5 s) that stalls the whole
  scanner. `ready` is latched once; a later loss is not reflected.
- `need` gates on state `running`, not on `ready`. The built-in facility names
  `logger`, `net`, `dns`, `firewall` resolve to nothing, but a real service file
  with that name resolves first (Laputa's `net` is one).
- `status()` hook text is appended to `status`; a hook that fails fails the
  whole `status` command. `stop_timeout_ms` defaults to 200.
- Installed service modules live in `packages/*/service.xsh`, outside the
  authorized Laputa scope.

## Substrate inventory

| Candidate | State | Names and exact gap |
|---|---|---|
| Nonblocking Unix datagram bind, send, receive | PRESENT | `linux.socket(AF_UNIX, SOCK_DGRAM \| SOCK_NONBLOCK)`, `linux.bind({family: "unix", address})`, `sendto`, `recvfrom`; `EAGAIN` is an `Err`. Needs documentation of datagram-versus-stream empty reads (datagram: empty `data` is a zero-length datagram, never EOF) and tests. |
| recvmsg metadata, truncation, credentials, owned descriptors | PARTIAL | `MSG_TRUNC`/`MSG_CTRUNC` are in `flags`; `SO_PASSCRED` plus control message (1, 2) gives credentials raw. Gaps: real datagram length; native close of `SCM_RIGHTS` descriptors (all of them, including partial or malformed cases) with a count; control buffer fixed at 1024 bytes; no `SCM_*` constants. Closed by S3. |
| Interface-bound UDP and packet operations | PRESENT (generic) | UDP: `SO_BINDTODEVICE`/`SO_BINDTOIFINDEX`, `SO_BROADCAST`, `SO_REUSEADDR` through `setsockopt_*`; unicast through `sendto` with an inet record; `AF_PACKET` open and raw `bind`/`sendto`/`recvfrom` with `sockaddr_ll` bytes. Missing only constants (`ETH_P_ARP`, `ETH_P_IP`, `PACKET_*`) and a documented `sockaddr_ll` layout, added in D2. The legacy `linux.dhcp_*` set is broadcast-only, truncates at 1500 and builds a packet in Rust; it is removed in D7. |
| Cancellation-aware polling of fds, signals, deadlines | MISSING | `unix.poll_fd` is single-descriptor and ignores signals while blocked (defect 1). Added in S1 as `unix.poll_fds`. |
| Suspend-aware `CLOCK_BOOTTIME` | MISSING | Only `time.monotonic()` (process-local `Instant`) and wall `time.now()` exist. Added in S2 as `time.boottime()`. Boot identity needs no primitive (`/proc/sys/kernel/random/boot_id`). |
| Complete, interruption-aware netlink observation with route identity | PARTIAL | `linux.network_dump` is complete and interruption-aware but omits route `tos`. Added in D1. Events are `netlink_open(NETLINK_ROUTE, RTMGRP_*)` plus `recvfrom`; `ENOBUFS` surfaces as an errno and means "re-observe". The dropped `NLMSG_DONE` flags of `netlink_request` are not needed: mutations use acknowledged non-dump requests and observations use `network_dump`. Not planned. |
| Safe rooted atomic replacement | PARTIAL | `FsRoot.write_atomic` fsyncs the file and renames, but replaces symlinks, does not fsync the directory and cannot require what it replaces. Added in D3: named parameters `mode: Int` and `replace: "any" \| "regular_or_absent"`, parent-directory fsync, and a documented race window that is closed only under the caller's lock. |
| Retained single-owner locks | PRESENT | `fs.lock(path, nonblocking: true)` returns an owned `FsLock`, held until `fs.unlock`, scope exit or process death, and not inherited across exec. Needs a test that a lock bound in `main` survives a service's lifetime and releases on `SIGKILL`; opens without `O_NOFOLLOW`, so lock files live in a root-owned directory. |

Other primitives the plan relies on and found present: `unix.notify_ready` /
`duplicate_fd` (readiness handoff), `fs.stat` (kind, dev, ino, uid),
`Path.chmod`, `FsRoot.read_result(max_bytes)`, `io.flush_stdout`,
`/dev/urandom` through `unix.open_fd`/`read_fd` (there is no random API).

## Design

### Lanes and file ownership

The lanes touch disjoint files. Registry and generated-doc files are shared
hot spots: the lane that owns the operation edits them, regenerates
`docs/SPEC.md` and `docs/reference/*` with `make docs` in the same commit, and
rebases before committing. A merge conflict in a generated file is resolved by
regenerating, never by hand.

Syslog lane (XSH-53) owns:

- Rust: `src/modules/unix.rs`, `src/runtime/process.rs` (signal wake),
  `src/runtime/eval/lowered_run.rs` and `lowered_run/module_unix.rs`
  (`poll_fds`), `src/modules/time.rs` (`boottime`),
  `src/modules/linux/net_prims.rs`, `net_prims/sockets.rs`, `net_prims/fake.rs`
  (`recvfrom`), and the `SCM_*`/`SO_MEMINFO` rows of
  `crates/xsh-registry/src/records/net_sockets.rs`.
- XSH: `core/syslogd.xsh`, `core/lib/syslog_*.xsh`, `core/tests/test-syslog-*.xsh`,
  `core/tests/support/syslog_*`.
- Rust tests: `tests/runtime/` files named `syslog_*` and `poll_*`;
  `tests/xsh/stdlib/unix_poll.xsh`, `tests/xsh/stdlib/time_boottime.xsh`.
- Shared substrate it owns for both lanes: S1 poll, S2 boottime.

DHCP lane (XSH-54) owns:

- Rust: `src/modules/linux/real/netlink.rs` (`tos`), `src/modules/linux/real/net.rs`
  and its `dhcp_*` registrations (removal), `src/modules/fs.rs` and the
  `FsRoot.write_atomic` registry rows, and the `ETH_P_*`, `PACKET_*`, `RTPROT_*`,
  `IFA_*` rows of `net_sockets.rs`.
- XSH: `core/dhcpc.xsh`, `core/lib/dhcp_*.xsh`, `core/ifup.xsh`,
  `core/ifdown.xsh`, `core/lib/nettools_fixture.xsh`, `core/tests/test-dhcp-*.xsh`,
  `core/tests/test-ifup.xsh`, `core/tests/test-ifdown.xsh`,
  `core/tests/support/dhcp_*`.
- Rust tests: `tests/runtime/` files named `dhcp_*` and `netlink_*`.

Neither lane edits `core/login.xsh`, `core/lib/accounts.xsh`, Laputa profiles,
packages, installer or deployment. `xinit/` changes (below) are a third work
item under the syslog lane's schedule (the first consumer), reviewed by the
integrator.

### Ordered increments

Each increment is one commit that passes its acceptance check with
`cargo dev gate PATH...`. "Native" means the Rust boundary tests plus XSH
consumer tests plus docs plus effect/resource registration (`src/syntax/node.rs`,
`tests/xsh/socket-effects.xsh`, `tests/fixtures/modules/standard-api-surface.jsonl`,
`docs/templates/SPEC.md`), all in the same commit.

Syslog lane:

| # | Increment | Owned files | Native extension | Tests | Acceptance |
|---|---|---|---|---|---|
| S1 | Cancellation-aware multi-descriptor poll | `unix.rs`, `process.rs`, `lowered_run*.rs`, registry rows | `unix.poll_fds(entries: List[{fd: Int, events: List[Str]}], timeout: Duration? = null) -> Result[List[{fd: Int, events: List[Str]}], Error]`. At most 64 distinct descriptors. Returns only entries with events, in input order; an empty list means the timeout elapsed; `null` waits indefinitely. A signal handled by the evaluator ends the wait through the same checkpoint as `time.sleep` (cancellation error or hook), never as a success. `EINTR` otherwise resumes to the original deadline. Mechanism: the existing handler also writes one byte to a nonblocking close-on-exec wake descriptor that every call adds to its set; no timeslicing. Effects: `process` and `net`. | Rust: `SIGTERM` during an indefinite poll returns within 100 ms and runs defers; hook runs once; no wake with nothing pending. XSH: ready/timeout/hangup/invalid reporting, duplicate and over-limit rejection, zero timeout. | Idle for 10 s: zero `ppoll` returns (strace count). Probe in this RFC (`SIGTERM` during `poll_fd(-1)`) exits 3 and not by `SIGKILL`. |
| S2 | Suspend-aware clock | `time.rs`, registry rows | `time.boottime() -> Result[Int, Error]`, integer nanoseconds since boot including suspend (`CLOCK_BOOTTIME`; macOS `CLOCK_MONOTONIC`; other hosts `Err`). Distinct from `time.monotonic()`; no origin guarantee beyond boot. | Rust: monotone non-decreasing, within 50 ms of `/proc/uptime`, differs from `time.monotonic()` origin. XSH: type separation (`Int` for both, documented units). A time namespace offset test moves boottime without moving wall time. | Docs state which clock each consumer must use; XSH-35 wording updated to cite it. |
| S3 | Datagram metadata and owned descriptors | `net_prims.rs`, `sockets.rs`, `fake.rs`, registry rows, `net_sockets.rs` | `linux.recvfrom(fd, max_bytes, flags, rights: Str = "keep")` with `rights: "close"`; result gains `size: Int` (real datagram length, equal to `data.len()` on a stream) and `rights_closed: Int`. Control buffer 4096 bytes. Every installed `SCM_RIGHTS` descriptor is closed natively under `rights: "close"`, including the partial and malformed-length cases; `MSG_CTRUNC` stays in `flags`. Constants `SCM_RIGHTS`, `SCM_CREDENTIALS`, `SO_MEMINFO`. Additive: existing callers (`nc`, `traceroute`, `mdev`, `lib.icmp`, `lib.nl80211`) are unchanged. | Rust: sender with 0, 1, 253 and 300 descriptors, plus credentials; `RLIMIT_NOFILE` exhaustion (`EMFILE`) leaves no leaked descriptor (count `/proc/self/fd` before and after); empty datagram versus oversize. XSH consumer test. | Descriptor count identical before and after 10 000 messages carrying descriptors; `size` correct for 0, `max_bytes`, `max_bytes + 1` and 64 KiB datagrams. |
| S4 | Parser and renderer libraries | `core/lib/syslog_parse.xsh`, `syslog_render.xsh` | none | Native: PRI 0..191 and out-of-range, missing/invalid PRI, RFC 3164 and libc header shapes, absent or malformed headers keep bounded content, tag/pid/host limits, trailing NUL, embedded NUL, non-UTF-8, every control character and newline escaped so one datagram renders exactly one physical line, forged record text in a payload stays inside the escaped message field, parser work linear in input (largest-input step count). | `xsht test` of the new files; a property test that any bytes yield one line and never an `Err`. |
| S5 | Limits and accounting | `core/lib/syslog_limit.xsh` | none | Native: token bucket per sender key `(uid, pid)` with a table of at most 256 senders and least-recently-used eviction; global ceiling; aggregate suppression counters by reason (`rate`, `oversize`, `ctrunc`, `rights`, `sink-blocked`, `sink-error`); no allocation growth with sender churn; arithmetic overflow cases. Kernel loss is reported only as `unobservable`. | Fixed-seed flood model: memory table never exceeds its bound; counters sum to the offered load. |
| S6 | Socket lifecycle and service | `core/syslogd.xsh`, `core/lib/syslog_socket.xsh` | none | Native with `test.*` fakes for ordering; see test environment for the real run. | Behavior below. |
| S7 | Real-client, adversarial and measurement suite | `core/tests/test-syslog-*.xsh`, `core/tests/support/syslog_*` | none | Test set below. | All cases run, not skipped, in the privileged Linux gate lane. |
| S8 | `xinit` companion | `laputa/xinit/xinit.xsh`, `docs/`, `tests/` | none | `xinit/tests`. | Companion section below. |
| S9 | Optional: `/dev/kmsg` ingestion | `core/lib/syslog_kmsg.xsh` | none | Sequence numbers, `EPIPE` overrun reported, bounded record size, only after S1 and S6 are accepted. | Owner confirms before start. |

S6 behavior. The service takes a socket path argument (default `/dev/log`) and
a state directory; nothing about the path comes from the environment.
1. Take the exclusive nonblocking `fs.lock` on `STATE/syslog.lock`; failure to
   take it exits nonzero: no competing receiver.
2. Validate the parent directory (owned by root, not group or world writable,
   not a symlink) with `fs.stat` on the opened parent.
3. Bind a nonblocking datagram socket at a private temporary name in that
   directory, `chmod 0666`, `SO_PASSCRED`, `SO_RCVBUF` set to 256 KiB, then
   publish with `fs.rename_noreplace`. `EEXIST` means something is there: probe
   it with a nonblocking `connect`. A live receiver (connect succeeds) exits
   nonzero. `ECONNREFUSED` on a socket-kind entry is stale: replace it with
   `fs.rename_exchange`, then remove the displaced entry only if its
   `(dev, ino)` is the one probed. Any other kind (file, symlink, directory)
   exits nonzero without touching it.
4. Only then write the readiness byte to `NOTIFY_FD` and print one start record
   (boot identity, limits, `kernel_loss=unobservable`).
5. Loop on `unix.poll_fds([socket], timeout)`; read at most 16 datagrams per
   wake (`recvfrom(..., 8192, rights: "close")`), then re-check the wake.
6. Output is one physical line per record on stdout. Record fields: monotonically
   increasing sequence, boot identity, `boottime` elapsed time at receipt,
   kernel credentials (`pid`, `uid`, `gid`), then separately the self-reported
   tag, pid, host and header time, then facility, severity and the escaped
   message (content capped at 2048 bytes with a `cut=N` marker). No wall-clock
   claim is made. The record is flushed with `io.flush_stdout()` after each batch.
   Before flushing, `unix.poll_fd(1, ["writable"], 0)` is checked: a stdout that
   is not writable (full pipe) counts `sink-blocked` and drops the batch rather
   than stalling the receive loop; a flush error counts `sink-error` and
   reports once per second on stderr, never to `/dev/log`. A removed or rotated
   sink is detected by comparing the target of `/proc/self/fd/1` and its
   `(dev, ino)` each second; copy-truncate rotation by `xinit` is tolerated
   because the descriptor is `O_APPEND`.
7. On `SIGTERM`/`SIGINT` stop reading, flush, print final counters, and remove
   the socket only if the path still has the `(dev, ino)` recorded after the
   bind. Restart and a crashed predecessor never remove a socket they did not
   create. Boot never waits on a remote destination: there is none.

Delivery is not lossless. The documented loss sites are: the kernel socket
queue (unobservable), userspace rate limits, the blocked-sink drop and a full
or failed sink.

DHCP lane:

| # | Increment | Owned files | Native extension | Tests | Acceptance |
|---|---|---|---|---|---|
| D1 | Route TOS identity | `real/netlink.rs`, registry rows, `net_sockets.rs` rows | `tos: Int` on `linux.network_dump` routes (additive). | Rust decoder tests for the header pairs `02 18 00 00 fe 02 fd 01 00 00 00 00` and `02 18 00 08 fe 02 fd 01 00 00 00 00 00` with identical attributes (distinct records) and destination length 0; malformed, truncated and interrupted dumps still rejected. XSH consumer test in a netns with a TOS route. | Closes the Laputa LAP-32 observer blocker without weakening dump checks. |
| D2 | Wire library | `core/lib/dhcp_wire.xsh`, `dhcp_arp.xsh`; constants rows | none | Native table tests: encode DISCOVER, REQUEST (selecting, init-reboot, renewing, rebinding), DECLINE, RELEASE with exact bytes and correct `ciaddr`/`siaddr`/broadcast flag; parse and validate every field in the contract: lengths 236+4 up to 1500, op, htype/hlen, cookie, xid, chaddr and client-id match, message type present once, server id, per-option length rules, pad/end, option overload (52) with file/sname, RFC 3396 concatenation, duplicates (singleton conflicts reject), bounded option and list counts, all reasons typed (`WireReject`). Truncation at every byte offset never raises. ARP probe/announce frames. The requested-option list is derived from the implemented-option set. | Corpus passes; fuzz-style generated corpus (fixed seed) yields only `Ok` or typed reject. |
| D3 | Atomic publication | `fs.rs`, registry rows | `FsRoot.write_atomic(path, data, mode: Int = default, replace: Str = "any")` where `replace: "regular_or_absent"` fails on a symlink, directory or special file; parent directory fsync. Race window documented. | Rust: symlink, directory, bind-mount (`EBUSY`), mode, crash-before-rename leaves old content and a stray temporary only. XSH test. | Existing `write_atomic` callers keep behavior (default `"any"`). |
| D4 | Timer and policy machine | `core/lib/dhcp_machine.xsh` | none | Pure `transition(state, event, now) -> {state, effects}`. States: `init`, `selecting`, `requesting`, `bound`, `renewing`, `rebinding`, `init_reboot`, `declined`, `admin_down`. Events are already-validated wire messages, timer expiries, link up/down, ARP result, admin and shutdown. Effects are a closed bounded list (at most 8 per transition): send (broadcast or unicast), arm timer, configure, deconfigure, decline, release, persist, publish. All time is integer boottime milliseconds with checked arithmetic; infinite lease (0xFFFFFFFF), zero lease, T1 >= T2, T2 >= lease and overflow handled by named rules; backoff 4 s doubling to 64 s with jitter from an injected source; unicast renewal at T1, broadcast rebinding at T2, expiry deconfigures. | Exhaustive transition table (every state by every event type), NAK in each state, out-of-order and duplicate replies ignored, never an unbounded effect list. | Table-driven tests only; no clock or socket in the module (compile check: no `time`/`net` effect). |
| D5 | Lease store and ownership | `core/lib/dhcp_store.xsh`, `dhcp_net.xsh` | none | Versioned JSON record (`version: 1`, interface name, ifindex, MAC, client id, boot id, boottime at write, lease fields, owned addresses and routes); read capped at 64 KiB, strict schema, unknown version rejected. A remembered lease from the same boot only seeds `init_reboot`; a different boot id discards the validity assumption; neither is applied without a REQUEST/ACK. `dhcp_net` plans from a `network_dump` filtered by ifindex and the owned record: add address with `NLM_F_EXCL`, routes tagged `RTPROT_DHCP`, delete only exact owned `(ifindex, address, prefix)` and `(ifindex, table, destination, tos, priority, protocol)`; foreign state untouched; the kernel connected route is recorded as kernel-owned and expected to vanish with its address; post-apply re-observation compares; partial failure marks the record `reconcile` and the next wake re-plans from observation. | Native with `test.linux_fake` netlink fixtures, plus namespace tests (D8). Interface recreation (changed ifindex, same name) and link loss tested; interface name alone never confers ownership. | No test mutation touches an address or route absent from the owned record. |
| D6 | Resolver coordinator | `core/lib/dhcp_dns.xsh` | none | One owner of `resolv.conf`, serialized by `fs.lock`. Per-interface contribution files (at most 3 servers, 6 search names, 1 KiB each) under the state directory; composition = operator static file first, then contributions ordered by interface name, capped at 3 servers; published with `FsRoot.write_atomic(..., replace: "regular_or_absent")` carrying a marker first line. A destination that is a symlink, or a regular file without our marker, is foreign: untouched, state `dns-conflict` reported. Removal deletes only the expiring interface's file and recomposes. Tailscale coexistence is "foreign file wins": with tailscaled managing the file, the coordinator neither edits nor restores it and keeps its contributions for when the file is released; split DNS is not implemented. | Native: ordering, bounds, static preserve, marker, conflict, concurrent updates through two lock holders, removal of one contributor. | Foreign symlink and foreign regular file survive byte for byte. |
| D7 | Client loop, ifup/ifdown, retire legacy | `core/dhcpc.xsh`, `core/ifup.xsh`, `core/ifdown.xsh`, `real/net.rs`, registry removals | Remove `linux.dhcp_socket/send/recv/close/send_release`, their effect rows, `build_dhcp_release`, docs and API fixtures (public API removal; open question 2). | `ClientIo`, a record of typed procs (`send`, `receive`, `now_boottime`, `wall`, `random`, `observe`, `apply`, `publish`), is the only seam: production builds it from live operations in `core/dhcpc.xsh`, tests build fakes; no environment variable selects it. Loop: `poll_fds([udp, netlink_events], deadline)`; `ENOBUFS` or a missing ifindex triggers a full re-observation; malformed replies are counted and dropped; one process per interface; exclusive `fs.lock` on `STATE/IFACE.lock`; readiness byte after the interface is resolved, the lock is held and the loop is armed (not after a lease). `ifup`/`ifdown` contract below. | Native with fakes for the loop (duplicate, out-of-order, unrelated, malformed, NAK, server absence, expiry); `test-ifup`/`test-ifdown` updated to assert real behavior (release sent, owned state only removed). | `rg 'linux\\.dhcp_'` finds nothing outside history; macOS `xsht check` of the new files passes. |
| D8 | Namespace integration and captures | `core/tests/test-dhcp-*.xsh`, `core/tests/support/dhcp_*`, `nettools_fixture.xsh` | none | See test environment. First step: correct `link_add_veth` to `VETH_INFO_PEER` type 1 and assert the peer keeps its requested name; existing netns consumers keep passing. | All cases run, not skipped, in the privileged lane. |
| D9 | Optional: option 121 | `dhcp_wire.xsh`, `dhcp_net.xsh` | none | RFC 3442 parsing, router precedence (121 overrides 3), bounds, prefix validation, requested only when implemented. | Owner confirms before start. |
| D10 | VM qualification and measurements | `laputa/xinit/tests/fixtures/*dhcp*_guest.xsh` | none | VM list below. | Recorded numbers in `docs/TESTING.md`. |

`ifup`/`ifdown` lifecycle (D7), with no Laputa policy in the XSH core:

- The supervised `dhcpc` owns the interface lock and the owned-resource record.
  `ifup IFACE` for an `inet dhcp` stanza takes the same lock nonblocking: held
  means a supervised client exists, so `ifup` removes any administrative-down
  mark, waits a bounded time (default 10 s, `--wait`) for the client's published
  state to leave `admin_down` and reports the lease state, and never starts a
  second acquisition. Not held means no client: `ifup` runs the retained
  one-shot mode (`dhcpc --once`), which uses the same wire, machine, store, net
  and DNS libraries, acquires one lease, configures it, records ownership and
  exits. Documented limit: no renewal, so a one-shot lease expires unmanaged.
- `ifdown IFACE`: if the lock is held, it writes the administrative-down mark
  (atomic file in the state directory) and waits a bounded time for the client
  to release, deconfigure only its owned resources and publish `admin_down`. The
  client stays running and idle, so `xinit` is never fought and no orphan
  remains. If not held, `ifdown` performs the same release/deconfigure from the
  owned record directly. Neither flushes an interface or removes a foreign
  default route. Static stanzas keep today's behavior. Hook strings keep their
  current shell execution, which is a separate defect for the owner.
- A crash leaves the lock released by the kernel and the record intact; the
  restarted client revalidates the record against observation.

### `xinit` companion (Laputa `xinit/` only)

| # | Change | Reason |
|---|---|---|
| X1 | Make hook-based `ready()` non-blocking: evaluate it from the scanner pass instead of a synchronous loop in `spawn_service`. | An absent DHCP lease must not stall the scanner, rescue or login for `ready_timeout_ms`. |
| X2 | `status()` hook failure degrades to `status-error=...` appended to the line; the command still succeeds. | Status must stay usable when a state file is missing. |
| X3 | Reference service modules `xinit/docs/syslog-service.xsh` and `dhcp-service.xsh` (service `logger`, and `dhcp-IFACE`), validated by `xinit check`. They use `readiness: "notify"`, `restart.mode: "always"`, a `stop_timeout_ms` of 2000, no `need` on DHCP, and a `status()` hook that reads the client's published state. | Four levels stay distinct: process running (`state`, `pid`), socket or client ready (notify byte), lease configured (`status()` text `lease=bound ...`), Internet reachable (never claimed). |
| X4 | Tests: a real `logger` service resolves before the facility and gates `need`; crash restart; bounded stop of a service that ignores `TERM`; scanner not stalled by a never-ready service; rotation by copy-truncate with a live `O_APPEND` writer. | Closes the verified gaps and anything left unverified in `docs/SUPERVISION.md`. |
| X5 | Documentation: logging integration (stdout is the only channel, mode 0600, rotation semantics, an unlinked log is undetectable by xinit), readiness vocabulary. | Contract for service authors. |

Installing these as `packages/*/service.xsh` entries is outside the authorized
scope and is left to the Laputa owner.

### Test environment

Everything that can be native is native: parsers, machine, limits, store, DNS
composition, planning, and the client loop through `ClientIo` fakes
(`xsht test`, gate `native-core/tests`). Rust tests cover only host boundaries:
poll wake, `recvfrom` descriptor handling, netlink decoding, `write_atomic`.

Namespaces (unprivileged `user`+`net`+`mount` through `linux.run_in_namespaces`,
as `test-ip.xsh` does; a skip needs a named kernel reason and the privileged
gate lane must report zero skips for these files):

- Syslog: `mount` namespace with a tmpfs over `/dev` so the real `/dev/log`
  path is private, since libc `syslog(3)` hard-codes it and the host already has
  one. A real C sender compiled by the image's `clang-23` against musl;
  concurrent senders with distinct uids (user namespace mapping), empty,
  malformed, binary, multiline, oversized and truncated messages, `SCM_RIGHTS`
  injection and descriptor exhaustion with a small `RLIMIT_NOFILE`, floods, a
  full pipe, `/dev/full`, a removed and a copy-truncated sink, restart over a
  stale socket, a live competing receiver, `SIGTERM` during idle and during a
  flood, and readiness observed not to precede bind. Descriptor passing uses
  `python3` (present in the image). A glibc client needs a glibc userland; the
  image has none (decision).
- DHCP: a veth pair across two network namespaces, the client in one and a
  controlled server in the other. The server is a test-only XSH program
  (`core/tests/support/dhcp_server.xsh`) built on the same wire library, driven
  by a script of replies (short leases; renew, rebind, NAK, expiry, changed DNS
  and routes, duplicate, out-of-order, unrelated xid, malformed, silence) and
  an independent wire check by a frame recorder on an `AF_PACKET` socket that
  writes pcap. Link flap, interface delete and recreate (new ifindex), foreign
  addresses/routes/Tailscale-like routes preserved, resolver conflicts,
  injected partial-apply failures, lost netlink notifications (an `ENOBUFS`
  fixture through `test.linux_fake`), concurrent `ifup`, crash and restart. A
  boottime jump uses a time namespace offset; a wall-clock jump is a native test
  of the machine with a fake wall source, never a change to a host clock.
- No `dnsmasq`, `udhcpd`, `tcpdump` or `kea` is on the host or in the image
  (`busybox` here has `udhcpc` only). An independent reference server needs an
  image package (decision); without it the XSH server is the only server and
  independence rests on the recorded pcap bytes checked against RFC examples.

VM (the existing disposable QEMU guest harness, driven from `xinit/tests`):

- VM pause and resume with QMP `stop`/`cont`, measured for monotonic, boottime
  and wall deltas and for lease expiry decisions; wall-clock step with the guest
  clock restored afterward; documented numbers, no assumption that a backend
  excludes the pause.
- Real virtio-net hot-unplug/replug (ifindex change) and link down/up.
- The scanner as PID 1: crash restart, `SIGTERM` reverse-order shutdown, bounded
  stop, rescue/login reachable with no DHCP server, `logger` ready before
  dependents, log rotation under load.
- musl and, if the decision allows, glibc clients against `/dev/log` inside the
  guest.
- aarch64 runs the same native and namespace tests in the pinned image (musl
  target); Laputa's aarch64 guest qualification is deferred by its TODO and is
  not claimed.

Measurements (after S7/D8, recorded, not gating): idle CPU and wakeups over
60 s (voluntary context switches from `/proc/PID/status`, `ppoll` return count
from `strace -c`), peak RSS (`VmHWM`), descriptor count before and after a
flood, sustained syslog throughput and drop counters from a C flooder with
several uids, and DHCP time to reconfigure after server return, link flap and
VM pause. Time-based figures follow the `perf-gate-ready` rule.

## Alternatives considered

- Adopt a maintained DHCP client or `dhcproto` (Laputa roadmap R2 suggests
  evaluating one first): rejected by the owner's contract, which requires the
  logic in XSH. Recorded here because Laputa's roadmap text still says
  otherwise.
- Time-sliced polling (for example 250 ms) instead of a wake descriptor in S1:
  simpler, but it makes idle wakeups nonzero and repeats the `time.sleep`
  pattern the measurements are meant to avoid. Rejected.
- Extend `linux.dhcp_*` instead of retiring it: keeps protocol bytes in Rust and
  a second parser. Rejected.
- A `netlink_dump` operation returning `NLMSG_DONE` flags: not needed once
  `network_dump` carries `tos`. Rejected for now.
- Per-interface xinit instances through templating: new `xinit` machinery.
  Rejected; one module per interface.
- Counting kernel socket drops: the probe shows they are not exposed to the
  receiver. Documented as unobservable.

## Open questions

Owner decisions are listed in the report that accompanies this plan; the
design above assumes the stated defaults.

1. New APIs conflict with the G0 admission list in `docs/ROADMAP.md`. Default:
   proceed under this campaign's authorization and record the exception.
2. Removing the five `linux.dhcp_*` operations is a public API removal. Default:
   remove in D7 after `ifup` migrates.
3. Applet names `syslogd` and `dhcpc` and service name `logger`. Default as
   written.
4. An image change for a glibc client and a reference DHCP server touches the
   pinned `Dockerfile.test` that gates work also edits. Default: no image change;
   the glibc test runs only if the owner provides a glibc image, and the XSH
   server is the only server.
5. Foreign `resolv.conf` policy is "refuse and report"; an image-provided plain
   file therefore blocks DHCP DNS until the operator moves it aside. Default as
   written.
6. `/dev/kmsg` (S9) and option 121 (D9) start only on the owner's go-ahead.

## Decision

Accepted on 2026-10-28 with these answers to the open questions:

1. Proceed. The G0 candidate is a frozen commit, so this work on `master` does
   not change it; the exception is recorded in `docs/ROADMAP.md`.
2. Remove the five `linux.dhcp_*` operations in D7 as a breaking change, so one
   wire implementation remains.
3. Names as written.
4. `Dockerfile.test` gains `dnsmasq` as the independent reference DHCP server and
   `tcpdump` for captures; the DHCP lane owns that edit. The glibc client test
   runs as a documented one-off integration in a disposable glibc container that
   shares only the test socket directory, not as a gate case.
5. Refuse and report.
6. S9 starts after S6 and S7 are accepted; D9 starts after D8 is accepted.
