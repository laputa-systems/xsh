# W9 d20 final comparison failure

## Result

The final comparison exited with status `1`. It checked 1,690 inputs and 6,760 outcomes. It recorded 6,732 mismatches. Every row is `AdapterError: common checked facts mismatch`. The complete report is in [mismatches.tsv](comparison/mismatches.tsv).

Both exact-owner proof runs completed. The reference proof and candidate proof each cover all 1,690 owners. Their receipts and worker logs are in [exact-reference](exact-reference/) and [exact-candidate](exact-candidate/).

The comparison completed with mismatches. It does not establish W9 parity acceptance.

## First mismatch

Input zero, `bench/consolidation/xsh/directory.xsh`, mismatches in all four profiles. The bounded probe found one candidate-only `assignment_facts` field in `CheckOutput::Debug`. The derived reference has no such field, and the current projection has no rule for it. The first-input diagnosis is in [first-input-diagnosis.md](first-input-diagnosis.md).

The original-to-derived-reference comparison first differs at input 36, `bench/stdlib-port/native_control.xsh`, in the default publication. The delta and receipt are in [comparison](comparison/).

## Run sources

The controller was clean at `2b221766d26f27415183666f03a91cd49238c3a1`, tree `803c159b85d3accc7cf65254ff8e5486f38bfce9`. The candidate was clean at `d20af40bcacce38ce0eac714de40853130557215`, tree `c8a3f4034de97b133cb613048136d1503cd7ff99`. The reference and derived-reference source identities are recorded in [sources-before.txt](run/sources-before.txt) and [sources-after.txt](run/sources-after.txt). The snapshots match.

The exact command, environment, status, stdout, and stderr are in [run](run/). The compare used the existing frozen captures and build receipts. This directory retains proof and comparison receipts, but no raw captures or harness binaries.

The [d20 to 449 source qualification](d20-to-449-qualification.md) checks selected W9 paths only. It is not a build or capture claim for main `449320b`.

## Resource observation

The Docker hash workers used the configured per-worker limits of one CPU and 1 GiB memory. The host Python coordinator and its forked comparison workers ran outside those cgroups. A sampled worker reached 10,638,120 kB RSS. A separate `smaps_rollup` sample measured 8,070,433 kB PSS. Sampled host `MemAvailable` stayed at or above 38,174,328 kB. These samples do not give the process peak.
