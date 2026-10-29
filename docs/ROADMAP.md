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
| 2 | Pyramid: scoped verification | active | [pyramid-campaign.md](../pyramid-campaign.md) | Record the P0 timing, memory and size baselines in a quiet window; that is the last open item. |
| 3 | Inference: general type, row, and effect inference | active | [full-inference-campaign.md](../full-inference-campaign.md) | Close the two open G3 items, then reduce annotations in G4. |
| 4 | Lean: connected compiler proof | active | [xsh-lean.md](../xsh-lean.md) | Resumed 2026-10-29; G0b froze the source at `65fcb0f42`. Next: G1b's test-only bridge and the connected milestone (XSH-44). |
| 5 | Performance and deployment | paused | [CAMPAIGN-PERFORMANCE-DEPLOYMENT.md](CAMPAIGN-PERFORMANCE-DEPLOYMENT.md) | Paused on 2026-10-25. G0 defers its performance obligations. XSH-1, XSH-3, XSH-14 and XSH-19 stay open in [TODO.md](../TODO.md). |
| 6 | Deser cutover | done, qualification open | [TODO.md](../TODO.md) | Correctness and platform qualification are open items there. |
| 7 | Supervised DHCPv4 and syslog (XSH-53, XSH-54) | done | [XSH-53 RFC](rfc/XSH-53-supervised-dhcp-and-syslog.md) | Roll out the existing services through Laputa's packages and xinit; re-qualify the selected XSH products, not a new DHCP/logging implementation. |
| 8 | util-linux cohort, btop, system-report | done | [util-linux README](../dev/util-linux/README.md), [BTOP.md](BTOP.md), [SYSTEM-REPORT.md](SYSTEM-REPORT.md) | Reuse system-report for remote inspection and redacted diagnostics; the util-linux resource rerun remains XSH-1. |
| 9 | SSH-native remote XSH | planned, next capability | [Remote XSH](#next-capability-ssh-native-remote-xsh) | Qualify one typed execution/file-transfer path against an established SSH implementation, then evolve the transport without rewriting its consumers. |

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
mandatory resident management daemon, or general configuration-management
framework is implied.

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
Target the user's macOS aarch64 administration workflow as well as Linux;
qualify the host/guest combinations separately. This does not extend G0's
platform claim or require completing Linux aarch64/Asterinas before a useful
Linux x86_64 guest slice can land.

### Stage 1: establish the remote-execution contract

Start with a narrow typed interface over an established SSH implementation.
The OpenSSL-less OpenSSH client in
[ssh-build](https://github.com/laputa-systems/ssh-build) is prior art and an
interoperability candidate, not an excuse to silently narrow algorithm support.
Keep the interface transport-independent so a native Graviola/OxiSH-backed
client can later satisfy the same contract. Do not write cryptographic
primitives or weaken host-key verification to get an initial connection.

Define the following behavior before committing to public API spellings:

- Explicit host identity, provisioned/pinned host-key verification, and
  authentication. Changed keys fail closed; first contact and key rotation
  require a deliberate trust decision. No implicit agent forwarding or secret
  material in logs, plans, or diagnostics.
- Structured executable/argv, environment, working directory, and byte-preserving
  paths. SSH exec carries a command string, not a native argv vector. Invoke a
  fixed reviewed remote entrypoint and send bounded, versioned data to it;
  never interpolate request values into remote shell source. Reuse the existing
  data/validation model rather than inventing an RPC framework. Separate
  protocol framing from the invoked program's input and output.
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

### Stage 2: integrate the native transport without changing policy

Evaluate the OxiSH client APIs and Graviola provider against Stage 1's tested
contract. Any native host extension should own only the protocol/crypto/resource
boundary; keep dispatch, retries, inspection, and deployment policy in XSH.
Compare dependency closure, supported algorithms, virtual CPU requirements,
stream ownership, and failure behavior. Retain the established implementation
as an interoperability oracle until the selected replacement is qualified.

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

Defer unrelated codecs, coreutils ports, desktop protocols, and generic fleet
orchestration. No D-Bus, systemd, logind, Ansible clone, or parallel test framework.
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
