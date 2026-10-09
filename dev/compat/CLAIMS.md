# Lane claims (campaign-utils)

One integrator owns the integration branch, shared results, baselines, and merges.
Lanes use GPT-6 Luna at xhigh, each in a separate worktree, and edit only their
owned paths. The campaign is running on native Linux x86_64 with the pinned
`nightly-2026-09-15` toolchain and verified mold 3.0.0 from the x86_64 GitHub
release. Docker is out of scope. Setup commands, the mold checksum, host
prerequisites, and GNU support files are documented in
[`README.md`](README.md#setup).

The local `campaign-utils` branch contains the merged `legacy-buckets` and
`stat-du-df` lanes in addition to the earlier work. `origin/campaign-utils` is
still at `5de29706`; the local results and campaign notes are being refreshed
before push. The last full uutils run was 2,813/5,974 and exposed 187
regressions. A corrected full run is pending the current utility lanes and
integrator fixes. Lane result files use scratch directories; only the
integrator writes `dev/compat/results/` and shared baseline files.

| Lane | State and evidence |
|---|---|
| `native-fs`, `native-proc-tty` | merged; typed filesystem, process, and tty primitives |
| `native-bytes-hash` | API wiring active in the resumed lane; primitives are merged, and the byte/checksum applets depend on the public signatures |
| `date` | merged; 183/185, two documented French locale punctuation differences |
| `trivial` | merged |
| `text-a1` | merged |
| `text-a2` | active in `/workspace/xsh-lanes/text-a2`; 17/663 before edits across nine applets |
| `text-b1`, `text-b2` | merged |
| `sort` | still to run or verify |
| `ls` | merged; focused `ls dir vdir split` result 314/365 with no regressions among earlier passes |
| `cp` | merged; focused slice 238/386 after regression fixes |
| `mv-ln` | merged; focused mv/ln slice 126/207 after regression fixes |
| `fs-basic` | merged; 188/203 |
| `fs-misc` | merged; 295/304, remaining failures in `gaps.json` |
| `perm` | merged; 106/149, remaining failures in `gaps.json` |
| `stat-du-df` | merged; 166/237, remaining failures in `gaps.json` |
| `legacy-buckets` | merged; removed all five non-uutils discard buckets; owned native tests 22/22 |
| `printf-env` | active in `/workspace/xsh-lanes/printf-env`; 178/252 before current edits, rerun pending |
| `proc-a` | active in `/workspace/xsh-lanes/proc-a`; native fixes and a clean uutils rerun pending |
| `native-bytes-hash` applet consumers | `bytes-enc` and `checksums` start after API wiring lands |
| `gnu-patch-classify` | merged; all 15 active patch files and 37 build-script rewrite groups classified |
| `sed`, `awk` | not started; long-running Wave 1 parser/runtime work |
| `sysreport-extract` | merged; keep collector and capture/replay gates green |

The root integrator implemented `io.stdin_until` and the streaming
`wc --files0-from=-` path in commit `8e1ee558`. The focused wc suite passes
59/59. The generated docs and tour project check passes, including 3/3 project
tests. The follow-up native byte/hash API work is now assigned to the resumed
`native-bytes-hash` lane.

Before starting or resuming any lane, render its current brief with
`python3 dev/compat/lanes.py brief LANE`. Rebase only after the integrator
requests it; lanes never push or merge. The integrator runs the full uutils
suite after each integration batch and compares results against the prior
committed report. Keep the test denominator pinned and add only exact, justified
exclusions.
