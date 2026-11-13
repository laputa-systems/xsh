# XSH roadmap

The SSH-native remote XSH milestone makes administering a
Laputa VM a checked, inspectable workflow rather than a collection of quoted
remote-shell commands. It serves Laputa's
[G2 remotely maintainable VM](https://github.com/laputa-systems/laputa/blob/master/docs/ROADMAP.md#g2-the-remotely-maintainable-vm)
milestone: provision, inspect, update, recover, and eventually rebuild a system
through XSH.

The table tracks campaign status and evidence. Work after G0 does not reopen
the frozen candidate. Each live document owns its campaign status, resume
instructions and open items.

| # | Campaign or milestone | Status | Live doc | Next step or reason |
|---|---|---|---|---|
| 1 | G0 release candidate | done | [G0](#g0-release-candidate) | Tagged `xsh-g0-rc1` at `b268b5007`. |
| 2 | Pyramid: scoped verification | done | [Testing](TESTING.md#gates), [Release engineering](RELENG.md#historical-p0-product-sizes) | Closed 2026-10-29. Historical P0 measurements used O3 products. |
| 3 | Inference: general type, row, and effect inference | done, residuals accepted | [Inference evidence](PERFORMANCE-INFERENCE.md#campaign-close) | Closed at `73b59f11d`; later repairs raise the core sites that erase alone to 4,670 of 4,677, with 246 of 250 joint views passing. The residual is XSH-66. |
| 4 | Lean: connected compiler proof | active; full-core obligations open | [xsh-lean.md](../xsh-lean.md), [formal inventory](../formal/coverage.json) | The shared type/value foundation and membership decision exist. Connect the production membership operation through the first [functional-core pilot](#internal-functional-cores), alongside the existing source/facts/IR work. All 77 universal obligations remain planned; model proofs and finite bridge cases do not close them. |
| 5 | Performance and deployment | deferred until the rest of the backlog is done | [TODO.md](../TODO.md) | Current-source qualification is XSH-3; XSH-1, XSH-14 and XSH-19 remain open. Timed gates are deferred by owner choice. |
| 6 | Deser cutover | current-source qualification active | [SPEC](SPEC.md#structured-text-codecs), [DESIGN.md](DESIGN.md) | Historical Linux functional run on 2026-11-03: Deser crate suites, codec, JSON, INI, CSV, TOML and YAML modules, and a full native run (15,243 passed, 11 host-environment failures that pass in the pinned `xsh-test` container, 152 skipped). Laputa's network plan input is the strict-consumer proof (former XSH-41): exact validation, with a related `require` repair here. Native macOS qualification is deferred by the owner; no macOS host is available. Current-source S6 runtime, startup and build CPU-time qualification remains open. |
| 7 | Supervised DHCPv4 and syslog (XSH-53, XSH-54) | done | [Core commands](CORE.md), [qualification evidence](TESTING.md#dhcp-client) | Roll out the existing services through Laputa's packages and xinit; re-qualify the selected XSH products, not a new DHCP/logging implementation. |
| 8 | util-linux cohort, btop, system-report | done | [util-linux README](../dev/util-linux/README.md), [BTOP.md](BTOP.md), [SYSTEM-REPORT.md](SYSTEM-REPORT.md) | Reuse system-report for remote inspection and redacted diagnostics; the util-linux resource rerun remains XSH-1. |
| 9 | SSH-native remote XSH: native client (XSH-55), xsh-infra (XSH-56) | Linux implementations delivered; qualification open | [Remote XSH](#ssh-native-remote-xsh), [XSH-55 RFC](rfc/XSH-55-native-ssh-client.md), [XSH-56 RFC](rfc/XSH-56-xsh-infra.md) | XSH-55 delivered the native client, `known_hosts` and ssh-agent trust, hybrid post-quantum key exchange, rekeying in both directions, typed `SshError` outcomes and a loopback OpenSSH fixture, and Laputa's R6 library administers one headless x86-64 VM over it (see the RFC's audit). An internal source review on 2026-11-05 fixed five protocol and cancellation defects and ran the client suite on glibc and musl; a second round fixed seven further risks and reran the suite on glibc only. Open: native macOS qualification (deferred by the owner), an independent protocol and security review before any claim beyond experimental, the review's remaining risks (unwiped SHA-256 contexts in the vendored Graviola, and a writer that never returns keeping `exec_stream` from returning), environment and working-directory options in Laputa's `remote_run`, and an acknowledged remote cancellation action. xsh-infra's remote execution is complete only over this client. |
| 10 | Archive and compression consolidation (XSH-57) | implemented; qualification gaps open | [SPEC](SPEC.md#15-standard-modules), [testing evidence](TESTING.md#archive-and-compression) | Laputa call sites migrated. Open: macOS and privileged coverage and built-in enum pattern matching; `xsht api enum:NAME` lists variants. Revisit ZIP creation and repacking if a caller needs them. |
| 11 | Syntax consolidation without feature removal | done | [SYNTAX-AUDIT.md](SYNTAX-AUDIT.md) | Every decided removal and reservation landed; see the audit's [Phase B status](SYNTAX-AUDIT.md#phase-b-status) and [final metrics](SYNTAX-AUDIT.md#final-metrics). |
| 12 | Internal functional cores | active; deadline selector linked | [Sequence and acceptance](#internal-functional-cores) | Connect structural membership and finish deadline boundary qualification under separate ownership. Require production use, a named formal relation, and removed complexity before expanding to cleanup and unwinding. |

When inference changes a Pyramid input or contract, rerun the affected checks.
Lean's source-bound correspondence uses the qualified source available after
the preceding work. Model proofs, finite witnesses, and Rust correspondence
remain distinct evidence.

The Deser cutover remains in place. Its performance thresholds are open; the
earlier waiver is not a pass. Native macOS qualification remains deferred.

## Internal functional cores

**Status: active.** Make small semantic functions and explicit state
transitions an organizing principle inside the implementation. Semantic
functions receive their operands, relevant state, and observations explicitly.
Adapters acquire observations, perform host operations, and attach diagnostics
and source blame. Owned mutable storage remains an implementation choice.

The source basis is the owner's assessment of SX
[`9e9e4e5`](https://github.com/joshuarli/sx/tree/9e9e4e5bf3fcb834748eafedd227e22f5c95b527)
and XSH `ed917341e`, rechecked against the current implementation. SX's
[core/capability boundary](https://github.com/joshuarli/sx/blob/9e9e4e5bf3fcb834748eafedd227e22f5c95b527/docs/ROADMAP.md#rules-for-every-campaign)
and [request/response machine](https://github.com/joshuarli/sx/blob/9e9e4e5bf3fcb834748eafedd227e22f5c95b527/rust/sx-kernel/src/machine.rs)
are useful patterns. Its native cores remain trusted implementations, and
its mutation lowering admits fewer aliasing and escape cases than XSH.
Adopt explicit semantic dependencies while preserving XSH's language contract.

Use [`duration.rs`](../src/duration.rs) as the implementation template:
typed operands, explicit results and semantic errors, one decision shared by
constant preparation and execution, and thin runtime adapters. Keep components
internal modules. The [existing architecture](ARCHITECTURE.md#packages) governs
whether a later concrete consumer justifies a separate crate.

### Sequence and ownership

Continue full Linux and Asterinas native qualification as a separate gate.
Start each pilot from an identified source freeze after its focused baseline
passes. Qualification repairs and test-cost work can continue independently;
partial runs cannot become a passing baseline. Performance, deployment, and
owner-deferred platform work keep their existing scope and gates.

| Order | Work | Boundary and owner | Completion condition |
|---|---|---|---|
| 1A | Structural membership correspondence | `src/runtime/eval/type_view.rs`, its production adapters, and `formal/Xsh/Core/Membership*.lean` | A declared common domain, explicit retained-contract assumptions, and evidence connecting the production decision to the independent Lean judgment. |
| 1B | Deadline selection | Deadline handling in `src/runtime/eval.rs` and native `within` behavior | A production selection function takes supplied monotonic time; deterministic policy checks and real adapter checks preserve ordering and delivery behavior. |
| 2 | Arithmetic/validation and effect algebra | Existing semantic owners, including `src/duration.rs` and `src/sema/effects.rs` | Share demonstrably duplicated decisions and formalize existing relations without moving already functional code merely for uniformity. |
| 3 | Cleanup and unwinding | `CleanupGroup` in the explicit runner | After both pilots meet their acceptance criteria, isolate action selection and error precedence before considering wider scheduling or resource transitions. |

Pilots 1A and 1B may proceed in parallel with separate ownership. Neither
requires completion of the other's implementation. Cleanup depends on their
accepted boundary discipline, including deadline shielding. It does not require
a blanket arithmetic or effect-system rewrite.

### Pilot 1A: structural membership

The first finite Rust/Lean comparison covers structural membership; its domain
and limits are recorded in [formal/README.md](../formal/README.md). This is
implementation evidence, not universal refinement. TYPE-MEMBER and the 77
whole-core obligations remain planned.

Specify the admitted value views and closed, non-recovery type constructors
before changing code. Start with structural families already represented in
both implementations. State exact acceptance, rejection, and unsupported cases,
including scalar distinctions, key restrictions, record fields, and nominal
identity where admitted. Extend the domain deliberately after the first link.

Preserve borrowed `ValueView` access across runtime, lowered, and computed-field
representations. Keep the existing allocation boundary: no second generic value
tree, record materialization, value cloning, or stream pulling to make the
formal interface convenient. Production argument/result validation must use
the same decision being compared; a test-only reimplementation is insufficient.

Document each production route's interpretation of retained stream, callable,
scheme, generic, and environment-path contracts. The Rust callback and Lean
`ContractDecisions` must refer to corresponding facts. Name the validity and
preservation assumptions, and distinguish those assumptions from discharged
properties. Rust's permissive `Unknown` and `Invalid` recovery cases stay
outside the initial non-recovery correspondence domain.

Membership, schema conversion, and callable/module authentication retain
separate meanings. Inspect schema decoding's `&Evaluator` dependency and
narrow it only when its actual immutable requirements can be named. A broad
shared reference alone is not evidence of effects.

Relate the admitted Rust inputs and results to `Membership.value` and
`Membership.check_iff`. Compare acceptance and rejection through the production
operation, include rejected corruptions, and minimize counterexamples. State
unsupported cases explicitly. Finite differential agreement is implementation
evidence; the existing Lean decision theorem proves agreement with its own
judgment and does not by itself establish Rust correspondence or TYPE-MEMBER.

### Pilot 1B: supplied-time deadline selection

The production selector now receives borrowed deadline state and a supplied
`Instant`, removing its dependency on the evaluator and clock acquisition.
The evaluator calls that same selector. A manual comparison covers all 2,925
admitted small inputs against Lean's `choose`; it is finite implementation
evidence, not universal Rust refinement. All twenty native `within` cases pass
before and after extraction on pinned Linux verification products. The full
current-source Linux and Asterinas qualification gates remain separate.

The selection rule uses borrowed deadline state and supplied time.
The evaluator retains clock acquisition, source-attributed timeout construction,
and delivery through its existing checkpoints. Establish the monotonic-time
abstraction and state relation explicitly. `BaseMachine`'s host-request/resume
pattern is a template, not an existing proof of `Instant` deadline behavior.

Require deterministic checks for unexpired scopes, earliest eligible expiry,
outermost ordering on equal deadlines, delivered entries, and closed scopes.
State the selection invariants in Lean against that abstraction, and identify
whether Rust agreement is proved or covered only by conformance evidence.
Preserve deadline identity and order through rearming and cleanup shielding and
restoration. Keep clock-domain, unit, and representability boundaries explicit;
coordinate their host qualification with [XSH-35](../TODO.md#xsh-35--p2--qualify-clock-domains-and-deadline-conversions).

Retain native `within` tests for real sleeps, child cancellation, stream and
worker checkpoints, swallowed interruptions, and cleanup. Those tests establish
adapter behavior that supplied-time decision tests cannot. Expand into a full
state/event transition component only when selection extraction simplifies the
implementation. Do not add a universal request interpreter or serialized action
object to ordinary synchronous operations.

### Follow-up boundaries

Arithmetic and validation follow the duration pattern where two production
routes currently decide the same meaning. Effect work should preserve unknown
versus known-empty contracts, exact inclusion versus permission coverage,
concrete versus unresolved symbolic contributions, and error capture that
retains other contributions. The existing effect algebra may need no physical
extraction; connect its exact relations instead.

For cleanup, first make the next action and failure disposition explicit.
Preserve reverse defer order, changing `errdefer` eligibility, managed-release
behavior, primary versus secondary errors, original exits and tracebacks,
forced cancellation, and deadline shielding. The runner still executes deferred
expressions and owns actual resources. Expand into frame scheduling or resource
transfer only when the initial boundary removes complexity.

### Acceptance and proof limits

Each pilot must deliver all four outcomes:

1. Inputs and types expose a smaller dependency surface, with observations
   supplied at a visible boundary.
2. A real production path uses the semantic function, and any superseded
   decision is removed.
3. Record the formal connection in the existing [inventory](../formal/coverage.json)
   and its owner documentation: model, property, admitted domain, implementation
   relation, evidence, and remaining assumptions. Keep one proof inventory.
   Keep model proofs, artifact certificates, Rust evidence, and external host
   assumptions distinct. Partial pilots close none of the 77
   universal rows unless the existing whole-core closure rule is met.
4. A before/after comparison shows removed duplication, a narrower context,
   less fixture setup, or fewer route-specific cases. Moving files or adding
   another wrapper alone does not meet this requirement.

Keep Lean meaning independent of Rust layout and preserve the
[six-layer contract](../xsh-lean.md#layers-and-representation). State refinement
may permit several implementation steps per semantic step while preserving
observable outcomes. Independent verifier authentication remains a trust
boundary rather than duplicated decision logic to delete.

Preserve practical mutation and exact evaluation order. In particular, compound
assignment reads the target after evaluating its right-hand side and commits
nothing on a failed update. Account for failure and divergence when considering
rewrites. A proc's empty effect bound does not imply purity: `print` and `eprint`
remain observable despite requiring no effect. No persistent-collection mandate,
new language syntax, public API, or application-policy subsystem follows from
this implementation work. Pure administration and service policy stay in
ordinary XSH libraries over narrow host operations.

## SSH-native remote XSH

**Status: Linux implementations delivered; qualification open.** The owner-selected direction is remote administration,
not another catalog of utility ports. Preserve the [design principles](DESIGN.md):
small reusable host mechanisms, ordinary XSH userspace policy, and mature
external engines where they reduce risk. No new language syntax, async runtime,
or mandatory resident management daemon is implied, and XSH itself gains no
configuration-management framework; xsh-infra is a separate application.

[XSH-55](rfc/XSH-55-native-ssh-client.md) provides a
native SSH client on OxiSH's sans-I/O protocol code and Graviola, exposed
through a small typed XSH API. [XSH-56](rfc/XSH-56-xsh-infra.md) provides
xsh-infra, a small pyinfra-inspired configuration-management tool written in
XSH in its own repository, as the first substantial consumer: native Rust
handles SSH protocol and I/O, XSH handles infrastructure policy.

### Sequence

XSH-55 and xsh-infra have delivered Linux implementations. Qualification and
review remain open in their RFCs. Native macOS qualification is deferred because
no macOS host is available. Laputa's administration library
([R6](https://github.com/laputa-systems/laputa/blob/master/docs/ROADMAP.md#ssh-native-xsh-administration))
consumes the same client.

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
