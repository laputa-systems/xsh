##! Cargo build and test steps, and the allocator preload their compiler runs under.
use stage as stages
use stage_contract as contract

## A `rustc` built for a musl host allocates through musl's allocator, and a
## thin-LTO build then spends most of its time in the kernel. Preloading this
## jemalloc into the compiler makes a release rebuild several times faster.
export const jemalloc_library = /usr/lib/libjemalloc.so.2

## Names another library to preload into build steps; set to the empty string,
## it disables the preload.
export const override_variable = "XSH_DEV_BUILD_PRELOAD"

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
      spec: {
        ...spec,
        stage: f"{spec.stage}-build",
        argv: test_build_argv(spec.argv),
      },
      preload: preload_value(library, inherited),
    ),
    Step(spec:, preload: null),
  ]
}

# The preload is an environment overlay that ends with the step, so a later
# step never inherits it.
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

## Runs a Cargo step that only compiles, under this machine's preload.
export proc run_build(spec: contract.CommandSpec) [fs, process, env, error, io] -> Result[Unit, Error] {
  execute(build_step(spec, detect_preload()?, env.get_or("LD_PRELOAD", "")?))
}

## Runs a `cargo test` step: compiles under this machine's preload, then runs
## the tests without it.
export proc run_test(spec: contract.CommandSpec) [fs, process, env, error, io] -> Result[Unit, Error] {
  for step in test_steps(spec, detect_preload()?, env.get_or("LD_PRELOAD", "")?) {
    execute(step)
  }
}
