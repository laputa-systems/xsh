import contextlib
import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import oracle_port


class OraclePortTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        tests = self.repo / "core/tests"
        (tests / "support").mkdir(parents=True)
        (tests / "data").mkdir()
        self.helper = (oracle_port.REPO / "core/tests/support/uu.xsh").read_text()
        (tests / "support/uu.xsh").write_text(self.helper)
        (tests / "data/fixture.bin").write_bytes(b"fixture\xff")
        (tests / "data/cycle").symlink_to(".", target_is_directory=True)
        self.test = tests / "test-uu-example.xsh"
        self.test.write_text('use support.uu as uu\ntest sample { assert false, "exact assertion" }\n')

    def tearDown(self):
        self.temp.cleanup()

    def test_preparation_preserves_original_tests_helper_and_fixture(self):
        corpus = self.root / "corpus"
        selected = oracle_port.prepare_corpus(self.repo, corpus, [self.test], "gnu")
        self.assertEqual(selected, ["core/tests/test-uu-example.xsh"])
        self.assertEqual((corpus / selected[0]).read_bytes(), self.test.read_bytes())
        self.assertEqual((corpus / "core/tests/data/fixture.bin").read_bytes(), b"fixture\xff")
        self.assertTrue((corpus / "core/tests/data/cycle").is_symlink())
        self.assertEqual((self.repo / "core/tests/support/uu.xsh").read_text(), self.helper)
        rewritten = (corpus / "core/tests/support/uu.xsh").read_text()
        self.assertEqual(rewritten.count('let executable = process.which(util)?'), 1)
        self.assertIn('let words = [executable].extend(args)', rewritten)
        self.assertEqual(rewritten.count("process.command_argv(s.ctx.xsh_bin, argv,"), 6)
        # Everything after the launch sites, including byte assertions, is unchanged.
        assertions = self.helper.index("## Creates a directory and every missing parent")
        self.assertTrue(rewritten.endswith(self.helper[assertions:]))

    def test_busybox_keeps_applet_prefix_for_text_and_lossless_path_words(self):
        rewritten = oracle_port.rewrite_helper(self.helper, "busybox")
        self.assertEqual(rewritten.count('let executable = p"/bin/busybox"'), 1)
        self.assertIn('[executable, fp"{util}"].extend(args)', rewritten)

    def test_native_separator_is_removed_only_from_interpreter_prefix(self):
        for reference in ("gnu", "busybox"):
            rewritten = oracle_port.rewrite_helper(self.helper, reference)
            self.assertNotIn('script.display(), "--"', rewritten)
            self.assertNotIn('script, p"--"', rewritten)
            self.assertEqual(rewritten.count(".extend(args)"), self.helper.count(".extend(args)"))
            self.assertIn("timeout: timeout", rewritten)
            self.assertEqual(rewritten.count("launch_metadata(vars, umask)?"), 1)
            self.assertNotIn("let script = applet_script(s, util)?", rewritten)
            self.assertIn("umask: Int? = null", rewritten)
            self.assertIn("export proc command(", rewritten)
            self.assertIn("export proc argv(", rewritten)
            self.assertEqual(rewritten.count("let argv = argv(s, util,"), 3)
            self.assertIn('Ok([s.ctx.xsh_bin, launcher, Path(metadata)].extend(words))', rewritten)

    def test_rewrite_changes_only_central_target_words_and_required_effects(self):
        for reference, target, words in (
            ("gnu", '  let executable = process.which(util)?', '[executable]'),
            ("busybox", '  let executable = p"/bin/busybox"', '[executable, fp"{util}"]'),
        ):
            expected = self.helper.replace('  let script = applet_script(s, util)?', target)
            expected = expected.replace('  let words = [s.ctx.xsh_bin, script, p"--"].extend(args)',
                                        f'  let words = {words}.extend(args)')
            expected = expected.replace(') [fs, error] -> Result[List[Path], Error] {',
                                        ') [fs, process, env, error] -> Result[List[Path], Error] {')
            self.assertEqual(oracle_port.rewrite_helper(self.helper, reference), expected)

    def test_changed_launch_shape_fails_before_running_oracle(self):
        for before, after in (
            ("let script = applet_script(s, util)?", "let applet = applet_script(s, util)?"),
            ("launch_metadata(vars, umask)?", "launch_metadata(vars, null)?"),
            ("let argv = argv(s, util, args, vars, umask)?", "let argv = argv(s, util, args, vars, null)?"),
            (") [fs, error] -> Result[List[Path], Error] {", ") [env, error] -> Result[List[Path], Error] {"),
        ):
            with self.subTest(fragment=before), self.assertRaisesRegex(ValueError, "helper launch shape changed"):
                oracle_port.rewrite_helper(self.helper.replace(before, after, 1), "gnu")

    def test_files_outside_core_tests_are_rejected(self):
        outside = self.root / "test-outside.xsh"
        outside.write_text("test outside {}\n")
        with self.assertRaises(ValueError):
            oracle_port.prepare_corpus(self.repo, self.root / "corpus", [outside], "gnu")

    def test_legacy_launchers_are_rejected(self):
        self.test.write_text("test legacy { assert true }\n")
        with self.assertRaisesRegex(ValueError, "must use support.uu"):
            oracle_port.prepare_corpus(self.repo, self.root / "corpus", [self.test], "gnu")

    def test_docker_runs_serially_with_only_readonly_mounts_and_no_network(self):
        command = oracle_port.docker_command(Path("/corpus-host"), Path("/release-host"), "isolated-run")
        self.assertIn("/corpus-host:/corpus:ro", command)
        self.assertIn("/release-host:/release:ro", command)
        self.assertEqual(command[command.index("--user") + 1], "1000:1000")
        self.assertEqual(command[command.index("--network") + 1], "none")
        self.assertEqual(command[-4:], ["/release/xsht", "test", "-j", "1"])

    def test_failed_native_run_propagates_status_retains_log_and_removes_corpus(self):
        release = self.root / "target/release"
        release.mkdir(parents=True)
        for name in ("xsh", "xsht"):
            binary = release / name
            binary.write_text("executable")
            binary.chmod(0o755)
        process = Mock(stdout=io.StringIO("observable failure\n"))
        process.wait.return_value = 7
        cleanup = Mock(returncode=0)
        with patch.object(oracle_port, "REPO", self.repo), patch.dict(
            "os.environ", {"XSH_BIN": str(release / "xsh")}
        ), patch("sys.argv", ["oracle_port.py", str(self.test)]), patch.object(
            oracle_port.subprocess, "Popen", return_value=process
        ), patch.object(oracle_port.subprocess, "run", return_value=cleanup) as remove, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(oracle_port.main(), 7)
        work = self.repo / ".work/oracle-port"
        self.assertEqual([entry.suffix for entry in work.iterdir()], [".log"])
        self.assertEqual(next(work.iterdir()).read_text(), "observable failure\n")
        self.assertEqual(remove.call_args.args[0][:3], ["docker", "rm", "-f"])


if __name__ == "__main__":
    unittest.main()
