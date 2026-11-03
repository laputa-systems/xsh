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
| `ssh.connect(options)` | an owned session capability |
| `ssh.exec(session, command, options)` | `{exit_code, signal, missing, stdout, stderr, certainty}` |
| `ssh.connect_all({targets, jobs})` | one owned session or `SshError` per target, in order |
| `ssh.info(session)` | `{host, port, user, usable, host_key, kex, host_key_algorithm, cipher_client_to_server, cipher_server_to_client}` |
| `ssh.close(session)` | `Unit` |

`ssh.connect` options: `host`, `port`, `user`, `timeout_ms`, and one value for
each of two choices.

- Trust: `host_key` (a `SHA256:` fingerprint) or `known_hosts` (a `Path`).
- Identity: `key` (an unencrypted Ed25519 OpenSSH private key file), or `agent`
  (the agent's socket `Path`) with `agent_identity` (the `SHA256:` fingerprint
  of the Ed25519 identity to sign with).

Giving both or neither of a pair is `SshError.Options`. The client reads no
environment variable and no `~/.ssh` file; a caller that wants `SSH_AUTH_SOCK`
passes it with `env.get`.

`ssh.exec` takes a command string, not an argv vector, and does not interpret
it. Options: `timeout_ms`, `max_output`, `env`, `stdin` (`Bytes`), `stdin_path`,
`stdout_path` and `stderr_path`. `env` is a record of `Str` values sent as SSH `env` requests before the
command; a server runs only the names it allows (OpenSSH's default is `LANG`
and `LC_*`, Dropbear sends none), and a refused name is `SshError.Rejected`
with `certainty = "not-submitted"`: the command did not run. The client has no
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

The session is an owned capability. Its scope closes it after the body's
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

Every failure is an `SshError` variant. Each carries `message` and
`certainty`: `"not-submitted"` means the request never reached the server and a
retry is safe; `"submitted"` means the command may have run, so observe its
effect before retrying. The client never replays a command after a failure and
never reconnects on its own.

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
every later call fails with `SshError.Unusable` and `certainty = "not-submitted"`,
because a half-read channel cannot be reused. Close it and connect again.
A failure that left the command possibly running (`certainty = "submitted"`) is
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

Each rekey checks the key again.

## Authentication

Ed25519 public-key authentication only. A key file must be an unencrypted
OpenSSH-format Ed25519 private key; an encrypted file fails with
`SshError.Key`. To keep an encrypted identity out of this process, load it into
`ssh-agent` and pass `agent` and `agent_identity`. The client asks the agent
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
command may stay silent indefinitely and only a signal ends the wait; a peer
that stops draining our input is a timeout after the connect `timeout_ms`.
SIGINT and SIGTERM cancel a running command promptly as `SshError.Canceled`.

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
server (`tests/protocol.rs`, `tests/descriptors.rs`) use a scripted peer built on
the OxiSH server state machine, which can send the malformed and out-of-order
traffic a real server will not.

Native macOS qualification is deferred: no macOS host is available, so macOS is
not claimed.

## Limits and threat model

The client is experimental. Passing these tests is not a security review or
formal verification of the protocol. A protocol and security review is required
before it guards anything beyond administration of hosts that the caller
already controls.

- It defends against an active network attacker who tries to impersonate the
  server (host-key verification before authentication), alter or inject packets
  (AEAD, strict key exchange), or exhaust memory (bounded windows, packets,
  agent messages and captured output).
- It does not defend against a compromised endpoint, a compromised
  `known_hosts` file or fingerprint source, or a malicious agent that signs for
  an identity the caller selected.
- Private key file contents are read into zeroized buffers; the parsed key held
  by Graviola, process memory dumps and swap are outside that guarantee.
- Debug output, errors and traces carry no key, stdin or output bytes.
- A command string runs under the remote account's login shell with that
  shell's semantics. Do not build it from untrusted values; use a fixed remote
  entrypoint and send data on stdin.
