##! Process-execution capability for development lifecycle stages.
use stage_contract as contract

## Operational failures produced at the tool and command boundary.
export error StageError = MissingTool(tool: Str) | Failed(stage: Str, target: Str, detail: Str)

## Constructs a command request before it crosses the process boundary.
export pure command(
  stage: Str,
  target: Str,
  executable: Str,
  argv: List[Str],
  cwd: Path,
  environment: Record,
) -> contract.CommandSpec {
  return {
    stage: stage,
    target: target,
    executable: executable,
    argv: argv,
    cwd: cwd,
    environment: environment,
  }
}

## Resolves a required external program with a named diagnostic on absence.
export proc require_tool(name: Str) [process, error] -> Result[Path] {
  match process.which(name) {
    Ok(tool_path) => return tool_path
    Err(_) => return Err(StageError.MissingTool(tool: name))
  }
}

## Creates a lifecycle directory when it does not already exist.
export proc ensure_dir(directory: Path) [fs, error] -> Result[Unit] {
  if ! directory.exists()? {
    directory.mkdir()?
  }
}

## Executes one visible direct process boundary and classifies failed status.
export proc execute(spec: contract.CommandSpec) [process, error, io] -> Result[Unit] {
  print f"[${spec.stage} target=${spec.target}] ${spec.argv.join(" ")}" 
  let status = process.run(process.command_argv(spec.executable, spec.argv, cwd: spec.cwd, env: spec.environment))?
  if status.ok {
    return Ok()
  }
  let detail = if status.exited() {
    f"exit ${status.exit_code()?}"
  } else if status.signaled() {
    f"signal ${status.signal_number()?}"
  } else {
    "unknown process status"
  }
  return Err(StageError.Failed(stage: spec.stage, target: spec.target, detail: detail))
}
