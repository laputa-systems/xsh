test test_script_path_is_the_script_as_invoked { |ctx|
  let root = test.temp_dir(ctx, name: "script-path")?.resolve()?
  let script = fp"{root}/probe.xsh"
  script.write("""proc main(...argv: List[Str]) [error, io, process] -> Result[Unit] {
  print process.script_path()?.display()
}
""")
  fp"{root}/alias".symlink(to: p"probe.xsh")

  let plain = run.text ${ctx.xsh_bin} $script
  assert plain == f"{script}\n"

  let separated = run.text ${ctx.xsh_bin} -- $script extra
  assert separated == f"{script}\n"

  let alias = fp"{root}/alias"
  let aliased = run.text ${ctx.xsh_bin} $alias
  assert aliased == f"{alias}\n", "a symlink alias must keep its own name"

  let stdout = fp"{root}/relative.txt"
  let relative = process.command {
    cwd = root
    stdout = stdout
    run ${ctx.xsh_bin} ./alias
  }
  assert process.run(relative)?.exited_with(0)
  assert stdout.read_text()? == "./alias\n", "a relative path must stay relative and unresolved"
}
