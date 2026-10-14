test test_filesystem_paths_and_walks_evaluate_operands_once { |ctx|
  let root = test.temp_dir(ctx, name: "operand-order-host-files")?
  fp"{root}/payload.txt".write("payload")
  fp"{root}/payload.bin".write(b"raw\0bytes")
  fp"{root}/.hidden.txt".write("hidden")
  fp"{root}/link".symlink(to: p"payload.txt")
  fp"{root}/remove-me".write("remove")

  let output = test.expect(
    ctx,
    r"""proc path(label: Str, value: Path) [io] -> Path {
  print $label
  return value
}
proc flag(label: Str, value: Bool) [io] -> Bool {
  print $label
  return value
}
proc exts(label: Str, value: List[Str]) [io] -> List[Str] {
  print $label
  return value
}
proc data(label: Str, value: Str) [io] -> Str {
  print $label
  return value
}
let root = Path(args[0])
let files = fs.files(
  path("files-path", root),
  hidden: flag("files-hidden", false),
  exts: exts("files-exts", ["txt"]),
  stat: flag("files-stat", false),
  gitignore: flag("files-gitignore", false),
)?.collect()
assert files |> any .name == "payload.txt"
assert ! (files |> any .name == ".hidden.txt")
let walked = fs.walk(
  path("walk-path", root),
  hidden: flag("walk-hidden", false),
  stat: flag("walk-stat", false),
  gitignore: flag("walk-gitignore", false),
)?.collect()
assert walked |> any .name == "payload.txt"
let children = fs.children(
  path("list-path", root),
  ordered: flag("list-ordered", true),
  stat: flag("list-stat", true),
)?.collect()
assert children |> any .name == "payload.txt"
assert path("read-text-path", p"{root}/payload.txt").read_text()? == "payload"
assert path("read-bytes-path", p"{root}/payload.bin").read_bytes()? == b"raw\0bytes"
assert path("exists-path", p"{root}/payload.txt").exists()?
assert path("executable-path", Path(args[1])).executable()?
assert path("du-path", root).du()? >= 0
assert path("metadata-path", p"{root}/payload.txt").metadata()?.size == 7
assert path("readlink-path", p"{root}/link").readlink()? == p"payload.txt"
assert path("resolve-path", p"{root}/payload.txt").resolve()?.name() == "payload.txt"
path("write-path", p"{root}/written").write(data("write-data", "written"))
path("mkdir-path", p"{root}/made").mkdir()
path("remove-path", p"{root}/remove-me").remove(missing_ok: false)
print "complete"
""",
    status: 0,
    args: [root, ctx.xsh_bin],
  )?
  assert output.stdout == """files-path
files-hidden
files-exts
files-stat
files-gitignore
walk-path
walk-hidden
walk-stat
walk-gitignore
list-path
list-ordered
list-stat
read-text-path
read-bytes-path
exists-path
executable-path
du-path
metadata-path
readlink-path
resolve-path
write-path
write-data
mkdir-path
remove-path
complete
""", output.stdout
}

test test_tempdir_scope_captures_output_and_tempdir_cleans_up { |ctx|
  let root = test.temp_dir(ctx, name: "operand-order-host-tempdir")?
  let scoped = fp"{root}/scoped"
  let path_file = fp"{root}/scratch-path"
  let output = test.expect(
    ctx,
    f"""proc mark_path(label: Str, value: Path) [io] -> Path {{
  print $label
  return value
}}
tempdir scratch at mark_path("scope-path", p"{scoped}") {{
  print $scratch.name
  fp"{{scratch}}/inside".write("payload")
}}
print \${{p"{scoped}".exists()?}}
let scratch = fs.tempdir()?
p"{path_file}".write(scratch.host_path()?.display())
print "tempdir-created"
""",
    status: 0,
  )?
  assert output.stdout == """scope-path
scoped
false
tempdir-created
"""
  let scratch = fp"{path_file.read_text()?}"
  assert ! scratch.exists()?
}

test test_result_and_error_constructors_evaluate_operands_once { |ctx|
  let output = test.expect(
    ctx,
    r"""error OperandFailure = Failed(first: Str, second: Str)
proc mark(label: Str, value: Str) [io] -> Str {
  print $label
  return value
}
proc made(prefix: Str) [io] -> OperandFailure {
  OperandFailure.Failed(
    second: mark(f"{prefix}-second", "two"),
    first: mark(f"{prefix}-first", "one"),
  )
}
let success: Result[Str, OperandFailure] = Ok(mark("ok-value", "ok"))
let failure: Result[Str, OperandFailure] =
  Err(cause: made("cause"), made("value"))
assert success? == "ok"
match failure {
  Err(problem) => print problem.first problem.second
  _ => print "wrong-result"
}
let direct = OperandFailure.Failed(
  second: mark("direct-second", "two"),
  first: mark("direct-first", "one"),
)
assert direct.first == "one"
assert direct.second == "two"
""",
    status: 0,
  )?
  assert output.stdout == """ok-value
cause-second
cause-first
value-second
value-first
one two
direct-second
direct-first
"""
}
