# XSH roadmap

The next capability milestone is SSH-native remote XSH: make administering a
Laputa VM a checked, inspectable workflow rather than a collection of quoted
remote-shell commands. It serves Laputa's
[G2 remotely maintainable VM](https://github.com/laputa-systems/laputa/blob/master/docs/ROADMAP.md#g2-the-remotely-maintainable-vm)
milestone: provision, inspect, update, recover, and eventually rebuild a system
through XSH.

The table preserves current campaign status and evidence. The next-phase plan
below does not reopen the frozen G0 candidate, resume a paused campaign, or
claim implementation. Each live doc owns its campaign's status, resume
instructions, and open items.

| # | Campaign or milestone | Status | Live doc | Next step or reason |
|---|---|---|---|---|
| 1 | G0 release candidate | done | [G0](#g0-release-candidate) | Tagged `xsh-g0-rc1` at `b268b5007`. |
| 2 | Pyramid: scoped verification | done | [pyramid-campaign.md](../pyramid-campaign.md) | Closed 2026-10-29 with the P0 timing, memory and size baselines recorded on `fcdae517e`. |
| 3 | Inference: general type, row, and effect inference | paused | [full-inference-campaign.md](../full-inference-campaign.md) | G3 closed. G4 at 97.0% erasure; resume from the campaign Status. |
| 4 | Lean: connected compiler proof | holding | [xsh-lean.md](../xsh-lean.md) | G0b, G1's base theorems and G1b's first certified vertical artifact landed 2026-10-29. Holding until the owner restarts lanes; the next steps are listed in the live doc. |
| 5 | Performance and deployment | paused | [CAMPAIGN-PERFORMANCE-DEPLOYMENT.md](CAMPAIGN-PERFORMANCE-DEPLOYMENT.md) | Paused on 2026-10-25. G0 defers its performance obligations. XSH-1, XSH-3, XSH-14 and XSH-19 stay open in [TODO.md](../TODO.md). |
| 6 | Deser cutover | done, qualification open | [TODO.md](../TODO.md) | Correctness and platform qualification are open items there. |
| 7 | Supervised DHCPv4 and syslog (XSH-53, XSH-54) | done | [XSH-53 RFC](rfc/XSH-53-supervised-dhcp-and-syslog.md) | Roll out the existing services through Laputa's packages and xinit; re-qualify the selected XSH products, not a new DHCP/logging implementation. |
| 8 | util-linux cohort, btop, system-report | done | [util-linux README](../dev/util-linux/README.md), [BTOP.md](BTOP.md), [SYSTEM-REPORT.md](SYSTEM-REPORT.md) | Reuse system-report for remote inspection and redacted diagnostics; the util-linux resource rerun remains XSH-1. |
| 9 | SSH-native remote XSH: native client (XSH-55), xsh-infra (XSH-56) | planned, after rows 2 to 4 | [Remote XSH](#next-capability-ssh-native-remote-xsh), [XSH-55 RFC](rfc/XSH-55-native-ssh-client.md), [XSH-56 RFC](rfc/XSH-56-xsh-infra.md) | Starts when Pyramid and inference close and Lean's G1b slice lands; see [Sequence](#sequence). |
| 10 | Archive and compression consolidation (XSH-57) | done | [XSH-57 RFC](rfc/XSH-57-archive-and-compression.md) | Landed on `master`; Laputa call sites migrated. Follow-ups are listed in the RFC outcome: an `xsht api` enum-variant query, macOS and privileged coverage, and GNU tar and bsdtar interoperability. |

When inference changes a Pyramid input or contract, rerun the affected checks.
Lean's source-bound correspondence uses the qualified source available after
the preceding work. Model proofs, finite witnesses, and Rust correspondence
remain distinct evidence.

The Deser cutover remains in place. Its performance thresholds are waived;
outstanding correctness and platform qualification are not passes.

## Next capability: SSH-native remote XSH

**Status: planned.** The owner-selected direction is remote administration,
not another catalog of utility ports. Preserve the [design principles](DESIGN.md):
small reusable host mechanisms, ordinary XSH userspace policy, and mature
external engines where they reduce risk. No new language syntax, async runtime,
or mandatory resident management daemon is implied, and XSH itself gains no
configuration-management framework; xsh-infra is a separate application.

Two campaigns implement it. [XSH-55](rfc/XSH-55-native-ssh-client.md) builds a
native SSH client on OxiSH's sans-I/O protocol code and Graviola, exposed
through a small typed XSH API. [XSH-56](rfc/XSH-56-xsh-infra.md) builds
xsh-infra, a small pyinfra-inspired configuration-management tool written in
XSH in its own repository, as the first substantial consumer: native Rust
handles SSH protocol and I/O, XSH handles infrastructure policy.

### Sequence

Both campaigns start after the inference campaign closes. Pyramid closed and
Lean's G1b slice landed on 2026-10-29. Lean continues alongside
them afterwards, since it touches only `formal/` and test-only bridge code.
XSH-55 comes first. XSH-56's pure planner and resource work may start before
XSH-55 lands, but its remote execution is not complete until it runs over the
native client. Laputa's administration library
([R6](https://github.com/laputa-systems/laputa/blob/master/docs/ROADMAP.md#ssh-native-xsh-administration))
consumes the same client. Work stops before XSH-55 starts: the owner reclaims
the accumulated build and worktree state first, and XSH-55 begins from a fresh
checkout state.

### Ownership and first consumer

XSH owns reusable connection, stream, process, file-transfer, and structured
request boundaries. Keep the remote dispatcher and administration libraries in
ordinary XSH wherever existing host operations suffice. Laputa owns machine
identity/provisioning, package admission, service policy, generation activation,
boot health, recovery, and backup/restore. A transport library must not become
a second PM or service manager.

The first consumer is one headless Laputa VM administered from a separate
machine: inspect its generation and xinit services, run an explicit command,
transfer a verified candidate, and obtain useful diagnostics after failure.
xsh-infra is the second: reconciling Alpine/Laputa-style hosts that have no
remote XSH runtime, over a probed remote POSIX shell.
Target the user's macOS aarch64 administration workflow as well as Linux;
qualify the host/guest combinations separately. This does not extend G0's
platform claim or require completing Linux aarch64/Asterinas before a useful
Linux x86_64 guest slice can land.

### Stage 1: the remote-execution contract over the native client

XSH-55 implements this contract directly with the native Graviola/OxiSH-backed
client; the runtime never invokes `ssh`, `scp` or `sftp`. The OpenSSL-less
OpenSSH client in [ssh-build](https://github.com/laputa-systems/ssh-build) is a
test oracle against the same servers, never a production fallback, and not an
excuse to narrow algorithm support. Do not write cryptographic primitives or
weaken host-key verification to get an initial connection.

Define the following behavior before committing to public API spellings:

- Explicit host identity, provisioned/pinned host-key verification, and
  authentication. Changed keys fail closed; first contact and key rotation
  require a deliberate trust decision. No implicit agent forwarding or secret
  material in logs, plans, or diagnostics.
- Structured executable/argv, environment, working directory, and byte-preserving
  paths. SSH exec carries a command string, not a native argv vector. Where
  the remote runs XSH, as on Laputa, invoke a fixed reviewed remote entrypoint
  and send bounded, versioned data to it; never interpolate request values into
  remote shell source. Reuse the existing data/validation model rather than
  inventing an RPC framework. Separate protocol framing from the invoked
  program's input and output. A host without XSH, as xsh-infra targets, goes
  through one audited quoting helper for an explicitly probed remote shell,
  tested with hostile values; that helper is the only place request values
  become shell source.
- Streaming stdin/stdout/stderr, explicit exit status versus transport failure,
  bounded buffering/backpressure, deadlines, and owned channel/process cleanup.
  Non-PTY execution comes first; PTY/interactive behavior is a separate explicit
  contract, not an implicit change to byte streams.
- Safe file staging under an authorized destination, digest verification, and
  atomic publication where supported. Specify replacement, permissions, cleanup,
  confinement, and durability separately; transport success is not an fsync or
  package-authenticity claim.
- Disconnect/cancellation semantics that distinguish not submitted, completed,
  and unknown outcome. A lost acknowledgement must not replay a mutation.
  Remote cancellation requires an acknowledged action or re-observation;
  closing the local connection is not proof the remote process stopped.

The remote endpoint remains an authorization boundary. Authenticate the user,
validate requests, and use that user's actual filesystem/process authority.
Privileged operations require explicit delegation; a typed request or effect
annotation does not grant root access. Keep a read-only inspection path useful
without elevated privileges.

**Acceptance:** real client/server tests prove exact argv and byte transport,
independent stdout/stderr, status/error distinctions, host-key mismatch refusal,
bounded slow-reader behavior, cancellation, interrupted transfer cleanup, and
an uncertain mutation outcome that is reported without automatic retry. Include
malformed/oversized requests and a changed remote protocol version. Then use
those same libraries to inspect xinit, transfer and verify one artifact, and
run a controlled operation on a disposable Laputa VM. Do not require a rewritten
SSH server to complete this slice.

### Stage 2: consumers keep policy in XSH

The native crate owns only the protocol, cryptography, and resource boundary;
dispatch, retries, inspection, and deployment policy stay in XSH, in Laputa's
administration library and in xsh-infra. Record the client's dependency
closure, supported algorithms, CPU requirements, stream ownership, and failure
behavior with its qualification.

The current ssh-build OxiSH server is Git-only; it is not a general login,
PTY, or file-transfer server. Do not broaden that server just to remove C from
Laputa. Laputa may keep its packaged Dropbear for remote login while the client
and XSH administration experience improve independently.

### Connect existing capabilities instead of restarting them

XSH-53/54 are implemented outside the frozen G0 candidate. Laputa should adopt
a matching XSH executable/core-script set, install the foreground DHCPv4/syslog
services, and exercise them through xinit with real lease/logging failures.
Repair exposed reusable boundaries here; do not repeat those campaigns or move
service policy into PID 1. Reuse [system-report](SYSTEM-REPORT.md) collectors for
bounded, redacted remote diagnostics.

Keep deployment semantics in Laputa's existing PM/system APIs. Remote staging,
health checks, reboot/fallback, and restoration must use the same local owners
rather than separate SSH-only implementations. The existing
[self-hosting proof](https://github.com/laputa-systems/laputa/blob/master/docs/SELF-HOSTING.md)
provides the later feedback loop: rebuild XSH inside Laputa, produce a successor,
and activate it through the normal system lifecycle. It does not require
rewriting the compiler in XSH or rebuilding the whole world before remote
administration is useful.

Defer unrelated codecs, coreutils ports, desktop protocols, and fleet
orchestration inside XSH or Laputa. xsh-infra is a deliberately small separate
tool, not an Ansible clone. No D-Bus, systemd, logind, or parallel test framework.
Use existing focused gates and explicit implementation slices; this planning
update changes no runtime behavior or previously recorded qualification result.

## G0 release candidate

G0 is complete. The candidate is the single immutable XSH commit `b268b5007`,
tagged `xsh-g0-rc1`, with its xsh, xsht, xshi, and core script products.

G0 stabilizes `master`. It admits only these changes:

- correctness, data-loss, and resource-ownership repairs;
- fixes that Laputa's G0 consumers need;
- test and documentation corrections;
- verification repairs without which a result cannot be trusted.

New APIs, codecs, language changes, speculative optimization, and verification
features wait for a later milestone. An inference failure belongs to G0 only
when it breaks a documented language guarantee or a Laputa program.

XSH-53 and XSH-54 (supervised DHCPv4 and syslog) add APIs on `master` under the
owner's campaign authorization. They are outside the frozen G0 candidate.

A result counts only for the executable and source it ran. Any later executable
change reruns the affected qualification.

### Scope exclusions

- Performance and resource gates are deferred. XSH-1, XSH-3 stay open and do
  not block G0. The owner waived the quiet-host requirement on 2026-10-28, and
  no G0 claim depends on a timing.
- The graphical profile is outside G0, so Laputa's Muon build blocker does not
  gate it.
- macOS, Linux aarch64, and Asterinas are outside the G0 claim.

### Recorded result

All four completion criteria hold for `b268b5007`.

1. **Native failures.** Every failing selector in the last full native run has a
   recorded cause, a focused reproducer, and a repair or an out-of-scope reason.
2. **Combined run.** A privileged run on `8d7330542`, whose products and tests
   are identical to the candidate's, passed 14,588 tests with 151 skips and one
   incomplete. The incomplete gate test saw commits land on the checkout during
   the run and passes on a quiet tree. Of the skips, 72 are the
   private-namespace lane's tests, which pass 72 of 72 on the same products. The
   other 79 need root, external network or TLS fixtures, or tools absent from
   the image.
3. **Laputa suites.** Laputa's check, test, PM, xinit, and system suites pass
   with the candidate's products, with recipes that rebuild rather than reuse
   cached artifacts.
4. **Laputa qualification.** An earlier `make verify` with the candidate's
   seed found no XSH defect and eight Laputa defects, fixed through Laputa
   `9485988`. One uninterrupted run at Laputa `7d550bc` then passed every step.

Identities of the passing run:

- **Seed:** XSH `b268b5007`, clean, `release, lto=false`, x86_64-musl. Product
  sha256 prefixes: `xsh` `abb9e5cffd31e9ec`, `xsht` `4e964e4271999830`, `xshi`
  `f6355b0fca563977`.
- **Kernel:** artifact `3c602079f1ed…`, rebuilt from clean.
- **Profile:** `qemu-dwl-foot` generation `d37ec0bbb131…`.
