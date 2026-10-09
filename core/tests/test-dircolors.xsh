type DircolorsRun = {status: Int, stdout: Str, stderr: Str}

proc dircolors_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C", SHELL: "/bin/bash", TERM: "screen"},
  stdin = b"",
) [fs, process, error] -> Result[DircolorsRun] {
  let root = test.temp_dir(ctx, name: "dircolors")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/dircolors.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err))?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_dircolors_default_shell_output_and_database { |ctx|
  let output = dircolors_run(ctx, ["-b"])?
  assert output.status == 0, output.stderr
  assert output.stdout.starts_with("LS_COLORS='rs=0:di=01;34:ln=01;36:mh=00:")
  assert output.stdout.ends_with("';\nexport LS_COLORS\n")
  let db = dircolors_run(ctx, ["-p"], {LC_ALL: "C"})?
  assert db.status == 0 and db.stdout.starts_with("# Configuration file for dircolors")
  assert "TERM screen*\n" in db.stdout and ".tar 01;31\n" in db.stdout
  let no_shell = dircolors_run(ctx, ["-p", "-p"], {LC_ALL: "C", SHELL: ""})?
  assert no_shell.status == 0 and no_shell.stdout == db.stdout, no_shell.stderr
}

test test_dircolors_csh_and_print_colors { |ctx|
  let csh = dircolors_run(ctx, ["-b", "-c"])?
  assert csh.status == 0 and csh.stdout.starts_with("setenv LS_COLORS '")
  assert csh.stdout.ends_with("'\n")
  let guessed_csh = dircolors_run(ctx, [], {LC_ALL: "C", SHELL: "/bin/tcsh", TERM: "screen"})?
  assert guessed_csh.status == 0 and guessed_csh.stdout.starts_with("setenv LS_COLORS '")
  let shown = dircolors_run(ctx, ["--print-ls-colors"], {LC_ALL: "C"})?
  assert shown.status == 0 and "\u{001b}[01;34mdi\t01;34\u{001b}[0m" in shown.stdout
  let bc = dircolors_run(ctx, ["-bc"])?
  assert bc.status == 0 and bc.stdout.starts_with("setenv LS_COLORS '")
  let cb = dircolors_run(ctx, ["-cb"])?
  assert cb.status == 0 and cb.stdout.starts_with("LS_COLORS='")
  let repeated = dircolors_run(ctx, ["-b", "-b"])?
  assert repeated.status == 0 and repeated.stderr == "" and repeated.stdout.starts_with("LS_COLORS='")
}

test test_dircolors_reads_database_and_filters_terms { |ctx|
  let matched = dircolors_run(ctx, ["-b", "-"], {LC_ALL: "C", TERM: "xterm-256color"}, b"TERM xterm*\nDIR 01;34\n.tar 01;31\n")?
  assert matched.status == 0
  assert matched.stdout == "LS_COLORS='di=01;34:*.tar=01;31:';\nexport LS_COLORS\n", matched.stdout
  let unmatched = dircolors_run(ctx, ["-b", "-"], {LC_ALL: "C", TERM: "dumb"}, b"TERM xterm*\nDIR 01;34\n")?
  assert unmatched.stdout == "LS_COLORS='';\nexport LS_COLORS\n", unmatched.stdout
  let term_glob = dircolors_run(ctx, ["-b", "-"], {LC_ALL: "C", TERM: "b_term"}, b"TERM [!a]_term\nDIR 01;34\n")?
  assert term_glob.stdout == "LS_COLORS='di=01;34:';\nexport LS_COLORS\n", term_glob.stdout
}

test test_dircolors_escapes_and_reports_invalid_input { |ctx|
  let quoted = dircolors_run(ctx, ["-b", "-"], {LC_ALL: "C", TERM: "screen"}, b"exec 'echo Hello;:'\n")?
  assert quoted.stdout == "LS_COLORS='ex='\\''echo Hello;\\:'\\'':';\nexport LS_COLORS\n", quoted.stdout
  let invalid = dircolors_run(ctx, ["-b", "-"], {LC_ALL: "C", TERM: "screen"}, b"BAD\n")?
  assert invalid.status == 1 and invalid.stdout == "" and "missing token" in invalid.stderr, invalid.stderr
  let exclusive = dircolors_run(ctx, ["-b", "--print-database"])?
  assert exclusive.status == 1 and "mutually exclusive" in exclusive.stderr
}

test test_dircolors_requires_output_shell_and_rejects_extra_files { |ctx|
  let no_shell = dircolors_run(ctx, [], {LC_ALL: "C", SHELL: ""})?
  assert no_shell.status == 1 and no_shell.stderr == "dircolors: no SHELL environment variable, and no shell type option given\n"
  let extra = dircolors_run(ctx, ["-c", "one", "two"])?
  assert extra.status == 1 and "extra operand 'two'" in extra.stderr
}
