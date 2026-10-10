type Ran = {status: Int, stdout: Str, stderr: Str}

# A directory laid out the way the installed bin directory is: the front end and
# the named applets are executable suffix-free files, each shebang points at
# this interpreter, and lib/gnu.xsh sits beside them for `use lib.gnu`.
proc front_end_dir(ctx: TestContext, applets: List[Str]) [fs, process, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "coreutils")?
  fp"{root}/lib".mkdir()
  fp"{ctx.core_dir}/lib/gnu.xsh".copy(to: fp"{root}/lib/gnu.xsh")?

  for name in ["coreutils"] + applets {
    let source = if name == "coreutils" { fp"{ctx.core_dir}/coreutils.xsh" } else { fp"{ctx.core_dir}/{name}.xsh" }
    let body = source.read_text()?.replace("#!/bin/xsh", with: f"#!{ctx.xsh_bin.display()}")
    let target = fp"{root}/{name}"
    target.write(body)?
    target.chmod(0o755)?
  }

  Ok(root)
}

# Runs `front` (the front end, or a name that links to it) with args. The
# captures live in their own directory: a file beside the applets would be
# listed as one.
proc front_run(ctx: TestContext, root: Path, front: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let captures = test.temp_dir(ctx, name: "coreutils-run")?
  let out = fp"{captures}/stdout"
  let err = fp"{captures}/stderr"
  let argv = [ctx.xsh_bin.display(), front.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?

  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_coreutils_runs_the_named_applet_with_its_arguments { |ctx|
  let root = front_end_dir(ctx, ["basename"])?
  let front = fp"{root}/coreutils"

  let direct = front_run(ctx, root, front, ["basename", "/tmp/demo.txt"])?
  assert direct.status == 0, direct.stderr
  assert direct.stdout == "demo.txt\n"
  assert direct.stderr == ""

  let option = front_run(ctx, root, front, ["--coreutils-prog=basename", "-s", ".txt", "/tmp/demo.txt"])?
  assert option.status == 0, option.stderr
  assert option.stdout == "demo\n"

  let failed = front_run(ctx, root, front, ["basename"])?
  assert failed.status == 1
  assert failed.stdout == ""
  assert failed.stderr == "basename: missing operand\nTry 'basename --help' for more information.\n", failed.stderr
}

test test_coreutils_unknown_program_names_the_program { |ctx|
  let root = front_end_dir(ctx, ["basename"])?
  let front = fp"{root}/coreutils"

  for program in ["blah", "", "lib", "../coreutils"] {
    let result = front_run(ctx, root, front, [f"--coreutils-prog={program}"])?
    assert result.status == 1, program
    assert result.stdout == "", program
    assert result.stderr == f"coreutils: unknown program '{program}'\n", result.stderr
  }

  let bare_name = front_run(ctx, root, front, ["blah", "--help"])?
  assert bare_name.status == 1
  assert bare_name.stderr == "coreutils: unknown program 'blah'\n", bare_name.stderr
}

test test_coreutils_invoked_under_an_applet_name_runs_that_name { |ctx|
  let root = front_end_dir(ctx, [])?
  let link = fp"{root}/blah"
  link.symlink(to: fp"{root}/coreutils")?

  let result = front_run(ctx, root, link, ["--version"])?
  assert result.status == 1
  assert result.stderr == "coreutils: unknown program 'blah'\n", result.stderr
}

test test_coreutils_alone_help_and_version { |ctx|
  let root = front_end_dir(ctx, ["basename"])?
  let front = fp"{root}/coreutils"

  let bare = front_run(ctx, root, front, [])?
  assert bare.status == 1
  assert bare.stdout == ""
  assert bare.stderr == "Try 'coreutils --help' for more information.\n", bare.stderr

  let help = front_run(ctx, root, front, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert "Usage: coreutils --coreutils-prog=PROGRAM_NAME" in help.stdout
  assert "Built-in programs:\n basename\n" in help.stdout, help.stdout
  assert "Use: 'coreutils --coreutils-prog=PROGRAM_NAME --help'" in help.stdout

  let version = front_run(ctx, root, front, ["--version"])?
  assert version.status == 0
  assert version.stderr == ""
  assert version.stdout.starts_with("coreutils ")
}

test test_coreutils_rejects_its_own_unknown_options { |ctx|
  let root = front_end_dir(ctx, [])?
  let front = fp"{root}/coreutils"

  let short = front_run(ctx, root, front, ["-x", "yes"])?
  assert short.status == 1
  assert short.stderr == "coreutils: invalid option -- 'x'\nTry 'coreutils --help' for more information.\n", short.stderr

  let long = front_run(ctx, root, front, ["--frob"])?
  assert long.status == 1
  assert long.stderr == "coreutils: unrecognized option '--frob'\nTry 'coreutils --help' for more information.\n", long.stderr
}

test test_coreutils_forwards_undecodable_operands_unchanged { |ctx|
  let root = front_end_dir(ctx, ["basename"])?
  let captures = test.temp_dir(ctx, name: "coreutils-bytes")?
  let out = fp"{captures}/stdout"
  let err = fp"{captures}/stderr"
  let name = Path.parse_bytes(b"/tmp/some-\xc0-file.k\xf3")?
  let words: List[Union[Str, Path]] = [ctx.xsh_bin, fp"{root}/coreutils", "basename", name]
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C"}, b"", out, err))?
  assert status.exit_code()? == 0, err.read_text()?
  assert out.read_bytes()? == b"some-\xc0-file.k\xf3\n"
}
