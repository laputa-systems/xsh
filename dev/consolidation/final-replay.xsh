#!/usr/bin/env -S xsh --

type Options = {
  source_root: Path,
  source_commit: Str,
  source_tree: Str,
  functional_close: Path,
  laputa_corpus: Path,
  reference_source_root: Path,
  run_id: Str,
}

type FunctionalClose = {functional_complete: Bool, commit: Str}
type ProfileNativeSuite = {status: Int}
type ProfileResult = {name: Str, native_suite: ProfileNativeSuite}
type ProfileReceipt = {state: Str, results: List[ProfileResult]}
type ForcedReceipt = {state: Str}
type ReadinessCheck = {exit_code: Int}
type PerformanceGate = {before: ReadinessCheck, after: ReadinessCheck}
type ContainerCleanup = {resolved: Bool}
type ComparisonStage = {
  label: Str,
  exit_code: Int,
  container_exit_code: Int,
  timing_accepted: Bool,
  timing_discarded: Bool,
  performance_gate: PerformanceGate,
  container_cleanup: ContainerCleanup,
}
type PairRound = {round: Int, accepted: Bool}
type PairValidationReference = {path: Str, sha256: Str, accepted: Bool}
type PairValidation = {accepted: Bool, rounds: List[PairRound]}
type SchedulerReceipt = {
  state: Str,
  complete: Bool,
  suite_round_count: Int,
  stages: List[ComparisonStage],
  pair_validation: PairValidationReference,
}

const IMAGE = "xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c"
const BRIDGE_RELATIVE = "dev/consolidation/final_scheduler_receipt_bridge.py"

pure valid_git_id(value: Str) -> Bool {
  return false when value.byte_len() != 40
  for digit in value { return false when digit not in "0123456789abcdef" }
  true
}

pure valid_run_id(value: Str) -> Bool {
  return false when value.is_empty() or value.byte_len() > 48
  return false when value.starts_with("-")
  for character in value {
    return false when character not in "abcdefghijklmnopqrstuvwxyz0123456789-"
  }
  true
}

proc git_output(root: Path, arguments: List[Str]) [process, error] -> Result[Str, Error] {
  let argv: NonEmpty[Str] = ["git", "-C", root.display(), @arguments]
  let output = run.capture --text --timeout=30s @argv
  let code = output.status.shell_code()?
  if code != 0 {
    fail f"git {arguments.join(" ")} failed with {code}: {output.stderr.trim()}"
  }
  output.stdout.trim()
}

proc execute(stage: Str, argv: List[Str], root: Path, scratch: Path) [process, error] {
  print f"[{stage}] {argv.join(" ")}"
  let status = process.run(process.command_argv(
    argv[0],
    argv,
    cwd: root,
    env: {TMPDIR: scratch},
  ))?
  if !status.exited_with(0) {
    fail f"{stage} failed with status {status.shell_code()?}"
  }
}

proc validate_inputs(options: Options) [fs, process, error, io] {
  guard valid_git_id(options.source_commit) else { fail "source commit must be a full lowercase Git ID" }
  guard valid_git_id(options.source_tree) else { fail "source tree must be a full lowercase Git tree ID" }
  guard valid_run_id(options.run_id) else { fail "run ID must use 1 to 48 lowercase letters, digits, or hyphens" }

  let root = options.source_root.resolve()?
  if root != options.source_root { fail "source root must be a canonical path" }
  if git_output(root, ["rev-parse", "--show-toplevel"])? != root.display() {
    fail "source root must be the Git checkout root"
  }
  if git_output(root, ["rev-parse", "HEAD"])? != options.source_commit {
    fail "source HEAD differs from --source-commit"
  }
  if git_output(root, ["rev-parse", "HEAD^{tree}"])? != options.source_tree {
    fail "source tree differs from --source-tree"
  }
  if git_output(root, ["status", "--porcelain=v1", "--untracked-files=all"])? != "" {
    fail "source root must be clean before final replay"
  }

  if !options.functional_close.exists() { fail "functional-close record does not exist" }
  if !options.functional_close.is_file() { fail "functional-close path is not a regular file" }
  let close_path = options.functional_close.resolve()?
  if close_path != options.functional_close { fail "functional-close path must not contain symlinks" }
  if !close_path.starts_with(root) { fail "functional-close record must remain under the source root" }
  if !close_path.is_file() { fail "functional-close record does not exist" }
  let close = json.read(close_path)?.require(FunctionalClose)?
  if !close.functional_complete or close.commit != options.source_commit {
    fail "functional-close record does not mark this exact source commit complete"
  }

  let laputa_corpus = options.laputa_corpus.resolve()?
  if laputa_corpus != options.laputa_corpus { fail "Laputa corpus path must not contain symlinks" }
  if !laputa_corpus.is_dir() { fail "Laputa corpus must be a checkout root" }
  if git_output(laputa_corpus, ["rev-parse", "--show-toplevel"])? != laputa_corpus.display() {
    fail "Laputa corpus path must be the Git checkout root"
  }
  if git_output(laputa_corpus, ["status", "--porcelain=v1", "--untracked-files=all"])? != "" {
    fail "Laputa corpus must be clean before final replay"
  }

  let reference_root = options.reference_source_root.resolve()?
  if reference_root != options.reference_source_root { fail "reference root must be a canonical path" }
  if !reference_root.is_dir() { fail "reference source root does not exist" }
  if reference_root == root { fail "reference source root must be an isolated worktree" }
  if !reference_root.starts_with(fp"{root}/.work/wt") {
    fail "reference source root must be under the repository worktree directory"
  }
  if git_output(reference_root, ["rev-parse", "--show-toplevel"])? != reference_root.display() {
    fail "reference path must be the worktree root"
  }
  if git_output(reference_root, ["rev-parse", "HEAD"])? != options.source_commit {
    fail "reference worktree HEAD differs from --source-commit"
  }
  if git_output(reference_root, ["rev-parse", "HEAD^{tree}"])? != options.source_tree {
    fail "reference worktree base tree differs from --source-tree"
  }

  let work = fp"{root}/.work"
  if work.exists() {
    guard work.resolve()? == work else { fail "work directory must not contain symlinks" }
    if !work.is_dir() { fail "work path must be a directory" }
  } else {
    work.mkdir()
  }
  let scratch = fp"{root}/.work/tmp"
  if scratch.exists() {
    guard scratch.resolve()? == scratch else { fail "scratch directory must not contain symlinks" }
    if !scratch.is_dir() { fail "scratch path must be a directory" }
  } else {
    scratch.mkdir()
  }
  let bridge = fp"{root}/{BRIDGE_RELATIVE}"
  if !bridge.is_file() { fail f"scheduler receipt bridge is missing: {bridge}" }
  let bridge_source = bridge.read_text()?
  if "def configure(args)" not in bridge_source or "--tree-id" not in bridge_source {
    fail "scheduler receipt bridge is not the dynamic-input version"
  }
  execute("source and scheduler reference preflight", [
    "python3", bridge.display(), "qualify",
    "--source-root", root.display(),
    "--commit", options.source_commit,
    "--tree-id", options.source_tree,
    "--functional-close", close_path.display(),
    "--reference-source-root", reference_root.display(),
    "--run-id", options.run_id,
  ], root, scratch)

  let profile_output = fp"{root}/.work/consolidation/final-performance-gates/profile-cost-{options.source_commit}"
  let forced_output = fp"{root}/.work/consolidation/final-performance-gates/forced-frames-{options.source_commit}"
  let scheduler_builds = fp"{root}/.work/consolidation/native-scheduler-builds/{options.source_commit}/{options.run_id}"
  let reference_build = fp"{root}/.work/final-review-evidence/final-correctness-gates/native-scheduler-reference-{options.source_commit}-{options.run_id}"
  let comparison_output = fp"{root}/.work/consolidation/native-scheduler-comparison/{options.run_id}"
  for output in [profile_output, forced_output] {
    if output.exists() { fail f"profile or forced-frame output already exists for this source; inspect it: {output}" }
  }
  for output in [scheduler_builds, reference_build, comparison_output] {
    if output.exists() { fail f"scheduler output already exists; choose a fresh run ID or inspect: {output}" }
  }
}

proc run_replay(options: Options) [fs, process, error, io] {
  validate_inputs(options)

  let {source_root: root, source_commit: commit, source_tree: tree_id, functional_close, laputa_corpus, reference_source_root, run_id, ..} = options
  let close = functional_close.display()
  let reference = reference_source_root.display()
  let scratch = fp"{root}/.work/tmp"
  let profile_script = fp"{root}/dev/consolidation/winddown/gate-tools/final-performance-gates/profile-cost.py"
  let forced_script = fp"{root}/dev/consolidation/winddown/gate-tools/final-performance-gates/forced-frames.py"
  let comparator = fp"{root}/dev/consolidation/winddown/gate-tools/native-scheduler-comparison/compare.py"
  let bridge = fp"{root}/{BRIDGE_RELATIVE}"
  let profile_output = fp"{root}/.work/consolidation/final-performance-gates/profile-cost-{commit}"
  let profile_receipt_path = fp"{profile_output}/receipt.json"
  let forced_output = fp"{root}/.work/consolidation/final-performance-gates/forced-frames-{commit}"
  let scheduler_builds = fp"{root}/.work/consolidation/native-scheduler-builds/{commit}/{run_id}"
  let reference_receipt_path = fp"{scheduler_builds}/reference-build-receipt.json"
  let candidate_receipt_path = fp"{scheduler_builds}/candidate-build-receipt.json"
  let reference_bin = fp"{scheduler_builds}/reference-bin"
  let candidate_bin = fp"{scheduler_builds}/candidate-bin"
  let comparison_output = fp"{root}/.work/consolidation/native-scheduler-comparison/{run_id}"

  execute("release opt3 normal suite", [
    "python3", profile_script.display(),
    "--tree", root.display(),
    "--commit", commit,
    "--functional-close", close,
    "--laputa-corpus", laputa_corpus.display(),
    "--output", profile_output.display(),
  ], root, scratch)

  let profile = json.read(profile_receipt_path)?.require(ProfileReceipt)?
  if profile.state != "complete" or profile.results.len() != 1 {
    fail "selected release profile receipt is incomplete or has an unexpected profile set"
  }
  if profile.results[0].name != "release-opt3" or profile.results[0].native_suite.status != 0 {
    fail "selected release opt3 native suite did not pass"
  }

  execute("forced-frame normal suite", [
    "python3", forced_script.display(),
    "--tree", root.display(),
    "--commit", commit,
    "--functional-close", close,
    "--laputa-corpus", laputa_corpus.display(),
    "--profile-receipt", profile_receipt_path.display(),
    "--output", forced_output.display(),
  ], root, scratch)

  let forced = json.read(fp"{forced_output}/receipt.json")?.require(ForcedReceipt)?
  if forced.state != "complete" { fail "forced-frame native suite receipt is incomplete" }

  let bridge_arguments = [
    "--source-root", root.display(),
    "--commit", commit,
    "--tree-id", tree_id,
    "--functional-close", close,
    "--profile-receipt", profile_receipt_path.display(),
    "--reference-source-root", reference,
    "--run-id", run_id,
  ]
  execute("adapt candidate scheduler receipt", [
    "python3", bridge.display(), "candidate", @bridge_arguments,
  ], root, scratch)

  execute("build reference scheduler binary", [
    "python3", bridge.display(), "reference-build", @bridge_arguments,
  ], root, scratch)

  execute("three paired scheduler rounds", [
    "python3", comparator.display(),
    "--source-root", root.display(),
    "--source-commit", commit,
    "--reference-source-root", reference,
    "--reference-build-receipt", reference_receipt_path.display(),
    "--candidate-build-receipt", candidate_receipt_path.display(),
    "--reference-bin-dir", reference_bin.display(),
    "--candidate-bin-dir", candidate_bin.display(),
    "--image", IMAGE,
    "--manifest-path", fp"{comparison_output}/native-test-ids.txt".display(),
    "--output-dir", comparison_output.display(),
    "--lock-path", fp"{root}/.work/consolidation/infra/native-suite.lock".display(),
    "--run-id", run_id,
  ], root, scratch)

  let comparison = json.read(fp"{comparison_output}/receipt.json")?.require(SchedulerReceipt)?
  if comparison.state != "complete" or !comparison.complete or comparison.suite_round_count != 3 {
    fail "scheduler comparison receipt is incomplete or lacks three rounds"
  }
  let pair_validation_path = fp"{comparison_output}/pair-validation.json"
  if comparison.pair_validation.path != pair_validation_path.display() {
    fail "scheduler comparison receipt points at an unexpected pair validation file"
  }
  let pair_validation = json.read(pair_validation_path)?.require(PairValidation)?
  if pair_validation.rounds.len() != 3 or !pair_validation.accepted {
    fail "scheduler comparison did not accept all three paired rounds"
  }
  for round in pair_validation.rounds {
    guard round.accepted else { fail f"scheduler pair round {round.round} was not accepted" }
  }
  if comparison.stages.len() != 10 {
    fail "scheduler comparison did not complete two list, two help, and six full-suite stages"
  }
  let expected_labels = [
    "reference-list", "candidate-list", "reference-help", "candidate-help",
    "round-1-reference-suite", "round-1-candidate-suite",
    "round-2-candidate-suite", "round-2-reference-suite",
    "round-3-reference-suite", "round-3-candidate-suite",
  ]
  let observed_labels = comparison.stages |> map .label |> sort
  if observed_labels != (expected_labels |> sort) {
    fail "scheduler comparison stages do not contain the expected two lists, two helps, and six suites"
  }
  for stage in comparison.stages {
    if stage.label not in expected_labels or stage.exit_code != 0 or stage.container_exit_code != 0 {
      fail f"scheduler stage failed or has an unexpected label: {stage.label}"
    }
    if !stage.timing_accepted or stage.timing_discarded or !stage.container_cleanup.resolved {
      fail f"scheduler stage did not pass readiness and cleanup checks: {stage.label}"
    }
    if stage.performance_gate.before.exit_code != 0 or stage.performance_gate.after.exit_code != 0 {
      fail f"scheduler stage readiness check failed: {stage.label}"
    }
  }
}

let options: Options = cli.parse(args, {
  source_root: {form: "--source-root PATH", kind: "Path", required: true},
  source_commit: {form: "--source-commit SHA", required: true},
  source_tree: {form: "--source-tree SHA", required: true},
  functional_close: {form: "--functional-close PATH", kind: "Path", required: true},
  laputa_corpus: {form: "--laputa-corpus PATH", kind: "Path", required: true},
  reference_source_root: {form: "--reference-source-root PATH", kind: "Path", required: true},
  run_id: {form: "--run-id ID", required: true},
})?

match run_replay(options) {
  Ok(_) => {}
  Err(error) => { print $error.message; exit 1 }
}
