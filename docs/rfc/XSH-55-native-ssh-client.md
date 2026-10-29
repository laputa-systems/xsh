# XSH-55: Native SSH client

Status: planned. Starts after the inference campaign closes
and Lean's current G1b slice lands; see the [roadmap](../ROADMAP.md#sequence).

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
