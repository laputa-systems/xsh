test test_du { |ctx|
  let target = test.temp_file(ctx, name: "du.txt", contents: b"abcdef")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" $target ?
  assert "du.txt" in output
  let apparent = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -b $target ?
  assert "6" in apparent
  let human = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -sh $target ?
  assert "K" in human
  let kilobytes = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -k $target ?
  assert kilobytes.starts_with("4\t")
  let apparent_size = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" --apparent-size $target ?
  assert apparent_size.starts_with("1\t")
}

test test_du_recursive_all_and_total { |ctx|
  let root = test.temp_dir(ctx, name: "du-tree")?
  fp"{root}/a.txt".write("aaa")
  fs.mkdir(fp"{root}/sub")
  fp"{root}/sub/b.txt".write("bb")
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" -a -c $root ?
  assert f"{root}/a.txt" in output
  assert f"{root}/sub/b.txt" in output
  assert f"{root}/sub" in output
  assert "total" in output
  let summarized = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/du.xsh" --summarize --total $root ?
  assert f"{root}" in summarized
  assert "total" in summarized
}

type DuResult = {status: Int, stdout: Str, stderr: Str}

proc du_in(ctx: TestContext, dir: Path, args: List[Str], input = b"") [fs, process, error] -> Result[DuResult] {
  let root = test.temp_dir(ctx, name: "du-run")?
  let stdin = test.temp_file(ctx, name: "du-stdin", contents: input)?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/du.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, dir, {}, stdin, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_du_files0_from { |ctx|
  let dir = test.temp_dir(ctx, name: "du-files0")?
  fp"{dir}/a".write("one")
  fp"{dir}/b".write("two")
  fp"{dir}/list".write(b"a\0b\0")

  let listed = du_in(ctx, dir, ["--files0-from=list"])?
  assert listed.status == 0, listed.stderr
  assert "a" in listed.stdout
  assert "b" in listed.stdout

  let piped = du_in(ctx, dir, ["--files0-from=-"], b"a\0b\0")?
  assert piped.status == 0, piped.stderr
  assert "a" in piped.stdout
  assert "b" in piped.stdout
}
