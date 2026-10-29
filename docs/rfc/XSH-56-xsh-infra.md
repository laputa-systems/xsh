# XSH-56: xsh-infra, configuration management written in XSH

Status: planned. Starts after [XSH-55](XSH-55-native-ssh-client.md) delivers
its native SSH contract; pure planner and resource work may start earlier. See
the [roadmap](../ROADMAP.md#sequence).

## Summary

xsh-infra is a practical, deliberately small configuration-management tool
written predominantly in XSH, inspired by pyinfra, and using XSH's native SSH
client for all remote transport. It lives in its own repository,
[laputa-systems/xsh-infra](https://github.com/laputa-systems/xsh-infra), not in
XSH or Laputa. It makes provisioning and reconciling a small fleet of
Alpine/Laputa-style machines safer and more inspectable than ad hoc shell
deployment, without Python, Ansible, systemd assumptions or a remotely
installed XSH runtime.

The boundary is the point of the campaign: native Rust handles SSH protocol
and I/O, XSH handles infrastructure policy. It tests whether XSH supports a
substantial application without hiding the implementation in Rust. It is not
a feature-for-feature pyinfra port or a Python compatibility layer.

This is an end-to-end implementation campaign: a working tool, examples,
tests, packaging and documentation, not an RFC, mock transport or collection of
SSH commands. XSH and Laputa gain no configuration-management framework from
it, and Laputa's own administration library stays on its PM, xinit, network
and system-report owners.

References: XSH, [pyinfra](https://github.com/pyinfra-dev/pyinfra),
[Laputa](https://github.com/laputa-systems/laputa) and
[ssh-build](https://github.com/laputa-systems/ssh-build).

## Prerequisites and discovery

Read current repository instructions and XSH-55's delivered API, and pin a
tested XSH revision and product. Verify every language and API assumption with
`xsht api` and the SPEC; do not invent syntax or treat illustrative APIs as
implemented. A narrow generic substrate defect that blocks the application is
reproduced with a minimal test, repaired in XSH, and the pin updated; the
engine never moves into Rust to evade an XSH limitation. If native SSH is
absent, name the exact blocker; no external SSH substitute, and remote
execution is not reported complete.

Inspect pyinfra's facts, operations, execution semantics and selected tests,
pin the reference version for differential tests, and preserve licenses and
provenance of anything adapted.

## Architecture

Ordinary `.xsh` modules implement inventory loading and validation, resource
declarations, remote fact gathering and parsing, pure desired-state comparison,
ordering and scheduling, apply and verify, service hooks and change
notifications, reporting and CLI, and file staging and activation policy. Rust
is limited to the existing generic XSH substrate and justified narrow repairs.

Runtime dependencies are a qualified XSH with native SSH, a compatible remote
SSH server, and an explicitly documented remote POSIX shell and utility
baseline. No Python, remote Python, remote XSH, persistent daemon, remote `jq`,
systemd, external `ssh`/`scp`/`sftp` fallback or hidden bootstrap installation.
Python and pyinfra may exist only in an isolated development-test environment
as a reference oracle and never ship.

Alpine Linux is the first target platform. Controller execution is qualified on
Linux and macOS according to XSH's actual platform matrix. ssh-build's Git-only
OxiSH server is not a provisioning endpoint; deployment fixtures use an
ordinary command-capable SSH server.

## Declaration model and interface

TOML inventory, and ordinary XSH deployment programs that import the library
and construct explicit typed resource descriptions without mutating while
declaring. Prefer a simple ordered resource list with stable explicit IDs and
specific dependencies only where needed; no source-line hashing, hidden global
host context, metaprogramming or plugin framework.

Commands equivalent to `doctor` (validate configuration and remote
capabilities), `plan` (read facts, report expected changes), `apply`
(reconcile and verify) and `check` (report compliance or drift). Support
inventory selection, host and group limits, bounded jobs, a clear failure
policy, and human-readable or JSON/JSONL reporting through existing XSH CLI
facilities; document the actual invocation. Validate inventory, duplicate IDs,
malformed dependencies, cycles, options, local source artifacts and
unsupported resources before any mutation. Deployment programs are trusted
operator code; a plan mode does not sandbox arbitrary XSH. Supported
declaration paths stay mutation-free, and the boundary is documented.

## Connections and transport

All remote transport uses XSH's native SSH module, with explicit host
identities, users, ports, authentication references and host-key trust.
Reuse authenticated connections across facts and mutations. Remote exec,
binary transfer and quoting sit behind one small, auditable XSH transport
boundary, the only place request values become remote shell source. A local or
testing path is fine; competing production backends are not.

Require and probe the documented remote shell contract rather than assuming a
login shell accepts POSIX syntax. Quote argv correctly and test hostile values;
reject unsupported path encodings instead of converting lossily. Large file
contents travel as binary channel data, never as shell arguments, base64
command strings or command substitutions. Set the remote locale for parsers,
prefer small unambiguous machine protocols over formatted `ls` output, and
distinguish unavailable utilities, permission errors, malformed output and
genuinely absent resources.

## Plan and apply semantics

Separate observed facts, desired state, pure comparison, intended actions,
observed execution results and post-apply verification. Planning runs no
package installs or cache updates, creates no remote staging, changes no
managed file and runs no activation hook; unavoidable authentication logs and
read side effects are acknowledged rather than claiming an untouched host.

Represent uncertainty honestly: an operation whose inputs an earlier operation
creates may be conditional or unknown at plan time, and an unreadable fact is
never "unchanged". Apply re-observes mutable state and computes each action
from current facts, invalidating caches after mutations; caches are scoped by
host, privilege context, query and generation. A saved plan binds to resource
and source digests and preconditions; a materially stale approval is rejected
or requires replanning, never silently applied as a larger change.

Operations form source-order barriers across hosts: operation N completes or
fails on its participating hosts before N+1 starts, and hosts within one
operation run concurrently up to the bound. A barrier does not mean success.
By default a failed host gets no further mutations; independent hosts finish
when policy permits; dependent and global stages abort when prerequisites
fail; the overall result fails whenever a required host failed. Per-host
failures are data through bounded parallel execution, not fleet-wide
fail-fast cancellation. Never retry an arbitrary mutation after a lost
connection; use XSH-55's execution certainty to reconnect, re-observe and
reconcile only where the resource semantics make that safe, and otherwise
report an unknown or partial outcome.

## Initial resource set

Each is implemented completely:

- **Directories:** presence, owner, group and mode, with explicit absent and
  type-conflict behavior; no implicit recursive deletion or unsafe symlink
  traversal.
- **Regular files:** local source or rendered content, checksum comparison,
  owner and mode, safe staging, optional validation, atomic replacement where
  supported, and post-write verification. A no-op rewrites nothing and leaves
  mtime alone.
- **Symlinks:** explicit target and replacement policy, never conflated with
  files or directories.
- **Alpine packages:** present or absent intent and supported version
  constraints through `apk`, using apk's version semantics. Metadata refresh is
  explicit, apply-only and separately reported.
- **Users and groups:** a documented Alpine-compatible subset for presence,
  IDs, membership, home and shell; conflicts are handled conservatively, and
  user data is never deleted or UIDs changed as an incidental step.
- **Commands:** explicit argv or explicit shell mode, with a read-only guard, a
  creates-style condition, a dependency or change trigger, or declared
  always-run behavior. Unguarded commands are not called idempotent.
- **Services and hooks:** a small service-adapter contract and a real OpenRC
  implementation for Alpine test hosts. Inspect Laputa's service-management
  contract before adding any adapter; do not invent an XSH PID 1 control API or
  introduce OpenRC into Laputa. Change-triggered validation, reload and
  restart; several changed files coalesce into one notification; activation
  never runs after a failed validation or dependency.

Package and service adapters stay in XSH. Unsupported platforms and
capabilities fail clearly rather than selecting a guessed implementation.

## File safety, privilege and secrets

Regular-file replacement creates a private, unpredictable staging object in a
trusted directory on the destination filesystem; streams data with bounded
memory; verifies the intended digest and detects source changes during
transfer; establishes ownership and mode safely; validates the staged
configuration where the validator supports it; promotes with tested
same-filesystem replacement; verifies the result; and cleans up safe-to-remove
staging. Validation runs before the live config is replaced, and a failing
validation leaves it in place; where a validator cannot read staged content, an
explicitly documented strategy is required instead of a claim of
prevalidation. No assumption of GNU `mv -T`, Linux-only commands or symlink
replacement behavior without capability checks and tests. State the
trusted-directory and local-attacker model; shell scripts provide no race-free
confinement. Atomic visibility is not whole-deployment atomicity or crash
durability, and rollback of package scripts, databases or service side effects
is not promised.

An explicit unprivileged, root or noninteractive-`doas` execution policy; no
privilege settings interpolated into unchecked shell fragments, and no
automatic rewrite of doas policy or SSH access. Secrets are explicit and
redacted in plans, diffs, command logs, errors and traces; prefer local secret
references and private streaming; secret staging files are never
world-readable; no secrets in inventory examples, plans, fixtures or release
artifacts.

## Concurrency control and reporting

A minimal, tested remote lock with ownership tokens and conservative stale-lock
recovery keeps two cooperating apply processes from modifying the same managed
namespace. A controller crash may leave a lock that needs operator recovery;
age or a reusable PID alone never justifies stealing it. Plan and check take
no mutating lock.

Outcomes are distinct: unchanged; changed and verified; skipped by condition;
blocked by dependency; failed; partially applied or outcome unknown. Primary
failures survive cleanup failures. Reports carry host, resource, phase and
cause without secrets, and sanitize untrusted terminal control sequences. No
claim of fleet-wide transactions, general rollback or exact dry-run prediction
of unmodeled effects.

## Qualification

Disposable real SSH and Alpine fixtures; never production inventory or the
operator's SSH configuration. For every resource: the first apply reaches the
intended state, a second apply reports no change, deliberate drift is detected
and repaired, plan and check perform no managed mutation, unavailable or denied
facts are not taken for absence, and a failed apply never becomes a false
success.

Integration scenarios:

- a complete Alpine example with packages, users and groups, files and a
  service;
- a validation failure that preserves the live config;
- a failed or partial transfer that preserves the destination and handles
  staging safely;
- hostile paths, quotes, newlines, empty values and leading hyphens;
- a binary artifact larger than normal capture limits;
- failed privilege escalation;
- unknown or changed host keys rejected before mutation;
- one failed host among several healthy ones;
- bounded concurrency and operation ordering;
- cancellation and disconnect at meaningful mutation boundaries;
- competing apply processes;
- stale observations and, if supported, stale saved plans;
- secret redaction across logs, errors and tracing.

Use pyinfra on separate equivalent disposable hosts as a differential
reference for the overlapping subset, comparing resulting state, change and
no-op behavior and failures, not command text or output decoration. Keep an
explicit compatibility and deviation table; investigate disagreements and test
intended semantics independently. Exercise real XSH programs through
`xsht check` and `xsht test`, not only Rust unit tests or mocks; every language
or runtime defect found gets a minimal reproducer and regression test in XSH.

## Packaging and completion

Deliver the XSH library and a usable entrypoint; examples with inventory,
deployment, files and templates and their expected behavior; pinned runtime and
dependency provenance; unit, property, differential and real SSH integration
tests; a capability and platform matrix; documentation for trust setup,
secrets, doas, planning, failures, locks, partial outcomes and recovery; a
short architecture document explaining deliberate omissions; and reproducible
installation and package checks following the owner's prerelease conventions.
Runtime artifacts exclude Python, pyinfra, toolchains, test servers, private
keys and build caches.

Measure startup, peak memory, connection count, and representative first and
no-op deployments against the pinned reference, separating handshake and
network time from controller overhead, under the perf-gate rules; native code
is not assumed faster. Keep history coherent with direct-to-main commits, no
ceremonial PRs, and never deploy to real hosts. Completion is exact commits,
runnable commands, demonstrated scenarios, test results, platform coverage,
measured costs and explicit limitations; a mocked transport or passing unit
suite is not completion.
