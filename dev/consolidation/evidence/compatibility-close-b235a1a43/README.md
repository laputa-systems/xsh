# Compatibility accounting at b235a1a43

Source `b235a1a43d06e44580fde4c1b8c58ff27a17ca78`, tree
`08402ddd079228e072f24642fdb82db5196fff11`. This records the separate quantities
required by [Phase A](../../CAMPAIGN.md). Origin mappings describe retained
ports and exceptions. They are not upstream execution results or a percentage
of the aggregate native suite.

## Pins and frozen origins

The [upstream lock](../../../../dev/checks/upstream.lock.json) pins GNU
Coreutils 9.12, uutils `e7c9f3194280835c4487c2945c68d5f01ccacc8d`
(`0.12.0-289-ge7c9f3194`), and BusyBox 1.36.1, archive SHA-256
`ea5494846c51d946e5e801d1c099b438f683af8147e0be24ca4001073143110f`.
The historical compatibility lock has the same three upstream versions.

The [frozen origin lists](../../../../core/tests/origins/freeze.json) are
unchanged. Counts are distinct upstream origin IDs, not executed test counts.

| Suite | Utility groups | Frozen IDs | Native origin claims | Exception entries | Claims also carrying an exception |
|---|---:|---:|---:|---:|---:|
| BusyBox | 68 | 638 | 638 | 0 | 0 |
| GNU | 61 | 514 | 513 | 3 | 2 |
| uutils | 106 | 5,653 | 5,419 | 235 | 1 |
| Total | | 6,805 | 6,570 | 238 | 3 |

The union is **6,570 + 238 - 3 = 6,805**. There are 235 exception-only IDs.
The strict origin check finds no missing, stray, detached, or duplicate claims.
It does not execute those ports or establish their behavioral equivalence.

The separate command inventory retains 108 upstream utilities: 106 in scope
and implemented, plus SELinux-gated `chcon` and `runcon`, both absent.
The expanded surface retains 125 implemented commands, including six optional
commands. These command denominators do not replace the origin denominators.

## Exceptions, exclusions, and skips

The [exception reasons](../../../../core/tests/origins/exceptions.json) retain
87 uutils IDs explicitly classified as host boundaries, with named Rust tests.
The other 148 uutils entries cover upstream internals, GNU-incompatible or
uutils-only contracts, scope differences, and one partial reference boundary.
A named Rust test is retained coverage ownership, not proof that it ran now.

Three entries overlap native claims and explain only part of an origin:
GNU `misc/no-fork.log` retains the other commands while excluding the static
musl `stdbuf` preload clause; GNU `sort/sort-locale.log` retains vectors while
explaining musl collation; uutils `test_tail::test_follow_name_shows_headers_on_creation`
retains its native rows while qualifying the GNU timing subset. The remaining
GNU exception, `du/move-dir-while-traversing.log`, has an upstream body disabled
by unconditional success and claims no retained Rust assertion.

The [runner exclusions](../../../../core/tests/origins/exclusions.json) remain
282 exact IDs: 137 `clap-wording`, 89 `gnu-semantics`, 34 `uutils-extension`,
17 `help-text`, three `thread-model`, and two `uutils-internal`. They overlap
neither the frozen passing-origin IDs nor the exception keys. Runner exclusions
remain counted as `excluded` in runner totals. No exclusion or denominator was
changed for this accounting.

Ignored Rust cases and runtime skips are separate again. The ordinary extra
harness ignores three guest-only clock setters; its privileged-feature build
also contains the guest-only BTRFS reflink case. Their explicit guest results
are below. Neither these ignores nor aggregate native skips reduce frozen
origin denominators.

## Actual upstream and reference execution evidence

The retired harness's reports remain readable in Git at `ae0017015^`.
For example, `git show ae0017015^:dev/compat/results/gnu-xsh.json` reads the
original GNU status map. Publication commit `a3350034695a` refreshed the uutils,
BusyBox, and GNU-XSH reports. These are historical reports, with failures
preserved, rather than results qualified for b235.

| Historical report under `dev/compat/results/` | Recorded outcomes |
|---|---|
| `uutils-integration.json` | 5,655 pass, 16 fail, 0 skip, 282 excluded, across 106 utilities |
| `busybox.json` | 639 pass, 50 fail, 74 skip, 0 excluded, across 74 utility entries |
| `gnu-xsh.json` | 514 PASS, 67 FAIL, 7 ERROR, 131 SKIP, across 719 selected logs |
| `gnu-uutils.json` | 543 PASS, 53 FAIL, 6 ERROR, 117 SKIP, across the same 719 logs |

The historical GNU differential records 494 shared passes, 49 uutils-pass/XSH-
non-PASS, 20 inverse cases, and 46 shared non-PASS cases. Its reporter omits
both-SKIP pairs and folds other non-PASS statuses into its failure cells.
These cells must not erase the raw ERROR and SKIP quantities above.
The retired uutils reporter can update selected utilities in an existing
report, so publication of its aggregate does not establish one matched build.

Actual upstream test output also survives in the current tree. For example,
[printf JUnit output](../../../compat/results/printf-b-followup/uutils-integration.junit.xml)
records 152 upstream tests on 2026-10-07 with 33 failures and no skips, matching
its report's 119 passes. The twelve retained printf/sort follow-up reports are
overlapping historical selections; they are not added together or treated as
current passes. The aggregate historical reports lack a complete current
source/product qualification and cannot be promoted from origin metadata.

The latest accepted [QEMU reference proof](../linux-qemu-0deeb9c7/README.md)
binds source `0deeb9c7fb79e52740c82a264754dbb29d8be693` and four exact products
to three GNU Coreutils 9.12 date-reference passes, three XSH date passes, and
one XSH BTRFS reflink pass. These are selected host-boundary reference checks,
not an upstream suite run. Reuse for b235 remains pending the QEMU owner's
comparison of all four fresh canonical product hashes with that accepted set.

## Checks and remaining qualification

Run here with `UUTILS_ROOT` unset, `PYTHONDONTWRITEBYTECODE=1`, and `TMPDIR`
under this worktree's ignored `.work/tmp`:

- `python3 core/tests/origins/check_origins.py --strict`: exit 0, all 6,805 IDs accounted for.
- `python3 dev/checks/check_exclusions.py`: exit 0, all 282 exclusions categorized and explained.
- `python3 dev/checks/check_inventory.py --check`: exit 0, retained inventory matches live sources offline.

Counts above also use the existing origin checker's `claims()` scanner and
`jaq` projections of the committed JSON. No upstream checkout, download, build,
or suite execution was performed for this accounting.

At this task's canonical checkpoint, the owner reported 223 integration passes,
23 boundary passes, 58 extra passes with three ignored, and 748 debug passes
with three ignored. These are repository test results, not upstream counts.
Final native and privileged results and QEMU product reuse qualification were
pending. The accounting introduces no additional full-upstream-suite obligation;
those existing acceptance results remain with their canonical and QEMU owners.
