---
name: xsh-compat-campaign
description: Coordinate the XSH core-utility compatibility campaign in Claude Code. Fans per-utility lanes out to Haiku 5.5 (high) agents in isolated worktrees and integrates them as the Sonnet 5.5 (high) coordinator. Use to port the better implementation of a utility from another branch, to fix remaining uutils failures, or to continue the campaign.
---

# XSH compatibility campaign: Claude coordinator

You are the coordinator and the only integrator. Lanes are `xsh-compat-lane`
agents on **Haiku 5.5 at high effort**; you are **Sonnet 5.5 at high**. Pass
`model: "haiku"` and `effort: "high"` explicitly on every lane spawn. The model
policy in `dev/compat/LANES.md` and `README.md` describes the Codex workflow;
this skill governs a Claude Code session. Everything else in those files
(integrator-owned paths, gates, results, no push) still applies.

**Scope: passing tests, nothing else.** No formatting, linting, refactors,
cleanup, or performance work, by lanes or by you, until full parity. A diff
line that does not move a test or support a new test is rejected at review.
Optimization is a later phase.

Two properties make wide Haiku fan-out safe, and both are mechanical, not
advisory:

- **Isolation by module.** A lane is one utility: it owns `core/U.xsh` and
  `core/tests/test-U.xsh` in its own worktree and branch. `lane.py gate`
  fails on any other changed path. Shared code (`core/lib/*`, `src/`,
  `crates/`, `dev/`, docs, results, `gaps.json`) is yours, and lanes
  only file `Requests:`.
- **A narrow, tight loop.** Inner: `xsht test core/tests/test-U.xsh` (about a
  second, no lock, run after every edit). Outer: `lane.py gate U` (one
  utility's uutils slice, serialized on the suite lock, at most 6 per lane).
  The same `gate` accepts the merge, so a lane cannot pass a weaker check than
  yours. The full suite is yours, and runs once per batch.

**Upstream tests are read-only; editing them is cheating.** The pinned uutils
and GNU tests, fixtures, and harness inputs are the oracle. Nobody (lane,
coordinator, or script) edits, deletes, skips, adds to, or special-cases them,
and no adapter or applet branches on the harness (`XSH_EXECUTION_PHRASE`,
`xsh-uutests`). `lane.py gate` fails when the checkout under `UUTILS_ROOT` is
not exactly the pinned commit with a clean `tests/` and `src/`. Do not make
those trees read-only: tests copy fixtures with their mode and write to them,
so a read-only checkout fails tests that pass. An exclusion is the only sanctioned
way a test stops counting: by exact ID, with a category and reason, in
`dev/compat/exclusions.json`. The coordinator proposes exclusions and the
owner approves them; never add one to hide a real failure. Native tests under
`core/tests` are ours, but their expected text is a contract: do not rewrite
one to satisfy an upstream test unless the native test is demonstrably wrong
(shown against upstream-generated fixtures), and say so in the commit.

**XSH first.** The point of the campaign is to put XSH to the test: command
semantics, parsers, formatting, traversal and policy are XSH. Rust is for
reusable OS or byte boundaries XSH cannot express (a syscall, descriptor
operation, codec, or a time range the runtime cannot hold), added as the
smallest primitive, never as a ported applet or a donor's pre-baked formatter.
When a request for Rust arrives, ask first whether XSH can do it; judge
exceptions yourself and record each with its reason. Lanes also report
`Language gaps:` (what they wrote versus what they wanted). Collect them in
`.work/claude-campaign/language-gaps.md`; they are a campaign output.

**GNU wording wins.** Some donor "wins" only match uutils or clap text
(`(invalid value 'X')`, a bare strerror for write errors, `basenc 0.13.0`), or
detect the harness (`XSH_EXECUTION_PHRASE`). Lanes report those as
`wording-conflict` and change nothing. At review, reject any diff that
rewrites an existing native test's expected text or branches on the harness.
Park a rejected-but-passing lane as `wording/U` (keep the branch, remove the
worktree) so the owner can reverse the policy later, and list the test IDs in
`.work/claude-campaign/requests.md` as exclusion candidates for batch close.

## Tooling

`scripts/lane.py` (next to this file), always run from the primary checkout:

| Command | Does |
|---|---|
| `lane.py plan [--donor REV]` | rank utilities by tests failing on master and passing on the donor (default `origin/campaign-utils`), with the master-only passes the donor would lose |
| `lane.py new U... [--donor REV]` | create `../xsh-claude-lanes/U` on branch `claude/U` from `master`, write `.work/claude-campaign/lanes/U/brief.txt` and `targets.txt` |
| `lane.py brief U [--donor REV]` | print a brief |
| `lane.py gate U [--committed]` | ownership, native test, uutils slice versus master's committed baseline; prints `GATE PASS`, `GATE NOOP` or `GATE FAIL` with IDs |
| `lane.py drop U... [--force]` | remove a merged lane's worktree and branch |

Without `--donor`, `new` makes a **fix lane**: the targets are every non-excluded
test failing on master. With `--donor`, it makes a **port lane**: targets are the
tests the donor passes and master fails, and the brief tells the lane to port by
hunk, never to copy the donor's file.

## Session setup (once)

1. Release binaries from the current `master`, with the musl allocator preload
   for compile steps only:
   `LD_PRELOAD=/usr/lib/libjemalloc.so.2 cargo build --release --target x86_64-unknown-linux-musl -p xsh --bins -p xsht --bin xsht`
   (this host is Alpine x86_64 musl; the binaries are
   `target/x86_64-unknown-linux-musl/release/{xsh,xsht}`). Rebuild after any
   change under `src/` or `crates/` and tell running lanes by message.
2. The pinned uutils checkout at `../ref/uutils-coreutils` must be at the
   commit in `dev/compat/upstream.lock.json`. If it is missing, fetch exactly
   that commit into the empty directory (`git init`, `git fetch --depth 1
   origin <sha>`, `git checkout FETCH_HEAD`).
3. `python3 dev/compat/stage.py` refuses a stage left by a root-owned run, so
   `lane.py` uses each worktree's own `target/compat-stage`; use
   `XSH_COMPAT_STAGE=.work/claude-campaign/stage-master` for runs from the
   primary checkout.
4. Prove the baseline reproduces before trusting any gate: run one slice from
   the primary checkout and compare it with the committed entry. The first slice
   compiles the uutils test crate (minutes); later slices take seconds.
5. If `.claude/agents/xsh-compat-lane.md` was added this session, the agent type
   is not loaded until the session restarts. Until then spawn
   `general-purpose` with `model: "haiku"`, `effort: "high"`, and a prompt that
   tells it to read that file's bullets as its standing contract and then its
   brief.

## Run a wave

1. **Choose.** `lane.py plan`. Take utilities with wins and zero master losses
   first (additive, safest), then mixed ones (the lane must port hunks and the
   gate catches any loss), then fix lanes with no donor. Skip a utility
   whose wins depend on donor-only native modules (the donor's `time`,
   `hash`, and `process` changes): that is a native request you own, not a
   lane.
2. **Create.** `lane.py new U1 U2 ...` for the wave. Group nothing: one utility
   per lane, even where utilities share a library. A library change is yours,
   made once, before or after the lanes, never by two lanes.
3. **Spawn.** One Agent call per lane, all in one message. The prompt is only:
   "Read your standing contract `.claude/agents/xsh-compat-lane.md` and your
   brief `<path>/brief.txt`, then follow the brief exactly." Keep the brief
   in the file so your context stays small.
   Start at **8 concurrent lanes**. The native loop is cheap and parallel; the
   uutils slice is not (one at a time, behind a lock), so the gate queue is the
   limit. Raise the count only while a lane's wait for a gate stays under about
   five minutes. One full suite at a time, ever.
4. **While lanes run**, do not touch lane files or the shared binaries. Answer
   `Requests:` as they arrive: batch library changes on `master`, then message
   affected lanes to rebase (`git -C <wt> rebase master`) only if they are
   blocked on it.
5. **Accept.** On each completion notice, trust nothing in the report:
   - `lane.py gate U --committed` yourself;
   - `git diff master...claude/U` and read it: reject discarded options, dead
     code, a copied donor file, comments that cite plans, branches or other
     implementations, weakened tests, and any churn that is not behavior
     (reformatting, renames, reordering);
   - `git merge --no-ff claude/U -m "U: <result line from the gate>"`;
   - `lane.py drop U`.
   Merge one lane at a time. Because owned files are disjoint, merges do not
   conflict; if one does, the lane touched something it did not own, so reject it.
6. **Close the batch.** After every wave (not every lane): rebuild if `src/`
   changed, run the full suite once
   (`dev/compat/run-uutils.sh`, results into scratch) plus the cheap campaign
   ratchets `check_ignored_options.py`, `check_kernel_reads.py` and
   `check_exclusions.py`, then
   `python3 dev/compat/compare.py dev/compat/results/uutils-integration.json NEW`.
   Any regression reverts the offending merge (`git revert -m 1`) and returns
   the lane with the IDs. Only when clean, copy the new results over the
   committed ones and regenerate generated files (`python3
   dev/compat/parity.py`), update `dev/compat/CAMPAIGN.md` with the totals,
   and commit.

## Escalation and limits

- A lane that reports unresolved IDs gets one re-brief with those IDs and its
  failure notes. A second miss goes to a Sonnet lane (`xsh-lane`, or yourself
  for library work), not a third Haiku attempt.
- Failure modes to expect from a small model: copying the donor wholesale,
  parsing an option and ignoring it, claiming success without a gate, editing a
  library to make a test pass, weakening a test. The gate catches the first
  four mechanically; reading the diff catches the rest.
- Never push. Never run formatters, `xsht fmt`, or `xsht lint --fix`. Lanes
  never run cargo, never commit more than their two files, never merge.
- Clean up before finishing: no running process, no stale worktree under
  `../xsh-claude-lanes`, no `claude/*` branch that is not merged or deliberately
  kept, and a one-paragraph handoff in `dev/compat/CAMPAIGN.md`.
