use stage_contract

proc test_static_runner_satisfies_the_stage_contract(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "stage-runner-contract")?
  fp"${root}/stage_fake.xsh".write("""
##! Static fake stage runner.
use stage_contract

## Accepts a command request without creating a child process.
export proc execute(spec: stage_contract.CommandSpec) [process, error, io] -> Result[Unit] {
  return Ok()
}
""")?
  let result = test.run_script(
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
    [],
    {XSH_MODULE_PATH: f"${root}:dev"},
  )?
  test.ok(result.success, result.stderr)?
}
