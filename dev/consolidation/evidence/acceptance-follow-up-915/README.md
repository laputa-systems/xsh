# Acceptance follow-up on 915

This receipt covers focused follow-up stages on clean source commit
`915e9d8bdad1fbc53de089d3037b989a573deb4a` (tree
`f59ef2c2da8377ffd67d95d27679e81bc15447a0`). Runs used the pinned Linux image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c` and
existing x86_64 Linux musl verification products. No source or product changed.

The seven `core/tests/test-gnu-head.xsh` tests pass, including
`test_gnu_head_head_c_log`, which previously failed under the bounded address
space when given an extremely large excluded tail count. The existing `xsh`
and `xsht` products match before and after:

- `xsh`: `a9117c2905afe522fe5eb7b78a21bf2bd6e9d1fa567473bdda548be7e1f24d0b`
- `xsht`: `3cec465683a95a354e58efc2f3dcb091c34fa4b0d49826343a2c8fced4061ffb`

The first suffix runner stopped at metrics because its ignored output directory
`.work/consolidation` did not exist. After creating it, the same metrics command
and following docs stage passed. The reached suffix stages report:

- `xsht check`: 1,542 files pass.
- `xsht lint`: exit 0, with 72 findings and 115 notes; this is not a clean lint
  result.
- `git diff --check`: pass.
- Current metrics check: pass under the recorded owner-approved integration
  allowances. It reports source lines `189274 -> 215360` (ceiling `193749`) and
  XSHT lines `49968 -> 53179` (ceiling `50367`). These are advisory growth
  figures, not a structural or performance acceptance.
- `docs.check`: pass; its tour project reports 3 passed, 0 failed, 0 skipped.

These receipts cover suffix stages and one focused head file on `915e9d8bd`.
They do not complete a canonical batch or rerun the full native suite. The
787 canonical native result remains 13,720 passed, 160 failed, and 92 skipped;
a separate census cleared 159 of 160 failures, and the focused head pass covers
the remaining repaired case. No functional close or performance credit is
claimed. Raw runner logs were kept in disposable `.work/tmp` state.
