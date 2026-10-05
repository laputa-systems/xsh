# The layout `xsht fmt` gives each construct. `xsht fmt` formats only a script
# that parses and checks, so every source here is a complete program.

# The text `xsht fmt` leaves in a file holding `source`, formatted at the
# `width` an `xsht-config.ini` beside it sets. A second pass must change
# nothing, which also proves that the formatted text parses and checks.
proc formatted_at(ctx: TestContext, source: Str, width: Int) [fs, process, error] -> Result[Str] {
  let root = test.temp_dir(ctx, name: "fmt-layout")?
  fp"{root}/xsht-config.ini".write(f"[format]\nline-width = {width}\n")
  let file = fp"{root}/candidate.xsh"
  file.write(source)
  let first = run.capture --text "xsht" fmt $file
  assert first.status.exited_with(0), first.stderr
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), f"{stable.stdout}{stable.stderr}"
  file.read_text()
}

# `formatted_at` the default width.
proc formatted(ctx: TestContext, source: Str) [fs, process, error] -> Result[Str] {
  formatted_at(ctx, source, 120)
}

test test_fmt_keeps_float_literals { |ctx|
  let source = "let a = 1.0\nlet b = 0.25\nlet c = 10e-3\nlet d = 1.5e6\nlet m = 1.float()\n"
  assert formatted(ctx, source)? == source
  let _ = test.expect(ctx, source + "print \${a + b} \${c * 100.0 + d + m}\n", status: 0, stdout: ["1.25 1500002"])?
}

test test_fmt_keeps_a_map_comprehension { |ctx|
  let source = """let items = [{name: "a", version: "1"}, {name: "b", version: ""}]
let by_name = {item.name: item.version for item in items if item.version != ""}
"""
  assert formatted(ctx, source)? == source
  let _ = test.expect(
    ctx,
    source + "print \${by_name.keys().join(\",\")} \${by_name[\"a\"]}\n",
    status: 0,
    stdout: ["a 1"],
  )?
}

test test_fmt_keeps_a_bracketed_computed_map_key { |ctx|
  let source = """let item = {name: "demo", version: "2"}
let by_name = {[item.name]: item.version}
"""
  assert formatted(ctx, source)? == source
  let _ = test.expect(ctx, source + "print \${by_name[\"demo\"]}\n", status: 0, stdout: ["2"])?
}

test test_fmt_keeps_type_patterns { |ctx|
  let source = r"""match json.decode("1.25")? {
  i is Int => print ${i.float()}
  f is Float => print ${f}
  _ is Null => print "null"
  _ => print "other"
}
"""
  assert formatted(ctx, source)? == source
  let _ = test.expect(ctx, source, status: 0, stdout: ["1.25"])?
}

test test_fmt_keeps_module_contract_types { |ctx|
  let source = """type Plugin = module {
  export let name: Str
  export optional let description: Str
  export proc execute(root: Path) [fs, error] -> Result[Unit, Error]
  export pure label(name: Str) -> Str
}

let plugin: Module[Plugin] = module.load(p"plugin.xsh")?.require(Plugin)?
"""
  assert formatted(ctx, source)? == source
}

test test_fmt_indents_a_grouped_run_invocation { |ctx|
  let source = r"""let make = "make"
let cc = "cc"
run (
$make
"ARCH=arm64"
f"CC={cc}"
"Image"
)?
"""
  assert formatted(ctx, source)? == r"""let make = "make"
let cc = "cc"
run (
  $make
  "ARCH=arm64"
  f"CC={cc}"
  "Image"
) ?
"""
}

test test_fmt_orders_signal_hook_effects_and_keeps_on_as_a_name { |ctx|
  let source = "on TERM --pre-cancel=50ms [error, process, fs] {\nprint \"stop\"\n}\nlet on = 1\n"
  assert formatted(ctx, source)? == "on TERM --pre-cancel=50ms [fs, process, error] {\n  print \"stop\"\n}\n\nlet on = 1\n"
}

test test_fmt_keeps_signal_hook_comments { |ctx|
  let source = "# before\non TERM [error, fs] {\n# inside\nprint \"stop\"\n}\n"
  assert formatted(ctx, source)? == "# before\non TERM [fs, error] {\n  # inside\n  print \"stop\"\n}\n"
}

test test_fmt_golden_covers_current_surface_syntax { |ctx|
  let source = r"""
proc write_note(note: Path) {
note.write("ok")?
}
var count=1
count+=2
var stats={code:0,comments:0}
stats.code+=1
var counts:Map[Int]={comments:0}
counts["comments"]=2
let label=f"count {count}"
let tool=fp"{Path("root")}/bin/tool"
let text=run.text printf "%s" ${label} ?
let raw=run.bytes printf "%s" raw ?
let files=g"src/*.rs"
run printf "%s\n" @g"src/*.rs" ?
let choice=if count>1{"many"}else{"one"}
let value=match Ok(count){Ok(n)=>n,Err(_)=>0}
p"tmp".remove(missing_ok:true)?
let slash=p"/tmp/xsh"
let multiline="alpha\nbeta"
let quoted_multiline="alpha\n\"\"\"\nbeta"
"""
  assert formatted(ctx, source)? == r"""proc write_note(note: Path) {
  note.write("ok")?
}

var count = 1
count += 2
var stats = {code: 0, comments: 0}
stats.code += 1
var counts: Map[Int] = {comments: 0}
counts["comments"] = 2
let label = f"count {count}"
let tool = fp"{Path("root")}/bin/tool"
let text = run.text printf "%s" ${label} ?
let raw = run.bytes printf "%s" raw ?
let files = g"src/*.rs"
run printf "%s\n" @g"src/*.rs" ?
let choice = if count > 1 { "many" } else { "one" }
let value = match Ok(count) { Ok(n) => n, Err(_) => 0 }
p"tmp".remove(missing_ok: true)?
let slash = /tmp/xsh
let multiline = "alpha\nbeta"
let quoted_multiline = "alpha\n\"\"\"\nbeta"
"""
}

test test_fmt_keeps_a_block_string_on_its_own_lines { |ctx|
  let source = "let multiline=\"\"\"alpha\nbeta\"\"\"\n"
  assert formatted(ctx, source)? == "let multiline = \"\"\"alpha\nbeta\"\"\"\n"
}

test test_fmt_spawn_and_wait_forms { |ctx|
  let source = r"""
let h=spawn run --cpumax=80 true ?
let s=wait h?
let hs=[spawn run true ?,spawn run false ?]
let statuses=wait hs?
let cmd=process.command {
cpu_max=80
run true
}
let h2=spawn (cmd)?
h2.cancel(signal:"TERM",kill_after:0ms)?
"""
  assert formatted(ctx, source)? == r"""let h = spawn run --cpumax=80 true ?
let s = wait h?
let hs = [spawn run true?, spawn run false?]
let statuses = wait hs?
let cmd = process.command {
  cpu_max = 80
  run true
}
let h2 = spawn cmd?
h2.cancel(signal: "TERM", kill_after: 0ms)?
"""
}

test test_fmt_retry_blocks { |ctx|
  let source = r"""proc fetch() [error] -> Result[Str] { Ok("fetched") }

let value=retry [1s,2s,0ms] {
  fetch()?
}?
let once=retry [] {
  Ok("done")
}
"""
  assert formatted(ctx, source)? == r"""proc fetch() [error] -> Result[Str] { Ok("fetched") }

let value = retry [1s, 2s, 0ms] {
  fetch()?
}?
let once = retry [] {
  Ok("done")
}
"""
}

test test_fmt_selective_retry { |ctx|
  let source = """error FetchError = Busy(message: Str) | Timeout(message: Str)

proc fetch() -> Result[Str, FetchError] { Err(FetchError.Busy(message: "busy")) }

let result=retry [0ms] on (FetchError.Busy | FetchError.Timeout) {
  fetch()?
}
"""
  assert formatted(ctx, source)? == """error FetchError = Busy(message: Str) | Timeout(message: Str)

proc fetch() -> Result[Str, FetchError] { Err(FetchError.Busy(message: "busy")) }

let result = retry [0ms] on (FetchError.Busy | FetchError.Timeout) {
  fetch()?
}
"""
}

test test_fmt_require_type_syntax { |ctx|
  let source = r"""
type Config = {name: Str, ports: List[Int], note: Str?}
let config = p"config.json"
let cfg=json.read(config)?.require(Config)?
let names=json.decode("[]")?.require(List[Str])?
"""
  assert formatted(ctx, source)? == r"""type Config = {name: Str, ports: List[Int], note: Str?}

let config = p"config.json"
let cfg = json.read(config)?.require(Config)?
let names = json.decode("[]")?.require(List[Str])?
"""
}

test test_fmt_keeps_literal_dollar_interpolation_markers { |ctx|
  let source = r"""
let name = "demo"
let plain = r"${name}"
let label = f"${{name}}:{name}"
run echo "\$name" "\${name}"
"""
  assert formatted(ctx, source)? == r"""let name = "demo"
let plain = r"${name}"
let label = f"${{name}}:{name}"
run echo "\$name" "\${name}"
"""
}

test test_fmt_keeps_a_literal_dollar_before_an_interpolation { |ctx|
  let source = r"""let i = 1
let ph = f"\${i}"
let both = f"\\\${i}"
"""
  assert formatted(ctx, source)? == source
}

test test_fmt_module_commands_destructuring_fallback_and_shorthand_interpolation { |ctx|
  let source = r"""fs.mkdir build?
fs.remove dist --missing-ok?
let pkg = {name: "demo", version: "1", release: "2"}
let {name,version,..}=pkg
let jobs=env.Str.JOBS??"1"
print $pkg.name "$pkg.name"
"""
  assert formatted(ctx, source)? == r"""fs.mkdir build ?
fs.remove dist --missing-ok ?
let pkg = {name: "demo", version: "1", release: "2"}
let {name, version, ..} = pkg
let jobs = env.Str.JOBS ?? "1"
print $pkg.name $pkg.name
"""
}

test test_fmt_uses_two_space_pipeline_continuation_indent { |ctx|
  let source = """let root = p"."
let names = fs.walk(root)
|> where .kind == "file"
|> map .name
"""
  assert formatted(ctx, source)? == """let root = p"."
let names = fs.walk(root)
  |> where .kind == "file"
  |> map .name
"""
}

test test_fmt_joins_a_short_pipeline_and_keeps_a_long_one_broken { |ctx|
  let source = r"""let root = p"."
let names = fs.walk(root)
|> where .kind == "file"
let sizes = fs.walk(root)
|> where .kind == "file"
|> map .size
print ${names |> count()} ${sizes |> count()}
"""
  assert formatted(ctx, source)? == r"""let root = p"."
let names = fs.walk(root) |> where .kind == "file"
let sizes = fs.walk(root)
  |> where .kind == "file"
  |> map .size
print ${names |> count()} ${sizes |> count()}
"""
}

test test_fmt_keeps_intentional_top_level_blank_lines { |ctx|
  let source = r"""let label = "one"

let lines = label.lines()

print ${lines |> count()}
"""
  assert formatted(ctx, source)? == source
}

test test_fmt_indents_a_block_under_a_pipeline_stage { |ctx|
  let source = """let doubled = [1, 2, 3]
|> par-map { |value|
value*2
}
"""
  assert formatted(ctx, source)? == """let doubled = [1, 2, 3]
  |> par-map { |value|
    value * 2
  }
"""
}

test test_fmt_wraps_long_constructs_at_default_width { |ctx|
  let source = r"""
let root = p"rootfs"
let work = p"work"
let tarball = p"pkg.tar"
let pkg = {name: "demo", ver: "1", rel: "2"}
let manifest = ["a"]
let checksums = ["sum"]
let source_index = 0
let service_target = "service"
let argv_prefix = ["--log"]
let proof_log = p"proof.log"
type Package = {dir: Path, exports: Record, name: Str, ver: Str, rel: Str, deps: List[Str], mkdeps: List[Str], sources: List[Path], checksums: List[Str], nostrip: Bool}
let paths = [fp"{root}/bin", fp"{root}/dev", fp"{root}/etc/rc.d", fp"{root}/proc", fp"{root}/root", fp"{root}/run", fp"{root}/sys", fp"{root}/tmp", fp"{root}/usr/lib/services"]
let metadata = {name: pkg.name, version: pkg.ver, release: pkg.rel, tarball: tarball.display(), manifest_count: manifest.len(), checksum: checksums[source_index], installed_root: root.display(), work_dir: work.display()}
let command = process.command_argv(service_target, argv_prefix.extend([proof_log.display(), "heartbeat"]), cwd: root, timeout: 5s, detach: true, new_session: true, ignore_hup: true)
proc main(rootfs: Path = Path("target/xsh-rootfs"), xsh_bin: Path = Path("target/debug/xsh"), auth_bin_dir: Path = Path("target/debug")) -> Result[Unit] {
return Ok()
}
"""
  let actual = formatted(ctx, source)?
  assert actual == r"""let root = p"rootfs"
let work = p"work"
let tarball = p"pkg.tar"
let pkg = {name: "demo", ver: "1", rel: "2"}
let manifest = ["a"]
let checksums = ["sum"]
let source_index = 0
let service_target = "service"
let argv_prefix = ["--log"]
let proof_log = p"proof.log"

type Package = {
  dir: Path,
  exports: Record,
  name: Str,
  ver: Str,
  rel: Str,
  deps: List[Str],
  mkdeps: List[Str],
  sources: List[Path],
  checksums: List[Str],
  nostrip: Bool,
}

let paths = [
  fp"{root}/bin",
  fp"{root}/dev",
  fp"{root}/etc/rc.d",
  fp"{root}/proc",
  fp"{root}/root",
  fp"{root}/run",
  fp"{root}/sys",
  fp"{root}/tmp",
  fp"{root}/usr/lib/services",
]
let metadata = {
  name: pkg.name,
  version: pkg.ver,
  release: pkg.rel,
  tarball: tarball.display(),
  manifest_count: manifest.len(),
  checksum: checksums[source_index],
  installed_root: root.display(),
  work_dir: work.display(),
}
let command = process.command_argv(
  service_target,
  argv_prefix.extend([proof_log.display(), "heartbeat"]),
  cwd: root,
  timeout: 5s,
  detach: true,
  new_session: true,
  ignore_hup: true,
)

proc main(
  rootfs: Path = Path("target/xsh-rootfs"),
  xsh_bin: Path = Path("target/debug/xsh"),
  auth_bin_dir: Path = Path("target/debug"),
) -> Result[Unit] {
  return Ok()
}
""", actual
  for line in actual.lines() {
    assert line.count_chars() <= 120, line
  }
}

test test_fmt_keeps_readable_multiline_package_shapes { |ctx|
  # A raw block string as the last statement, which the test strings above
  # cannot hold between their own delimiters.
  let script = "let script = r\"\"\"print f\"{value}\"\n\"\"\"\n"
  let source = """type MakeTask = {name: Str}
type CompileTasks = {tasks: List[MakeTask]}
export type CMultiTarget = {
  tasks: List[MakeTask],
  groups: Map[CompileTasks],
  outputs: Map[Path],
  deps: List[Str],
}
pure c_program(options: Record) -> Record { options }
let cc = "cc"
let triple = "x86_64-linux-musl"
let cflags = ["-O2"]
let defs = ["NDEBUG"]
let includes = [p"include"]
let sources = [p"tool.c"]
let path_value = p"src/tool.cxx"
let ext = ".o"
let target = c_program({
  cc,
  triple,
  cflags,
  defs,
  includes,
  root: p".",
  sources,
  out_dir: p"obj",
  out: p"obj/tool",
  libs: [],
  ldflags: [],
  deps: [],
})
let value = path_value.display().replace("/", with: "_").replace(".cxx", with: ext).replace(".cpp", with: ext).replace(".cc", with: ext).replace(".c", with: ext).replace(".S", with: ext).replace(".s", with: ext)
""" + script
  let actual = formatted(ctx, source)?
  assert actual == """type MakeTask = {name: Str}

type CompileTasks = {tasks: List[MakeTask]}

export type CMultiTarget = {
  tasks: List[MakeTask],
  groups: Map[CompileTasks],
  outputs: Map[Path],
  deps: List[Str],
}

pure c_program(options: Record) -> Record { options }

let cc = "cc"
let triple = "x86_64-linux-musl"
let cflags = ["-O2"]
let defs = ["NDEBUG"]
let includes = [p"include"]
let sources = [p"tool.c"]
let path_value = p"src/tool.cxx"
let ext = ".o"
let target = c_program({
  cc,
  triple,
  cflags,
  defs,
  includes,
  root: p".",
  sources,
  out_dir: p"obj",
  out: p"obj/tool",
  libs: [],
  ldflags: [],
  deps: [],
})
let value = path_value.display()
  .replace("/", with: "_")
  .replace(".cxx", with: ext)
  .replace(".cpp", with: ext)
  .replace(".cc", with: ext)
  .replace(".c", with: ext)
  .replace(".S", with: ext)
  .replace(".s", with: ext)
""" + script, actual
}

test test_fmt_indents_a_sole_multiline_record_argument_in_nested_contexts { |ctx|
  let source = """pure make_row() -> Result[Record] {
  return Ok({
    name: "demo",
    enabled: true,
  })
}

pure push_row(rows: List[Record]) -> List[Record] {
  return rows.push({
    name: "demo",
    enabled: true,
  })
}
"""
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

test test_fmt_expands_a_sole_long_record_argument_and_keeps_it_stable { |ctx|
  let source = """type Parsed = {repo: Str, all: Bool, roots: List[Str], target: Str, output: Str}

proc resolve_repo_root(repo: Str) [error] -> Result[Path] { Ok(Path(repo)) }

pure repo_plan(options: Record) -> Record { options }

proc example(parsed: Parsed) [error] -> Result[Record] {
  return repo_plan({repo: resolve_repo_root(parsed.repo)?, all: parsed.all, roots: parsed.roots, target: parsed.target, output: parsed.output})
}
"""
  let actual = formatted(ctx, source)?
  assert "return repo_plan({\n" in actual, actual
}

test test_fmt_keeps_an_indented_multiline_format_string_argument { |ctx|
  let source = "proc write_line(target: Path, name: Str) [fs, error] {\n  target.write_atomic(f\"\"\"hello {name}\n\"\"\")?\n}\n\nproc touch(target: Path) [fs, error] {\n  target.write_atomic(\"\")?\n}\n\nproc write_path(name: Str) [fs, error] {\n  touch(fp\"\"\"hello {name}\n\"\"\")?\n}\n"
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

test test_fmt_keeps_a_sole_multiline_literal_argument_compact { |ctx|
  let source = "let tool = p\"tool.txt\"\ntool.write(\"\"\"demo\n\"\"\")?\n"
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

test test_fmt_canonicalizes_proc_effect_order { |ctx|
  let actual = formatted(ctx, "proc main() [io, error, fs, env, process, net, time] {\n  return Ok()\n}\n")?
  assert actual == "proc main() [fs, net, process, env, time, error, io] {\n  return Ok()\n}\n", actual
}

test test_fmt_skips_the_statement_after_a_fmt_skip_comment { |ctx|
  let source = "let before = 1\n# fmt: skip\nlet value=1+2\nlet after = 3\n"
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

test test_fmt_keeps_a_trailing_comment_on_a_skipped_statement { |ctx|
  let actual = formatted(ctx, "# fmt: skip\nlet value=1+2 # keep with the skipped statement\n\nlet after=3\n")?
  assert actual == "# fmt: skip\nlet value=1+2 # keep with the skipped statement\n\nlet after = 3\n", actual
}

test test_fmt_keeps_an_authored_block_gap_after_a_skipped_trailing_comment { |ctx|
  let actual = formatted(
    ctx,
    "proc main() {\n  # fmt: skip\n  let value=1+2 # keep with the skipped statement\n\n  let after=3\n}\n",
  )?
  assert actual == "proc main() {\n  # fmt: skip\n  let value=1+2 # keep with the skipped statement\n\n  let after = 3\n}\n", actual
}

test test_fmt_wraps_long_if_and_match_expressions_in_safe_contexts { |ctx|
  let source = """pure render(text: Str) -> Str { text }

let user_name = "root"
let mode = "test"
let result: Result[Str] = Ok("value")
let choice = if user_name == "administrator" and mode == "production" { "allow" } else { "deny" }
let label = render(match result { Ok(value) => value, Err(_) => "fallback" })
"""
  let actual = formatted_at(ctx, source, 60)?
  assert actual == """pure render(text: Str) -> Str { text }

let user_name = "root"
let mode = "test"
let result: Result[Str] = Ok("value")
let choice = if user_name == "administrator" and mode == "production" {
  "allow"
} else {
  "deny"
}
let label = render(
  match result {
    Ok(value) => value,
    Err(_) => "fallback",
  },
)
""", actual
}

test test_fmt_keeps_a_multiline_match_expression { |ctx|
  let source = "let loaded: Result[Str] = Ok(\"#!/bin/sh\\n\")\nlet shebang = match loaded {\n  Ok(text_value) => (text_value.split(\"\\n\").get(0) ?? \"\")\n  Err(_) => \"\"\n}\n"
  let actual = formatted(ctx, source)?
  assert actual == "let loaded: Result[Str] = Ok(\"#!/bin/sh\\n\")\nlet shebang = match loaded {\n  Ok(text_value) => text_value.split(\"\\n\").get(0) ?? \"\",\n  Err(_) => \"\",\n}\n", actual
}

test test_fmt_keeps_a_multiline_call_argument_list { |ctx|
  let source = """pure make_command(target: Str, argv: List[Str], cwd: Path) -> Str { target }

let target = "build"
let root = p"."
let command = make_command(
  target,
  args,
  cwd: root,
)
"""
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

test test_fmt_indents_broken_call_arguments_in_nested_blocks { |ctx|
  let source = "pure make(target: Str, options: Record) -> Str { target }\n\nproc build(target: Str) {\nlet value=make(\ntarget,\n{\nalpha:1,beta:2,gamma:3,delta:4,epsilon:5,zeta:6},\n)\n}\n"
  let actual = formatted(ctx, source)?
  assert actual == "pure make(target: Str, options: Record) -> Str { target }\n\nproc build(target: Str) {\n  let value = make(\n    target,\n    {\n      alpha: 1,\n      beta: 2,\n      gamma: 3,\n      delta: 4,\n      epsilon: 5,\n      zeta: 6,\n    },\n  )\n}\n", actual
}

test test_fmt_breaks_multiline_comprehensions_before_each_clause { |ctx|
  let source = """let pkg = {upstream_sources: [{source: p"a.tar", kind: "tar", architectures: ["x"], checksums: ["sum"]}]}
let items = [{name: "a", version: "1"}]
let upstream_sources = [{
  source: source.source.display(),
  kind: source.kind,
  architectures: source.architectures,
  checksums: source.checksums,
} for source in pkg.upstream_sources]
let by_name = {
  item.name: item.version
  for item in items
}
"""
  let actual = formatted(ctx, source)?
  assert actual == """let pkg = {upstream_sources: [{source: p"a.tar", kind: "tar", architectures: ["x"], checksums: ["sum"]}]}
let items = [{name: "a", version: "1"}]
let upstream_sources = [
  {
    source: source.source.display(),
    kind: source.kind,
    architectures: source.architectures,
    checksums: source.checksums,
  }
  for source in pkg.upstream_sources
]
let by_name = {
  item.name: item.version
  for item in items
}
""", actual
}

test test_fmt_breaks_a_long_call_chain_between_calls { |ctx|
  let source = "let common: List[Record] = []\nlet files = common.push({path: p\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\", kind: \"binary\"}).push({path: p\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\", kind: \"binary\"}).push({path: p\"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\", kind: \"binary\"})\n"
  let actual = formatted(ctx, source)?
  assert "\n  .push" in actual, actual
  assert "common.push(" in actual, actual
  # The broken chain is still one statement: every call lands in `files`.
  let _ = test.expect(ctx, actual + "print \${files.len()}\n", status: 0, stdout: ["3"])?
}

test test_fmt_keeps_run_capture_records { |ctx|
  let source = "let text = run.capture --text printf \"%s\" hi\nlet raw = run.capture --bytes printf \"%s\" hi\n"
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

test test_fmt_fixture_covers_comments_commands_blocks_and_records { |ctx|
  let actual = formatted(ctx, p"tests/fixtures/syntax/valid/formatting.xsh".read_text()?)?
  assert actual == r"""# formatter fixture
use fs

proc main(args: List[Str]) {
  # nested comment
  let config = {name: "demo", enabled: true}
  let values = ["one", "two"]

  if true {
    run echo "hello "${args[0]} ?
  } else {
    eprint "no"
  }
}

main(args)?
""", actual
}

test test_fmt_keeps_a_trailing_statement_comment { |ctx|
  let actual = formatted(ctx, "let value=1 # keep this with the binding\nlet after=3\n")?
  assert actual == "let value = 1 # keep this with the binding\nlet after = 3\n", actual
}

test test_fmt_keeps_a_nested_block_comment_once { |ctx|
  let source = """proc main() {
  let before=1
  if true {
    # explain command shape
    run echo "ok" ?
  }
  let after=2
}
print "done"
"""
  let actual = formatted(ctx, source)?
  assert actual == """proc main() {
  let before = 1
  if true {
    # explain command shape
    run echo "ok" ?
  }

  let after = 2
}

print "done"
""", actual
  assert actual.split("# explain command shape").len() == 2, actual
}

# Each snippet formats to text that parses, checks, and formats to itself.
test test_fmt_round_trips_baseline_snippets { |ctx|
  let root = test.temp_dir(ctx, name: "snippets")?
  fp"{root}/types.xsh".write("##! Shared types.\n\n## A package.\nexport type Package = {name: Str, root: Path}\n")
  let snippets = [
    "let value = 1 + 2 * 3\n",
    "let tmp_path = fp\"{Path(\"tmp\")}/space name\"\n",
    "let raw_bytes = b\"a\\xff\\n\"\n",
    "run printf \"%s\\n\" \"hello world\" ?\n",
    "run make -j\${cpu.count()} ?\n",
    "proc main(args: List[Str]) -> Result[Unit] {\n  return Ok()\n}\n\nmain(args)?\n",
    "pure trim_one(value: Str) -> Str {\n  return value.trim()\n}\n",
    "if true {\n  print \"yes\"\n} else {\n  eprint \"no\"\n}\n",
    "while false {\n  break\n}\n",
    "for value in [\"a\", \"b\"] {\n  print \${value}\n}\n",
    "match Ok(\"x\") {\n  Ok(value) => print \${value}\n  _ => print \"other\"\n}\n",
    "type Package = {name: Str, root: Path}\nlet pkg: Package = {name: \"demo\", root: Path(\"src\")}\n",
    "use types as t\nlet pkg: t.Package = {name: \"demo\", root: Path(\"src\")}\n",
    "let record = {name: \"demo\", enabled: true}\n",
    "let files = fs.walk(\"src\")\n|> where {\n  .kind == \"file\"\n}\n\n",
    "let out = [1, 2] |> par-map { |x|\n  x * 2\n}\n",
  ]
  for snippet in snippets {
    let file = fp"{root}/snippet.xsh"
    file.write(snippet)
    let first = run.capture --text "xsht" fmt $file
    assert first.status.exited_with(0), f"{snippet}: {first.stderr}"
    let stable = run.capture --text "xsht" fmt --check $file
    assert stable.status.exited_with(0), f"{snippet}: {stable.stdout}{stable.stderr}"
  }
}

test test_fmt_pretty_corpus_has_a_stable_golden_shape { |ctx|
  let actual = formatted_at(ctx, p"tests/fixtures/syntax/valid/pretty.xsh".read_text()?, 60)?
  assert actual == """# curated formatter corpus
let source = p"."
let items = [
  {name: "one", enabled: true},
  {name: "two", enabled: false},
]
let source_shaped = [
  1,
  2,
]
let rows = [
  {
    name: "short",
  },
  {
    name: "a deliberately long record value that forces its sibling to break too",
  },
]
let nested = [
  {
    meta: {
      name: "short",
    },
  },
  {
    meta: {
      name: "another deliberately long nested record value",
    },
  },
]
let filtered = [item.name for item in items if item.enabled]
let by_name = {
  item.name: f"{item.name}"
  for item in items
  if item.enabled
}
let chain = source.display()
  .replace("/", "_")
  .replace("-", "_")
# fmt: skip
let skipped=1+2
""", actual
}

test test_fmt_keeps_string_concatenation { |ctx|
  let source = "let b = \"b\"\nlet x = \"a\" + b + \"c\"\n"
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

test test_fmt_keeps_a_path_method_receiver_quoted { |ctx|
  let source = """proc patch_path() [fs, error] -> Result[Path] {
  if p"../pkg/files/x86-jump-label-patch.c".exists()? {
    return p"../pkg/files/x86-jump-label-patch.c"
  }
  return p"../pkg/files/default.c"
}
"""
  let actual = formatted(ctx, source)?
  assert actual == """proc patch_path() [fs, error] -> Result[Path] {
  if p"../pkg/files/x86-jump-label-patch.c".exists()? {
    return ../pkg/files/x86-jump-label-patch.c
  }

  return ../pkg/files/default.c
}
""", actual
}

test test_fmt_round_trips_value_branch_blocks_and_record_arms { |ctx|
  let source = "let choice = if true { let detail = 2; detail } else { 3 }\nlet record = match choice { 2 => {}, _ => {\"run\": 4} }\nlet text = match choice { 2 => { let detail = \"é\"; detail }, _ => \"other\" }\n"
  let actual = formatted(ctx, source)?
  assert "{ {" not in actual, actual
  assert "let detail = 2" in actual, actual
  assert "\"run\": 4" in actual, actual
}

test test_fmt_keeps_bare_block_and_literal_distinctions { |ctx|
  let source = "let value = 7\nlet shorthand = {value}\nlet empty = {}\nlet named = {if: 1}\nlet computed = {[\"key\"]: 2}\nlet grouped = { (value) }\nlet result = { let next = 8; next }\nlet negative = { false }\nlet accessed = { shorthand.value }\n"
  let actual = formatted(ctx, source)?
  assert "(value)" in actual, actual
  for malformed in ["let bad = {field:}\n", "let bad = {[\"key\"]:}\n"] {
    let file = test.temp_file(ctx, name: "malformed.xsh", contents: bytes.from_text(malformed))?
    let checked = run.capture --text "xsht" check $file
    assert checked.status.exited_with(2), checked.stderr
    assert "err[parse." in checked.stderr, checked.stderr
  }
}

test test_fmt_keeps_half_open_slicing_bounds_and_unicode { |ctx|
  let source = r"""let data = b"abcd"
let all = data[..]
let prefix = data[..2]
let suffix = data[2..]
let middle = data[-3..-1]
let unicode = "é🦀"[..1][..]
print ${all.base64()} ${prefix.base64()} ${suffix.base64()} ${middle.base64()} $unicode
"""
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

test test_fmt_keeps_a_guard_out_of_run_argv { |ctx|
  let source = r"""proc cached(value: Str?) [] -> Str {
  return value when value != null
  return "missing"
}
proc command(selected: Bool) [process] -> Status {
  return (run.status /usr/bin/true) unless !selected
  return run.status /usr/bin/true when unless
}
stream items() [] -> Stream[Int] {
  yield 1 when true
  yield 2 unless false
}
let value = loop {
  break 4 when false
  break 5 unless false
}
"""
  let actual = formatted(ctx, source)?
  assert "return (run.status /usr/bin/true) unless ! selected" in actual, actual
  assert "return run.status /usr/bin/true when unless" in actual, actual
}

test test_fmt_keeps_propagation_adjacent_to_a_grouped_run_payload_before_a_guard { |ctx|
  let source = r"""proc capture(selected: Bool) [process, error] -> Str {
  return (run.text /usr/bin/printf "selected")? when selected
  return "fallback"
}
"""
  let actual = formatted(ctx, source)?
  assert "(run.text /usr/bin/printf \"selected\")? when selected" in actual, actual
}

test test_fmt_breaks_multi_clause_comprehensions_one_clause_per_line { |ctx|
  let source = "let entries = [{key: \"k\", ok: true, values: [1]}]\nlet values = [inner for outer in [1] if outer > 0 for inner in [outer] if inner < 2]\nlet by_key = {entry.key: inner for entry in entries if entry.ok for inner in entry.values}\n"
  let actual = formatted(ctx, source)?
  assert "\n  for outer in [1]\n  if outer > 0\n  for inner in [outer]\n  if inner < 2\n" in actual, actual
  assert "\n  for entry in entries\n  if entry.ok\n  for inner in entry.values\n" in actual, actual
}

test test_fmt_keeps_unicode_comments_between_comprehension_clauses { |ctx|
  let source = "let values = [\n  # sélection\n  inner\n  for outer in [1]\n  # répétition\n  for inner in [outer]\n  # filtre\n  if inner > 0\n]\n"
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

test test_fmt_keeps_a_deferred_block_as_a_statement_body { |ctx|
  let source = r"""proc cleanup() [error] {
  defer {
    # café remains inside cleanup
    let message = "done"
    print $message
    assert true
  }
}

cleanup()
"""
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

# `?? { |failure| ... }` is a handler block and `?? {name: ...}` is a record
# fallback; formatting keeps both, and each still yields its own kind of value.
test test_fmt_keeps_error_fallback_blocks_apart_from_record_fallbacks { |ctx|
  let source = "let recovered = Ok(false) ?? { |failure|\n  let _ = failure\n  false\n}\nlet record = Ok({name: \"original\"}) ?? {name: \"record\"}\n"
  let actual = formatted(ctx, source)?
  assert actual == source, actual
  let failing = """error Demo = Failed(message: Str)

type Named = {name: Str}

let failed: Result[Bool, Demo] = Err(Demo.Failed(message: "demo"))
let recovered = failed ?? { |failure|
  let _ = failure
  false
}
let missing: Result[Named, Demo] = Err(Demo.Failed(message: "demo"))
let record = missing ?? {name: "record"}
"""
  let stable = formatted(ctx, failing)?
  assert stable == failing, stable
  let _ = test.expect(ctx, failing + "print \$recovered \$record.name\n", status: 0, stdout: ["false record"])?
}

test test_fmt_keeps_keyword_and_dotted_field_labels { |ctx|
  let source = "type Entry = {type: Str, in: Int}\nlet value = Entry(type: \"file\", in: 2)\nlet selected = match true { _ => {type: value.type, in: value.in, r\"wire.type\": 3} }\nlet {type: kind, in: ordinal, ..} = value\n"
  let actual = formatted(ctx, source)?
  assert "{type: value.type, in: value.in, \"wire.type\": 3}" in actual, actual
}

# `try { ... }` captures its body's result, while `try` stays an ordinary
# field label and command word.
test test_fmt_keeps_a_try_capture_body_and_result_tail { |ctx|
  let source = "let value = try {\n  let nested = Ok(7)\n  nested\n}\nlet empty = try {}\nlet fields = {try: 7}\nrun printf try\n"
  let actual = formatted(ctx, source)?
  assert "let value = try {\n  let nested = Ok(7)\n  nested\n}\n" in actual, actual
  assert "let fields = {try: 7}\nrun printf try\n" in actual, actual
}

test test_fmt_keeps_named_stream_configuration_and_spreads { |ctx|
  let source = "let jobs = 2\nlet values = [1, 2] |> par-map(jobs:) { |item| item + 1 } |> sort(...{desc: true})\nlet batches = values |> batch(count: 2, max_argv: false)\nprint batches.len()\n"
  let actual = formatted(ctx, source)?
  assert "par-map(jobs:)" in actual, actual
  assert "sort(...{desc: true})" in actual, actual
}

test test_fmt_keeps_bare_bytes_stdin_as_one_operand { |ctx|
  let source = "let copied = run.bytes cat < b\"a\\0\\xff\" ?\n"
  let actual = formatted(ctx, source)?
  assert actual == source, actual
}

test test_fmt_keeps_wire_enum_constant_expressions { |ctx|
  let source = "const prefix = \"rea\"\nenum State: Str {\n  # External spelling stays stable.\n  Ready = prefix + \"dy\",\n  Empty = \"\",\n}\n"
  let actual = formatted(ctx, source)?
  for fragment in ["enum State: Str", "# External spelling stays stable.", "prefix + \"dy\"", "Empty = \"\""] {
    assert fragment in actual, actual
  }
}

# `cli main` is an entry declaration, while `cli.parse` stays an ordinary
# module call.
test test_fmt_keeps_a_signature_cli_entry_declaration { |ctx|
  let actual = formatted(ctx, "cli main(root: Path, jobs: Int = 4) [fs, error] { print \$root \$jobs }\n")?
  assert actual.starts_with("cli main("), actual
  let ordinary = test.temp_file(
    ctx,
    name: "cli-call.xsh",
    contents: bytes.from_text("let parsed = cli.parse(args, {})?\n"),
  )?
  let checked = run.capture --text "xsht" check $ordinary
  assert "[parse." not in checked.stderr, checked.stderr
}

test test_fmt_keeps_accept_policy_expressions { |ctx|
  let source = "pure policy() -> List[Int] { [0,1] }\nlet codes = [0,1]\nrun.status --accept=(codes) sh --accept=[9]\nrun.status --accept=policy() --timeout=1s sh\nlet child = spawn run --timeout=1s --accept=[0,1] sh ?\nlet command = process.command {\naccept=[0,1]\nrun sh\n}\n"
  let actual = formatted(ctx, source)?
  assert "--accept=codes sh --accept=[9]" in actual, actual
}

# A parameter that takes its type from its default is printed without one.
test test_fmt_does_not_synthesize_a_type_for_a_defaulted_parameter { |ctx|
  let source = "const config = {jobs: 4}\npure choose(jobs = config.jobs + 1, label = \"café\") -> Int { jobs }\n"
  let actual = formatted(ctx, source)?
  assert "jobs = config.jobs + 1" in actual, actual
  assert "Unknown" not in actual, actual
}
