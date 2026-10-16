# W9 source scope qualification

`d20af40bcacce38ce0eac714de40853130557215` (`c8a3f4034de97b133cb613048136d1503cd7ff99`) is an ancestor of main `449320b1844cb5c7cc361df2fe27e387b37b12d1` (`b1f2632d9fc3b48ceb5747843a6631db6e0aeb1c`). Comparing those commits under `src crates tests core` shows only:

- `src/modules/process.rs` — ESRCH handling and Linux sampling tests.
- `tests/core_compat_extra/cp.rs` — privileged mount-worker test selection.
- `crates/xsh-fuzz/tests/soundness.rs` — generated replacement test fixture.

The W9 source inputs are identical at both revisions: `tests/sema.rs` (`c6c11b4ab1902bf622cbfc655f5987a2ce069951d91f129b7bd3cdd5e7a3cdd9`), `dev/consolidation/w9-publication-owner-helper.rs` (`23fafe5b4140396242543bb4c59b0bcc1e0d4b59280f1f0e8dabf0cbd8431562`), `dev/consolidation/winddown/gate-tools/w9-repository-parity.rs` (`2ff5aaa74f8ae3d0597a3c14796d8643211a5b57b8b95147ba87205540ca95b0`), `dev/consolidation/w9_publication_exact_owner_runner.py` (`6cc37ba51cea2e93ce315de1fbe5b57a16ada7b8196bae997fc349390269468b`), and `dev/consolidation/w9-publication-run.py` (`d5941a6f2889bc8f4252a5aac7b0a278beff80d0a97e8661ffea766599b42f80`).

The retained debug publication harness selects only `xsh --test integration` with the W9 parity overlay. The exact-owner proof runs `sema::w9_exact_repository_owners_receipt`. The cp and fuzz changes are separate test targets; the process change is linked into the xsh harness but is outside those W9 test paths.

This is a scoped source-equivalence qualification only. Existing build, source, and capture receipts bind the d20 commit/tree and binaries; they do not record a fresh build or raw capture from main `449320b`.
