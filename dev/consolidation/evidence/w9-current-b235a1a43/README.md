# W9 evidence reuse at b235a1a

## Source scope

Phase A of the [campaign](../../CAMPAIGN.md#phase-a--finish-the-existing-campaign-without-expanding-it) allows earlier executable evidence when the semantic scope is unchanged. The accepted [0deeb9 proof](../w9-current-0deeb9/README.md) reports 1,690 inputs, 6,760 outcomes, zero mismatches, and a verified assignment-facts proof.

The diff from `0deeb9c7fb79e52740c82a264754dbb29d8be693` to `b235a1a43d06e44580fde4c1b8c58ff27a17ca78` changes no files in `src/`, `crates/`, or `stdlib/`, and no W9 publication tool. In `core/`, only `tests/test-nc.xsh` and `tests/test-uu-tail-1.xsh` change. The other changes are documentation and evidence. The W9 module-checking scope therefore matches the accepted 0deeb9 result under the Phase A reuse rule.

This record gives b235a1a43 no new full-corpus outcomes. The checks below qualify its source and harness setup and record the input-0 probe.

## Fresh b235 checks

The official schema-4 source-cohort receipt binds candidate `b235a1a43d06e44580fde4c1b8c58ff27a17ca78`, tree `08402ddd079228e072f24642fdb82db5196fff11`, to controller `3c0c01206db71443662242167ac5f0a4bfafc62b`, tree `ddfc7e60f70f0de26b618b3c0eabefd8a2b36122`. The receipt pins the 1,690-row input manifest, corpus BLAKE3, x86_64 musl verification profile, and Docker image. Its SHA-256 is `621adf81d9b553277c30ebeb2b384838487dc696ed86f4b84fc040e0ad3b0c80`.

The reference adapter and exact-owner harnesses, candidate adapter and exact-owner harnesses, and derived-reference harness built from their pinned sources. The build logs record `fresh=false` for the Cargo `integration` artifact in all five builds. The exact-owner dry-run admitted the candidate and all 1,690 inputs. The adapter capture dry-run planned 68 ranges, four workers on CPUs 0–3, 1 GiB per worker, and 6,760 outcomes.

The input-0 probe passed for the reference, derived-reference, and candidate adapters and for both exact-owner roles. Each adapter projection had 55 rows. The adapter-reference and adapter-derived-reference projections matched the exact-reference projection. The adapter-candidate projection matched exact-candidate. The source audit reported two native hash rows for the reference and candidate roles. Source snapshots stayed identical from prepare through the probe.

The adapter capture command then stopped before capture output because the shared publication lock was held by the separate 3c comparison. The tool log records the lock rejection. No b235 adapter capture, exact-owner capture, derived capture, assignment-facts capture, or comparison ran. Full-corpus capture produced no outcome rows and received no full-corpus credit. The one-input probe is recorded separately. No b235 mismatch file or terminal parity record exists.

## Retained artifacts

`source-cohort.receipt` and the five build receipts bind source and binary identities. Their Cargo JSON files retain the `integration` artifact freshness rows. The source snapshots and build logs retain the prepare and input-0 probe results. `adapter-capture-preflight.log` records the bounded plan. `capture-lock-rejection.log` records why full capture did not start.
