proc build(extra: List[Str]) [process, error] {
  # begin example
  let compile = ["cc", "-O2", "-c", "main.c"]
  run @compile @extra ?

  let status = run.status @compile -fsyntax-only
  let version = run.text @(["cc", "--version"]) ?
  let job = spawn run @compile ?
  let plan = process.command {
    cwd = p"build"
    run @compile
  }
  # end example
  let _ = wait job?
  print f"{status.success} {version} {process.run(plan)?.success}"
}
