##! Cargo build and test steps, and the allocator preload their compiler runs under.
use stage as stages
use stage_contract as contract
use targets as target_policy

## A `rustc` built for a musl host allocates through musl's allocator, and a
## thin-LTO build then spends most of its time in the kernel. Preloading this
## jemalloc into the compiler makes a release rebuild several times faster.
export const jemalloc_library = /usr/lib/libjemalloc.so.2

## Names another library to preload into build steps; set to the empty string,
## it disables the preload.
export const override_variable = "XSH_DEV_BUILD_PRELOAD"

const runner_lock_variable = "XSH_CARGO_TEST_LOCK_FILE"

## A `rustc -vV` report without the host line.
export error ToolchainError = MissingHost(report: Str)

## Reads the host target triple from a `rustc -vV` report.
export pure host_triple(report: Str) -> Result[Str, Error] {
  for line in report.lines() {
    if let ["host:", triple] = line.fields() {
      return triple
    }
  }

  Err(ToolchainError.MissingHost(report:))
}

## Chooses the library to preload into the compiler: `configured` when it is
## set and `library` otherwise, on a musl host, when that file exists. An empty
## `configured` disables the preload, and a missing file means none.
export proc preload_for(host: Str, configured: Str?, library: Path) [fs, error] -> Result[Path?, Error] {
  return null unless host.ends_with("-musl")

  return null when configured == ""

  let chosen = if configured == null { library } else { fp"{configured}" }

  return null unless chosen.exists()

  chosen
}

## Reads the override and the compiler's host to choose this machine's preload.
## A machine without `rustc` builds nothing and has none.
export proc detect_preload() [fs, process, env, error] -> Result[Path?, Error] {
  let configured: Str? = if let Ok(value) = env.get(override_variable) { value } else { null }

  return null when configured == ""

  return null unless process.which("rustc") is Ok(_)

  let report = run.text rustc -vV
  preload_for(host_triple(report)?, configured, jemalloc_library)
}

## One process a Cargo step runs, and the `LD_PRELOAD` value it runs under.
export type Step = {spec: contract.CommandSpec, preload: Str?}

## The `LD_PRELOAD` value for a build: the library, ahead of any library the
## caller's environment already preloads.
export pure preload_value(library: Path, inherited: Str) -> Str {
  return library.display() when inherited.trim() == ""

  f"{library}:{inherited.trim()}"
}

pure cargo_lock_path(cwd: Path) -> Path {
  let worktree_parent = cwd.parent()
  let worktree_area = worktree_parent.parent()
  let shared_root = if worktree_parent.name() == "wt" and worktree_area.name() == ".work" {
    worktree_area.parent()
  } else {
    cwd
  }

  fp"{shared_root}/.work/consolidation/infra/cargo.lock"
}

pure uses_shared_cargo_lock(host_os: target_policy.HostOs) -> Bool {
  host_os == .Linux
}

proc ensure_cargo_lock_directory(lock_file: Path) [fs, error] {
  let infra = lock_file.parent()
  let consolidation = infra.parent()
  let work = consolidation.parent()
  stages.ensure_dir(work)
  stages.ensure_dir(consolidation)
  stages.ensure_dir(infra)
}

## Turns a `cargo test` argv into the argv that builds the same test
## executables without running them.
export pure test_build_argv(argv: List[Str]) -> List[Str] {
  let build = collect {
    for word in argv {
      break when word == "--"
      yield word
    }
  }

  [@build, "--no-run"]
}

pure has_cargo_glob(value: Str) -> Bool {
  "*" in value or "?" in value or "[" in value or "]" in value
}

pure test_has_single_harness_selection(argv: List[Str]) -> Bool {
  return false unless argv.len() >= 3 and argv[0] == "cargo" and argv[1] == "test"

  var target_count = 0
  var package_count = 0
  var expects_test_name = false
  var expects_package_name = false
  for word in argv {
    break when word == "--"

    if word.starts_with("--package=") or (word.starts_with("-p") and word != "-p") or word.starts_with("--test=") {
      return false
    }

    if expects_test_name {
      return false when word.starts_with("-") or word == "" or has_cargo_glob(word)
      expects_test_name = false
      continue
    }

    if expects_package_name {
      return false when word.starts_with("-") or word == "" or has_cargo_glob(word)
      expects_package_name = false
      continue
    }

    if word == "--test" {
      target_count += 1
      expects_test_name = true
    } else if word == "--lib" {
      target_count += 1
    } else if word in ["-p", "--package"] {
      package_count += 1
      expects_package_name = true
    } else if word in [
      "--all",
      "--all-targets",
      "--bench",
      "--benches",
      "--bin",
      "--bins",
      "--doc",
      "--example",
      "--examples",
      "--no-run",
      "--test",
      "--tests",
      "--workspace",
    ] {
      return false
    }
  }

  target_count == 1 and ! expects_test_name and package_count == 1 and ! expects_package_name
}

pure test_has_matching_target(argv: List[Str], expected: Str) -> Bool {
  var target_count = 0
  var target_matches = false
  var expects_target = false
  for word in argv {
    break when word == "--"

    return false when word.starts_with("--target=")

    if expects_target {
      return false when word.starts_with("-") or word == ""
      target_matches = word == expected
      expects_target = false
      continue
    }

    if word == "--target" {
      target_count += 1
      expects_target = true
    }
  }

  target_count == 1 and ! expects_target and target_matches
}

## The process a step that only compiles runs as.
export pure build_step(spec: contract.CommandSpec, library: Path?, inherited: Str) -> Step {
  return Step(spec:, preload: null) when library == null

  Step(spec:, preload: preload_value(library, inherited))
}

## The processes one `cargo test` step runs as. With a library, the test
## executables build under it first, and the tests then run without it,
## because a process a test spawns must not inherit the library.
export pure test_steps(spec: contract.CommandSpec, library: Path?, inherited: Str) -> List[Step] {
  return [Step(spec:, preload: null)] when library == null

  [
    Step(
      {
        ...spec,
        stage: f"{spec.stage}-build",
        argv: test_build_argv(spec.argv),
      },
      preload_value(library, inherited),
    ),
    Step(spec:, preload: null),
  ]
}

proc execute(step: Step) [process, env, error, io] -> Result[Unit, Error] {
  let preload = step.preload

  if preload == null {
    stages.execute(step.spec)
  } else {
    env ({LD_PRELOAD: preload}) {
      stages.execute(step.spec)
    }
  }
}

proc execute_with_cargo_lock(step: Step, lock_file: Path) [process, env, error, io] -> Result[Unit, Error] {
  let preload = step.preload

  if preload == null {
    stages.execute_with_flock(step.spec, lock_file)
  } else {
    env ({LD_PRELOAD: preload}) {
      stages.execute_with_flock(step.spec, lock_file)
    }
  }
}

proc cargo_test_runner_path(spec: contract.CommandSpec) [fs, error] -> Result[Path, Error] {
  let runner = fp"{spec.cwd}/dev/cargo_test_runner.sh"
  guard runner.exists() else {
    return Err(stages.StageError.MissingTool(tool: runner.display()))
  }

  runner
}

proc validate_linux_test(spec: contract.CommandSpec) {
  guard test_has_single_harness_selection(spec.argv) else {
    return Err(stages.StageError.Failed(
      stage: spec.stage,
      target: spec.target,
      detail: "Linux Cargo test requires exactly one explicit --test or --lib target",
    ))
  }

  guard test_has_matching_target(spec.argv, spec.target) else {
    return Err(stages.StageError.Failed(
      stage: spec.stage,
      target: spec.target,
      detail: "Linux Cargo test requires exactly one matching --target triple",
    ))
  }

  guard spec.target in ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"] else {
    return Err(stages.StageError.Failed(
      stage: spec.stage,
      target: spec.target,
      detail: "Linux Cargo test requires a supported Linux target triple",
    ))
  }
}

proc validate_linux_test_environment(spec: contract.CommandSpec) [env, error] {
  let reserved = [
    "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUNNER",
    "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUNNER",
    runner_lock_variable,
  ]
  for name in reserved {
    guard name not in spec.environment.keys() else {
      return Err(stages.StageError.Failed(
        stage: spec.stage,
        target: spec.target,
        detail: f"Linux Cargo test cannot override {name}",
      ))
    }
    if let Ok(_) = env.get(name) {
      return Err(stages.StageError.Failed(
        stage: spec.stage,
        target: spec.target,
        detail: f"Linux Cargo test cannot override {name}",
      ))
    }
  }
}

proc execute_linux_test(step: Step, lock_file: Path, runner: Path) [process, env, error, io] -> Result[Unit, Error] {
  let lock_path = lock_file.display()
  let runner_path = runner.display()
  match step.spec.target {
    "x86_64-unknown-linux-musl" => env ({
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUNNER: runner_path,
      XSH_CARGO_TEST_LOCK_FILE: lock_path,
    }) {
      execute_with_cargo_lock(step, lock_file)
    }
    "aarch64-unknown-linux-musl" => env ({
      CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUNNER: runner_path,
      XSH_CARGO_TEST_LOCK_FILE: lock_path,
    }) {
      execute_with_cargo_lock(step, lock_file)
    }
    else => Err(stages.StageError.Failed(
      stage: step.spec.stage,
      target: step.spec.target,
      detail: "Linux Cargo test requires a supported Linux target triple",
    ))
  }
}

## Runs a Cargo step that only compiles, under this machine's preload.
export proc run_build(spec: contract.CommandSpec) [fs, process, env, error, io] -> Result[Unit, Error] {
  let linux_host = uses_shared_cargo_lock(target_policy.host_os_tag(system.uname()?.sysname)?)
  let step = build_step(spec, detect_preload()?, env.get_or("LD_PRELOAD", "")?)
  if linux_host {
    let lock_file = cargo_lock_path(spec.cwd)
    ensure_cargo_lock_directory(lock_file)
    execute_with_cargo_lock(step, lock_file)
  } else {
    execute(step)
  }
}

## Runs a `cargo test` step: compiles under this machine's preload, then runs
## the tests without it.
export proc run_test(spec: contract.CommandSpec) [fs, process, env, error, io] -> Result[Unit, Error] {
  let linux_host = uses_shared_cargo_lock(target_policy.host_os_tag(system.uname()?.sysname)?)
  let library = detect_preload()?
  let inherited = env.get_or("LD_PRELOAD", "")?
  if linux_host {
    validate_linux_test(spec)
    validate_linux_test_environment(spec)
    let runner = cargo_test_runner_path(spec)?
    let lock_file = cargo_lock_path(spec.cwd)
    ensure_cargo_lock_directory(lock_file)
    if library != null {
      let build = test_steps(spec, library, inherited)[0]
      execute_with_cargo_lock(build, lock_file)
    }
    execute_linux_test(Step(spec:, preload: null), lock_file, runner)
  } else {
    for step in test_steps(spec, library, inherited) {
      execute(step)
    }
  }
}
