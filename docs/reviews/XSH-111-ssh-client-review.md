# XSH-111: Protocol and security review of the native SSH client

## Scope and revision

- XSH `master` at `a975b266f`, reviewed on branch `review/xsh-111`.
- `crates/oxish-client` is ssh-build `client/` at
  `c9733b574409e9c18c2b0d10b868534f64dd97bc` (`crates/oxish-client/vendor/ORIGIN.md`).
  The vendoring identity was taken as given; `scripts/vendor-ssh-client.sh verify` was not run.
- Read in full: `src/{client,kex,wire,stream,channel,auth,agent,known_hosts,host_key,exec,session,error,cpu}.rs`,
  `vendor/oxish-proto/src/{io,crypto,lib}.rs` and the parts of `named.rs`, `auth.rs`
  and `key_exchange.rs` the client calls, `vendor/oxish-graviola/src/lib.rs`,
  `src/modules/ssh.rs`, `docs/ssh.md` and `docs/rfc/XSH-55-native-ssh-client.md`.
- Graviola itself was out of scope. Its call sites were reviewed: nonces, sequence numbers,
  key-derivation inputs, tag-check order, constant-time tag comparison and wiping on drop.
- References: RFC 4251 to 4254, 4344, 5647, 8308 and 8731, OpenSSH `PROTOCOL` (strict key
  exchange, `chacha20-poly1305@openssh.com`) and the `mlkem768x25519-sha256` draft.

Every demonstrated finding has a probe in `crates/oxish-client/tests/review_probes.rs`.
Each probe drives the scripted peer in `tests/support/mod.rs` and asserts the correct
behaviour, so it fails against this revision. Run them with:

```sh
cargo test --offline --locked -p oxish-client --test review_probes
```

The expected result is 5 failed and 0 passed.

| # | Finding | Severity | Status |
|---|---|---|---|
| 1 | A TCP end after the exit status returns truncated output as a complete success | Medium | demonstrated |
| 2 | Requests held before the exec reply bypass the memory bound (about 64 MiB per session) | Low | demonstrated |
| 3 | A client rekey that starts before authentication finishes rejects in-flight replies | Low | demonstrated |
| 4 | Errors after submission lose their variant, so a host-key substitution at a rekey reaches XSH as `Transport` | Low | demonstrated |
| 5 | The client never closes a channel first, so a server that waits for the client's close times the command out | Low | demonstrated |
| 6 | `docs/ssh.md` contradicts the client on cancellation signals and on session poisoning | Low | reasoned |

## 1. A TCP end after the exit status returns truncated output as a complete success

- Severity: medium
- Status: demonstrated
- Location: `crates/oxish-client/src/stream.rs:807`, with `crates/oxish-client/src/wire.rs:601-606`. The behaviour is documented in `docs/ssh.md:98-100`.
- Probe: `a_connection_end_between_the_exit_status_and_channel_eof_is_not_a_complete_result`

`run` turns any error into `Ok(ExecResult)` when two conditions hold: the socket read
returned 0 (`peer_closed`) and an exit status or signal has been recorded. It does not
require the authenticated `SSH_MSG_CHANNEL_EOF` or `SSH_MSG_CHANNEL_CLOSE`.

A TCP FIN carries no SSH authentication. An on-path attacker, which the threat model in
`docs/ssh.md:257-259` covers, can inject one at any point. OpenSSH sends `exit-status` as
soon as it reaps the child. `session.c` `session_exit_message()` deliberately leaves the
channel's read side open because "there could be some more data waiting in the pipe".
`channels.c` delays EOF until stderr has drained.

The exit status therefore routinely precedes the tail of the output: up to the client's
2 MiB window plus the pipe buffer for bulk output such as `cat`, `tar` or a `stdout_path`
transfer. The exit-status packet is easy to spot. Its length travels in clear under
`aes128-gcm`, and segment sizes reveal it under ChaCha20.

Scenario: the server sends output, then `exit-status 0`, then more output, EOF and CLOSE.
The attacker sends a FIN right after the exit-status packet. `ssh.exec` returns
`{exit_code: 0, stdout: <prefix>}`, or leaves a truncated `stdout_path` file, with no error.
Only `ssh.info(session).usable == false` hints at the cut.

In the probe, the peer sends data and `exit-status 0`, then hangs up without EOF. The client
returns `Ok(ExecResult { status: Exited(0), stdout_len: 19 })`.

A reset is not a usable alternative for the attacker: it yields an error, because only a
clean end sets `peer_closed`.

Fix: accept the status-then-disconnect result only when `channel.eof_received` is also
true. EOF covers normal and extended data, so nothing can be missing after it. Otherwise
return `Error::Submitted`.

Update `docs/ssh.md:98-100`. Update `a_connection_closed_after_the_exit_status_still_returns_the_result`
(`tests/protocol.rs:117`), which hangs up without EOF and so encodes the current behaviour.

## 2. Requests held before the exec reply bypass the memory bound

- Severity: low
- Status: demonstrated
- Location: `crates/oxish-client/src/channel.rs:253-294`, in particular the pushes at lines 282-289
- Probe: `requests_held_before_the_exec_reply_stay_within_the_memory_budget`

`Channel::start` holds messages that arrive before the exec reply. Two bounds apply: 1024
packets (`MAX_EARLY_PACKETS`, line 30), and `LOCAL_WINDOW` bytes for data and extended data.

`CHANNEL_REQUEST`, `CHANNEL_EOF` and `CHANNEL_CLOSE` packets are held with no byte
accounting. `pre_reply_recipient` reads only the recipient, so an unknown request can carry
up to about 64 KiB of body.

Scenario: a server sends 1023 `CHANNEL_REQUEST` packets of about 64 KiB before
`CHANNEL_SUCCESS`, and the client holds about 64 MiB for that session. The probe sends 1000
requests of 60,000 bytes and measures 64,446,464 bytes of peak growth.

The budget the project's hostile tests enforce is 24 MiB (`tests/hostile.rs:18`). A control
run sent the same requests after the reply and measured 131,072 bytes; the control was not
kept.

`docs/ssh.md:259-266` lists the memory bounds, and this case is not among them. The RFC row
says "bounded by the receive window and 1024 packets", which holds for data packets only.

Only an authenticated server can do this, and the threat model excludes compromised
endpoints, hence low severity. The documented bound is still false. It multiplies across
concurrent sessions (`ssh.connect_all`, `par-map`).

Fix: charge every held packet's payload against one byte budget. The rekey queue already
does this: `kex.rs:400-416` uses `LOCAL_WINDOW + 256 KiB` over all packets.

## 3. A client rekey that starts before authentication finishes rejects in-flight replies

- Severity: low; reachable through the library, not through XSH's defaults
- Status: demonstrated
- Location: `crates/oxish-client/src/kex.rs:428-439` (with `is_connection_layer`,
  `kex.rs:362-380`), reached from `Connection::next_message`
  (`crates/oxish-client/src/client.rs:330`) inside `auth::authenticate`
  (`crates/oxish-client/src/auth.rs:212`, `264`)
- Probe: `a_client_rekey_due_during_authentication_keeps_the_in_flight_service_accept`

Before the server's `KEXINIT` arrives, a client-initiated rekey queues only
connection-layer messages and fails on anything else. RFC 4253 section 7.1 restricts only
the party that has sent `KEXINIT`. The server may keep sending `SERVICE_ACCEPT`,
`USERAUTH_*` or banners until it has seen the client's `KEXINIT`.

`rekey_if_due` runs in every `next_message`, including during authentication.

Scenario: a library caller connects, and `RekeyLimits.interval` (default one hour), the byte
limit or the packet limit is reached before `authenticate`. The client sends
`SERVICE_REQUEST` and then its `KEXINIT`. The server's `SERVICE_ACCEPT` is already in
flight, so authentication fails with
`Protocol { detail: "expected KeyExchangeInit, got ServiceAccept" }`. The probe uses a
300 ms interval and a 400 ms pause.

XSH authenticates right after connecting (`src/modules/ssh.rs:276-280`). Its default limits
cannot be reached within the 30 s authentication deadline unless the server itself sends
about 1 GiB of banners.

Fix: before the server's `KEXINIT`, queue every message outside the key-exchange range
(20-49), except `DISCONNECT`, and deliver them in order through `pending` as connection
traffic already is.

## 4. Errors after submission lose their variant

- Severity: low
- Status: demonstrated
- Location: `crates/oxish-client/src/error.rs:248-261` (`after_submission`), applied at
  `crates/oxish-client/src/stream.rs:613-618`; mapped in `src/modules/ssh.rs:166`; claimed in
  `docs/ssh.md:117`
- Probe: `a_host_key_change_at_a_rekey_after_submission_is_still_a_host_key_mismatch`

After the exec request is sent, `after_submission` rewrites every error except a few into
`Error::Submitted { detail: other.to_string() }`. The rewritten variants include
`HostKeyMismatch`, `HostKey`, `Protocol`, `Channel` and `Authentication`.

XSH maps `Submitted` to `SshError.Transport`. The variant now depends on timing:

- When a rekey presents a different host key while a command runs, the result is
  `SshError.Transport`, described as "a lost connection". The probe gets
  `Submitted { detail: "host key mismatch: expected SHA256:..., got SHA256:..." }`.
- The same substitution at a rekey between commands, during `Channel::open`, is
  `HostKeyMismatch` with `NotSubmitted`.

`docs/ssh.md:117` says the host-key variants "all happen before authentication". A rekey
mismatch, which the client correctly refuses, happens after it. A program that alerts on
`HostKeyMismatch` and retries `Transport` treats a mid-command key substitution as a network
blip. The existing test `a_rekey_with_a_different_host_key_is_refused_even_when_both_are_trusted`
(`tests/hostile.rs:442`) avoids the issue by matching only the message text.

Fix: carry certainty beside the variant rather than in place of it. For example, use one
error struct with a kind and a certainty, or give `Submitted` a boxed cause. Derive the XSH
variant from the kind. Correct `docs/ssh.md:117`.

## 5. The client never closes a channel first

- Severity: low
- Status: demonstrated
- Location: `crates/oxish-client/src/stream.rs:899-906`
- Probe: `a_finished_channel_is_closed_by_the_client_when_the_server_waits_for_it`

`drive` sends `CHANNEL_CLOSE` only after it receives the server's. It returns only when both
sides have closed. RFC 4254 section 5.3 lets either side initiate the close; the receiver
must answer it. OpenSSH's client closes once it has sent and received EOF.

Scenario: an RFC-compliant server sends `exit-status 0` and EOF, then waits for the client's
`CHANNEL_CLOSE`. `ssh.exec` waits until `timeout_ms` and reports `Timeout`/`Submitted`,
although the outcome is known. Without `timeout_ms` it waits until a signal. In the probe the
client returns `Timeout { certainty: Submitted }` after 3.006 s.

OpenSSH and Dropbear close first, so no tested server shows this.

Fix: send `CHANNEL_CLOSE` once `eof_received && eof_sent` and the held output has drained.
Keep waiting, within the deadline, for the server's mandatory reply, so channel id 0 is
reused only after both closes.

## 6. Documentation that contradicts the client

- Severity: low
- Status: reasoned

1. `docs/ssh.md:210-211` says that "the client sends no signal to" a timed-out or cancelled
   command. A cancelled command does get one. `ssh.exec` passes an interrupt
   (`src/modules/ssh.rs:483`), and the interrupt ends `drive` with `Cancelled`. `run` then
   calls `cancel_remote` (`crates/oxish-client/src/stream.rs:803-805`), which sends
   `signal TERM` (`stream.rs:841`, `channel.rs:211-219`) and waits up to 2 s.
   Only a timeout sends nothing. An operator who relies on the documented "no signal" to let
   a cancelled long job keep running gets it terminated.
   Fix the documentation to match XSH-110.
2. `docs/ssh.md:127` says that "after any failure the session is dead". Some failures leave
   the session usable: environment validation (`stream.rs:597`, before `begin_operation`),
   a NUL in the command, `Busy`, and `Local` file-open failures
   (`src/modules/ssh.rs:433-472`). Keeping the session is the better behaviour; narrow the
   sentence to failures after the request reached the connection.

## Cleared

### Key exchange

- The exchange hash takes V_C, V_S, I_C, I_S, K_S, Q_C, Q_S and K in that order
  (`kex.rs:475-535`). I_S is re-encoded with its message byte (`wire.rs:48-53`).
- `curve25519-sha256` encodes K as an mpint (`kex.rs:166-171`). The hybrid sets
  K = SHA-256(K_PQ || K_CL) and encodes it as a string, as OpenSSH does (`kex.rs:172-193`).
- The hybrid reply must be exactly 1088 + 32 bytes (`kex.rs:178`). An all-zero X25519 output
  is refused in both modes (Graviola `mid/x25519.rs:53-56`).
- The trust decision and the signature check over H both complete before `NEWKEYS`
  (`kex.rs:537-564`). The signature algorithm must equal the key algorithm and the
  negotiated `ssh-ed25519` (`host_key.rs:115-150`).
- Negotiation follows the client's preference order, which puts the hybrid first. Only AEAD
  ciphers are offered, and `none` compression is required (`kex.rs:287-349`). Stripping
  names from either proposal changes I_C or I_S, so the host signature fails.
- Strict key exchange:
  - It is required on the first exchange (`kex.rs:332-335`, `498`).
  - During that exchange any packet other than the expected one is fatal, including
    `IGNORE` (queue `None`, `kex.rs:418-441`).
  - The send sequence number resets after `NEWKEYS` is sent, and the receive sequence number
    after `NEWKEYS` is received (`wire.rs:831-844`).
  - Rekeys inherit the setting (`kex.rs:499`).
- A guessed `first_kex_packet_follows` packet is refused (`kex.rs:288-290`).
- Key derivation (`crypto.rs:228-276`):
  - The letters A to D give the IV and key for each direction; no integrity keys are needed
    under AEAD.
  - The base is K || H || letter || session_id, and longer outputs extend as the RFC
    specifies.
  - Rekeys keep the first session id (`kex.rs:549-558`).
- Ephemeral keys are fresh for each exchange.

### Rekeying

- The limits are 1 GiB, 2^28 packets and one hour, with both directions summed
  (`client.rs:30-38`). The counters reset when the new opener is installed
  (`wire.rs:857-867`).
- The limits sit far below the 32-bit sequence space:
  - The ChaCha20 state refuses to go past 2^32 - 1 packets per key
    (`oxish-graviola/src/lib.rs:267-273`).
  - The GCM invocation counter uses `checked_add` (`lib.rs:380-392`).
  - Each new key starts a fresh nonce state.
- Client-initiated rekeys:
  - Connection traffic sent before the server's `KEXINIT` is queued, bounded by bytes over
    all packets (`kex.rs:382-416`), and delivered in order (`client.rs:450`).
  - After the server's `KEXINIT`, any connection message is refused.
  - Server-initiated rekeys start in that refusing state (`Queue::new(true)`).
  - `IGNORE`, `DEBUG` and `UNIMPLEMENTED` are skipped and `DISCONNECT` is fatal.
- During an exchange the client sends only `KEXINIT`, `ECDH_INIT` and `NEWKEYS`. Replies to
  queued requests go out after the exchange, through `filter`.
- A rekey must present the first host key (`kex.rs:522-529`) and passes the trust check
  again.
- Each exchange is bounded by `bound_phase` (`client.rs:429-452`).
- A rekey cannot re-enter itself. `kex::receive` reads the transport directly, and
  `KEXINIT` never enters `pending`.

### User authentication

- The signed data is session_id || 50 || user || `ssh-connection` || `publickey` || TRUE ||
  algorithm || blob (`oxish-proto/src/auth.rs:630-644`). The request carries the same blob
  (`auth.rs:240-261`).
- The service accept must name `ssh-userauth` (`auth.rs:214-224`).
- Banners are skipped and never displayed. A flood of them is bounded by the
  authentication deadline.
- Partial success is reported as `Authentication` (`auth.rs:268-275`). This is correct for a
  client that has a single method. `PK_OK` is ignored.
- Agent:
  - The identity is chosen by the SHA-256 fingerprint of its blob and must be of type
    `ssh-ed25519` (`auth.rs:170-195`).
  - Replies are bounded to 256 KiB and 1024 identities, and a zero-length reply is refused
    (`agent.rs:186-219`).
  - The signature must be `ssh-ed25519` over 64 bytes (`agent.rs:222-242`).
  - The agent is never forwarded; no `auth-agent` request exists.
- Agent socket trust: the client does not check the socket's owner, and neither does
  OpenSSH. Whoever controls the path learns the user name, the session id and the public
  key. A forged signature fails at the server.
- Key file: it must be a regular file of at most 64 KiB with owner-only permissions. It is
  opened nonblocking, and an encrypted key is refused (`auth.rs:54-123`).

### Channels

- Every channel message's recipient is checked (`channel.rs:337-343`,
  `stream.rs:993-1057`).
- Receiving data:
  - Each packet may carry at most 32 KiB, checked against the window with `checked_sub`
    (`channel.rs:347-357`).
  - Window is returned only for bytes that have left the client. `consumed` never exceeds
    the window, so `consumed +=` and `local_window +=` cannot overflow
    (`channel.rs:360-377`).
- Sending data:
  - The packet size is the minimum of the peer's window, the peer's maximum packet size and
    32 KiB (`channel.rs:380-395`).
  - Window-adjust overflow is refused (`channel.rs:323-334`).
  - A maximum packet size of 0 is refused.
- Data after EOF or CLOSE is refused (`stream.rs:1096-1103`). Unknown extended-data types
  still consume window.
- `exit-status` and `exit-signal` are accepted at most once. Missing completion is never
  exit 0 (`stream.rs:555-567`, `1074-1087`). The signal name is sanitized.
- Unknown channel requests get a FAILURE reply when they want one. Global requests and
  server-opened channels are refused (`client.rs:370-401`). Replies stop the reading at
  64 KiB (`wire.rs:893-899`).
- The `signal` request has the form RFC 4254 section 6.9 specifies (`channel.rs:211-219`).
  The grace wait is bounded to 2 s with the interrupt removed (`wire.rs:394-399`).
- Local channel id 0 is reused only after both CLOSEs, so a late message cannot reach the
  next command.
- Certainty:
  - `mark_submitted` runs before the exec request is written (`channel.rs:250-251`).
  - `begin_operation` resets the certainty for each command (`wire.rs:377-384`).
  - A rejected `env` or `exec` request is correctly reported as `NotSubmitted`.

### Host verification

- `known_hosts` matching (`known_hosts.rs`):
  - Host names are lowercased.
  - The `[host]:port` form applies only when the port is not 22.
  - Comma lists, `*` and `?`, and `!` exclusions are supported.
  - Hashed and `@cert-authority` entries, and unknown markers, are refused.
  - The key-type field must equal the blob's type.
- `@revoked` wins over any trusting line. A revoked entry needs a host match, as in OpenSSH's
  `load_hostkeys` with `HKF_WANT_MATCH`.
- Blob comparison cannot be evaded by re-encoding. `host_key::verify` accepts only
  `ssh-ed25519` followed by one string and no trailing bytes, so a revoked or pinned key has
  exactly one encoding.
- Pinned fingerprints are canonicalized (`host_key.rs:17-37`).
- Only `ssh-ed25519` is offered, so the host-key algorithm cannot be downgraded.
- A mismatch fails before any credential is sent.

### Input bounds, panics and AEAD use

- Packet lengths are capped at 64 KiB before any wait (`oxish-proto` `PacketLength`,
  `io.rs:47-63`). Padding must be at least 4 bytes and fit within the length
  (`io.rs:108-118`). Block alignment is checked (`wire.rs:203-208`, `635-642`).
- The identification line may be at most 255 bytes with its CRLF. It is read one byte at a
  time, so no packet bytes are consumed with it (`wire.rs:756-784`).
- The read buffer is bounded by 272 KiB while flushing, and otherwise by one packet plus
  16 KiB. Agent messages, the rekey queue, the reply queue and held output (window plus
  96 KiB, preallocated) are all bounded. Finding 2 is the only gap.
- No panic reachable from the peer was found:
  - Every `SshReader` read is bounds-checked.
  - Arithmetic on peer values is checked or bounded.
  - The `assert!` at `io.rs:58` applies to a fixed 4-byte array.
  - The `unreachable!` at `stream.rs:731` sits in a closure that only receives an `Err`.
  - The XSH `structured()` panic needs an unregistered variant, and every name used is
    registered (`crates/xsh-registry/src/errors.rs:441-462`).
- AEAD:
  - GCM decrypts, compares the tag in constant time, and wipes the buffer on failure
    (Graviola `mid/aes_gcm.rs:93-125`).
  - ChaCha20-Poly1305 verifies Poly1305 in constant time before it decrypts anything
    (`mid/ssh_chacha20poly1305.rs:90-115`).
  - Every failure is fatal, so unauthenticated plaintext is never used.

### Secrets

- The following are `Zeroizing` or wiped on drop: packet payloads, K, derived keys
  (`KeySourceSide`), held output, and the stdin and copy buffers.
- Graviola wipes the X25519 and ML-KEM private keys and shared secrets
  (`mid/x25519.rs:62-66`, `177-180`; `mid/mlkem768.rs:201-205`, `370-373`).
- No copy of K is left behind when its encoding buffer grows: the first allocation holds
  only the length prefix (`kex.rs:168-191`).
- `WriteState::reserve` runs before a packet is encoded. Key derivation writes into
  exact-size buffers.
- Residual: stack temporaries such as Graviola's by-value `SharedSecret::as_bytes()`
  (`kex.rs:207`) are not wiped, a general Rust limit. Receive-buffer growth is already listed
  among the RFC's risks.

## Not covered

- Graviola's implementation and its constant-time properties.
- The OpenSSH and Dropbear fixture suites were not run, because no finding needed a real
  server. Finding 1's premise about OpenSSH packet order comes from reading OpenSSH source;
  it was not reproduced here.
- `Connection::session` with a pseudo-terminal or keepalive, and `exec_stream_to_writers`,
  were read but not probed; XSH uses `Connection::exec`.
- macOS.
