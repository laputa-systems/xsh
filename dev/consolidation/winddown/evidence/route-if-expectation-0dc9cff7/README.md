# ExprIf route mismatch on 0dc9cff7

The 428-step replay completed every exact test invocation but failed one route-artifact comparison. The only mismatch was the normal `ExprIf` artifact for `tests/xsh/expression-route-oracles.xsh::test_expr_if_pure_selection`.

The artifact records `explicit_frame_control_path`; the old mapping expected `in_place_evaluator_path`. This source form lowers each branch to a scheduled value block, so the `ExprIf` is not eligible for the normal in-place path. The repair changes only that expected route. It keeps the child source, forced-frame expectation, strict artifact matcher, and direct-IR in-place proof unchanged.

`route-replay/` preserves the original receipt, exact logs, source checks, probes, and artifacts. `matrix-0dc9cff7-20261017.json` preserves the resulting strict matrix. It admits 187 of 188 route-artifact proofs and reports two `ExprIf` normal gaps because the mismatched artifact and its probe receipt were not admitted. `canonical-replay.json` records the product hashes used by the replay. These files are unchanged copies; their receipts retain the original scratch paths.

The replay selected source commit `0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4` and tree `b4f9db3f8ee2f38a6c62fea876976619db424ed7`. The corrected evidence configuration changes that tree, so the 0dc9 products and source lock cannot qualify a replay of the corrected configuration. A fresh freeze and source-qualified products are required for route acceptance.
