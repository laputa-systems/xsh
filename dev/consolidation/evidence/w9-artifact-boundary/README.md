# W9 artifact boundary checks

Merged source: `7cf20b6c1cb3ef40916c46a24f8485d2090da391`.
The five-file repair adds 241 lines and removes three. Rust production
changes are zero.

Modern adapter builds ask the pinned builder for its planned command using
the same source, overlay, role, and environment as the actual build. Cargo
output must select one executable inside that exact target directory.
Historical builds retain their source-root boundary and rejection diagnostic.
The builder and its pins are unchanged.

All five Python suites pass on the clean merged source:

| Module under `dev/consolidation/` | Tests | Complete output |
|---|---:|---|
| `w9-publication-run-test.py` | 51 | [run.log](run.log) |
| `test_w9_publication_adapter_runner.py` | 67 | [adapter.log](adapter.log) |
| `test_w9_publication_exact_owner_runner.py` | 50 | [exact-owner.log](exact-owner.log) |
| `test_w9_publication_reference_correction.py` | 8 | [correction.log](correction.log) |
| `w9_source_build_test.py` | 2 | [source-builder.log](source-builder.log) |

Each runs with `python3 MODULE`, `PYTHONDONTWRITEBYTECODE=1`, and `TMPDIR`
under the repository's `.work/tmp`. All 178 tests pass. Shell syntax and
`git diff --check` pass. The tests reject artifacts from another role,
commit, or overlay directory and reject malformed or symlinked target plans.
Listing fixtures check the external executable path and source environment.

These checks establish no real harness build or capture result. The earlier
[builder failures](../w9-builder-preflight/README.md) remain retained.
