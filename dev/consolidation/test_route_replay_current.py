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

    def test_help_uses_retained_executor_without_requiring_repository_argument(self):
        driver = SimpleNamespace(main=mock.Mock(return_value=0))
        with mock.patch.object(runner, "load_route_driver", return_value=driver):
            self.assertEqual(runner.main(["--help"]), 0)

        driver.main.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
