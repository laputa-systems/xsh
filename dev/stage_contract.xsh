##! Static contract for the repository-local process-execution seam.
## A fully resolved command with lifecycle diagnostics and no ambient execution state.
export type CommandSpec = {
  stage: Str,
  target: Str,
  executable: Str,
  argv: List[Str],
  cwd: Path,
  environment: Record,
}

## The one static-module substitution seam for process execution.
export type StageRunner = module {
  export proc execute(spec: CommandSpec) [process, error, io] -> Result[Unit]
}
