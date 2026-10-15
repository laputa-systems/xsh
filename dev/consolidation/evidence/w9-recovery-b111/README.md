# Recovered W9 tooling checks

Merged source: `b111cb3d8d97edf42d39447d4ee895702d434e40`, tree
`5a90de450803672fb0f6a37646d3ac701a72e706`.
The eight-file change adds 3,641 lines and removes 408. Rust production
changes are zero. The [capture contract](../../w9-reference-correction.md#recovered-source-captures)
describes the new receipt formats and preserved historical rules.

All five Python modules pass on the clean merged source:

| Module under `dev/consolidation/` | Tests |
|---|---:|
| `w9-publication-run-test.py` | 48 |
| `test_w9_publication_adapter_runner.py` | 65 |
| `test_w9_publication_exact_owner_runner.py` | 49 |
| `test_w9_publication_reference_correction.py` | 8 |
| `w9_source_build_test.py` | 2 |

Each ran with `python3 MODULE`, `PYTHONDONTWRITEBYTECODE=1`, and `TMPDIR`
pointing to the repository's `.work/tmp`. The five complete logs retain all
172 results. Shell syntax (`bash -n dev/consolidation/w9-source-build.sh`) and
`git diff --check` also pass. The executable builder's SHA-256 is
`ef12d889fb0d58bd76ec0fa35102492a9d991dcb726744bbcd2c86f19f3e9c07`,
matching its pin.

Fixtures create their own source roots, wrapper inputs, and reference data;
they do not require the deleted historical checkouts. The modern correction
fixture exercises real receipt associations. A separate fixture parses all
68 ranges for each of the original, derived, and candidate captures, with
1,690 inputs and 6,760 outcomes per capture, and runs one real owner-projection
comparison. Source qualification, builds, capture execution, and raw hashing
use fixture boundaries in these tests. Historical parsing and malformed/mixed
schema rejection retain separate coverage.

These are tooling checks. No Rust harness build, real capture, parity result,
or timing result is established here. Fresh qualified captures remain required.
