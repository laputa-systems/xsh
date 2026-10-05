proc pack(out: Path, inputs: List[Path]) [process, error] -> Result[Status] {
  # begin example
  let tar = process.which("tar")?
  let argv: List[Union[Str, Path]] = [tar, "-cf", out, @inputs]
  let plan = process.command_argv(tar, argv)
  # end example
  process.run(plan)
}
