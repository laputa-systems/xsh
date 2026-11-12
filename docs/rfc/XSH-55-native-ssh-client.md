# XSH-55: Native SSH client

Status: delivered on Linux x86-64 as an experimental client, with the open items
listed under [Open](#open). Native macOS qualification is deferred by the owner
because no macOS host is available. The Rust crate is `crates/xsh-ssh`; the
interface is [Native SSH](../ssh.md).

## Audit

Every row of the RFC and of the roadmap's
[Stage 1 contract](../ROADMAP.md#stage-1-the-remote-execution-contract-over-the-native-client)
is classified against evidence on `master` with a test that fails without the
behavior (D delivered, P partial, M missing). "2026-10-31" is the state the
seven earlier commits left; "Now" is after this campaign. Interop means a real
OpenSSH 10.0 server in the loopback fixture, and Dropbear 2026.94 for the VM drill.

| Requirement | 2026-10-31 | Now | Evidence |
|---|---|---|---|
| Pinned vendored substrate, origin manifest, update steps, builds without a sibling checkout, one Graviola | D | D | `vendor/ORIGIN.md`, `scripts/check-ssh-crypto.sh` |
| Crate independent of XSH values; thin adapter; no async runtime or unbounded thread creation | D | D `Connection::exec_stream` runs one bounded scoped connection thread per call; the XSH adapter adds none | `Cargo.toml`, `src/modules/ssh.rs` |
| `connect` with explicit connection, trust and identity options returning an owned session | P pin and key file only | D | `tests/xsh/ssh-connect.xsh` |
| Bounded captured exec, streaming and file-backed stdin, independent sinks | P stdin past the window failed; no limit test | D | `stdin_larger_than_the_window_is_echoed_exactly`, `output_past_the_capture_limit_is_reported_and_poisons_the_session`, 128 MiB `tests/stream.rs` |
| Negotiated algorithms and authenticated host identity | M | D | `ssh.info`, `hybrid_post_quantum_exchange_is_negotiated` |
| Explicit close and reliable scope cleanup | D | D | `ssh_scope_releases_session`, `ssh_return_from_a_with_body_releases_the_session`, `tests/descriptors.rs` |
| Typed records and errors with execution certainty as a field; handles cannot be forged | P string kinds; handle is a 256-bit random token | P typed `SshError`; the handle is an opaque value holding a process-wide counter, not a token (corrected by the review) | `tests/xsh/ssh-exec.xsh`; a program cannot construct the value |
| Outcomes: status, signal, rejection, failure classes, timeout, cancellation, missing completion | P no rejection or cancellation | D | `tests/protocol.rs`, `signal_is_an_outcome` |
| Execution certainty; no replay after reconnect; no automatic reconnect | P timeouts only | D | `a_disconnect_after_submission_is_potentially_executed`, `a_rejected_exec_request_did_not_run` |
| Argv helper for a named shell, hostile values, NUL refused | M | D POSIX `sh` via `shlex.join` | `tests/xsh/ssh-argv.xsh` |
| Successive commands on one connection; concurrent use of one session rejected; independent parallel sessions | P | D | `one_connection_serves_successive_commands`, `independent_sessions_run_in_parallel`, `ssh_exec_rejects_concurrent_use` |
| Partial and fragmented I/O, simultaneous stdin/stdout/stderr and control traffic, window and packet limits, backpressure, per-direction EOF and close, unsupported requests answered | P blocking exec and no receive limits | D | `tests/protocol.rs`, `saturated_stderr_does_not_deadlock` |
| Sessions in scope cleanup, deadlines and cancellation; nested scope, early return, worker cancellation | P | D a native SIGTERM test cancels a running exec (exit 3 within 5 s, defer run; 30 s without the adapter's interrupt) | above, `tests/hostile.rs`, `ssh_exec_running_when_sigterm_arrives_is_canceled_promptly` |
| Guarantee about remote descendants documented | M | D | `docs/ssh.md` |
| Graviola the only provider; no accept-all verifier | D | D | `scripts/check-ssh-crypto.sh` |
| Hybrid KEX preferred, one classical alternative, Ed25519 | P classical only | D | `kex::tests`, `hybrid_post_quantum_exchange_is_negotiated` |
| Client KEX, exchange hash, server signature check, key derivation, session-bound auth, strict KEX | P signature check untested | D | `a_tampered_host_signature_is_refused_before_authentication`, `a_message_before_kexinit_breaks_strict_key_exchange` |
| Server- and client-initiated rekey with queued traffic; byte, packet and time thresholds; identity kept | P server side only, streaming path only | D | `client_initiated_rekeys_carry_queued_traffic_without_loss`, `the_session_identity_survives_a_client_rekey`, `server_initiated_rekeys_do_not_disturb_captured_output` |
| Invalid lengths, ids, windows, state transitions, authenticated packets | M | D | `tests/protocol.rs` |
| Transport messages (ignore, debug, unimplemented) and a server key exchange accepted in every phase, including authentication | M | D after the review | `tests/authentication.rs` |
| Unsupported CPU fails clearly before any instruction | M | D | `cpu::tests` |
| Pinned fingerprint and `known_hosts`; unknown, changed, revoked fail before authentication; hashed and CA entries rejected; no TOFU, no automatic write | P pin only | D | `tests/connect.rs`, `known_hosts::tests`, `ssh_known_hosts_refuses_unknown_changed_and_revoked_keys` |
| Ed25519 key files; encrypted file refused; ssh-agent signing, bounded, never forwarded | P no agent | D | `tests/agent.rs`, `tests/authentication.rs`, `agent::tests`, `encrypted_key_is_rejected_explicitly` |
| No password, keyboard-interactive, PTY, forwarding; secrets out of Debug and errors; zeroize where practical | P | D key file buffer, shared secret, decrypted payloads and stdin the client reads are wiped, and Graviola wipes its key and cipher state; its SHA-256 contexts and process dumps are not | `debug_does_not_expose_payloads`, `stdin_bytes_are_wiped_before_the_client_frees_them`, `docs/ssh.md` |
| Required tests (pinned and `known_hosts`, auth, binary I/O, status and signal, absent status, premature disconnect, windows, stalled peers, reuse, both rekey directions, fragmented identification, malformed input, deadlines and cancellation, failure after submission) | P | D | `tests/connect.rs`, `exec.rs`, `stream.rs`, `protocol.rs`, `agent.rs`, `descriptors.rs` |
| Feature-disabled and minimal builds | P they warned, which the distribution build denies | D | `cargo check -p xsh --no-default-features` and `--features net,tools`, warning-free |
| Runnable examples: exec, transfer, reuse, errors, parallel hosts | P 1 of 5 | D | `examples/ssh/` |
| Parser and state-machine tests plus targeted fuzz or property tests | M | D | `arbitrary_connection_messages_never_hang_or_panic_the_client`, `kex::tests`, `known_hosts::tests` |
| Public API docs, compatibility matrix, threat model, residual limits | P | D | `docs/ssh.md` |
| Provenance and dependency-graph checks; reproducible interop commands | P no Linux fixture | D | `scripts/check-ssh-crypto.sh`, `crates/xsh-ssh/fixture/ssh-fixture.sh` |
| Before and after size, startup, memory and throughput | P macOS only | D build time deferred: host busy | [Measurements](#measurements) |
| Linux musl and macOS arm64 lanes, distinguishing cross-compilation from execution | P | P x86-64 musl executed: the `xsh-ssh` fixture suites and the native `tests/xsh/ssh-` suite; macOS arm64 and aarch64 cross-checked only | [Measurements](#measurements), [review](#protocol-and-security-review) |
| Stage 1: host identity, pinned verification, changed key fails closed, no agent forwarding or secrets in logs | D | D | as above |
| Stage 1: structured argv, environment, working directory, byte-preserving paths | M | P Laputa `run` takes a UTF-8 argv and a timeout; no environment, working directory or non-UTF-8 path | `tests/system/remote_guest.xsh` in Laputa |
| Stage 1: fixed remote entrypoint with bounded versioned data; one audited quoting helper | M | D | Laputa `system/remote_guest.xsh`, `ssh-argv.xsh` |
| Stage 1: streaming, exit status versus transport failure, backpressure, deadlines, owned cleanup | P | D | above |
| Stage 1: safe staging, digest verification, no-replace publication, cleanup | M | D | Laputa drill; durability and authenticity are not claimed |
| Stage 1: not submitted, completed and unknown outcomes; no replay; remote cancellation needs an acknowledged action | P | P outcomes and reconciliation are delivered; no remote cancellation action exists | `certainty`, `remote_staged` |
| Stage 1 acceptance: real client and server, host-key refusal, slow reader, cancellation, interrupted transfer, uncertain mutation, malformed and oversized requests, changed protocol version | M | D | Laputa `tests/integration/remote_admin.py` and `tests/system/remote_guest.xsh` |
| First consumer: one headless Laputa VM | M | D Linux x86-64 under KVM | Laputa `docs/REMOTE-ADMIN.md` |
| xsh-infra: typed errors carrying certainty | P message text | D `SshError` variants with a `certainty` field | `ssh_exec_timeout_carries_certainty`, `ssh_lost_connection_after_submission_is_a_submitted_transport_error` |
| xsh-infra: parallel connect with sessions the caller owns | M | D `ssh.connect_all` | `ssh_connect_all_opens_hosts_in_parallel_and_hands_them_to_the_caller` |
| xsh-infra: session state after a streaming timeout | M broken pipe | D `ssh.info.usable` and `SshError.Unusable` | `ssh_session_reports_itself_dead_after_a_stdin_timeout` |
| xsh-infra: exec environment | M | D `env` option as SSH `env` requests | `environment_requests_precede_the_command_in_order`, `ssh_exec_environment_is_sent_when_the_server_accepts_it` |

## Protocol and security review

2026-11-05, Linux. A source review of `crates/xsh-ssh` (the client, the packet
and key-exchange layer it calls in the vendored `oxish-proto`, and the
`oxish-graviola` provider glue) and `src/modules/ssh.rs`, read against RFC 4251
to 4254, 4344, 5647, 8308, 8709, 8731, the OpenSSH `PROTOCOL` strict key
exchange and `chacha20-poly1305@openssh.com` notes, and the hybrid
`mlkem768x25519-sha256` draft. It was carried out by the implementing team's own
tooling, not by an independent reviewer, so it does not lift the experimental
label. Graviola's assembly and its constant-time claims were not reviewed.
Each defect below has a test that failed before its fix, except the zeroization row (not observable) and the error-wakeup row (its test covers the decision only).

### Defects found and fixed

| Severity | Defect | Fix and test |
|---|---|---|
| Medium | Authentication read raw packets. Any `SSH_MSG_IGNORE`, `DEBUG` or `UNIMPLEMENTED` (legal at any time, RFC 4253 section 11) or a server-initiated key exchange (section 9) before the service accept or the auth result failed the connection with a protocol error. | Authentication reads through the connection's message filter. `tests/authentication.rs` |
| Medium | A pipe, FIFO or terminal given as `stdin_path` was read with a blocking `read` inside the event loop. A source with nothing to give stopped packet processing, the command deadline and cancellation, and could deadlock against a command that waited for its output to be read. | `InputSource` exposes the descriptor; the client polls it with the connection. `a_stdin_that_never_produces_does_not_defeat_the_command_deadline`, `input_that_arrives_late_on_a_pipe_is_sent_when_it_arrives` |
| Medium | A `stdout_path` or `stderr_path` that is a pipe or FIFO whose reader stopped draining blocked the event loop the same way. | Writes to a file that is not a regular file wait for writability in 50 ms slices, honor the deadline and interrupt, and write at most `PIPE_BUF`. `an_output_file_that_stops_draining_does_not_defeat_the_command_deadline`, `output_to_a_pipe_arrives_whole_when_the_pipe_is_drained` |
| Low | ssh-agent exchanges used per-call socket timeouts: a silent agent ignored cancellation until the timeout ended (5.5 s measured against a 0.3 s cancel), and a dripping agent restarted the timeout on every byte (6.0 s measured against a 1.2 s deadline). | The agent socket is polled against the operation deadline and interrupt. `the_caller_can_cancel_authentication_while_the_agent_is_silent`, `an_agent_that_drips_its_reply_cannot_hold_authentication_past_its_deadline` |
| Low | A service accept naming a service other than `ssh-userauth` was accepted (RFC 4253 section 10). | Refused. `a_service_accept_for_another_service_is_refused` |
| Low | `Transport::recv_newkeys` used a raw receive, so an `IGNORE` or `DEBUG` between a rekey reply and `NEWKEYS` (RFC 4253 section 11) aborted the session. | `NEWKEYS` is read through the key-exchange filter; the initial exchange stays strict. `a_server_message_between_its_rekey_reply_and_newkeys_is_skipped` |
| Medium | A key file was read with no permission check, no size bound and a blocking open, so a FIFO there blocked authentication for good. | Opened non-blocking; must be a regular file under 64 KiB; refused when the caller owns it and group or others can access it, as OpenSSH does. `a_private_key_readable_by_others_is_refused`, `a_fifo_at_the_key_path_is_refused_without_blocking`, `a_key_file_past_the_size_bound_is_refused` |
| Medium | The adapter opened `stdin_path` and created `stdout_path` and `stderr_path` before the deadline, so a FIFO without a peer blocked it. | `exec::open_stdin_file` and `exec::create_output_file` open non-blocking and restore blocking mode; an output FIFO without a reader fails at once. `tests/fifo.rs` |
| Low | A wakeup carrying only `POLLERR` or `POLLNVAL` spun the command loop. | Either flag in the connection poll is a connection failure carrying the pending socket error. `wire::tests`; no peer can produce an error-only wakeup, so the test covers the flag decision, not an end-to-end spin |
| Low | A server-opened channel arriving while the client waited for the channel-open, `env` or `exec` reply failed the session; replies queued while waiting (including a global request's `REQUEST_FAILURE`) were not flushed until the command loop ran; a channel-open failure was not checked against its recipient channel. | Channel opens are refused in the connection filter, the blocking receive flushes its queue, and the failure must name the local channel. `server_opened_channels_and_global_requests_are_refused_while_*`, `a_channel_open_failure_for_another_channel_is_refused` |
| Low | A server that sent the exit status and closed the TCP connection without a channel close produced a transport error. | A clean end of stream after a recorded status or signal returns the result and marks the session unusable, as OpenSSH does; a reset, a failed write or an end without a status stays an error. `a_connection_closed_after_the_exit_status_still_returns_the_result` |
| Low | The key-exchange shared secret, decrypted packet payloads, the receive buffer's consumed plaintext and outgoing data packets were freed unwiped. | `Zeroizing` buffers, and wiping at buffer compaction. No test: wiping is not observable through safe code. |

### Third round: fixed

2026-11-08, Linux x86-64 musl, an additional internal pass by the same
tooling (not the independent review). Each row has a test that failed before
its fix.

| Severity | Defect | Fix and test |
|---|---|---|
| Medium | While a `stdout_path` or `stderr_path` pipe was not draining, the client waited inside the sink write and served no traffic, so a server keepalive (`ClientAliveInterval`) or key exchange went unanswered. | Output for a file that is not a regular file is held and written when its descriptor polls writable, from the loop that serves the socket; window is returned only for bytes that left the client. `server_requests_are_answered_while_an_output_file_is_not_draining`; `output_held_for_a_full_pipe_is_written_after_the_server_hangs_up` guards the end-of-stream path the change introduced |
| Low | A packet whose length is not a whole number of cipher blocks (RFC 4253 section 6, RFC 5647 section 7.2) was accepted. Only an authenticated peer can send one. | Refused in the client transport, which keeps the vendored decoder a copy of ssh-build. `a_packet_length_off_the_cipher_block_is_refused` |
| Low | The 64 KiB stdin read buffer was freed unwiped, and the send buffer could reallocate between copying a data payload in and sealing it. | `Zeroizing` read buffer; the transport reserves room for a whole packet first (`WriteState::reserve`, a local vendored change listed in `vendor/ORIGIN.md`). `stdin_bytes_are_wiped_before_the_client_frees_them`, a test allocator that inspects every block the client thread frees |
| Low | A server identification line of 256 or 257 bytes with its CRLF was accepted (RFC 4253 section 4.2 allows 255). | `an_identification_line_past_255_bytes_with_its_crlf_is_refused` |

Corrections to the earlier review: padding under 4 bytes was already refused by
the vendored decoder (`padding_under_four_bytes_is_refused` now guards it), and
Graviola already wipes its signing-key, key-agreement, AES, GHASH and ChaCha
state on drop; the SHA-256 contexts are what it leaves.

Two untested thresholds now have tests: the packet-count and elapsed-time
rekey limits (`the_packet_count_alone_starts_a_rekey`,
`the_elapsed_time_alone_starts_a_rekey`), and `known_hosts` host matching is
compared with `ssh-keygen -F` over 16 patterns and 14 endpoints
(`tests/known_hosts.rs`).

### Fourth round: fixed

2026-11-10, Linux x86-64 musl. Each fixed row has a test that failed before
its change.

| Severity | Defect | Fix and test |
|---|---|---|
| Medium | A server could send channel data, extended data, EOF, close or a completion before its reply to the `exec` request, and the command failed as a protocol error after submission. RFC 4254 section 5.4 lets the sender continue without waiting for a reply, and section 6.5 orders neither the reply nor the command's traffic; OpenSSH and Dropbear reply first, but the RFC does not require it. | `Channel::exec` holds those messages, bounded by the receive window and 1024 packets, and delivers them in order once the reply succeeds. `output_and_completion_before_the_exec_reply_are_delivered` |
| Medium | `Connection::exec_stream` called the caller's writers synchronously, so one that blocked stopped the connection from answering keepalives, key exchanges and other requests, as a stalled output file once did. | The connection runs on one bounded scoped thread with nonblocking pipes for stdin and both outputs; the calling thread, which owns the non-`Send` writers and input source, feeds and drains them, holding at most the pipe capacity and the receive window. A writer that never returns still keeps the call from returning, but no longer stops service. `a_stalled_writer_does_not_stop_the_connection_from_being_served` |

### Agent establishment and private-key buffers

2026-11-12, Linux x86-64 musl. A separate review reproduced two gaps in the
previous bounds. Independent verification of the fixed implementation remains
pending. The client remains experimental.

| Defect | Fix and regression |
|---|---|
| `Agent::connect` entered a blocking Unix-socket connect before installing the authentication deadline or interrupt. A full listen queue held a 50 ms connect beyond the probe's 300 ms watchdog. | Socket establishment is nonblocking. Linux `EAGAIN` retries a fresh owned socket within the idle timeout, operation deadline and cancellation checks. Writable readiness is used only for a pending connection, whose peer address must then exist. `agent::tests` covers a full backlog, deadline, cancellation and successful connection after draining it. `authentication_deadline_covers_agent_socket_establishment` and `authentication_cancellation_covers_agent_socket_establishment` exercise the real authentication caller. |
| Private-key base64 strings grew while holding encoded key material. Wiping the final allocation left earlier reallocations unwiped. Base64 decode errors also freed partially decoded material before a `Zeroizing` wrapper existed. | Key-file reads and base64 collectors reserve their complete bound before filling. Both decoders write into `Zeroizing` buffers before validation. `private_key_loading_wipes_freed_and_reallocated_buffers_on_success_and_errors` inspects deallocation and reallocation for raw and encoded fixture seed bytes on success, malformed base64, missing footer, encrypted metadata and invalid padding. The original allocator probe reports zero leaked allocations for both the client loader and direct parser, previously two and one. |

The vendored `OpenSshKeyV1::to_bytes` and `pem_encode` serialization paths also
grow sensitive buffers. The client key-loading path calls neither method.
Their reallocation behavior remains outside this loading-path qualification.

The SHA-256 contexts remain unwiped as a dependency limit, not an `xsh-ssh`
defect it can fix without a fork: `oxish-graviola` wraps Graviola's
`Sha256Context` (`vendor/oxish-graviola/src/lib.rs:569`), neither type wipes on
drop (`vendor/graviola/src/high/hash.rs:167`,
`vendor/graviola/src/mid/sha2.rs:13`), and `oxish-proto` creates and consumes
the context inside `HandshakeHash` and `KeyDerivation`, so `xsh-ssh` holds no
copy it can wipe. The fix belongs in ssh-build's one authoritative copy of
those crates.

Contract changes: `auth::authenticate` is crate-private and takes the
connection; `Connection::exec_stream` takes `Option<&mut dyn InputSource>`
(implemented for `File`) instead of `dyn Read`. The later rows add `exec::open_stdin_file`, `exec::create_output_file` and
`Transport::peer_closed`, replaces `Transport::recv_newkeys` with
`Transport::accept_newkeys`, and changes `Packet::payload` to
`Zeroizing<Vec<u8>>`.

### Checked and held

- Strict key exchange: the first packet must be `KEXINIT`, anything but the
  expected message during the first exchange is fatal, both sequence numbers
  reset at `NEWKEYS`, and the exchange hash covers both proposals so a stripped
  indicator fails the host signature.
- Exchange-hash order, mpint for `curve25519-sha256`, string for the hybrid,
  all-zero X25519 refused, hybrid reply length checked, signature verified
  before `NEWKEYS` and after the trust decision on the same blob, a rekey must
  present the first key.
- Packet bounds: length capped at 64 KiB before any wait, AEAD tag checked
  before use, padding and payload arithmetic checked, nonce counters refuse to
  wrap, rekey far below the 32-bit sequence space.
- Channels: recipient checked on every message, receive window and packet size
  enforced, peer window overflow refused, exit status or signal at most once,
  server-opened channels refused, unknown requests answered, data after EOF
  refused, a close without completion is never exit zero.
- Resource bounds: read buffer, reply queue, traffic held during a key
  exchange, agent message and identity count, identification line, captured
  output; no length is allocated before it is checked.
- Trust: `known_hosts` agrees with `ssh-keygen -F`; hashed and CA entries are
  refused; revocation wins; the pinned fingerprint is canonicalized; there is
  no bypass path in `Connection::connect`.
- Secrets: key file text, its base64 and PKCS#8 forms, the shared secret and decrypted payloads are `Zeroizing`; no
  subscriber is installed in `xsh`, and the packet `Debug` forms print lengths;
  errors carry static key-parser strings.

### Risks supported by the code but not demonstrated

- Not zeroized: the SHA-256 contexts that hash the shared secret into the
  exchange hash and keys. This is a Graviola dependency limit: the context is
  Graviola's type with no wipe on drop, wrapped by `oxish-graviola` and consumed
  inside `oxish-proto`, so `xsh-ssh` holds no copy it can wipe; changing it
  needs an ssh-build patch (see [Fourth round](#fourth-round-fixed)). Also not
  zeroized: the caller's own `ExecOptions::stdin` buffer and the XSH value it
  came from, the receive buffer while it grows, and whatever a caller's writer
  keeps after the bounded pipe copies it. Swap and core dumps are outside any
  guarantee.
- A writer passed to `Connection::exec_stream` that never returns still keeps
  the call from returning, although it no longer stops the connection from
  serving the command; a writer that is merely slow holds up to the bounded
  pipe and the receive window.
- An output FIFO whose reader opens later than the command start is refused
  (`ENXIO`) rather than waited for.
- The session handle is a counter into a process-wide registry, and `ssh.exec`
  and `ssh.info` do not compare the calling evaluator with the session's owner
  (by design: a handle works in any evaluator it is passed to). Unforgeability
  rests on programs being unable to construct the value, not on an unguessable
  token as the earlier audit row said.

## Measurements

Linux x86-64 musl, `dist` profile built natively (the pinned-image `cargo dev dist`
did not link on this host, so absolute bytes differ from the [size
record](../RELENG.md#size-record) and only the deltas are comparable).
`dist` builds use `-p xsh -p xsht`, so Cargo's feature unification keeps `ssh`
enabled in them: the shipped products are SSH-enabled.

| Product, stripped | `master` before | After | Change |
|---|---|---|---|
| `xsh` | 31,507,216 | 31,657,872 | +0.48% |
| `xsht` | 42,899,856 | 43,080,080 | +0.42% |
| `xsh` built without `ssh` | | 30,920,016 | `ssh` is 737,856 bytes, 2.4% |

The 5% size gate passes. Beyond the vendored substrate, `ssh` adds `xsh-ssh`,
`oxish-proto`, `oxish-graviola`, `thiserror`, `tracing` and their proc-macro
crates to `xsh`; Graviola is already in the graph. The `xsh-ssh` closure is 29
crates, none of them another cryptographic provider
(`scripts/check-ssh-crypto.sh`).

Startup (`xsh --startup`, CPU time, 40 interleaved runs, `perf-gate-ready` clear
before and after): 0.6 ms median before and after. A 256 MiB `cat` echo through
the loopback fixture (5 interleaved pairs, same gate): median 1.37 s before and
1.47 s after (196 and 183 MB/s, about 7% slower), peak memory 28.3 MiB before and
31.0 MiB after. Build time was not measured: the host was never quiet for the
length of a fat-LTO build, so that gate is deferred: host busy.

Executed lanes: the Linux x86-64 glibc host ran every suite against Alpine
OpenSSH 10.0, Dropbear 2026.94 served the Laputa KVM drill, and the pinned
`xsh-test` musl container ran the `cargo dev gate` cases `test-ssh-unit`,
`test-registry`, `test-signatures`, `test-api`, `test-registry-reference`,
`test-modules-unit`, `check-api-snippets` and `check-docs` (all pass). On
2026-11-05, after the review's fixes, all 113 `xsh-ssh` tests also passed on both
the glibc host and `x86_64-unknown-linux-musl` against the loopback fixture
(`crates/xsh-ssh/fixture/ssh-fixture.sh exec cargo test -p xsh-ssh [--target
x86_64-unknown-linux-musl]`). On 2026-11-08, on an x86-64 musl host, the
third round's tree passed the whole `xsh-ssh` suite for
`x86_64-unknown-linux-musl` and all 34 native `tests/xsh/ssh-` tests with
`verification` products, both against the loopback fixture. Cross-checked only, not executed:
`aarch64-apple-darwin` and `aarch64-unknown-linux-musl` (`cargo check -p xsh-ssh`).

## Open

- Native macOS arm64 qualification, deferred by the owner; no macOS host exists.
  The code that already compiled there compiles by cross-check only.
- An independent protocol and security review before any claim beyond
  experimental. The [source review](#protocol-and-security-review) above found and
  fixed five defects, a second round fixed seven more that it could only support
  from the code, a third internal pass fixed four with failing-first tests, and
  a fourth fixed two; the risks that remain are listed. None of them is the independent review, and
  the tests, fuzz loops and fixtures are not formal verification.
- One command at a time per session, with no channel multiplexing. A session is dead after
  any failure; a timeout does not keep the transport reusable.
- Hashed `known_hosts` names (Graviola has no SHA-1) and `@cert-authority` entries
  are refused. Only Ed25519 host and user keys, no password or
  keyboard-interactive authentication, no PTY or forwarding.
- An acknowledged remote cancellation action; closing a channel does not prove
  that a remote process stopped. Laputa's `run` takes no environment or working
  directory and carries only UTF-8 paths. `ssh.exec` takes no working directory
  and no stream timeout separate from the whole-command timeout.
- Laputa: `first_boot.apply` leaves state-disk files owned by the builder, so the
  drill seeds the operator at the builder's uid; reboot-stable identity, network
  loss, privilege delegation, activation of a staged candidate, and aarch64
  guests are not covered.
- `cargo dev dist` does not link on this host (the container's linker
  environment), so the size record in `RELENG.md` is not refreshed.

## Summary

XSH programs establish authenticated SSH connections, run remote commands,
stream binary input and output, inspect remote outcomes, and reuse connections,
through a native client built on OxiSH's sans-I/O protocol code and Graviola.
A reusable Rust crate (`crates/xsh-ssh`) owns protocol, cryptography and I/O; a
thin adapter exposes a small typed XSH API; ordinary XSH helpers own shell
conventions and convenience policy. The first consumers are Laputa's remote
administration library ([Laputa R6](https://github.com/laputa-systems/laputa/blob/master/docs/ROADMAP.md#r6-separate-disposable-development-from-long-lived-vm-policy))
and [xsh-infra](XSH-56-xsh-infra.md).

This is an implementation campaign: it delivers working code, real
interoperability tests, documentation and commits. It does not build an
interactive `ssh` replacement or an SSH server.

The runtime never invokes `ssh`, `scp` or `sftp`, embeds Python, or depends on
an external SSH client. ssh-build's OpenSSH client is a test oracle against the
same servers, never a production fallback. This replaces the roadmap's earlier
plan to qualify the contract over an established client first; the behavior
contract in the roadmap's remote-execution section is unchanged and is this
campaign's acceptance floor.

## Repositories and baseline

- XSH: this repository.
- [ssh-build](https://github.com/laputa-systems/ssh-build): vendored
  `oxish-proto` and `oxish-graviola`, patched Graviola, crypto dependency
  checks, protocol hardening, strict-KEX requirements, packet limits and tests.
  It holds the one authoritative copy of shared OxiSH/Graviola patches.
- [OxiSH](https://github.com/djc/oxish): upstream reference.

The first increment reads each repository's AGENTS.md and its architecture,
specification, testing, roadmap, security, provenance and dependency policies,
and records exact source revisions. Prior descriptions, including this RFC, are
hypotheses until checked against code. Inspect:

- XSH native module registration, effect tracking, checked IR, resource
  ownership, cancellation and deadlines, streams, and the `xsh-net` boundary;
- ssh-build's vendored crates, patched Graviola, dependency checks, hardening
  and tests;
- upstream OxiSH changes relevant to client support.

OxiSH has been server-oriented. Audit every direction-sensitive assumption:
identification exchange, algorithm negotiation, KEX, key derivation,
authentication, channel IDs, windows and rekeying. A server state machine does
not become a client by flipping a flag.

## Architecture and source ownership

Follow the `xsh-net` pattern:

- `crates/xsh-ssh` implements the client and its I/O driver with no dependency
  on XSH values, source spans or evaluator state.
- A thin XSH adapter registers signatures, converts values, owns handles,
  participates in effects and cancellation, and maps errors.
- Ordinary XSH helpers implement shell conventions and convenience policy.
- Configuration, package, service, inventory, deployment state and rollback
  stay out of the native crate.

Reuse `oxish-proto` and `oxish-graviola` without the OxiSH server, its CLI, Git
execution policy, privilege separation or Tokio. Use XSH's existing I/O
facilities. A bounded, explicit polling or worker design is acceptable; a new
general async runtime, a runtime per command, or unbounded thread creation is
not.

Import a reproducible, pinned subset of ssh-build's vendored source with an
origin manifest and an update procedure. XSH builds without a sibling checkout.
Client-specific code lives in `xsh-ssh`. Preserve upstream licenses and
provenance, keep no independently diverging copy of cryptographic code, and
resolve Graviola once in the enabled runtime dependency graph.

## Public contract

Verify every spelling with `xsht` against current XSH syntax and naming. The
API covers:

- `connect` with explicit connection, authentication and trust options,
  returning an owned session;
- raw SSH exec with bounded captured stdout and stderr;
- exec with streaming or file-backed stdin and independent output sinks;
- negotiated algorithms and the authenticated host identity;
- explicit close and reliable automatic scope cleanup.

Use typed records and errors, not dictionaries or public integer handle IDs.
Session handles cannot be forged.

A remote nonzero exit is an execution outcome, not a transport failure. The
result distinguishes remote exit status, remote exit signal, request rejection,
connection/protocol/authentication failure, timeout or cancellation, and
missing completion information. EOF or channel close without a valid
completion is never exit status zero. Failures carry execution certainty: a
failure before submission differs from a connection lost after the request may
have reached the server, so callers can decide whether a retry is safe. No
command is replayed after a reconnect.

SSH exec transports a command string, not an argv array; raw exec stays
explicit. An argv helper targets a named, supported remote shell, quotes
correctly, rejects NUL, and is tested with empty arguments, quotes,
whitespace, newlines, dollar signs, leading hyphens and command-substitution
text. Nothing implies that an arbitrary login shell has POSIX semantics.

## Connections, channels, streaming and lifetimes

One authenticated connection serves successive commands. The first contract may
serialize commands per session; independent sessions run concurrently through
XSH's existing bounded parallelism. Concurrent use of one handle, if
unsupported, is rejected deterministically, never raced over shared cipher
state or left blocked.

The driver handles partial reads and writes and fragmented or coalesced
packets; progresses stdin, stdout, stderr and control traffic together;
enforces windows and packet sizes; bounds buffering with backpressure; handles
EOF and close independently per direction; answers supported and unsupported
requests correctly; and moves large transfers without buffering the payload. A
transfer path exceeds XSH's captured-output limit without raising it or
loading the file into memory; binary exec-channel streams suffice, SFTP is not
required.

Sessions and live channel or stream handles join XSH resource ownership, scope
cleanup, deadlines and cancellation, with tests for failures in nested scopes,
early returns and worker cancellation. Closing a channel or connection does
not prove remote descendants ended; document the actual guarantee. Remote
signal requests are best-effort unless a server's behavior is established.

## Cryptography and protocol correctness

Graviola is the only in-process SSH cryptographic provider: no OpenSSL,
aws-lc, ring, another SSH implementation or home-grown primitive, as primary
or fallback. This governs the SSH implementation and new runtime dependencies,
not XSH's existing hashing utilities. Reuse the tested SSH packet
construction; `chacha20-poly1305@openssh.com` is not a generic AEAD.

A small explicit algorithm policy prefers the supported modern hybrid KEX,
permits one documented modern classical alternative, and starts with Ed25519
authentication and host keys. Advertise only implemented, tested algorithms;
no legacy fallback.

Implement and test client-side initial key exchange; exchange-hash
construction and server signature verification; directional key derivation and
packet protection; authentication bound to the session identifier; strict KEX
and relevant extension negotiation; server- and client-initiated rekeying with
queued channel traffic; bounded byte, time and packet rekey thresholds before
nonce exhaustion; and malformed or out-of-sequence messages. Session identity
survives rekeys. Never reuse a key/nonce pair, reset sequence numbers
incorrectly, or discard unexpected packets to pass interoperability.

Check the pinned Graviola's actual CPU requirements and fail clearly on
unsupported hardware before any unsupported instruction runs.

## Trust and authentication

Host verification is mandatory and precedes client authentication. Support an
explicit pinned host key or fingerprint and a strictly documented
`known_hosts` mode. Unknown, changed or revoked keys, malformed trust material
and unsupported trust semantics fail closed. For `known_hosts`, handle the
implemented hostname and port forms and applicable revocations. Hashed
entries, certificate-authority markers, wildcards and other features are
either implemented and tested or rejected with an explicit unsupported-trust
error; pinning is an acceptable alternative. No accept-all verifier, insecure
flag, silent trust-on-first-use or automatic `known_hosts` write.

Support Ed25519 key authentication with explicitly supported key formats, and
local ssh-agent signing over its socket so encrypted identities can stay in an
agent. Bound and validate agent messages, select identities deliberately, and
never forward the agent. Dependency and security documentation distinguish
agent-mediated signing from in-process Graviola operations. Never run
`ssh-add` or ask users to strip passphrases; unsupported encrypted key files
fail with a clear error. No password prompting, keyboard-interactive
authentication, PTY, X11, forwarding or `~/.ssh/config` `ProxyCommand`.

Private keys, passphrases, input payloads and secret-bearing arguments stay out
of Debug output, traces, errors and logs. Zeroize sensitive material where
practical and document the remaining ownership limits.

## Testing and qualification

Disposable OpenSSH servers bind to loopback or isolated test networks with
fresh keys and explicit trust material; host verification is never weakened
for a test. ssh-build's OpenSSH client is a comparison oracle against those
servers. The Git-only OxiSH server covers transport, authentication and Git
exec interoperability, and keeps rejecting general shell commands.

Required tests:

- pinned and supported `known_hosts` connections succeed;
- unknown, changed and revoked host keys fail before authentication;
- valid and invalid authentication, and unsupported algorithms;
- exact binary stdin and stdout with separate stderr;
- remote nonzero status and signal outcomes;
- absent exit status and premature disconnect;
- simultaneous input and output larger than the SSH windows;
- stalled peers and saturated stderr without deadlock;
- sequential connection reuse and independent parallel sessions;
- both rekey directions during sustained traffic;
- fragmented identification followed immediately by binary packets;
- invalid lengths, IDs, windows, state transitions and authenticated packets;
- deadlines, cancellation and cleanup without leaked descriptors or threads;
- failure after submission reported as potentially executed.

Add deterministic parser and state-machine tests and targeted fuzz or property
tests in the existing infrastructure; existing tests stay intact. Run the Linux
musl and macOS arm64 qualification lanes, and distinguish cross-compilation
from executed platform tests. Report unavailable hardware and unexecuted lanes.

## Delivery

- The native client and XSH integration, with feature-disabled and minimal
  builds still working; qualified SSH-enabled products use the native backend,
  not a stub or subprocess.
- Runnable `.xsh` examples for exec, binary transfer, connection reuse, errors
  and parallel hosts.
- Public API documentation and a precise compatibility matrix.
- A threat model and residual limitations.
- Source provenance and dependency-graph checks.
- Regression tests and reproducible interoperability commands.
- Before/after build size, startup, memory and basic throughput measurements,
  under the repository's perf-gate rules.
- The concrete API contract xsh-infra and Laputa's administration library
  consume.

The client is labeled experimental. Passing tests is not production security
or formal verification of the protocol; name the protocol and security review
required before stronger claims. Commit coherent changes; do not rewrite
unrelated history, retag the G0 candidate, submit upstream PRs or deploy to real
machines. The new APIs land on `master` outside the frozen G0 candidate, as
XSH-53 and XSH-54 did.
