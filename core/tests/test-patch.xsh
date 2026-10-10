use data.patch.tree

type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", vars: Record = {LC_ALL: "C", TERM: "xterm"}, cwd: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "patch-capture")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/patch.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), cwd ?? root, vars, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8()?, stderr: err.read_text()?})
}

test test_patch_applies_unified_stdin { |ctx|
  let root = test.temp_dir(ctx, name: "patch-root")?
  let target = fp"{root}/file"
  target.write("old\n")
  let patch_text = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n"
  let result = invoke(ctx, ["-p1", "-d", root.display()], patch_text)?
  assert result.status == 0, result.stderr
  assert target.read_text()? == "new\n"
}

test test_patch_rejects_escape_and_checks_dry_run { |ctx|
  let root = test.temp_dir(ctx, name: "patch-root")?
  let escaped = invoke(ctx, ["-p0", "-d", root.display()], b"--- /dev/null\n+++ ../outside\n@@ -0,0 +1 @@\n+bad\n")?
  assert escaped.status != 0
  assert !fp"{root}/../outside".exists()?
  fp"{root}/file".write("old\n")
  let dry = invoke(ctx, ["--dry-run", "-d", root.display()], b"--- file\n+++ file\n@@ -1 +1 @@\n-old\n+new\n")?
  assert dry.status == 0, dry.stderr
  assert dry.stdout == "checking file file\n"
  assert fp"{root}/file".read_text()? == "old\n"
}

test test_patch_named_original_and_input_file { |ctx|
  let root = test.temp_dir(ctx, name: "patch-original")?
  let target = fp"{root}/target"
  target.write("old\n")
  let input = test.temp_file(ctx, name: "patch-input", contents: b"--- old-name\n+++ new-name\n@@ -1 +1 @@\n-old\n+new\n")?
  let result = invoke(ctx, [target.display(), input.display()])?
  assert result.status == 0, result.stderr
  assert target.read_text()? == "new\n"
}

test test_patch_multi_file_failure_keeps_prior_applied_file { |ctx|
  let root = test.temp_dir(ctx, name: "patch-multi")?
  fp"{root}/first".write("old\n")
  fp"{root}/second".write("different\n")
  let input = b"--- first\n+++ first\n@@ -1 +1 @@\n-old\n+new\n--- second\n+++ second\n@@ -1 +1 @@\n-old\n+new\n"
  let result = invoke(ctx, ["-d", root.display()], input)?
  assert result.status == 1
  assert fp"{root}/first".read_text()? == "new\n"
  assert fp"{root}/second".read_text()? == "different\n"
}

# Octal permission text such as "644" as a number.
pure octal(text: Str) -> Int {
  var value = 0
  for digit in text { value = value * 8 + (digit.parse_int() ?? 0) }
  value
}

# What one corpus case produced, compared with what GNU patch produced and
# recorded under `out/`. Returns a description of each difference.
proc check_case(ctx: TestContext, root: Path) [fs, process, env, error] -> Result[List[Str], Error] {
  let name = root.name()
  let work = test.temp_dir(ctx, name: "patch-case")?
  let _ = fs.copy_tree(fp"{root}/in", fp"{work}/w", parents: true)?
  var mtimes: List[Str] = []
  if fp"{root}/mtimes".exists()? { mtimes = fp"{root}/mtimes".read_text()?.lines().collect() }
  if fp"{root}/setup".exists()? {
    for line in fp"{root}/setup".read_text()?.lines() {
      let words = line.split(" ")
      if words[0] == "chmod" { fp"{work}/w/{words[2]}".chmod(octal(words[1]))? }
      if words[0] == "touch" {
        let seconds = words[1].parse_int()?
        fs.set_times(fp"{work}/w/{words[2]}", atime_sec: seconds, mtime_sec: seconds)?
      }
    }
  }
  var arguments: List[Str] = []
  if fp"{root}/args".exists()? { arguments = fp"{root}/args".read_text()?.lines().collect() }
  var input = b""
  if fp"{root}/stdin".exists()? { input = fp"{root}/stdin".read_bytes()? }
  var recorded_env = ""
  if fp"{root}/env".exists()? { recorded_env = fp"{root}/env".read_text()? }
  var vars: Record = {LC_ALL: "C", TERM: "xterm"}
  if recorded_env.starts_with("PATCH_VERSION_CONTROL=") {
    vars = {LC_ALL: "C", TERM: "xterm", PATCH_VERSION_CONTROL: recorded_env.byte_slice(22).trim()}
  } else if recorded_env.starts_with("POSIXLY_CORRECT=") {
    vars = {LC_ALL: "C", TERM: "xterm", POSIXLY_CORRECT: recorded_env.byte_slice(16).trim()}
  } else if recorded_env.starts_with("QUOTING_STYLE=") {
    vars = {LC_ALL: "C", TERM: "xterm", QUOTING_STYLE: recorded_env.byte_slice(14).trim()}
  } else if recorded_env.starts_with("VERSION_CONTROL=") {
    vars = {LC_ALL: "C", TERM: "xterm", VERSION_CONTROL: recorded_env.byte_slice(16).trim()}
  }
  let out = fp"{work}/stdout"
  let err = fp"{work}/stderr"
  let script = fp"{ctx.core_dir}/patch.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(arguments), fp"{work}/w", vars, input, out, err)
  let status = process.run(plan)?
  var problems: List[Str] = []
  let expected_status = fp"{root}/out/status".read_text()?.trim().parse_int()?
  if status.exit_code()? != expected_status {
    problems += [f"{name}: exit status {status.exit_code()?}, expected {expected_status}"]
  }
  let expected_out = fp"{root}/out/stdout".read_bytes()?
  if out.read_bytes()? != expected_out {
    problems += [f"{name}: stdout differs\n--- expected\n{expected_out.utf8() ?? "(binary)"}--- actual\n{out.read_bytes()?.utf8() ?? "(binary)"}"]
  }
  let expected_err = fp"{root}/out/stderr".read_bytes()?
  if err.read_bytes()? != expected_err {
    problems += [f"{name}: stderr differs\n--- expected\n{expected_err.utf8() ?? "(binary)"}--- actual\n{err.read_bytes()?.utf8() ?? "(binary)"}"]
  }
  let manifest = tree.describe(fp"{work}/w", mtimes)?
  let expected_manifest = fp"{root}/out/manifest".read_text()?.lines().collect()
  if manifest != expected_manifest {
    problems += [f"{name}: tree differs\n--- expected\n{expected_manifest.join("\n")}\n--- actual\n{manifest.join("\n")}"]
    return Ok(problems)
  }
  for row in expected_manifest {
    if !row.starts_with("f ") { continue }
    var relative = row.byte_slice(4)
    let stamp = relative.find(" mtime=") ?? -1
    if stamp >= 0 { relative = relative.byte_slice(0, stamp) }
    let stored = fp"{root}/out/tree/{relative}"
    let wanted = if stored.exists()? { stored.read_bytes()? } else { fp"{root}/in/{relative}".read_bytes()? }
    let actual = fp"{work}/w/{relative}".read_bytes()?
    if actual != wanted {
      problems += [f"{name}: content of {relative} differs\n--- expected\n{wanted.utf8() ?? "(binary)"}--- actual\n{actual.utf8() ?? "(binary)"}"]
    }
  }
  Ok(problems)
}

# Each directory under data/patch is one case recorded from GNU patch: the
# starting tree, the arguments, standard input, and the exact standard
# output, standard error, exit status, and resulting tree.
test test_patch_matches_recorded_gnu_patch_results { |ctx|
  let corpus = fp"{ctx.core_dir}/tests/data/patch"
  var problems: List[Str] = []
  var count = 0
  var names: List[Str] = []
  for entry in fs.children(corpus, stat: true)? {
    if entry.kind == "dir" { names += [entry.name] }
  }
  let ordered = names |> sort
  for name in ordered {
    if !fp"{corpus}/{name}/out/status".exists()? { continue }
    count += 1
    problems += check_case(ctx, fp"{corpus}/{name}")?
  }
  assert count > 0, "no recorded cases found"
  assert problems.is_empty(), f"{problems.len()} differences in {count} cases:\n{problems.join("\n")}"
}

# GNU patch prints its own copyright banner; the applets here announce
# themselves with the project's common version line instead.
test test_patch_version_uses_project_banner { |ctx|
  let result = invoke(ctx, ["--version"])?
  assert result.status == 0, result.stderr
  assert result.stdout.starts_with("patch (XSH core)"), result.stdout
}
