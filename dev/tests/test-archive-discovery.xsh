type ToolRun = {succeeded: Bool, transcript: Str}

proc xsht(ctx: TestContext, root: Path, arguments: List[Path]) [fs, process, error] -> Result[ToolRun] {
  let executable = fp"{ctx.xsh_bin.parent()}/xsht"
  let stdout = fp"{root}/xsht.stdout"
  let stderr = fp"{root}/xsht.stderr"
  let command = process.command_argv(
    executable,
    [executable, @arguments],
    cwd: root,
    stdout:,
    stderr:,
  )
  let status = process.run(command)?
  {succeeded: status.ok, transcript: stdout.read_text()? + stderr.read_text()?}
}

test test_project_discovery_skips_archived_xsh_and_keeps_explicit_files { |ctx|
  let root = test.temp_dir(ctx, name: "archive-discovery")?
  let active = fp"{root}/src/active.xsh"
  let evidence_root = fp"{root}/dev/consolidation/evidence"
  let archived_project = fp"{evidence_root}/procps-exit-races/integration-787/attempt3/generated-workspace"
  let archived = fp"{archived_project}/core/tests/test-procps.xsh"
  let project_config = fp"{ctx.core_dir.parent()}/xsht-config.ini".read_text()?
  fp"{root}/src".mkdir()
  fp"{root}/core".mkdir()
  archived_project.mkdir()
  archived.parent().mkdir()
  fp"{root}/xsht-config.ini".write(project_config)
  fp"{archived_project}/xsht-config.ini".write(project_config)
  active.write("print $active_missing\n")
  archived.write("print $archive_missing\n")

  let checked = xsht(ctx, root, [p"check"])?
  assert ! checked.succeeded
  assert "active_missing" in checked.transcript
  assert "archive_missing" not in checked.transcript

  let linted = xsht(ctx, root, [p"lint"])?
  assert ! linted.succeeded
  assert "active_missing" in linted.transcript
  assert "archive_missing" not in linted.transcript

  let explicit = xsht(ctx, root, [p"lint", archived])?
  assert ! explicit.succeeded
  assert "archive_missing" in explicit.transcript
}
