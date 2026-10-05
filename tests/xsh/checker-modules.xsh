# What `xsht check` reported for one program.
type Checked = {status: Status, stderr: Str}

# Runs only the checker over `source`: these programs call host APIs.
proc check(ctx: TestContext, source: Str) [fs, process, error] -> Result[Checked] {
  let file = test.temp_file(ctx, name: "checked.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  {status: checked.status, stderr: checked.stderr}
}

# Checks the entry `main` beside one module file, `module_file`, holding
# `module_source`.
proc check_with_module(
  ctx: TestContext,
  main: Str,
  module_file: Str,
  module_source: Str,
) [fs, process, error] -> Result[Checked] {
  let root = test.temp_dir(ctx, name: "modules")?
  let file = fp"{root}/{module_file}"
  file.parent().mkdir()
  file.write(module_source)
  fp"{root}/main.xsh".write(main)
  let checked = run.capture --text "xsht" check fp"{root}/main.xsh"
  {status: checked.status, stderr: checked.stderr}
}

# Requires `checked` to be a rejection carrying every diagnostic in `codes`.
proc expect_codes(checked: Checked, codes: List[Str]) [error] {
  assert checked.status.exited_with(2), checked.stderr
  for code in codes {
    assert f"[{code}]" in checked.stderr, f"expected {code}: {checked.stderr}"
  }
}

# Requires `checked` to carry none of `codes`; other diagnostics may still
# reject the program.
proc expect_free_of(checked: Checked, codes: List[Str]) [error] {
  for code in codes {
    assert f"[{code}]" not in checked.stderr, f"unexpected {code}: {checked.stderr}"
  }
}

test test_checker_reports_shadowing_as_the_cause_of_module_like_method_calls { |ctx|
  let checked = check(
    ctx,
    r"""
let path = Path.parse_bytes(b"/tmp/input")?
let text = path.read_text()?
""",
  )?
  expect_codes(checked, ["check.standard-module-shadow"])
  expect_free_of(checked, ["check.unknown-module-api"])
}

test test_checker_accepts_error_bindings_and_preserves_error_fail_effect { |ctx|
  let bindings = check(ctx, "\nproc keep_error() { let error = \"missing value\" }\n")?
  expect_free_of(bindings, ["check.standard-module-shadow"])

  let call = check(
    ctx,
    r"""
proc validate() [error] -> Result[Unit] {
  return error.fail("missing value")?
}
""",
  )?
  expect_free_of(call, ["check.standard-module-shadow", "check.effect-violation", "check.type-mismatch"])
}

test test_checker_rejects_standard_module_shadowing { |ctx|
  for source in [
    "let json = 1\n",
    "let archive = 1\n",
    "let args = [\"one\"]\n",
    "var process = 1\n",
    "let time = 1\n",
    "let system = 1\n",
    "let user = 1\n",
    "let group = 1\n",
    "let linux = 1\n",
    "let map = 1\n",
    "let module = 1\n",
    "type fs = Str\n",
    "proc bytes() -> Result[Unit] { return Ok() }\n",
    "pure hash() -> Int { return 1 }\n",
    "proc bad(env: Str) -> Result[Unit] { return Ok() }\n",
    r"""for cpu in [1] { print ${cpu} }
""",
    r"""match Ok(1) { Ok(path) => print ${path} }
""",
  ] {
    let checked = check(ctx, source)?
    assert checked.status.exited_with(2), f"{source}: {checked.stderr}"
    assert "[check.standard-module-shadow]" in checked.stderr, f"{source}: {checked.stderr}"
  }
}

test test_checker_rejects_standard_module_aliases { |ctx|
  expect_codes(check(ctx, "use json as files\n")?, ["check.standard-module-alias"])
}

test test_checker_requires_alias_for_hyphenated_module_name { |ctx|
  let pkg = "##! Package module.\n## The package.\nexport let pkg = {name: \"x\"}\n"
  let missing_alias = check_with_module(ctx, "use PKGBUILD-x86_64\n", "PKGBUILD-x86_64.xsh", pkg)?
  expect_codes(missing_alias, ["check.hyphenated-module-alias"])

  let with_alias = check_with_module(ctx, "use PKGBUILD-x86_64 as PKGBUILD_x86_64\n", "PKGBUILD-x86_64.xsh", pkg)?
  assert with_alias.status.exited_with(0), with_alias.stderr

  let nested_hyphen_alias = check_with_module(
    ctx,
    "use build-essential-native.proof as build_proof\n",
    "build-essential-native/proof.xsh",
    "##! Proof module.\n## Whether the proof holds.\nexport let verified = true\n",
  )?
  assert nested_hyphen_alias.status.exited_with(0), nested_hyphen_alias.stderr
}

test test_checker_enforces_module_export_boundaries { |ctx|
  let ok = check_with_module(
    ctx,
    "use helper\nhelper.build(\"demo\")?\n",
    "helper.xsh",
    r"""
export proc build(name: Str) -> Result[Unit, Error] {
  print ${name}
  return Ok()
}

let secret = "local"
""",
  )?
  expect_free_of(ok, ["check.unresolved-proc-command", "check.module-top-level"])

  let leaked = check_with_module(
    ctx,
    r"""use helper
print ${secret}
""",
    "helper.xsh",
    r"""
let secret = "local"
export let pkg = {name: "demo"}
""",
  )?
  expect_codes(leaked, ["check.unresolved-name"])
  expect_free_of(leaked, ["check.module-top-level"])

  let mutation = check_with_module(
    ctx,
    "use helper\n",
    "helper.xsh",
    r"""
var count = 0
export let pkg = {name: "demo"}
""",
  )?
  expect_codes(mutation, ["check.module-top-level"])

  let typed_alias = check_with_module(
    ctx,
    r"""
use helper as h
let pkg: h.Package = {name: "demo", root: Path("src")}
let label: Str = h.label(pkg)
""",
    "helper.xsh",
    r"""
export type Package = {name: Str, root: Path}

export pure label(pkg: Package) -> Str {
  return pkg.name
}
""",
  )?
  expect_free_of(typed_alias, ["check.unknown-type", "check.type-mismatch"])

  let nested_typed_alias = check_with_module(
    ctx,
    r"""
use helper as h
let pkg: h.Package = {leaf: {name: "demo"}}
""",
    "helper.xsh",
    r"""
export type Leaf = {name: Str}
export type Package = {leaf: Leaf}
""",
  )?
  expect_free_of(nested_typed_alias, ["check.unknown-type", "check.type-mismatch"])

  let package_type = r"""
export type Package = {name: Str, root: Path}
"""
  let typed_default_namespace = check_with_module(
    ctx,
    r"""
use helper
let pkg: helper.Package = {name: "demo", root: Path("src")}
""",
    "helper.xsh",
    package_type,
  )?
  expect_free_of(typed_default_namespace, ["check.unknown-type", "check.type-mismatch"])

  let bare_type = check_with_module(
    ctx,
    r"""
use helper
let pkg: Package = {name: "demo", root: Path("src")}
""",
    "helper.xsh",
    package_type,
  )?
  expect_codes(bare_type, ["check.unknown-type"])

  let private_type = check_with_module(
    ctx,
    r"""
use helper as h
let secret: h.Secret = {name: "demo"}
""",
    "helper.xsh",
    r"""
type Secret = {name: Str}
export type Package = {name: Str}
""",
  )?
  expect_codes(private_type, ["check.unknown-type"])
}

test test_checker_requires_retained_docs_for_public_exports { |ctx|
  let undocumented = check_with_module(ctx, "use plugin\n", "plugin.xsh", "\nexport let value: Int = 1\n")?
  expect_codes(undocumented, ["check.missing-module-doc", "check.missing-public-doc"])

  let documented = check_with_module(
    ctx,
    "use plugin\n",
    "plugin.xsh",
    r"""
##! Test module documentation.

## Exposes a documented value.
export let value: Int = 1
""",
  )?
  assert documented.status.exited_with(0), documented.stderr
}

test test_checker_rejects_orphaned_and_duplicate_module_docs { |ctx|
  let checked = check_with_module(
    ctx,
    "use plugin\n",
    "plugin.xsh",
    r"""
##! First module documentation.
# ordinary commentary separates module doc blocks
##! Duplicate module documentation.

## Orphaned documentation.
let value = 1

## Exposes a documented value.
export let exported: Int = value
""",
  )?
  expect_codes(checked, ["check.duplicate-module-doc", "check.orphan-doc-comment"])
}

test test_checker_export_doc_comment_attaches_across_ordinary_comment_lines { |ctx|
  let accepted = check_with_module(
    ctx,
    "use plugin\n",
    "plugin.xsh",
    r"""
##! Module documentation.

## Exposes a documented value.
# An implementation note between the doc and its declaration.
export let exported: Int = 1
""",
  )?
  assert accepted.status.exited_with(0), accepted.stderr

  let separated = check_with_module(
    ctx,
    "use plugin\n",
    "plugin.xsh",
    r"""
##! Module documentation.

## Exposes a documented value.

export let exported: Int = 1
""",
  )?
  expect_codes(separated, ["check.orphan-doc-comment"])
}

test test_checker_rejects_removed_verbose_apis { |ctx|
  for case in [
    {
      source: "use env\nlet home = env.get_path(\"HOME\") ?\n",
      code: "check.unsupported-api",
    },
    {
      source: "use path\nlet display = path.display(Path(\"src\"))\n",
      code: "check.unsupported-api",
    },
    {
      source: "use hash\nlet digest = hash.sha256_file(Path(\"archive.tar\"))?\n",
      code: "check.unknown-module-api",
    },
    {
      source: "use fs\nlet _written = fs.write_text(Path(\"out\"), \"text\") ?\n",
      code: "check.unknown-module-api",
    },
    {
      source: "let p = Path(\"out\")\nlet _written = p.write_text(\"text\") ?\n",
      code: "check.unknown-method",
    },
    {
      source: "let _ = [1].get_or(4, 0)\n",
      code: "check.unknown-method",
    },
    {
      source: "let m: Map[Int] = map.empty()\nlet _ = m.get_or(\"missing\", 0)\n",
      code: "check.unknown-method",
    },
    {
      source: "let _ = Path(\"Cargo.toml\").read()?\n",
      code: "check.unknown-method",
    },
    {
      source: "let _ = module.require({}, {name: \"Str\"})?\n",
      code: "check.unknown-module-api",
    },
    {
      source: "let _ = regex.matches(\"WARN\", \"WARN\")?\n",
      code: "check.unknown-module-api",
    },
    {
      source: "let _ = time.format(0, \"%Y\", utc: true)?\n",
      code: "check.unknown-module-api",
    },
    {
      source: "let _ = [1] |> collect(1)\n",
      code: "check.arity",
    },
    {
      source: "let _ = [1] |> collect(jobs: 1)\n",
      code: "check.arity",
    },
    {
      source: "let _ = [1] |> collect { . }\n",
      code: "check.arity",
    },
  ] {
    let checked = check(ctx, case.source)?
    assert checked.status.exited_with(2), f"{case.source}: {checked.stderr}"
    assert f"[{case.code}]" in checked.stderr, f"expected {case.code} for {case.source}: {checked.stderr}"
  }
}

test test_checker_rejects_linux_names_moved_to_unix { |ctx|
  for source in [
    "let _ = linux.require_pid1()?\n",
    "let _ = linux.wait_pid1_event()?\n",
    "let _ = linux.reap_child_events()?\n",
    "let command = process.command_argv(\"true\", [\"true\"])\nlet _ = linux.spawn_process_group(command)?\n",
    "let command = process.command_argv(\"true\", [\"true\"])\nlet _ = linux.spawn_with_tty(command, tty: \"tty1\")?\n",
    "let _ = linux.kill_process_group(1, \"TERM\")?\n",
    "let command = process.command_argv(\"true\", [\"true\"])\nlet _ = linux.exec(command)?\n",
    "let _ = linux.set_hostname(\"xsh\")?\n",
    "let _ = linux.uptime_seconds()?\n",
  ] {
    let checked = check(ctx, source)?
    assert checked.status.exited_with(2), f"{source}: {checked.stderr}"
    assert "[check.unknown-module-api]" in checked.stderr, f"{source}: {checked.stderr}"
  }
}

test test_checker_rejects_fs_path_contract_errors { |ctx|
  for case in [
    {
      source: "use fs\nlet listing = fs.children(1) ?\n",
      code: "check.type-mismatch",
    },
    {
      source: "use fs\nlet _written = fs.write(Path(\"out\"), 1) ?\n",
      code: "check.type-mismatch",
    },
    {
      source: "let p = Path(\"file\")\nlet renamed = p.with_ext(Path(\"log\"))\n",
      code: "check.type-mismatch",
    },
    {
      source: "pure bad(p: Path) -> Result[Path] { return p.resolve()? }\n",
      code: "check.pure-effect",
    },
    {
      source: "use fs\nlet _removed = fs.remove_manifest(Path(\"root\"), [1]) ?\n",
      code: "check.type-mismatch",
    },
    {
      source: "use fs\nlet _lock = fs.lock(Path(\"pm.lock\"), shared: 1) ?\n",
      code: "check.type-mismatch",
    },
  ] {
    let checked = check(ctx, case.source)?
    assert checked.status.exited_with(2), f"{case.source}: {checked.stderr}"
    assert f"[{case.code}]" in checked.stderr, f"expected {case.code} for {case.source}: {checked.stderr}"
  }
}

test test_checker_handles_hash_module_apis { |ctx|
  let checked = check(
    ctx,
    r"""
let digest = hash.sha256(b"abc")
let hex = digest.hex()
let encoded = digest.base64()
let file_digest = hash.sha256(Path("archive.tar"))?
hash.verify_file(Path("archive.tar"), sha256: hex)?
let parsed = hash.parse_check_line(f"{hex}  archive.tar")?
""",
  )?
  expect_free_of(
    checked,
    ["check.unknown-module-api", "check.unknown-method", "check.type-mismatch", "check.named-arg"],
  )
}

test test_checker_handles_bytes_module_apis { |ctx|
  let checked = check(
    ctx,
    r"""
let decoded = b"ok".utf8()?
let roundtrip = b"ok".base64().base64_decode()?
let b32_roundtrip = b"ok".base32().base32_decode()?
let size: Int = b"abcdef".len()
let part: Bytes = b"abcdef".slice(offset: 2, length: 3)
let dump: Str = b"abcdef".dump(format: "canonical")
let markers: List[Str] = b"\0abcde\0".strings(min_len: 4)
let copied = bytes.copy(Path("source.bin"), Path("dest.bin"), block_size: 2, count: 1, skip: 0, seek: 0, overwrite: true)?
let copied_bytes: Int = copied.bytes
let copied_blocks: Int = copied.blocks
let copied_file = bytes.copy_file(Path("source.bin"), Path("dest.bin"), source_offset: 1, dest_offset: 4, length: 2, create: true, truncate: false)?
let copied_file_bytes: Int = copied_file.bytes
let copied_file_blocks: Int = copied_file.blocks
let comparison = b"abc".compare(b"abd")
let equal: Bool = comparison.equal
let offset: Int = comparison.byte
let line: Int = comparison.line
let left: Int = comparison.left
let right: Int = comparison.right
""",
  )?
  expect_free_of(checked, ["check.unknown-module-api", "check.type-mismatch", "check.field-access"])

  for case in [
    {source: "let decoded = b\"b2s=\".base64_decode()?\n", code: "check.unknown-method"},
    {source: "let decoded = b\"N5XA====\".base32_decode()?\n", code: "check.unknown-method"},
    {source: "let decoded = \"ok\".utf8()?\n", code: "check.unknown-method"},
    {source: "let comparison = b\"a\".compare(\"b\")\n", code: "check.type-mismatch"},
  ] {
    expect_codes(check(ctx, case.source)?, [case.code])
  }
}

test test_checker_handles_args_and_regex_modules { |ctx|
  let checked = check(
    ctx,
    r"""
type Options = {root: Path, jobs: Int, define: List[Str], verbose: Bool}
let opts: Options = cli.parse(args, {
  root: {kind: "Path", default: Path("dest")},
  jobs: {kind: "Int", default: 1},
  define: {kind: "Str", repeated: true},
  verbose: {kind: "Bool", default: false},
})?
type Cli = {command: Str, action: Str, root: Path, raw: List[Str]}
let cli: Cli = cli.commands(
  ["audit", "root", "tail"],
  rootless_default: "smoke",
  commands: {smoke: {positionals: ["root"], types: {root: "Path"}, rest: "raw"}},
  fallback_command: {positionals: ["action", "root"], types: {root: "Path"}, rest: "raw", command_like: true},
)?
let cli_command: Str = cli.command
let start_re: Regex = regex.compile("^WARN")?
let ok: Bool = start_re.matches("WARN build")
let find_re: Regex = regex.compile("WARN|ERR")?
let matches = find_re.find("WARN build")
let first: Str = matches[0].text
let capture_re: Regex = regex.compile("^([^=]+)=(.*)$")?
let captures: List[Str] = capture_re.captures("key=value")
let rewrite_re: Regex = regex.compile("\\s+")?
let rewritten: Str = rewrite_re.replace("a  b", with: " ")
let compiled: Regex = regex.compile("WARN|ERR")?
let compiled_pattern: Str = compiled.pattern
let compiled_ok: Bool = compiled.matches("WARN build")
let compiled_matches = compiled.find("WARN build")
let compiled_first: Str = compiled_matches[0].text
let compiled_pair: Regex = regex.compile("^([^=]+)=(.*)$")?
let compiled_captures: List[Str] = compiled_pair.captures("key=value")
let compiled_space: Regex = regex.compile("\\s+")?
let compiled_rewritten: Str = compiled_space.replace("a  b", with: " ")
""",
  )?
  expect_free_of(
    checked,
    ["check.unknown-module-api", "check.unknown-method", "check.type-mismatch", "check.named-arg"],
  )
}

test test_checker_handles_path_literals_methods_and_expr_env_blocks { |ctx|
  let checked = check(
    ctx,
    r"""
let root = p"src"
let child = fp"{root}/main.c"
let child_suffix = "include/main.h"
let formatted_child = fp"{root}/{child_suffix}"
let trimmed = "  warn ".trim()
let lines = "a\nb\n".lines()
let collected = lines.collect()
let byte_lines = b"a\nb\n".lines().collect()
let encoded = b"abc".base64()
let decoded = encoded.base64_decode() ?
let digest = b"abc".sha256().hex()
let chunks = b"abcd".chunks(2)
let part = b"abcd".slice(1, 2)
let dump = b"abcd".dump("hex-u8")
let markers = b"\0abcd\0".strings(3)
let comparison = b"abc".compare(b"abd")
env ({
  HOME: root,
  CC: formatted_child.display(),
  JOBS: 4,
}) {
  print ${trimmed} ${lines[0]} ${digest}
} ?
""",
  )?
  expect_free_of(
    checked,
    ["check.call-target", "check.type-mismatch", "check.env-value", "check.unknown-method"],
  )
}

test test_checker_handles_implicit_standard_modules_and_pipe_shorthand { |ctx|
  let checked = check(
    ctx,
    r"""
let p = p"build.log"
let file_text = fs.read_text(p) ?
let file_bytes = p.read_bytes() ?
let decoded = file_bytes.utf8() ?
let warnings = decoded |> text.lines() |> where { "warn" in . }
let names = [{path: "b"}, {path: "a"}] |> map .path |> sort
let jobs = cpu.count()
let home = env.Path.HOME ?
""",
  )?
  expect_free_of(checked, ["check.unresolved-name", "check.call-target", "check.type-mismatch"])
}

test test_checker_rejects_non_json_compatible_public_json_apis { |ctx|
  let optional_value = check(
    ctx,
    r"""
type Metadata = {name: Str?, count: Int?}
let metadata: Metadata = {name: null, count: 0}
let encoded = json.encode(metadata) ?
""",
  )?
  expect_free_of(optional_value, ["check.json-compatible", "check.type-mismatch"])

  let ok = check(
    ctx,
    r"""
let status = run false
let metadata = {
  name: "demo",
  root: Path("src").display(),
  digest: b"abc".base64(),
  ok: status.ok,
  error: Error(kind: "example", message: "shown").kind,
}
let json_text = json.encode(metadata) ?
let pretty = json.encode(metadata, pretty: true) ?
let lines = json.encode_lines([metadata]) ?
let updated = json.set(metadata, ["name"], "other") ?
""",
  )?
  expect_free_of(ok, ["check.json-compatible", "check.type-mismatch"])

  for source in [
    r"""
type Metadata = {root: Path?}
let metadata: Metadata = {root: null}
let encoded = json.encode(metadata) ?
""",
    r"""
let metadata = {root: Path("src")}
let json_text = json.encode(metadata) ?
""",
    r"""
let status = run false
json.write("out.json", {status: status}) ?
""",
    r"""
let metadata = {root: Path("src")}
let updated = json.set(metadata, ["name"], "demo") ?
""",
  ] {
    let checked = check(ctx, source)?
    assert checked.status.exited_with(2), f"{source}: {checked.stderr}"
    assert "[check.json-compatible]" in checked.stderr, f"{source}: {checked.stderr}"
  }
}

test test_checker_handles_standard_module_signatures_and_status_methods { |ctx|
  let checked = check(
    ctx,
    r"""
let p = Path("tmp")
let exists = fs.exists(p) ?
let listing = fs.children(p)
fs.children(p) |> sort-by { .size } |> table.print(columns: ["name", "size"])
let processes = process.list() |> where { "xsh" in .command } |> count()
let port_rows = process.port(1)
let pid_port_rows = process.ports(1)
let shell = process.which("sh")?
let sig = process.signal("TERM")?
let _kill = process.kill(1, signal: "0")?
let _unix_kill = unix.kill_all("xsh-test-helper", signal: "TERM")?
let spawned = process.spawn(process.command {
  detach = true
  new_session = true
  ignore_hup = true
  stdout = Path("builder.out")
  stderr = Path("builder.err")
  stdout_append = true
  stderr_append = true
  run sh -c "true"
})?
let argv_words = process.argv_words("true")?
let command_from_str = process.command_argv("true", argv_words)
let command_from_path = process.command_argv(Path("true"), ["true"], Path("."))
let command_from_named = process.command_argv("true", ["true"], stdout: Path("out.log"), stderr: Path("err.log"), stdout_append: true, stderr_append: true, timeout: 1s, ignore_hup: true, cpu_max: 80)
let marker: Path = /tmp/marker
let command_with_path_argv = process.command_argv("echo", ["echo", marker])
let mixed_argv = [Path("echo"), "hello", marker]
let command_with_mixed_argv_var = process.command_argv(Path("echo"), mixed_argv)
let planned_status = process.run(command_from_str)?
let parsed_number = "0x2a".parse_int()?
let tokens = cli.tokens(["-dc", "--wrap=0", "file"], ["wrap"])?
let elf_info = elf.inspect(p)?
let _elf_needed: Str = elf_info.needed[0]
let _elf_tag: Str = elf_info.dynamic_tags[0].tag
let _written = io.write_stdout("typed")?
let measured = time.measure(process.command { run true })?
pure parse_command(text: Str) -> Result[Command] {
  let words = process.argv_words(text)?
  process.command_argv("true", words)
}
let now_ms = time.now()
let slept = time.sleep(1ms)?
let host = system.hostname()?
let os = system.uname()?
let me = user.current()?
let me_again = user.by_uid(me.uid)?
let named_me = user.lookup(me.name)?
let current_group = group.current()?
let group_again = group.by_gid(current_group.gid)?
let named_group = group.lookup(current_group.name)?
let usage = p.du() ?
let meta = fs.metadata(p) ?
let cwd = fs.cwd()?
let file = fp"{p}/file"
let copy = fp"{p}/copy"
let renamed_path = fp"{p}/renamed"
let _atomic = fs.write_atomic(file, "data") ?
let _copy = fs.copy(file, copy) ?
let copied_tree = fs.copy_tree(p, fp"{p}/tree", parents: true)?
let copied_files: Int = copied_tree.files
let _renamed = fs.rename(copy, renamed_path, overwrite: true) ?
let _touched = fp"{p}/stamp".touch() ?
let _truncated = renamed_path.truncate(0) ?
let _installed = fs.install(file, fp"{p}/bin/tool", 0o755, parents: true) ?
let _installed_as = fs.install_as(file, fp"{p}/bin/owned", 0o755, me, current_group, parents: true) ?
let _mode = fs.chmod(file, 384) ?
let _owner = fs.chown(file, me) ?
let _group = fs.chgrp(file, current_group) ?
let lock = fs.lock(fp"{p}/pm.lock") ?
let _unlock = fs.unlock(lock) ?
let removed = fs.remove_manifest(p, [Path("bin/tool")], missing_ok: true) ?
let removed_count: Int = removed.removed
let _fifo = fs.mkfifo(fp"{p}/control", 0o600) ?
let _synced_file = fs.fsync(file) ?
let _synced_all = fs.sync() ?
let _link = fs.symlink(file, fp"{p}/link") ?
let _hard = file.hardlink(at: fp"{p}/hard") ?
let target = fp"{p}/link".readlink() ?
let _unlinked = fp"{p}/hard".unlink() ?
let _rmdir = fp"{p}/empty".remove_dir() ?
let diffed = diff.unified(fp"{p}/old", fp"{p}/new", context: 1) ?
let patched = patch.apply(p, diffed.text, strip_components: 0, overwrite: true) ?
let patched_files: Int = patched.files
let child_events = unix.reap_child_events()?.collect()
let _child_pid: Int = child_events[0].pid
let _device_write = linux.write_device(/dev/urandom, fp"{p}/seed")?
let _device_read = linux.read_device(/dev/urandom, fp"{p}/seed", bytes: 512)?
let uevents = linux.uevent_stream()?
type Uevent = {action: Str, subsystem: Str, devname: Str, devpath: Str}
for event in uevents {
  let _uevent: Uevent = event
  break
}
let _mount = linux.mount("proc", /proc, fstype: "proc", options: ["nosuid", "noexec", "nodev"])?
let _mount_all = linux.mount_all()?
let _umount_all = linux.umount_all(types: ["proc", "tmpfs"])?
let _swapon = linux.swapon_all()?
let _swapoff = linux.swapoff_all()?
let root = linux.root_device()?
let _hostname = unix.set_hostname("xsh")?
let _link_up = linux.link_up("lo")?
let _set_ipv4 = linux.set_ipv4_address("eth0", "192.0.2.10", "255.255.255.0")?
let _route = linux.add_default_ipv4_route("192.0.2.1", interface: "eth0")?
let interfaces = linux.interfaces()?.collect()
let _interface_name: Str = interfaces[0].name
let _interface_flag: Str = interfaces[0].flags[0]
let _interface_mtu: Int = interfaces[0].mtu
let _interface_mac: Str = interfaces[0].mac
let _interface_addr: Str = interfaces[0].addresses[0].addr
let meminfo = linux.meminfo()?
let _mem_total: Int = meminfo.total
let modules = linux.modules()?.collect()
let _module_count: Int = modules.len()
let messages = linux.dmesg()?.collect()
let _message_count: Int = messages.len()
let _is_proc_mount = linux.is_mountpoint(/proc)?
let usage = linux.disk_usage(/)?.collect()
let _usage_total: Int = usage[0].total
let block_devices = linux.block_devices()?.collect()
let _block_device_path: Path = block_devices[0].path
let _block_device_partitioned: Bool = block_devices[0].partitioned
let _sysctl_value = linux.sysctl_get("kernel.pid_max")?
let _sysctl_set = linux.sysctl_set("kernel.pid_max", _sysctl_value)?
let attrs = linux.file_attrs(file)?
let _attrs_flags: Int = attrs.flags
let _attrs_indexed_directory: Bool = attrs.indexed_directory
let _attrs_secure_deletion: Bool = attrs.secure_deletion
let _attrs_undelete: Bool = attrs.undelete
let _attrs_sync: Bool = attrs.sync
let _attrs_dirsync: Bool = attrs.dirsync
let _attrs_immutable: Bool = attrs.immutable
let _attrs_append_only: Bool = attrs.append_only
let _attrs_no_dump: Bool = attrs.no_dump
let _attrs_no_atime: Bool = attrs.no_atime
let _attrs_compression_requested: Bool = attrs.compression_requested
let _attrs_journaled_data: Bool = attrs.journaled_data
let _attrs_no_tailmerging: Bool = attrs.no_tailmerging
let _attrs_top_of_directory_hierarchies: Bool = attrs.top_of_directory_hierarchies
let _set_attrs = linux.set_file_attrs(file, attrs.flags)?
let version = linux.file_version(file)?
let _version: Int = version
let _set_version = linux.set_file_version(file, version)?
let _sysctl = linux.sysctl_load_dirs([/etc/sysctl.d], fallback: /etc/sysctl.conf)?
let uptime = unix.uptime_seconds()?
let current_tty = unix.tty()?
let identity = unix.id()?
let _uid: Int = identity.uid
let _group_name: Str = identity.groups[0].name
let tty_attrs = unix.tty_attrs()?
let _tty_echo: Bool = tty_attrs.echo
let _set_tty_attrs = unix.set_tty_attrs(tty_attrs)?
let _kill_all = linux.kill_all(signal: "TERM", except_pid1: true)?
let _chroot = linux.chroot(/sysroot)?
let _mknod = linux.mknod(/dev/null, "char", 1, 3)?
let _insmod = linux.insmod(/lib/modules/demo.ko, params: "debug=1")?
let _rmmod = linux.rmmod("demo", force: true)?
let _pivot_root = linux.pivot_root(/sysroot, /sysroot/oldroot)?
let _switch_root = linux.switch_root(/sysroot, /sbin/init)?
let epoch_ms = linux.hwclock()?
let _set_hwclock = linux.set_hwclock(epoch_ms)?
let _set_system_clock = linux.set_system_clock(epoch_ms)?
let rfkill = linux.rfkill_list()?.collect()
let _rfkill_name: Str = rfkill[0].name
let _rfkill_block = linux.rfkill_block(rfkill[0].id)?
let _rfkill_unblock = linux.rfkill_unblock(rfkill[0].id)?
let loop_device = linux.loop_attach(file)?
let _loop_detach = linux.loop_detach(loop_device)?
let loops = linux.loop_list()?.collect()
let _loop_file: Path = loops[0].file
let _mkswap = linux.mkswap(file)?
let _swapon_device = linux.swapon(file, priority: 1)?
let _swapoff_device = linux.swapoff(file)?
let child = unix.spawn_process_group(command_from_str)?
let logged_file_child = unix.spawn_process_group_log(command_from_str, /tmp/service.log)?
let _logged_file_pid: Int = logged_file_child.pid
let logged_child = unix.spawn_logged_process_group(command_from_str, command_from_str)?
let _log_pid: Int = logged_child.log_pid
let tty_child = unix.spawn_with_tty(command_from_str, tty: "tty1")?
let _kill_group = unix.kill_process_group(child.pid, "TERM")?
let _pid1_setup = unix.pid1_setup(["TERM"], subreaper: true, allow_non_pid1: true)?
let pid1_event = unix.wait_pid1_event()?
let _pid1_event_kind: Str = pid1_event.kind
let pid1_shutdown = unix.shutdown_process_groups([child.pid], 0ms)?
let _pid1_term_sent: Int = pid1_shutdown.term_sent
let _exec = unix.exec(command_from_str)?
let _halt = linux.halt()?
let _poweroff = linux.poweroff()?
let _reboot = linux.reboot()?
let parent = file.parent
let renamed = file.with_ext("log")
let stripped = renamed.strip_prefix(p) ?
let path_meta = file.metadata()?
let _path_copy = file.copy(to: fp"{p}/copy2")?
let _path_rename = fp"{p}/copy2".rename(to: fp"{p}/renamed2", overwrite: true)?
let _path_touch = fp"{p}/stamp2".touch()?
let _path_truncate = fp"{p}/renamed2".truncate(0)?
let _path_hard = file.hardlink(at: fp"{p}/hard2")?
let path_target = fp"{p}/link".readlink()?
let _path_unlink = fp"{p}/hard2".unlink()?
let _path_rmdir = fp"{p}/empty2".remove_dir()?
let display = p.display()
let home = env.Path.HOME ?
let home_text = env.Str.HOME ?
let path_entries = env.PathList.PATH ?
let status = run false
let ok = status.exited_with(1)
let code = status.exit_code() ?
""",
  )?
  expect_free_of(
    checked,
    [
      "check.arity",
      "check.type-mismatch",
      "check.unknown-module-api",
      "check.try-result",
      "check.builder-field",
      "check.named-arg",
    ],
  )
}
