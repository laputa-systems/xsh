from pathlib import Path
import unittest
from unittest.mock import Mock

import batches


SELECTORS = (
    "core/tests/test-util-linux-process-regressions.xsh::test_util_linux_flock_keeps_lock_through_waited_new_session_child",
    "core/tests/test-util-linux-process-regressions.xsh::test_util_linux_rev_reverses_utf8_split_across_read_chunks",
    "core/tests/test-util-linux-process-regressions.xsh::test_util_linux_chrt_preserves_signal_termination",
    "core/tests/test-util-linux-storage-regressions.xsh::test_util_linux_rooted_storage_commands_join_block_and_mount_identity",
)
ROOT = Path("/repo")
PRODUCTS = Path("/products")


def case_lines(
    selector,
    *,
    running="running 1 tests",
    case=None,
    summary="test result: ok. 1 passed; 0 failed; 0 skipped",
):
    return [
        running,
        case or f"{selector} ... ok 1ms",
        summary,
    ]


def batch_output(selectors=SELECTORS):
    lines = []
    for selector in selectors:
        lines.append(f"XSH-EXACT-BEGIN:{selector}")
        lines.extend(case_lines(selector))
        lines.append(f"XSH-EXACT-END:{selector}")
    return ("\n".join(lines) + "\n").encode()


class ExactBatchTests(unittest.TestCase):
    def test_dispatches_one_readonly_container_and_returns_each_exact_summary(self):
        container = Mock(return_value=batch_output())

        summaries = batches.run_exact_batch(
            SELECTORS,
            known_selectors=SELECTORS,
            root=ROOT,
            products=PRODUCTS,
            container=container,
        )

        self.assertEqual(
            summaries,
            tuple(batches.CaseSummary(selector, 1, 0, 0) for selector in SELECTORS),
        )
        container.assert_called_once()
        script = container.call_args.args[0]
        self.assertEqual(script.splitlines()[0], "set -e")
        self.assertEqual(script.count("/products/xsht test --exact "), len(SELECTORS))
        self.assertEqual(script.count(" 2>&1"), len(SELECTORS))
        mounts = container.call_args.kwargs["mounts"]
        self.assertEqual(
            mounts,
            [
                "type=bind,source=/repo,target=/work,readonly",
                "type=bind,source=/products,target=/products,readonly",
            ],
        )
        self.assertEqual(container.call_args.kwargs["timeout"], 30)
        self.assertEqual(container.call_args.kwargs["capabilities"], [])

    def test_invalid_empty_duplicate_unknown_and_injected_selectors_fail_before_dispatch(self):
        container = Mock(return_value=batch_output())
        invalid_batches = (
            (),
            (SELECTORS[0], SELECTORS[0]),
            ("core/tests/test-not-declared.xsh::test_unknown",),
            (SELECTORS[0] + "; touch /work/owned",),
            SELECTORS + ("core/tests/test-extra.xsh::test_extra",),
        )
        for selectors in invalid_batches:
            with self.subTest(selectors=selectors), self.assertRaises(ValueError):
                batches.run_exact_batch(
                    selectors,
                    known_selectors=SELECTORS,
                    root=ROOT,
                    products=PRODUCTS,
                    container=container,
                )
        container.assert_not_called()

    def test_single_case_cannot_accept_an_eleven_pass_summary(self):
        selector = SELECTORS[0]
        lines = [f"XSH-EXACT-BEGIN:{selector}"]
        lines.extend(case_lines(selector, summary="test result: ok. 11 passed; 0 failed; 0 skipped"))
        lines.append(f"XSH-EXACT-END:{selector}")
        container = Mock(return_value=("\n".join(lines) + "\n").encode())

        with self.assertRaisesRegex(ValueError, "did not pass"):
            batches.run_exact_batch(
                (selector,),
                known_selectors=SELECTORS,
                root=ROOT,
                products=PRODUCTS,
                container=container,
            )

    def test_duplicate_missing_or_extra_summaries_are_rejected(self):
        selector = SELECTORS[0]
        summary = "test result: ok. 1 passed; 0 failed; 0 skipped"
        cases = (
            ["running 1 tests", f"{selector} ... ok 1ms"],
            ["running 1 tests", f"{selector} ... ok 1ms", summary, summary],
        )
        for lines in cases:
            output = (
                "\n".join(
                    [f"XSH-EXACT-BEGIN:{selector}", *lines, f"XSH-EXACT-END:{selector}"]
                )
                + "\n"
            ).encode()
            with self.subTest(lines=lines), self.assertRaises(ValueError):
                batches.run_exact_batch(
                    (selector,),
                    known_selectors=SELECTORS,
                    root=ROOT,
                    products=PRODUCTS,
                    container=Mock(return_value=output),
                )

    def test_skip_failure_zero_matches_wrong_selector_and_merged_stderr_fail(self):
        selector = SELECTORS[0]
        outcomes = (
            case_lines(
                selector,
                case=f"{selector} ... skipped: unavailable 0ms",
                summary="test result: ok. 0 passed; 0 failed; 1 skipped",
            ),
            case_lines(
                selector,
                case=f"{selector} ... FAILED 1ms",
                summary="test result: FAILED. 0 passed; 1 failed; 0 skipped",
            ),
            case_lines(
                selector,
                running="running 0 tests",
                summary="test result: ok. 0 passed; 0 failed; 0 skipped",
            ),
            case_lines(selector, case=f"{SELECTORS[1]} ... ok 1ms"),
            [*case_lines(selector), "xsht diagnostic on stderr"],
        )
        for lines in outcomes:
            output = (
                "\n".join(
                    [f"XSH-EXACT-BEGIN:{selector}", *lines, f"XSH-EXACT-END:{selector}"]
                )
                + "\n"
            ).encode()
            with self.subTest(lines=lines), self.assertRaises(ValueError):
                batches.run_exact_batch(
                    (selector,),
                    known_selectors=SELECTORS,
                    root=ROOT,
                    products=PRODUCTS,
                    container=Mock(return_value=output),
                )

    def test_missing_or_reordered_boundaries_and_partial_batch_cannot_pass(self):
        one_case = batch_output((SELECTORS[0],)).decode().splitlines()
        outputs = (
            "\n".join(one_case[1:]) + "\n",
            batch_output((SELECTORS[1], SELECTORS[0], *SELECTORS[2:])).decode(),
            batch_output(SELECTORS[:1]).decode(),
            batch_output().decode() + "unframed output\n",
        )
        for output in outputs:
            with self.subTest(output=output), self.assertRaises(ValueError):
                batches.run_exact_batch(
                    SELECTORS,
                    known_selectors=SELECTORS,
                    root=ROOT,
                    products=PRODUCTS,
                    container=Mock(return_value=output.encode()),
                )

    def test_container_status_and_stderr_failure_are_propagated_unchanged(self):
        failure = RuntimeError("reference container exited 7: stderr=xsht failed")
        container = Mock(side_effect=failure)

        with self.assertRaises(RuntimeError) as caught:
            batches.run_exact_batch(
                SELECTORS,
                known_selectors=SELECTORS,
                root=ROOT,
                products=PRODUCTS,
                container=container,
            )

        self.assertIs(caught.exception, failure)
        container.assert_called_once()


if __name__ == "__main__":
    unittest.main()
