# Native SSH

`ssh` is XSH's native SSH client (experimental, see [Limits](#limits-and-threat-model)).
It opens one TCP connection, verifies the server's host key before it sends any
credential, authenticates one user with an Ed25519 identity, and runs command
strings on that connection. It is built on the vendored OxiSH packet code and
Graviola; the runtime never runs `ssh`, `scp` or `sftp`.

```xsh
let options = {
  host: "web-1.example",
  port: 22,
  user: "deploy",
  key: p"/etc/xinfra/id_ed25519",
  host_key: "SHA256:...",
  timeout_ms: 10000,
}

with session = ssh.connect(options)? {
  let result = ssh.exec(session, "printf 'hello'", {})?
  print f"{result.exit_code} {result.stdout.utf8()?}"
}
```

## API

| Call | Result |
|---|---|
| `ssh.connect(options)` | an owned `SshSession` handle |
| `ssh.exec(session, command, options)` | `{exit_code, signal, missing, stdout, stderr}` |
| `ssh.connect_all({targets, jobs})` | one owned `SshSession` or `SshError` per target, in order |
| `ssh.info(session)` | `{host, port, user, usable, host_key, kex, host_key_algorithm, cipher_client_to_server, cipher_server_to_client}` |
| `ssh.certainty(error)` | an `ssh.Certainty`, read from any `SshError` |
| `ssh.close(session)` | `Unit` |

`ssh.connect` options: `host`, `port`, `user`, `timeout_ms`, and one value for
each of two choices.

- Trust: `host_key` (a `SHA256:` fingerprint) or `known_hosts` (a `Path`).
- Identity: `key` (an unencrypted Ed25519 OpenSSH private key file), or `agent`
  (the agent's socket `Path`) with `agent_identity` (the `SHA256:` fingerprint
  of the Ed25519 identity to sign with).

Giving both or neither of a pair is `SshError.Options`. `timeout_ms` bounds how
long the peer may stay silent while the client waits on it. Connecting, authenticating
and each key exchange also have a whole-operation deadline of 30 seconds, or
`timeout_ms` when that is longer, however steadily the peer sends; SIGINT and
SIGTERM end them as `SshError.Canceled`. In `ssh.connect_all` a host that stalls
fails only its own entry. The client reads no
environment variable and no `~/.ssh` file; a caller that wants `SSH_AUTH_SOCK`
passes it with `env.get`.

`ssh.exec` takes a command string, not an argv vector, and does not interpret
it. Options: `timeout_ms`, `max_output`, `env`, `stdin` (`Bytes`), `stdin_path`,
`stdout_path` and `stderr_path`. `env` is a record of `Str` values sent as SSH `env` requests before the
command; a server runs only the names it allows (OpenSSH's default is `LANG`
and `LC_*`, Dropbear sends none), and a refused name is `SshError.Rejected`
with `certainty = .NotSubmitted`: the command did not run. The client has no
working-directory option; put `cd` in the command. With both `stdin` and `stdin_path` the bytes go
first and the file follows, so a framing header can precede a streamed payload.
Path options stream file contents without loading them into the controller, so
a transfer can exceed `max_output` (default 16 MiB, applied to each captured
stream).

`ssh.connect_all` opens several hosts in parallel (`jobs`, 1 to 16, default 4)
and returns a list with a session or an `SshError` for each target, so one
failure does not hide the others. Every session belongs to the scope that called
it, which keeps it alive for the `par-map` workers that use it later; a session
returned from a worker is closed when that worker ends.

`SshSession` is an opaque handle. A program cannot build one from a record,
show it in a string, serialise it to JSON or read a field from it, and traces
record no value for it; `ssh.info` reports what a program may know about the
session. A handle works in any evaluator it is passed to, including `par-map`
workers, and nowhere else. The session is an owned capability. Its scope closes it after the body's
defers; `ssh.close(session)` releases it early. One command runs per session at
a time: concurrent use of one session is rejected with `SshError.Busy`, never
serialized silently. Independent sessions run concurrently.

## Building a command

SSH exec carries one command string. For a remote account whose login shell is
POSIX (`sh`, `ash`, `dash`, `bash`), build it from an argv vector with
`shlex.join`, which quotes empty arguments, quotes, whitespace, newlines,
`$`, backticks, `$(...)` and leading hyphens so the shell sees each value
unchanged; `tests/xsh/ssh-argv.xsh` runs these cases through a real shell. No
other login shell is assumed to have POSIX semantics, and a command string with
a NUL byte is refused before submission. Where the remote runs XSH, send a
fixed, reviewed program as the command and the request as bounded stdin data
instead of interpolating values into source.

## Outcomes and errors

A remote exit status or signal is a result, not an error. `exit_code` is the
remote status, `signal` the remote signal name (and `exit_code` is `-1`), and
`missing` is true when the channel closed with neither; that case is never exit
zero.
A server that sends the status and then ends the TCP connection without closing
the channel still yields that status, as with OpenSSH; the session is unusable
afterwards. A reset, a failed write or an end without a status stays an error.

Every failure is an `SshError` variant. Each carries `message` and
`certainty`, an `ssh.Certainty`: `NotSubmitted` means the request never reached the
server and a retry is safe; `Submitted` means the command may have run, so observe
its effect before retrying. The wire strings are `"not-submitted"` and
`"submitted"`; they display as such in f-strings and `print`, and a comparison
uses the variant (`ssh.certainty(error) == .Submitted`). An `ssh.exec` result
carries no certainty: a returned result means the command was submitted. The client never replays a command after a failure and
never reconnects on its own.

Read the certainty of an error of any variant with `ssh.certainty(error)`; a
value typed `SshError` exposes only `message` as a field.

| Variant | Meaning |
|---|---|
| `Options`, `Disabled` | invalid options; a build without the `ssh` feature |
| `HostKeyMismatch`, `HostKeyUnknown`, `HostKeyRevoked`, `HostKeyInvalid` | the server key is not the trusted one, is not listed, is revoked, or its signature is wrong; all happen before authentication |
| `TrustMaterial` | a fingerprint or `known_hosts` file is malformed or uses an unsupported feature |
| `Algorithm`, `Hardware` | no common algorithm; the processor lacks a feature Graviola requires |
| `Key`, `Authentication` | an unusable, encrypted or non-Ed25519 identity or an agent failure; the server refused the identity |
| `Rejected` | the server refused the exec request; the command did not run |
| `Channel`, `Protocol`, `Transport` | a violated channel or protocol rule; a lost connection |
| `Timeout`, `Canceled` | the deadline passed; the caller or a signal cancelled |
| `OutputLimit` | a captured stream passed `max_output` |
| `Unusable`, `Busy`, `Session`, `Local` | an earlier failure poisoned the session; it is in use; the handle is closed or unknown; a local stdin or output file failed |

After any failure the session is dead: `ssh.info(session).usable` is false and
every later call fails with `SshError.Unusable` and `certainty = .NotSubmitted`,
because a half-read channel cannot be reused. Close it and connect again.
A failure that left the command possibly running (`certainty = .Submitted`) is
reconciled through the new session; the old command may still be running.

## Trust

A host key must be trusted before authentication starts. There is no
accept-anything mode, no trust on first use and no automatic `known_hosts`
write; first contact and rotation are a deliberate decision by whoever
provisions the fingerprint or the file.

`host_key` accepts one unpadded-base64 `SHA256:` fingerprint, the format of
`ssh-keygen -lf`. `known_hosts` reads an OpenSSH file and honors plain host
names, `[host]:port` for ports other than 22, comma lists, `*` and `?`
wildcards, `!` negation and `@revoked` lines, case-insensitively. A key listed
for the endpoint is trusted; a different key for a listed endpoint is
`HostKeyMismatch`; an endpoint with no entry is `HostKeyUnknown`; a revoked key
is `HostKeyRevoked` even when another line trusts it. Hashed host names
(`|1|...`) and `@cert-authority` lines are refused with `TrustMaterial` rather
than skipped, because skipping them would silently change what the file means.

Each rekey checks the key again and requires the key of the first exchange.

## Authentication

Ed25519 public-key authentication only. A key file must be an unencrypted
OpenSSH-format Ed25519 private key; an encrypted file fails with
`SshError.Key`. To keep an encrypted identity out of this process, load it into
`ssh-agent` and pass `agent` and `agent_identity`. Like OpenSSH, the client
refuses a key file it owns that group or others can access, and refuses one that
is not a regular file or is larger than 64 KiB, with `SshError.Key`. The client asks the agent
only to sign the authentication request for that one fingerprint, bounds and
validates every agent message, and never forwards the agent. There is no
password, keyboard-interactive, PTY, X11, port forwarding or SFTP, and no
`ProxyCommand`.

## Algorithms and compatibility

| Area | Offered | Notes |
|---|---|---|
| Key exchange | `mlkem768x25519-sha256`, `curve25519-sha256` | the hybrid is preferred; strict key exchange is mandatory |
| Host key | `ssh-ed25519` | |
| Cipher | `aes128-gcm@openssh.com`, `chacha20-poly1305@openssh.com` | AEAD only, so no MAC is used |
| Compression | `none` | |
| Authentication | `publickey` with `ssh-ed25519` | |

A server that offers no common algorithm, does not negotiate strict key
exchange, or sends a guessed key-exchange packet is refused. Re-exchanges start
after 1 GiB, 2^28 packets or one hour under the current keys, or whenever the
server starts one, and keep the session identifier. Traffic that arrives during
a re-exchange is delivered afterwards, in order.

Tested servers: OpenSSH 10.0 (Alpine 3.22, the loopback fixture) and Dropbear
2026.94 (Laputa's maintained x86-64 VM, which negotiates the hybrid exchange). Graviola needs AES, PCLMULQDQ, BMI1,
ADX, AVX and AVX2 on x86-64 and NEON, AES, PMULL and SHA-2 on aarch64; the
client reports the missing feature as `SshError.Hardware` before any
instruction runs.

## Deadlines, cancellation and cleanup

`timeout_ms` on `ssh.exec` bounds the whole command. Without it a running
command may stay silent indefinitely and only a signal ends the wait. A peer
that does not take the output waiting for it is different: when no byte of that
output is accepted for the connect `timeout_ms`, the command fails with
`SshError.Timeout`, however much the peer sends meanwhile, and the client stops
reading while that output is waiting. SIGINT and SIGTERM cancel connecting,
authenticating (including a wait on `ssh-agent`) and a running command promptly
as `SshError.Canceled`. Work begun
after the first signal, such as releasing a lock in a `defer`, is not cancelled by
it; a second signal cancels that too.

`stdin_path` may name a pipe, FIFO or terminal: the client reads it only when
it is readable, so a source with nothing to give does not stop the connection or
its deadline. A `stdout_path` or `stderr_path` that is not a regular file is
written only when it is writable, in pieces of at most 4096 bytes. While its
reader stops draining, the client holds what the server sent, returns no window
for it, and keeps answering keepalives, key exchanges and other requests; the
command ends at its `timeout_ms` or on a signal. Opening such a file
does not block: a `stdin_path` FIFO opens at once without a writer, and a
`stdout_path` or `stderr_path` FIFO without a reader fails with `SshError.Local`.

Closing a channel or connection does not prove that a remote process stopped,
and the client sends no signal to it. A timed-out or cancelled command may
still be running; reconcile through a separate session.

## Examples

`examples/ssh/` has runnable programs for [a single command](../examples/ssh/exec.xsh),
[connection reuse](../examples/ssh/reuse.xsh), [binary transfer](../examples/ssh/transfer.xsh),
[errors](../examples/ssh/errors.xsh) and [parallel hosts](../examples/ssh/parallel.xsh).
Run them through the fixture with `XSH_MODULE_PATH=examples/ssh`.

## Qualification

Reproduce the interoperability suites on Linux with Docker:

```sh
crates/xsh-ssh/fixture/ssh-fixture.sh exec cargo test -p xsh-ssh
crates/xsh-ssh/fixture/ssh-fixture.sh exec target/debug/xsht test tests/xsh/ssh-
```

The fixture starts two disposable Alpine OpenSSH servers on loopback with fresh
host keys, `RekeyLimit 1M`, and exports `XINFRA_FIXTURE_*`. Tests that need no
server (`tests/protocol.rs`, `tests/hostile.rs`, `tests/descriptors.rs`) use a scripted peer built on
the OxiSH server state machine, which can send the malformed and out-of-order
traffic a real server will not.

Native macOS qualification is deferred: no macOS host is available, so macOS is
not claimed.

## Limits and threat model

The client is experimental. Passing these tests is not a security review or
formal verification of the protocol. An independent protocol and security review
is required before it guards anything beyond administration of hosts that the
caller already controls; the [source review](rfc/XSH-55-native-ssh-client.md#protocol-and-security-review)
of 2026-11-05 is not that review and lists the risks it could not demonstrate.

- It defends against an active network attacker who tries to impersonate the
  server (host-key verification before authentication), alter or inject packets
  (AEAD, strict key exchange), or exhaust memory. Memory is bounded by the packet
  size (64 KiB), the 2 MiB receive window, and fixed limits: 256 KiB of unread
  bytes while a write waits; 64 KiB of unsent replies, after which the client
  stops reading; traffic held during a key exchange up to the window plus 256 KiB
  and 4096 packets, with any connection-layer message after the server's
  key-exchange init refused; agent messages; and `max_output` for every captured
  stream, which a file option does not lift for the other streams. Time is
  bounded by the deadlines above and, for stalled writes, by a deadline that
  reads do not reset.
- It does not defend against a compromised endpoint, a compromised
  `known_hosts` file or fingerprint source, or a malicious agent that signs for
  an identity the caller selected.
- Private key file contents, and every copy this client makes while checking
  and parsing them, the key-exchange shared secret, the derived key material
  held by the client, decrypted packet payloads and the receive buffer's consumed
  bytes are wiped when dropped or compacted. The private-key copy inside
  Graviola, key state inside its ciphers and hash contexts, buffers a growing
  allocation leaves behind, caller stdin bytes before they are packetized,
  process memory dumps and swap are outside that guarantee.
- Text the server supplies (a disconnect description, a channel-open failure
  reason, an exit-signal name, offered authentication methods and algorithm
  names) loses its control and bidirectional-override characters and is cut to
  512 characters before it reaches an error message or a value.
- A rekey must present the host key of the first exchange, as OpenSSH requires,
  even when the trust material lists other keys for the host.
- Debug output, errors and traces carry no key, stdin or output bytes.
- A server that grants a window of 0 and keeps the connection alive can hold a
  command that has input to send for as long as it likes; set `timeout_ms` on
  `ssh.exec` to bound it.
- Lines a server sends before its identification line are refused rather than
  skipped, and a `first_kex_packet_follows` guess is refused, so a server that
  does either cannot be used. The MAC algorithms in the proposal are advertised
  for compatibility but never negotiated: both ciphers are AEAD.
- Resolving a name runs on a helper thread that the deadline abandons rather
  than stops; it ends when the resolver or the TCP connect attempt does.
- A command string runs under the remote account's login shell with that
  shell's semantics. Do not build it from untrusted values; use a fixed remote
  entrypoint and send data on stdin.
