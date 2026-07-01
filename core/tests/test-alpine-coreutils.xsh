pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc require_alpine() [fs, env, error] {
  if env.bool("XSH_SKIP_LIVE_COREUTILS_COMPARISONS")? {
    test.skip("live coreutils comparison disabled")
  }

  let alpine_release = /etc/alpine-release

  if ! alpine_release.exists()? {
    test.skip("Alpine-only coreutils comparison")
  }
}

proc xsh_out(tool: Str, argv: List[Str]) [process, error] -> Result[Str] {
  let script = fp"${tool}.xsh"
  return run.text xsh_bin() $script -- @argv ?
}

proc alpine_out(tool: Str, argv: List[Str]) [process, error] -> Result[Str] {
  return run.text $tool @argv ?
}

pure shell_quote(text: Str) -> Str {
  return f"'${text.replace("'", "'\\''")}'"
}

pure shell_words(argv: List[Str]) -> Str {
  return [shell_quote(arg) for arg in argv].join(" ")
}

proc xsh_out_in(root: Path, tool: Str, argv: List[Str]) [fs, process, error] -> Result[Str] {
  let xsh = xsh_bin().resolve()?
  let script = fp"${tool}.xsh".resolve()?
  let joined = shell_words(argv)
  let command = f"cd ${shell_quote(root.display())} && ${shell_quote(xsh.display())} ${shell_quote(script.display())} -- ${joined}"
  return run.text sh -c $command ?
}

proc alpine_out_in(root: Path, tool: Str, argv: List[Str]) [process, error] -> Result[Str] {
  let joined = shell_words(argv)
  let command = f"cd ${shell_quote(root.display())} && ${tool} ${joined}"
  return run.text sh -c $command ?
}

proc assert_same(tool: Str, argv: List[Str]) [process, error] {
  test.eq(xsh_out(tool, argv)?, alpine_out(tool, argv)?)?
}

proc assert_same_in(root_xsh: Path, root_alpine: Path, tool: Str, argv: List[Str]) [fs, process, error] {
  test.eq(xsh_out_in(root_xsh, tool, argv)?, alpine_out_in(root_alpine, tool, argv)?)?
}

proc tree_snapshot(root: Path) [fs, error] -> Result[Str] {
  var rows: List[Str] = []

  for entry in fs.walk(root) |> sort-by .path {
    let rel = entry.path.relative_to(root).display()
    continue when rel == "."

    if entry.kind == "file" {
      rows = rows.push(f"file ${rel} ${entry.path.read_text()?}")
    } else if entry.kind == "symlink" {
      rows = rows.push(f"symlink ${rel} -> ${entry.path.readlink()?.display()}")
    } else {
      rows = rows.push(f"${entry.kind} ${rel}")
    }
  }

  return rows.join("\n")
}

proc test_alpine_text_filters(ctx: TestContext) [fs, process, env, error] {
  require_alpine()?
  let lines = test.temp_file(ctx, name: "lines.txt", contents: b"b\na\na\nc\n")?
  let csv = test.temp_file(ctx, name: "data.csv", contents: b"a,b,c\nd,e,f\n")?
  let words = test.temp_file(ctx, name: "words.txt", contents: b"one two\nthree\n")?
  let long = test.temp_file(ctx, name: "long.txt", contents: b"abcdef\n")?
  assert_same("cat", [lines.display()])?
  assert_same("head", ["-n", "2", lines.display()])?
  assert_same("tail", ["-n", "2", lines.display()])?
  assert_same("sort", ["-u", "-r", lines.display()])?
  let nums = test.temp_file(ctx, name: "nums.txt", contents: b"10\n2\n1\n")?
  assert_same("sort", ["-n", nums.display()])?
  assert_same("sort", ["-nr", nums.display()])?
  let keyed = test.temp_file(ctx, name: "keyed.txt", contents: b"b,20\na,3\nc,1\n")?
  assert_same("sort", ["-t", ",", "-k2", "-n", keyed.display()])?
  assert_same("uniq", ["-c", lines.display()])?
  assert_same("cut", ["-d", ",", "-f", "2", csv.display()])?
  assert_same("cut", ["-d", ",", "-f", "1,3", csv.display()])?
  assert_same("cut", ["-d", ",", "-f", "2-", csv.display()])?
  assert_same("cut", ["-c", "2-4", long.display()])?
  assert_same("fold", ["-w", "3", long.display()])?
  assert_same("paste", ["-s", "-d:", lines.display()])?

  assert_same(
    "printf",
    [
      """%s %s
""",
      "hello",
      "xsh",
      "again",
    ],
  )?

  assert_same("seq", ["2", "2", "6"])?
  assert_same("wc", ["-lwc", words.display()])?
}

proc test_alpine_head_tail_multi_file_headers(ctx: TestContext) [fs, process, env, error] {
  require_alpine()?
  let one = test.temp_file(ctx, name: "one.txt", contents: b"1a\n1b\n1c\n")?
  let two = test.temp_file(ctx, name: "two.txt", contents: b"2a\n2b\n2c\n")?
  assert_same("head", ["-n", "1", one.display(), two.display()])?
  assert_same("head", ["-q", "-n", "1", one.display(), two.display()])?
  assert_same("tail", ["-n", "1", one.display(), two.display()])?
  assert_same("tail", ["-q", "-n", "1", one.display(), two.display()])?
}

proc test_alpine_path_tools(ctx: TestContext) [fs, process, env, error] {
  require_alpine()?
  let root = test.temp_dir(ctx, name: "paths")?
  let target = fp"${root}/target.txt"
  target.write("ok")?
  let link = fp"${root}/link.txt"
  fs.symlink(target, link)?
  assert_same("basename", ["-a", "-s", ".txt", "/tmp/demo.txt", "/tmp/other.txt"])?
  assert_same("dirname", ["/tmp/demo/file.txt"])?
  assert_same("readlink", ["-f", link.display()])?
  assert_same("realpath", [link.display()])?
}

proc test_alpine_listing_and_stat(ctx: TestContext) [fs, process, env, error] {
  require_alpine()?
  let root = test.temp_dir(ctx, name: "listing")?
  fp"${root}/alpha.txt".write("abc")?
  fp"${root}/.hidden".write("hidden")?
  fs.mkdir(fp"${root}/dir")?
  assert_same("ls", ["-a", "-p", root.display()])?
  assert_same("ls", ["-d", "-p", fp"${root}/dir".display()])?
  assert_same("stat", ["-c", "%s %F %n", fp"${root}/alpha.txt".display()])?
  assert_same("stat", ["-c", "%s %b %B %u %g %Y", fp"${root}/alpha.txt".display()])?
}

proc test_alpine_du_modes(ctx: TestContext) [fs, process, env, error] {
  require_alpine()?
  let root = test.temp_dir(ctx, name: "du")?
  fp"${root}/alpha.txt".write("abcdef")?
  fs.mkdir(fp"${root}/sub")?
  fp"${root}/sub/beta.txt".write("nested")?
  assert_same("du", [fp"${root}/alpha.txt".display()])?
  assert_same("du", ["-b", root.display()])?
  assert_same("du", ["-s", root.display()])?
  assert_same("du", ["-a", "-c", root.display()])?
  test.contains(xsh_out("du", ["-sh", fp"${root}/alpha.txt".display()])?, "K")?
}

proc prepare_file_ops(root: Path) [fs, error] {
  fp"${root}/src".write("new")?
  fp"${root}/dst".write("old")?
  fp"${root}/msrc".write("move-new")?
  fp"${root}/mdst".write("move-old")?
  fs.mkdir(fp"${root}/bucket")?
  fs.mkdir(fp"${root}/tree")?
  fp"${root}/tree/nested".write("nested")?
  fs.mkdir(fp"${root}/remove-me")?
  fp"${root}/remove-me/dead".write("dead")?
  fp"${root}/mode".write("mode")?
  fp"${root}/owner".write("owner")?
  fp"${root}/split-input".write("abcdef")?
}

proc test_alpine_file_operation_options(ctx: TestContext) [fs, process, env, error] {
  require_alpine()?
  let xroot = test.temp_dir(ctx, name: "file-ops-xsh")?
  let aroot = test.temp_dir(ctx, name: "file-ops-alpine")?
  prepare_file_ops(xroot)?
  prepare_file_ops(aroot)?
  assert_same_in(xroot, aroot, "cp", ["-n", "src", "dst"])?
  assert_same_in(xroot, aroot, "cp", ["-t", "bucket", "src", "dst"])?
  assert_same_in(xroot, aroot, "cp", ["-s", "src", "sym"])?
  assert_same_in(xroot, aroot, "cp", ["-l", "src", "hard"])?
  assert_same_in(xroot, aroot, "cp", ["-R", "tree", "tree-copy"])?
  assert_same_in(xroot, aroot, "mv", ["-n", "msrc", "mdst"])?
  assert_same_in(xroot, aroot, "rm", ["-rf", "remove-me", "missing"])?
  assert_same_in(xroot, aroot, "tee", ["-ai", "tee-out"])?
  assert_same_in(xroot, aroot, "chmod", ["600", "mode"])?
  assert_same_in(xroot, aroot, "chmod", ["u+x,g+r", "mode"])?
  assert_same_in(xroot, aroot, "chown", ["0:0", "owner"])?
  assert_same_in(xroot, aroot, "chgrp", ["0", "owner"])?
  assert_same_in(xroot, aroot, "split", ["-b", "3", "split-input", "part-"])?
  assert_same_in(xroot, aroot, "stat", ["-c", "%a %u %g", "mode", "owner"])?
  test.eq(tree_snapshot(xroot)?, tree_snapshot(aroot)?)?
}

proc prepare_touch_ops(root: Path) [fs, error] {
  fp"${root}/reference".write("reference")?
  fp"${root}/target".write("target")?
}

proc test_alpine_touch_reference_and_no_create(ctx: TestContext) [fs, process, env, error] {
  require_alpine()?
  let xroot = test.temp_dir(ctx, name: "touch-xsh")?
  let aroot = test.temp_dir(ctx, name: "touch-alpine")?
  prepare_touch_ops(xroot)?
  prepare_touch_ops(aroot)?
  assert_same_in(xroot, aroot, "touch", ["-r", "reference", "target"])?
  assert_same_in(xroot, aroot, "touch", ["-c", "missing"])?
  assert_same_in(xroot, aroot, "stat", ["-c", "%Y", "reference", "target"])?
  test.eq(tree_snapshot(xroot)?, tree_snapshot(aroot)?)?
}
