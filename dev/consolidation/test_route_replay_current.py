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
    def test_current_route_driver_uses_the_promoted_path(self):
        self.assertEqual(
            runner.DRIVER_RELATIVE,
            pathlib.Path("dev/consolidation/route_replay_driver.py"),
        )

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

    def test_live_route_config_pins_the_current_oracle_source(self):
        config = runner.read_route_config(ROOT)
        driver = SimpleNamespace(PINNED_SOURCE_COMMIT="previous")

        self.assertEqual(
            runner.configure_source_pin(driver, config),
            config[runner.SOURCE_PIN_KEY],
        )

    def test_configured_source_pin_replaces_historical_pin(self):
        driver = SimpleNamespace(PINNED_SOURCE_COMMIT="previous")
        configured = "a" * 40

        self.assertEqual(
            runner.configure_source_pin(driver, {runner.SOURCE_PIN_KEY: configured}),
            configured,
        )
        self.assertEqual(driver.PINNED_SOURCE_COMMIT, configured)

    def test_invalid_configured_source_pin_is_rejected(self):
        driver = SimpleNamespace(PINNED_SOURCE_COMMIT="previous")

        with self.assertRaisesRegex(runner.CurrentReplayError, "full lowercase Git commit ID"):
            runner.configure_source_pin(driver, {runner.SOURCE_PIN_KEY: "not-a-commit"})

    def test_entrypoint_forwards_plan_arguments_after_configuring_source_pin(self):
        driver = SimpleNamespace(
            PINNED_SOURCE_COMMIT="previous",
        )
        forwarded_argv = []

        def driver_main():
            forwarded_argv.extend(sys.argv)
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
            mock.patch.object(runner, "read_route_config", return_value={runner.SOURCE_PIN_KEY: "b" * 40}),
            mock.patch.object(runner, "load_route_driver", return_value=driver),
        ):
            self.assertEqual(runner.main(arguments), 0)

        self.assertEqual(driver.PINNED_SOURCE_COMMIT, "b" * 40)
        driver.main.assert_called_once_with()
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

    def test_current_case_rows_cover_all_validated_source_ids(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        config = runner.read_route_config(ROOT)
        runner.configure_source_pin(driver, config)
        schema_rows = helper.parse_schema((ROOT / driver.SCHEMA_PATH).read_text())

        rows = driver.build_case_rows(ROOT, helper, config, schema_rows)
        expected = helper.expected_route_witness_mappings(
            ROOT, config[runner.SOURCE_PIN_KEY], schema_rows
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
        self.assertEqual(len({driver.case_slug(row) for row in rows}), len(rows))
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

    def test_current_case_rows_reject_a_missing_exact_source_id(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        config = runner.read_route_config(ROOT)
        runner.configure_source_pin(driver, config)
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
            driver.build_case_rows(ROOT, helper, incomplete, schema_rows)

    def test_current_case_rows_reject_duplicate_and_mismatched_source_ids(self):
        driver = runner.load_route_driver(ROOT)
        helper = driver.load_helper(ROOT)
        config = runner.read_route_config(ROOT)
        runner.configure_source_pin(driver, config)
        schema_rows = helper.parse_schema((ROOT / driver.SCHEMA_PATH).read_text())
        duplicate = copy.deepcopy(config)
        duplicate["route_witness_mappings"].append(
            copy.deepcopy(duplicate["route_witness_mappings"][0])
        )
        with self.assertRaisesRegex(driver.ReplayError, "duplicate route-witness test ID"):
            driver.build_case_rows(ROOT, helper, duplicate, schema_rows)

        mismatched = copy.deepcopy(config)
        mismatched["route_witness_mappings"][0]["tag"] = "ExprIf"
        with self.assertRaisesRegex(driver.ReplayError, "tag/source mapping differs"):
            driver.build_case_rows(ROOT, helper, mismatched, schema_rows)

    def test_help_uses_retained_executor_without_requiring_repository_argument(self):
        driver = SimpleNamespace(main=mock.Mock(return_value=0))
        with mock.patch.object(runner, "load_route_driver", return_value=driver):
            self.assertEqual(runner.main(["--help"]), 0)

        driver.main.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
