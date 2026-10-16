use stage_contract

test test_static_runner_satisfies_the_stage_contract { |ctx|
  let root = test.temp_dir(ctx, name: "stage-runner-contract")?
  fp"{root}/stage_fake.xsh".write("""
##! Static fake stage runner.
use stage_contract

## Accepts a command request without creating a child process.
export proc execute(spec: stage_contract.CommandSpec) [process, error, io] -> Result[Unit, Error] {
  return Ok()
}
""")
  test.expect(
    ctx,
    """
use stage_contract
use stage_fake

proc main() [process, error, io] -> Result[Unit] {
  let runner: stage_contract.StageRunner = stage_fake
  let spec: stage_contract.CommandSpec = {
    stage: "fixture",
    target: "fixture-target",
    executable: "tool",
    argv: ["tool", "--check"],
    cwd: p".",
      environment: {},
  }
  runner.execute(spec)?
}

main()?
""",
    status: 0,
    args: [],
    env: {XSH_MODULE_PATH: f"{root}:dev"},
  )?
}

test test_direct_harness_runtime_does_not_require_the_cargo_lock { |ctx|
  let root = test.temp_dir(ctx, name: "direct-harness-runtime")?
  let log = fp"{root}/args.log"
  let executable = fp"{root}/harness"
  executable.write(
    f"""#!{ctx.xsh_bin}
p"{log}".write(args.join("|"))?
""",
    mode: 0o755,
  )
  let repository = fs.cwd()?
  test.expect(
    ctx,
    f"""
use stage

let root = p"{root}"
let executable = p"{executable}"
stage.execute(stage.command("native-test", "fixture", executable.display(),
  [executable.display(), "--exact", "selected_test"], root, {{}}))?
""",
    status: 0,
    stdout: [f"[native-test target=fixture] {executable} --exact selected_test"],
    args: [],
    env: {
      PATH: "",
      XSH_MODULE_PATH: fp"{repository}/dev",
    },
  )?
  assert log.read_text()? == "--exact|selected_test"
}
