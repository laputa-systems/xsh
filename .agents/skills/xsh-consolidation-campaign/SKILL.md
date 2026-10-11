---
name: xsh-consolidation-campaign
description: Implement or resume the approved XSH consolidation campaign, coordinate independent lanes, and verify its structural and language contracts. Use for this campaign rather than unrelated XSH edits.
---

# XSH consolidation campaign

Read `dev/consolidation/CAMPAIGN.md`, its handoff log, `ITEMS.md`, and the
design for the assigned work. Those files own decisions, progress, and gates.
Follow repository `AGENTS.md`; never push or run formatters.

The owner approved the campaign and all six designs on 2026-10-11. Existing
approval persists across resumed sessions. Continue independent work while
recording concrete contract or oracle conflicts for the integrator.

One integrator owns the `consolidation` branch, shared paths, review, and
merges. Every editing lane has its own worktree and exclusive file set.
Compatibility work has a separate branch and does not silently change the
baseline. Integrate between campaigns only at recorded batch boundaries.
Keep worktrees and build artifacts on disk outside `/tmp`. While compatibility
is WIP, exclude `dev/compat` from check/lint and automatic compatibility
ratchets. Keep sibling Laputa verification focused and lightweight.

Explicitly delegate every ready independent item, using as many available
slots as ownership and dependencies permit. All subagents use
`gpt-6.1-sol`: `medium` for routine work, `high` for complex implementation
and review. Set model and reasoning effort on each spawn; use a fresh
context and a self-contained brief when the tool requires it. No Luna or
unassigned nested delegation.

Briefs name the base commit, worktree/branch, owned paths, approved contract,
observable end state, metric and size budget, exact gates, commit authority,
and `Requests:` for shared files. An agent's completion is not acceptance.

Run build and test queues within host memory independently of agent count.
Use the pinned Linux test environment, isolate lane build directories,
reuse artifacts only for their exact source revision, and run one full
native suite at a time. Structural and correctness gates come first;
stricter performance thresholds follow functional completion.

After each accepted batch, record merged commits, gates, metric evidence,
migrations of existing tests, and remaining items in the handoff log.
Complete every workstream's observable and structural end state before
claiming completion, and report any unresolved item candidly.
