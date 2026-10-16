import copy
import importlib.util
import pathlib
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock


ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNNER_PATH = ROOT / "dev/consolidation/route_replay_current.py"
RUNNER_SPEC = importlib.util.spec_from_file_location("route_replay_current", RUNNER_PATH)
runner = importlib.util.module_from_spec(RUNNER_SPEC)
RUNNER_SPEC.loader.exec_module(runner)


class CurrentRouteReplayTests(unittest.TestCase):
    def test_current_route_inputs_reject_symlinked_repository_paths(self):
        scratch = ROOT / ".work/tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch, prefix="route-replay-symlink-") as temporary:
            fixture_root = pathlib.Path(temporary)
            external = fixture_root / "external"
            external.mkdir()
            repository = fixture_root / "repo"
            repository.mkdir()
            (repository / "dev").symlink_to(external, target_is_directory=True)

            with self.assertRaisesRegex(runner.CurrentReplayError, "contains a symlink"):
                runner.checked_source_file(repository, runner.CONFIG_RELATIVE)

    def test_live_route_config_supplies_a_validated_source_commit(self):
        config = runner.read_route_config(ROOT)
        driver = runner.load_route_driver(ROOT)

        self.assertEqual(
            driver.route_source_commit(config),
            config["route_witness_source_commit"],
        )

    def test_source_commit_is_taken_from_the_passed_config(self):
        driver = runner.load_route_driver(ROOT)
        configured = "a" * 40

        self.assertEqual(
            driver.route_source_commit({"route_witness_source_commit": configured}),
            configured,
        )

    def test_invalid_configured_source_commit_is_rejected(self):
        driver = runner.load_route_driver(ROOT)

        with self.assertRaisesRegex(driver.ReplayError, "full lowercase Git commit ID"):
            driver.route_source_commit({"route_witness_source_commit": "not-a-commit"})

    def test_entrypoint_forwards_arguments_and_config_without_mutating_driver(self):
        config = {"route_witness_source_commit": "b" * 40}
        driver = SimpleNamespace()
        forwarded_argv = []
        forwarded_config = []

        def driver_main(received_config):
            forwarded_argv.extend(sys.argv)
            forwarded_config.append(received_config)
            return 0

        driver.main = mock.Mock(side_effect=driver_main)
        arguments = [
            "plan",
            "--repo",
            str(ROOT),
            "--head",
            "h",
            "--tree",
            "t",
            "--product-head",
            "product-head",
            "--product-tree",
            "product-tree",
        ]
        old_argv = sys.argv[:]

        with (
            mock.patch.object(runner, "read_route_config", return_value=config),
            mock.patch.object(runner, "load_route_driver", return_value=driver),
        ):
            self.assertEqual(runner.main(arguments), 0)

        self.assertEqual(forwarded_config, [config])
        driver.main.assert_called_once_with(config)
        self.assertEqual(forwarded_argv, [str(RUNNER_PATH), *arguments])
        self.assertEqual(sys.argv, old_argv)

    def test_retained_executor_rejects_product_source_mismatch(self):
        driver = runner.load_route_driver(ROOT)
        head = "a" * 40
        tree = "b" * 40
        for product_head, product_tree in (("c" * 40, tree), (head, "d" * 40)):
            arguments = SimpleNamespace(
                repo=ROOT,
                head=head,
                tree=tree,
                product_head=product_head,
                product_tree=product_tree,
            )
            with self.subTest(product_head=product_head, product_tree=product_tree):
                with mock.patch.object(
                    driver,
                    "source_snapshot",
                    return_value={"head": head, "tree": tree, "clean": True},
                ):
                    with self.assertRaisesRegex(
                        driver.ReplayError,
                        "verification products must be explicitly pinned",
                    ):
                        driver.validate_inputs(arguments)

    def test_current_replay_rows_cover_all_validated_artifact_source_ids(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        config = runner.read_route_config(ROOT)
        schema_rows = helper.parse_schema((ROOT / driver.SCHEMA_PATH).read_text())

        replay_rows = driver.build_replay_rows(ROOT, helper, config, schema_rows)
        rows = [row["artifact"] for row in replay_rows if row["artifact"] is not None]
        expected = helper.expected_route_witness_mappings(
            ROOT, config["route_witness_source_commit"], schema_rows
        )
        expression_tags = {
            row["tag"] for row in schema_rows if row["kind"] == "Expr"
        }
        expected_ids = {
            test_id
            for test_id, mapping in expected.items()
            if mapping["tag"] in expression_tags
        }

        self.assertEqual({row["test_id"] for row in rows}, expected_ids)
        self.assertEqual(
            {row["tag_name"] for row in rows},
            {row["tag"] for row in schema_rows if row["kind"] == "Expr"},
        )
        for tag in ("ExprIf", "ExprMatch"):
            tag_rows = [row for row in rows if row["tag_name"] == tag]
            self.assertEqual(len(tag_rows), 2)
            self.assertEqual(len({row["test_id"] for row in tag_rows}), 2)
            self.assertEqual(len({row["forced_route"] for row in tag_rows}), 1)
        self.assertEqual(len({driver.case_slug(row) for row in replay_rows}), len(replay_rows))
        for row in rows:
            self.assertEqual(row["ordinal"], row["schema"]["ordinal"])
            self.assertEqual(
                row["child_source_sha256"],
                driver.sha256_bytes(row["source_bytes"]),
            )
            for mode in driver.MODES:
                self.assertEqual(
                    row[f"{mode}_route"],
                    row["mapping"]["mode_expectations"][mode]["route_id"],
                )

    def test_current_replay_rows_union_exact_artifact_behavior_and_order_ids(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        config = runner.read_route_config(ROOT)
        schema_rows = helper.parse_schema((ROOT / driver.SCHEMA_PATH).read_text())
        mappings = helper.validate_route_witness_mappings(ROOT, config, schema_rows)
        schema_by_tag = {row["tag"]: row for row in schema_rows}
        artifact_ids = {
            test_id for test_id, mapping in mappings.items() if "child_source" in mapping
        }
        behavior_ids = {
            test_id
            for test_id, mapping in mappings.items()
            if schema_by_tag[mapping["tag"]]["kind"] != "Expr"
        }
        order_ids = {
            test_id
            for candidate in config["source_order_candidates"].values()
            for test_id in candidate["ids"]
        }

        rows = driver.build_replay_rows(ROOT, helper, config, schema_rows)
        rows_by_id = {row["test_id"]: row for row in rows}

        self.assertEqual(len(rows), len(rows_by_id))
        self.assertEqual(set(rows_by_id), artifact_ids | behavior_ids | order_ids)
        self.assertGreater(
            sum(len(candidate["ids"]) for candidate in config["source_order_candidates"].values()),
            len(order_ids),
        )
        for test_id, row in rows_by_id.items():
            self.assertEqual("artifact" in row["roles"], test_id in artifact_ids)
            self.assertEqual("behavior" in row["roles"], test_id in behavior_ids)
            self.assertEqual("source-order" in row["roles"], test_id in order_ids)
            self.assertEqual(row["artifact"] is not None, test_id in artifact_ids)

    def test_replay_selection_expands_when_live_order_config_adds_an_exact_id(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        config = runner.read_route_config(ROOT)
        schema_rows = helper.parse_schema((ROOT / driver.SCHEMA_PATH).read_text())
        current_rows = driver.build_replay_rows(ROOT, helper, config, schema_rows)
        current_ids = {row["test_id"] for row in current_rows}
        current_order_ids = {
            test_id
            for candidate in config["source_order_candidates"].values()
            for test_id in candidate["ids"]
        }
        mappings = helper.validate_route_witness_mappings(ROOT, config, schema_rows)
        known_ids = helper.xsh_test_ids(ROOT, config["xsh_oracle_files"]) | set(mappings)
        additional_id = next(iter(sorted(known_ids - current_ids)))
        expanded = copy.deepcopy(config)
        candidate = next(iter(expanded["source_order_candidates"].values()))
        candidate["ids"].append(additional_id)

        expanded_rows = driver.build_replay_rows(ROOT, helper, expanded, schema_rows)
        expanded_by_id = {row["test_id"]: row for row in expanded_rows}

        self.assertEqual(set(expanded_by_id), current_ids | {additional_id})
        self.assertIn("source-order", expanded_by_id[additional_id]["roles"])
        self.assertNotIn(additional_id, current_order_ids)
        self.assertIsNone(expanded_by_id[additional_id]["artifact"])

    def test_source_order_candidates_reject_duplicate_and_unvalidated_ids(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        config = runner.read_route_config(ROOT)
        schema_rows = helper.parse_schema((ROOT / driver.SCHEMA_PATH).read_text())
        mappings = helper.validate_route_witness_mappings(ROOT, config, schema_rows)
        known_ids = helper.xsh_test_ids(ROOT, config["xsh_oracle_files"]) | set(mappings)
        candidate_tag = next(iter(config["source_order_candidates"]))

        duplicated = copy.deepcopy(config)
        duplicate_ids = duplicated["source_order_candidates"][candidate_tag]["ids"]
        duplicate_ids.append(duplicate_ids[0])
        with self.assertRaisesRegex(driver.ReplayError, "malformed or duplicated"):
            driver.source_order_candidate_ids(helper, duplicated, schema_rows, known_ids)

        unknown = copy.deepcopy(config)
        unknown["source_order_candidates"][candidate_tag]["ids"][0] = (
            "tests/xsh/stdlib/process_frames.xsh::test_not_a_current_oracle"
        )
        with self.assertRaisesRegex(driver.ReplayError, "not current validated tests"):
            driver.source_order_candidate_ids(helper, unknown, schema_rows, known_ids)

    def test_source_order_replay_rejects_a_field_absent_from_schema(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        config = runner.read_route_config(ROOT)
        schema_rows = helper.parse_schema((ROOT / driver.SCHEMA_PATH).read_text())
        candidate_tag = next(
            tag
            for tag, candidate in config["source_order_candidates"].items()
            if len(candidate["fields"]) > 1
        )

        unknown_field = copy.deepcopy(config)
        unknown_field["source_order_candidates"][candidate_tag]["fields"].append(
            "not_a_schema_field"
        )
        with self.assertRaises(helper.EvidenceError):
            driver.build_replay_rows(ROOT, helper, unknown_field, schema_rows)

    def test_native_exact_success_and_artifact_environment_are_mode_specific(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        test_id = "tests/xsh/stdlib/process_frames.xsh::test_run_capture_and_pipeline_keep_operand_order_on_frames"
        successful = (
            b"running 1 tests\n"
            + test_id.encode() + b" ... ok 0ms\n"
            + b"test result: ok. 1 passed; 0 failed; 0 skipped\n"
        )
        self.assertTrue(helper.native_exact_test_passed(successful, test_id))
        self.assertEqual(driver.accepted_test_exit_code(0, True), 0)
        self.assertFalse(helper.native_exact_test_passed(successful, test_id + "_other"))
        self.assertFalse(
            helper.native_exact_test_passed(
                successful.replace(b"running 1 tests", b"running 2 tests"),
                test_id,
            )
        )
        self.assertEqual(
            driver.accepted_test_exit_code(
                0,
                helper.native_exact_test_passed(
                    successful.replace(
                        b"test result: ok. 1 passed; 0 failed; 0 skipped",
                        b"test result: ok. 0 passed; 0 failed; 0 skipped",
                    ),
                    test_id,
                ),
            ),
            1,
        )
        self.assertEqual(driver.accepted_test_exit_code(124, True), 124)

        target = ROOT / ".work/target-final-canonical/x86_64-unknown-linux-musl/verification"
        output_root = ROOT / ".work/tmp/route-replay-command-test"
        validated = {
            "root": ROOT,
            "output_root": output_root,
            "products": {"xsh": {"path": target / "xsh"}, "xsht": {"path": target / "xsht"}},
        }
        behavior_row = {"test_id": test_id, "roles": ("behavior",), "artifact": None}
        command = driver.native_command(
            validated,
            behavior_row,
            "forced",
            None,
            output_root / "container-ids/step.cid",
            "route-step",
        )
        binary_index = command.index(
            "/artifacts/target/x86_64-unknown-linux-musl/verification/xsht"
        )
        self.assertEqual(command[binary_index + 1 :], ["test", "--exact", test_id])
        self.assertIn("XSH_INTERNAL_TEST_FORCE_FRAMES=1", command)
        self.assertFalse(any(argument.startswith("XSHT_INTERNAL_ROUTE_WITNESS_") for argument in command))

        artifact_row = {"test_id": test_id, "roles": ("artifact",), "artifact": {}}
        witness_path = output_root / "probes/test/normal/witness"
        artifact_command = driver.native_command(
            validated,
            artifact_row,
            "normal",
            witness_path,
            output_root / "container-ids/step.cid",
            "route-step",
        )
        self.assertIn(f"XSHT_INTERNAL_ROUTE_WITNESS_TEST_ID={test_id}", artifact_command)
        self.assertIn(
            "XSHT_INTERNAL_ROUTE_WITNESS_OUTPUT_DIR=/scratch/probes/test/normal/witness",
            artifact_command,
        )

    def test_current_replay_rows_reject_a_missing_exact_source_id(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        config = runner.read_route_config(ROOT)
        schema_rows = helper.parse_schema((ROOT / driver.SCHEMA_PATH).read_text())
        incomplete = copy.deepcopy(config)
        incomplete["route_witness_mappings"].remove(
            next(
                mapping
                for mapping in incomplete["route_witness_mappings"]
                if mapping.get("mapping_kind") != "artifact_capability"
            )
        )

        with self.assertRaisesRegex(driver.ReplayError, "source map differs"):
            driver.build_replay_rows(ROOT, helper, incomplete, schema_rows)

    def test_current_replay_rows_reject_duplicate_and_mismatched_source_ids(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        config = runner.read_route_config(ROOT)
        schema_rows = helper.parse_schema((ROOT / driver.SCHEMA_PATH).read_text())
        duplicate = copy.deepcopy(config)
        duplicate["route_witness_mappings"].append(
            copy.deepcopy(duplicate["route_witness_mappings"][0])
        )
        with self.assertRaisesRegex(driver.ReplayError, "duplicate route-witness test ID"):
            driver.build_replay_rows(ROOT, helper, duplicate, schema_rows)

        mismatched = copy.deepcopy(config)
        mismatched["route_witness_mappings"][0]["tag"] = "ExprIf"
        with self.assertRaisesRegex(driver.ReplayError, "tag/source mapping differs"):
            driver.build_replay_rows(ROOT, helper, mismatched, schema_rows)

    def test_help_uses_retained_executor_without_requiring_repository_argument(self):
        driver = SimpleNamespace(main=mock.Mock(return_value=0))
        with mock.patch.object(runner, "load_route_driver", return_value=driver):
            self.assertEqual(runner.main(["--help"]), 0)

        driver.main.assert_called_once_with(None)


if __name__ == "__main__":
    unittest.main()
