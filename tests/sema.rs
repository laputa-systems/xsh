#![allow(clippy::single_call_fn)]

use xsh::sema::check::{AnnotationFactKind, CheckOptions, Checker};
use xsh::source::SourceId;
use xsh::syntax::parser::Parser;

#[test]
fn checker_accepts_result_unit_proc_expression_calls() {
    let output = check(
        r#"
proc compile(src: Path) -> Result[Unit] {
  return Ok()
}
compile(Path("main.c"))?
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.proc-command-syntax",
            "check.expr-proc",
            "check.arity",
        ],
    );
}

#[test]
fn checker_accepts_nominal_error_construction_matching_and_facets() {
    let output = check(
        r#"
error FsError = NotFound(file: Path) : NotFound | PermissionDenied(file: Path, op: Str) : PermissionDenied

pure missing(file: Path) -> Result[Str, FsError] {
  return Err(FsError.NotFound(file: file))
}

let result = missing(Path("missing"))
match result {
  Ok(text) => { print ${text} }
  Err(FsError.NotFound { file }) => { print ${file.display()} }
  Err(is PermissionDenied) => { print "permission denied" }
  Err(error) => { print ${error.message} }
}
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.error-constructor",
            "check.pattern-constructor",
            "check.pattern-type",
            "check.type-mismatch",
        ],
    );
}

#[test]
fn checker_accepts_float_literals_methods_json_and_schema_checks() {
    let output = check(
        r#"
type Metric = {ratio: Float, samples: List[Float]}

let ratio: Float = 1.5
let seconds = 250.float() / 1000.0
let rounded = seconds.round()?
let text = ratio.format(precision: 2)
let raw: Any = json.decode("{\"ratio\":1.5,\"samples\":[0.25,1.25]}")?
let metric = raw.require(Metric)?
let encoded = json.encode({ratio, seconds})?
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.type-mismatch",
            "check.unknown-method",
            "check.json-compatible",
        ],
    );
}

#[test]
fn checker_rejects_mixed_numeric_float_operations() {
    let output = check(
        r#"
let bad = 1 + 1.0
let also_bad = 1.0 < 2
"#,
    );

    assert!(has_code(&output, "check.type-mismatch"));
}

#[test]
fn checker_explains_unknown_methods_and_list_concatenation() {
    let source = r#"
let text = "abc"
let bad_length = text.length()
let left: List[Str] = ["a"]
let right: List[Str] = ["b"]
let joined = left + right
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = Checker::check_arena(&parsed.arena, source).diagnostics;

    let unknown = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("check.unknown-method"))
        .expect("expected unknown method diagnostic");
    assert!(unknown.message.contains("`length` on Str"));
    assert!(unknown
        .notes
        .iter()
        .any(|note| note.contains("count_chars") && note.contains("byte_len")));

    let list = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("check.operator-type"))
        .expect("expected list operator diagnostic");
    assert!(list
        .notes
        .iter()
        .any(|note| note.contains(".extend(other)")));
}

#[test]
fn checker_allows_nested_args_and_repeated_discard_bindings() {
    let output = check(
        r#"
let _ = 1
let _ = 2
proc local() {
  let args = "local"
  print $args
}
"#,
    );
    assert_no_codes(&output, &["check.duplicate-name", "check.standard-module-shadow"]);
}

#[test]
fn checker_reports_nominal_error_migration_and_payload_errors() {
    let output = check(
        r#"
error FsError = NotFound(file: Path) : NotFound
let old = Error(kind: "parse", message: "bad")
let bad = FsError.NotFound()
let result: Result[Str, FsError] = Err(FsError.NotFound(file: Path("missing")))
match result {
  Err({kind: "not-found"}) => { print "old" }
  Err(error) => { print ${error.kind} }
}
"#,
    );

    assert!(has_code(&output, "check.error-removed"));
    assert!(has_code(&output, "check.error-constructor"));
}

#[test]
fn checker_rejects_stage_5_acceptance_cases() {
    let cases = [
        ("make -j4\n", "check.unresolved-proc-command"),
        (
            "pure helper(src: Str) -> Str { return src }\nhelper hi\n",
            "check.command-pure",
        ),
        (
            "proc compile(src: Path) -> Result[Unit] { return Ok() }\ncompile Path(\"main.c\") ?\n",
            "check.proc-command-syntax",
        ),
        (
            "let b = b\"x\"\nproc compile(src: Path) -> Result[Unit] { return Ok() }\ncompile (b) ?\n",
            "check.proc-command-syntax",
        ),
        (
            "proc bad() -> Result[Unit] { return 1 }\n",
            "check.type-mismatch",
        ),
        ("run.text echo hi\n", "check.ignored-result"),
        (
            "proc bad() -> Unit { run.status false ? }\n",
            "check.try-context",
        ),
        ("let b = b\"x\"\nrun echo (b) ?\n", "check.argv-conversion"),
        ("let xs = [\"echo\"]\nrun @xs value ?\n", "check.run-target"),
        (
            "proc needs(value: Str) -> Result[Unit] { return Ok() }\nlet b = b\"x\"\nneeds (b) ?\n",
            "check.proc-command-syntax",
        ),
        (
            "proc needs(data: Bytes) -> Result[Unit] { return Ok() }\nneeds abc ?\n",
            "check.proc-command-syntax",
        ),
        (
            "proc bad() -> Result[Str] { print \"x\" }\nbad ?\n",
            "check.type-mismatch",
        ),
        (
            "proc print() -> Result[Unit] { return Ok() }\n",
            "check.core-command-shadow",
        ),
        (
            "pure bad() -> Result[Unit] { let status = run.status false ?\nreturn Ok() }\n",
            "check.pure-run",
        ),
    ];

    for (source, code) in cases {
        let output = check(source);
        assert!(
            has_code(&output, code),
            "expected {code} in diagnostics: {:?}",
            output
        );
    }
}

#[test]
fn checker_rejects_standard_module_shadowing() {
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
        "let record = 1\n",
        "type fs = Str\n",
        "proc bytes() -> Result[Unit] { return Ok() }\n",
        "pure hash() -> Int { return 1 }\n",
        "proc bad(env: Str) -> Result[Unit] { return Ok() }\n",
        "for cpu in [1] { print ${cpu} }\n",
        "match Ok(1) { Ok(path) => print ${path} }\n",
    ] {
        let output = check(source);
        assert!(
            has_code(&output, "check.standard-module-shadow"),
            "expected standard module shadowing diagnostic for {source:?}: {output:?}",
        );
    }
}

#[test]
fn checker_accepts_entry_signal_hook_with_prior_bindings() {
    let output = check(
        r#"
let marker = Path("/tmp/xsh-signal")

on SIGINT [fs, error] {
  marker.write("interrupted\n")?
  abort(130)
}
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.signal-hook",
            "check.duplicate-signal-hook",
            "check.effect-violation",
            "check.unresolved-name",
            "check.unknown-method",
        ],
    );
}

#[test]
fn checker_rejects_invalid_signal_hooks() {
    let cases = [
        ("on 15 [] {\n}\n", "check.signal-hook"),
        ("on SIGKILL [] {\n}\n", "check.signal-hook"),
        ("on SIGCHLD [] {\n}\n", "check.signal-hook"),
        ("on SIGPIPE [] {\n}\n", "check.signal-hook"),
        (
            "on TERM [] {\n}\non SIGTERM [] {\n}\n",
            "check.duplicate-signal-hook",
        ),
        (
            "on SIGINT [] {\n  later.remove()?\n}\nlet later = Path(\"/tmp/x\")\n",
            "check.unresolved-name",
        ),
        (
            "proc bad() {\n  on SIGINT [] {\n  }\n}\n",
            "check.signal-hook",
        ),
        ("if true {\n  on SIGINT [] {\n  }\n}\n", "check.signal-hook"),
        (
            "for value in [1] {\n  on SIGINT [] {\n  }\n}\n",
            "check.signal-hook",
        ),
        ("export on SIGINT [] {\n}\n", "check.signal-hook"),
        ("on SIGINT [] {\n  return\n}\n", "check.signal-hook"),
        ("on SIGINT [] {\n  break\n}\n", "check.signal-hook"),
        ("on SIGINT [] {\n  continue\n}\n", "check.signal-hook"),
        ("on SIGINT [] {\n  1\n}\n", "check.signal-hook"),
        (
            "on SIGINT [] {\n  time.sleep(1ms)?\n}\n",
            "check.effect-violation",
        ),
        (
            "on SIGINT [error] {\n  Ok(\"bad\")\n}\n",
            "check.signal-hook",
        ),
    ];

    for (source, code) in cases {
        let output = check(source);
        assert!(
            has_code(&output, code),
            "expected {code} in diagnostics for {source:?}: {output:?}"
        );
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn checker_accepts_platform_signal_hook_names() {
    let output = check(
        r#"
on HUP [] {
}

on SIGUSR1 [] {
}

on ALRM [] {
}

on XCPU [] {
}

on XFSZ [] {
}
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.signal-hook",
            "check.duplicate-signal-hook",
            "check.unresolved-name",
        ],
    );
}

#[test]
fn checker_rejects_signal_hooks_in_modules_and_interactive_input() {
    let module_output = check_with_module("use helper\n", "on SIGINT [] {\n}\n");
    assert!(
        has_code(&module_output, "check.signal-hook-module"),
        "expected module signal hook diagnostic: {module_output:?}"
    );

    let interactive_output = check_interactive("on SIGINT [] {\n}\n");
    assert!(
        has_code(&interactive_output, "check.signal-hook"),
        "expected interactive signal hook diagnostic: {interactive_output:?}"
    );
}

#[test]
fn checker_handles_collection_modules() {
    let ok = check(
        r#"
let numbers = [1].push(2)
let more = numbers.extend([3])
let flat = numbers.extend(more)
let contains = flat.contains(3)
let fallback: Int = flat.get(9, 4)
let get_or_fallback: Int = flat.get(10, 5)
let first: Int = flat.get(0, 0)
var argv = ["cc"]
argv = argv.extend([p"main.c", "-o", p"main.o"])
let argv_command = process.command_argv(p"cc", argv)
let m0: Map[Int] = {}
let m1 = m0.set("one", 1)
let value = m1.get("one")?
let fallback = m1.get("missing", 2)
let get_or_fallback = m1.get("missing", 3)
let keys = m1.keys()
let values = m1.values()
let by_name = {row.name: row.version for row in [{name: "pkg", version: "1"}]}
let version: Str = by_name.get("pkg")?
let groups0: Map[List[Str]] = {}
let groups1 = groups0.push("pkg", "one")
let groups2 = groups1.push("pkg", "two")
let grouped: List[Str] = groups2.get("pkg")?
let row = {name: "pkg", version: "1"}
let has_name = row.has("name")
let field: Str = row.get("name")?
let fields = row.keys()
let checked = record.require(row, {name: "Str"}, optional: {version: "Str"})?
"#,
    );
    assert_no_codes(&ok, &["check.type-mismatch", "check.unknown-module-api"]);

    for source in [
        "let row = {name: \"pkg\"}\nlet _ = record.has(row, \"name\")\n",
        "let row = {name: \"pkg\"}\nlet _ = record.get(row, \"name\")?\n",
        "let row = {name: \"pkg\"}\nlet _ = record.keys(row)\n",
    ] {
        let output = check(source);
        assert!(has_code(&output, "check.unknown-module-api"));
    }

    let bad_list = check("let xs = [1].push(\"two\")\n");
    assert!(has_code(&bad_list, "check.type-mismatch"));

    let bad_map = check(
        r#"
let m0: Map[Int] = map.empty()
let m1 = m0.set("one", "bad")
"#,
    );
    assert!(has_code(&bad_map, "check.type-mismatch"));

    let bad_map_push = check(
        r#"
let m0: Map[Int] = map.empty()
let m1 = m0.push("one", 1)
"#,
    );
    assert!(has_code(&bad_map_push, "check.type-mismatch"));
}

#[test]
fn checker_handles_dynamic_module_load_and_proc_call_values() {
    let ok = check(
        r#"
let loaded = module.load(Path("package.xsh"))?
let build: Proc = loaded.build
let label: Pure = loaded.label
let name: Str = label.call("demo")
build.call("dest")?
"#,
    );
    assert_no_codes(
        &ok,
        &[
            "check.type-mismatch",
            "check.call-target",
            "check.unknown-module-api",
        ],
    );

    let direct = check(
        r#"
let loaded = module.load(Path("package.xsh"))?
let build: Proc = loaded.build
let result = build("dest")
"#,
    );
    assert!(has_code(&direct, "check.unresolved-call"));

    let pure_proc = check(
        r#"
pure bad(build: Proc) -> Result[Unit] {
  build.call("dest")?
  return Ok()
}
"#,
    );
    assert!(has_code(&pure_proc, "check.pure-effect"));
}

#[test]
fn checker_allows_value_returning_proc_calls_in_expression_position() {
    let ok = check(
        r#"
proc load_names(root: Path) -> Result[List[Str]] {
  return ["demo"]
}
let names = load_names(Path("src"))?
"#,
    );
    assert_no_codes(&ok, &["check.expr-proc", "check.pure-effect"]);

    let pure_proc = check(
        r#"
proc load_name() -> Result[Str] {
  return "demo"
}
pure bad() -> Result[Str] {
  return load_name()?
}
"#,
    );
    assert!(has_code(&pure_proc, "check.pure-effect"));
}

#[test]
fn checker_rejects_standard_module_aliases() {
    let output = check("use json as files\n");

    assert!(has_code(&output, "check.standard-module-alias"));
}

#[test]
fn checker_requires_alias_for_hyphenated_module_name() {
    let missing_alias =
        check_with_module("use PKGBUILD-x86_64\n", "export let pkg = {name: \"x\"}\n");
    assert!(has_code(&missing_alias, "check.hyphenated-module-alias"));

    let with_alias = check_with_module(
        "use PKGBUILD-x86_64 as PKGBUILD_x86_64\n",
        "export let pkg = {name: \"x\"}\n",
    );
    assert!(!has_code(&with_alias, "check.hyphenated-module-alias"));

    let nested_hyphen_alias = check_with_module(
        "use build-essential-native.proof as build_proof\n",
        "export let verified = true\n",
    );
    assert!(!has_code(
        &nested_hyphen_alias,
        "check.hyphenated-module-alias"
    ));
}

#[test]
fn checker_allows_proc_expression_splice_to_supply_multiple_arguments() {
    let output = check(
        r#"
proc pair(a: Str, b: Str) -> Result[Unit] {
  return Ok()
}
let parts = ["a", "b"]
pair(@parts)?
"#,
    );

    assert_no_codes(&output, &["check.arity", "check.splice-target"]);
}

#[test]
fn checker_allows_plain_values_for_result_returns() {
    let output = check(
        r#"
pure label(value: Str) -> Result[Str] {
  value
}

pure returned(value: Str) -> Result[Str] {
  return value
}

proc path_value() -> Result[Path] {
  ./target/value
}

let one = label("ok")?
let two = returned("ok")?
let path = path_value()?
"#,
    );

    assert_no_codes(&output, &["check.type-mismatch", "check.missing-return"]);
}

#[test]
fn checker_handles_batch_stream_stage() {
    let ok = check(
        r#"
let chunks = [Path("a"), Path("b")] |> batch --count=1 --max-argv
"#,
    );
    assert_no_codes(&ok, &["check.stream-batch", "check.stream-stage-option"]);

    let missing_limit = check("[1, 2] |> batch\n");
    assert!(has_code(&missing_limit, "check.stream-batch"));

    let bad_argv = check("[{ name: \"a\" }] |> batch --max-argv\n");
    assert!(has_code(&bad_argv, "check.stream-batch"));
}

#[test]
fn checker_handles_adapter_stream_stages() {
    let ok = check(
        r#"
let paths = "a.txt\nb.log\n" |> text.lines() |> map { |line| Path(line) }
let chunks = b"abcd" |> bytes.chunks(2)
let rows = "{\"name\":\"a\"}\n" |> json.lines()
let streamed = "{\"name\":\"b\"}\n" |> json.stream()
let words = "one two".words()
let split = "a,b".split(",")
let fields = "a::b".fields(delimiter: ":")
let joined = fields.join(separator: "|")
let replaced = joined.replace("|", ",")
let reversed = replaced.reverse()
let wrapped = "alpha beta".wrap(8)
let slug = "alpha beta".translate(" ", "-")
let deleted = "a-b".delete("-")
let squeezed = "nooo".squeeze(chars: "o")
let line_count = "a\nb\n".count_lines()
let word_count = "a b".count_words()
let char_count = "hé".count_chars()
let byte_count = "hé".count_bytes()
"#,
    );
    assert_no_codes(&ok, &["check.stream-input", "check.type-mismatch"]);

    let bad = check("b\"abc\" |> text.lines()\n");
    assert!(has_code(&bad, "check.type-mismatch"));

    let bad_stream = check("[\"a\"] |> map { . } |> text.lines()\n");
    assert!(has_code(&bad_stream, "check.stream-adapter"));

    let missing_chunk_size = check("b\"abc\" |> bytes.chunks()\n");
    assert!(has_code(&missing_chunk_size, "check.arity"));

    let bad_json = check("1 |> json.stream()\n");
    assert!(has_code(&bad_json, "check.type-mismatch"));
}

#[test]
fn checker_handles_user_stream_producers() {
    let ok = check(
        r#"
stream nums() -> Stream[Int] {
  for n in range(3) {
    yield n
  }
  return
}

let total = nums() |> sum
"#,
    );
    assert_no_codes(
        &ok,
        &["check.yield", "check.stream-return", "check.type-mismatch"],
    );

    let outside = check("yield 1\n");
    assert!(has_code(&outside, "check.yield"));

    let bad_return = check(
        r#"
stream bad() -> Stream[Int] {
  return 1
}
"#,
    );
    assert!(has_code(&bad_return, "check.stream-return"));

    let bad_yield = check(
        r#"
stream bad() -> Stream[Int] {
  yield "no"
}
"#,
    );
    assert!(has_code(&bad_yield, "check.type-mismatch"));

    let bad_delegation = check(
        r#"
stream bad() -> Stream[Int] {
  yield range(3)
}
"#,
    );
    assert!(has_code(&bad_delegation, "check.yield-stream"));
}

#[test]
fn checker_handles_standard_module_signatures_and_status_methods() {
    let output = check(
        r#"
let p = Path("tmp")
let exists = fs.exists(p) ?
let listing = fs.ls(p)
fs.ls(p) |> sort-by { .size } |> table.print(columns: ["name", "size"])
let processes = process.list() |> where { "xsh" in .command } |> count()
let port_rows = process.port(1)
let pid_port_rows = process.ports(1)
let shell = process.which("sh")?
let sig = process.signal("TERM")?
let _kill = process.kill(1, signal: "0")?
let _unix_kill = unix.kill_all("xsh-test-sleeper", signal: "TERM")?
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
let year = time.format(0, "%Y", utc: true)?
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
let file = fp"${p}/file"
let copy = fp"${p}/copy"
let renamed_path = fp"${p}/renamed"
let _atomic = fs.write_atomic(file, "data") ?
let _copy = fs.copy(file, copy) ?
let copied_tree = fs.copy_tree(p, fp"${p}/tree", parents: true)?
let copied_files: Int = copied_tree.files
let _renamed = fs.rename(copy, renamed_path, overwrite: true) ?
let _touched = fp"${p}/stamp".touch() ?
let _truncated = renamed_path.truncate(0) ?
let _installed = fs.install(file, fp"${p}/bin/tool", 0o755, parents: true) ?
let _installed_as = fs.install_as(file, fp"${p}/bin/owned", 0o755, me, current_group, parents: true) ?
let _mode = fs.chmod(file, 384) ?
let _owner = fs.chown(file, me) ?
let _group = fs.chgrp(file, current_group) ?
let lock = fs.lock(fp"${p}/pm.lock") ?
let _unlock = fs.unlock(lock) ?
let removed = fs.remove_manifest(p, [Path("bin/tool")], missing_ok: true) ?
let removed_count: Int = removed.removed
let _fifo = fs.mkfifo(fp"${p}/control", 0o600) ?
let _synced_file = fs.fsync(file) ?
let _synced_all = fs.sync() ?
let _link = fs.symlink(file, fp"${p}/link") ?
let _hard = file.hardlink(fp"${p}/hard") ?
let target = fp"${p}/link".readlink() ?
let _unlinked = fp"${p}/hard".unlink() ?
let _rmdir = fp"${p}/empty".remove_dir() ?
let diffed = diff.unified(fp"${p}/old", fp"${p}/new", context: 1) ?
let patched = patch.apply(p, diffed.text, strip_components: 0, overwrite: true) ?
let patched_files: Int = patched.files
let child_events = unix.reap_child_events()?.collect()
let _child_pid: Int = child_events[0].pid
let _device_write = linux.write_device(/dev/urandom, fp"${p}/seed")?
let _device_read = linux.read_device(/dev/urandom, fp"${p}/seed", bytes: 512)?
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
let _path_copy = file.copy(fp"${p}/copy2")?
let _path_rename = fp"${p}/copy2".rename(fp"${p}/renamed2", overwrite: true)?
let _path_touch = fp"${p}/stamp2".touch()?
let _path_truncate = fp"${p}/renamed2".truncate(0)?
let _path_hard = file.hardlink(fp"${p}/hard2")?
let path_target = fp"${p}/link".readlink()?
let _path_unlink = fp"${p}/hard2".unlink()?
let _path_rmdir = fp"${p}/empty2".remove_dir()?
let display = p.display()
let home = env.Path.HOME ?
let home_text = env.Str.HOME ?
let path_entries = env.PathList.PATH ?
let status = run false
let ok = status.exited_with(1)
let code = status.exit_code() ?
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.arity",
            "check.type-mismatch",
            "check.unknown-module-api",
            "check.try-result",
            "check.builder-field",
            "check.named-arg",
        ],
    );
}

#[test]
fn checker_reports_empty_process_argv() {
    let output = check(
        r#"
let command = process.command_argv("echo", [])
"#,
    );
    assert!(has_code(&output, "check.process-argv-empty"));
}

#[test]
fn checker_rejects_removed_verbose_apis() {
    let cases = [
        (
            "use text\nlet ok = text.contains(\"abc\", \"a\")\n",
            "check.unknown-module",
        ),
        (
            "use env\nlet home = env.get_path(\"HOME\") ?\n",
            "check.unsupported-api",
        ),
        (
            "use path\nlet display = path.display(Path(\"src\"))\n",
            "check.unsupported-api",
        ),
        (
            "use hash\nlet digest = hash.sha256_file(Path(\"archive.tar\"))?\n",
            "check.unknown-module-api",
        ),
        (
            "use fs\nlet _written = fs.write_text(Path(\"out\"), \"text\") ?\n",
            "check.unknown-module-api",
        ),
        (
            "let p = Path(\"out\")\nlet _written = p.write_text(\"text\") ?\n",
            "check.unknown-method",
        ),
        ("let _ = [1].get_or(4, 0)\n", "check.unknown-method"),
        (
            "let m: Map[Int] = map.empty()\nlet _ = m.get_or(\"missing\", 0)\n",
            "check.unknown-method",
        ),
        (
            "let _ = Path(\"Cargo.toml\").read()?\n",
            "check.unknown-method",
        ),
        (
            "let _ = module.require({}, {name: \"Str\"})?\n",
            "check.unknown-module-api",
        ),
        (
            "let _ = regex.matches(\"WARN\", \"WARN\")?\n",
            "check.unknown-module-api",
        ),
        ("let _ = [1] |> collect(1)\n", "check.arity"),
        (
            "let _ = [1] |> collect --jobs=1\n",
            "check.stream-stage-option",
        ),
        ("let _ = [1] |> collect { . }\n", "check.arity"),
    ];

    for (source, code) in cases {
        let output = check(source);
        assert!(
            has_code(&output, code),
            "expected {code} in diagnostics: {output:?}"
        );
    }
}

#[test]
fn checker_accepts_pipeline_collect_terminal() {
    let output = check(
        r#"
let xs: List[Int] = [1, 2, 3] |> collect()
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.unresolved-call",
            "check.type-mismatch",
            "check.stream-terminal-stage",
        ],
    );
}

#[test]
fn checker_rejects_linux_names_moved_to_unix() {
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
        let output = check(source);
        assert!(
            has_code(&output, "check.unknown-module-api"),
            "expected moved linux API to be rejected for {source:?}: {output:?}"
        );
    }
}

#[test]
fn checker_rejects_process_time_system_identity_calls_in_pure_functions() {
    for source in [
        "pure bad() -> Int {\n  let _ = process.list()?\n  return 0\n}\n",
        "pure bad() -> Int {\n  let _ = process.port(1)?\n  return 0\n}\n",
        "pure bad() -> Int {\n  let _ = process.which(\"sh\")?\n  return 0\n}\n",
        "pure bad(command: Command) -> Int {\n  let _ = process.run(command)?\n  return 0\n}\n",
        "pure bad() -> Int {\n  let _ = time.now()\n  return 0\n}\n",
        "pure bad() -> Int {\n  let _ = time.format(0, \"%Y\")?\n  return 0\n}\n",
        "pure bad() -> Int {\n  let _ = system.hostname()?\n  return 0\n}\n",
        "pure bad() -> Int {\n  let _ = user.current()?\n  return 0\n}\n",
        "pure bad() -> Int {\n  let _ = group.current()?\n  return 0\n}\n",
    ] {
        let output = check(source);
        assert!(
            has_code(&output, "check.pure-effect"),
            "expected pure-effect diagnostic for {source:?}: {output:?}",
        );
    }
}

#[test]
fn checker_enforces_method_call_effects() {
    let missing = check(
        r#"
proc bad(target: Path) [error] -> Result[Str] {
  return target.read_text()?
}
"#,
    );
    assert!(has_code(&missing, "check.effect-violation"));

    let accepted = check(
        r#"
proc good(target: Path) [fs, error] -> Result[Str] {
  return target.read_text()?
}
"#,
    );
    assert_no_codes(&accepted, &["check.effect-violation"]);
}

#[test]
fn checker_handles_spawn_wait_process_handles() {
    let output = check(
        r#"
proc start() [process, error] -> Result[ProcessHandle] {
  return spawn run true ?
}

proc main() [process, error] -> Result[Unit] {
  let h: ProcessHandle = start()?
  let pid: Int = h.pid
  let command: Str = h.command
  let argv: List[Str] = h.argv
  let detached: Bool = h.detached
  let s: Status = wait h?
  let h2: ProcessHandle = spawn run true ?
  h2.cancel()?
  let handles: List[ProcessHandle] = [spawn run true ?, spawn run false ?]
  let statuses: List[Status] = wait handles?
  return Ok()
}
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.type-mismatch",
            "check.wait-target",
            "check.spawn-run-kind",
            "check.spawn-run-shape",
            "check.effect-violation",
        ],
    );
}

#[test]
fn checker_rejects_invalid_spawn_wait_shapes() {
    let missing_effect = check(
        r#"
proc start() [error] -> Result[ProcessHandle] {
  return spawn run true ?
}
"#,
    );
    assert!(has_code(&missing_effect, "check.effect-violation"));

    let wait_non_handle = check("let x = wait 1\n");
    assert!(has_code(&wait_non_handle, "check.wait-target"));

    let capture_spawn = check("let h = spawn run.text printf ok\n");
    assert!(has_code(&capture_spawn, "check.spawn-run-kind"));

    let pipeline_spawn = check("let h = spawn run printf ok | run cat\n");
    assert!(has_code(&pipeline_spawn, "check.spawn-run-shape"));
}

#[test]
fn checker_handles_foundation_builders_literals_and_context() {
    let output = check(
        r#"
let mode = 0o755
let command = process.command {
  timeout = 30s
  detach = true
  new_session = false
  ignore_hup = true
  run --timeout=1s echo ok
}
let _command = command
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.type-mismatch",
            "check.builder-field",
            "check.builder-entry",
            "check.unknown-module-api",
        ],
    );
}

#[test]
fn checker_handles_hash_module_apis() {
    let output = check(
        r#"
let digest = hash.sha256(b"abc")
let hex = digest.hex()
let encoded = digest.base64()
let file_digest = hash.sha256(Path("archive.tar"))?
hash.verify_file(Path("archive.tar"), sha256: hex)?
let parsed = hash.parse_check_line(f"${hex}  archive.tar")?
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.unknown-module-api",
            "check.unknown-method",
            "check.type-mismatch",
            "check.named-arg",
        ],
    );
}

#[test]
fn checker_handles_bytes_module_apis() {
    let output = check(
        r#"
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
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.unknown-module-api",
            "check.type-mismatch",
            "check.field-access",
        ],
    );

    let bad_base64_decode = check(
        r#"
let decoded = b"b2s=".base64_decode()?
"#,
    );
    assert!(has_code(&bad_base64_decode, "check.unknown-method"));

    let bad_base32_decode = check(
        r#"
let decoded = b"N5XA====".base32_decode()?
"#,
    );
    assert!(has_code(&bad_base32_decode, "check.unknown-method"));

    let bad_decode = check(
        r#"
let decoded = "ok".utf8()?
"#,
    );
    assert!(has_code(&bad_decode, "check.unknown-method"));

    let bad_compare = check(
        r#"
let comparison = b"a".compare("b")
"#,
    );
    assert!(has_code(&bad_compare, "check.type-mismatch"));
}

#[test]
fn checker_handles_args_and_regex_modules() {
    let output = check(
        r#"
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
let rewritten: Str = rewrite_re.replace("a  b", " ")
let compiled: Regex = regex.compile("WARN|ERR")?
let compiled_pattern: Str = compiled.pattern
let compiled_ok: Bool = compiled.matches("WARN build")
let compiled_matches = compiled.find("WARN build")
let compiled_first: Str = compiled_matches[0].text
let compiled_pair: Regex = regex.compile("^([^=]+)=(.*)$")?
let compiled_captures: List[Str] = compiled_pair.captures("key=value")
let compiled_space: Regex = regex.compile("\\s+")?
let compiled_rewritten: Str = compiled_space.replace("a  b", " ")
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.unknown-module-api",
            "check.unknown-method",
            "check.type-mismatch",
            "check.named-arg",
        ],
    );
}

#[test]
fn checker_handles_path_literals_methods_and_expr_env_blocks() {
    let ok = check(
        r#"
let root = p"src"
let child = fp"${root}/main.c"
let child_suffix = "include/main.h"
let formatted_child = fp"${root}/${child_suffix}"
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
env {
  HOME = root
  CC = formatted_child.display()
  JOBS = 4
} {
  print ${trimmed} ${lines[0]} ${digest}
} ?
"#,
    );

    assert_no_codes(
        &ok,
        &[
            "check.call-target",
            "check.type-mismatch",
            "check.env-value",
            "check.unknown-method",
        ],
    );
}

#[test]
fn checker_accepts_narrow_function_tail_values() {
    let output = check(
        r#"
pure object_path(src: Path) -> Path {
  src.with_ext("o")
}
proc wrap(path: Path) -> Result[Path] {
  Ok(path)
}
proc compile(src: Path) -> Result[Path] {
  let obj = object_path(src)
  wrap (obj)
}
pure empty_names() -> List[Str] {
  []
}
proc empty_result_names() -> Result[List[Str]] {
  []
}
let command = process.command {
  run true
}
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.missing-return",
            "check.type-mismatch",
            "check.non-tail-expression",
            "check.empty-list",
        ],
    );
}

#[test]
fn checker_resolves_tail_bare_identifier_candidates() {
    let values = check(
        r#"
pure pure_tail(value: Str) -> Result[Str] {
  value
}
proc proc_tail(value: Str) -> Result[Str] {
  value
}
"#,
    );
    assert_no_codes(
        &values,
        &[
            "check.pure-command",
            "check.unresolved-proc-command",
            "check.type-mismatch",
        ],
    );

    let proc_precedence = check(
        r#"
proc value() -> Result[Str] {
  Ok("proc")
}
proc choose(value: Str) -> Result[Str] {
  value
}
"#,
    );
    assert_no_codes(
        &proc_precedence,
        &["check.unresolved-proc-command", "check.type-mismatch"],
    );
}

#[test]
fn checker_rejects_ambiguous_or_non_result_tail_values() {
    let non_tail = check(
        r#"
pure bad() -> Int {
  1
  2
}
"#,
    );
    assert!(has_code(&non_tail, "check.non-tail-expression"));
    let non_tail_text = check_messages(
        r#"
pure bad() -> Int {
  1
  2
}
"#,
    );
    assert!(
        non_tail_text
            .iter()
            .any(|message| message.contains("expression has type `Int`"))
    );

    let bare_value_non_tail = check(
        r#"
proc bad(value: Str) -> Result[Str] {
  value
  value
}
"#,
    );
    assert!(has_code(
        &bare_value_non_tail,
        "check.unresolved-proc-command"
    ));

    let nested_result = check(
        r#"
pure bad() -> Result[Result[Int]] {
  Ok(1)
}
"#,
    );
    assert!(has_code(&nested_result, "check.type-mismatch"));

    let ambiguous = check(
        r#"
pure bad(flag: Bool) -> Int {
  if flag {
    return 1
  }
}
"#,
    );
    assert!(has_code(&ambiguous, "check.missing-return"));
}

#[test]
fn checker_allows_local_mutation_inside_pure_functions() {
    let output = check(
        r#"
pure count_chars(value: Str) -> Int {
  var count = 0

  for ch in value.split("") {
    count += 1
  }

  return count
}

pure split_name(input: Record) -> Str {
  var {name} = input
  name = name.trim()
  return name
}
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.pure-var",
            "check.pure-assignment",
            "check.assign-let",
            "check.type-mismatch",
        ],
    );
}

#[test]
fn checker_rejects_nonlocal_assignment_inside_pure_functions() {
    let global = check(
        r#"
var counter = 0

pure bump() -> Int {
  counter += 1
  return counter
}
"#,
    );
    assert!(has_code(&global, "check.pure-assignment"));

    let param = check(
        r#"
pure trim_name(name: Str) -> Str {
  name = name.trim()
  return name
}
"#,
    );
    assert!(has_code(&param, "check.pure-assignment"));
    assert!(has_code(&param, "check.assign-let"));
}

#[test]
fn checker_rejects_undefined_utility_names_as_proc_commands() {
    let source = r#"
echo hi
false
"#;
    let output = check(source);

    assert!(has_code(&output, "check.unresolved-proc-command"));

    let output = check_interactive(
        r#"
basename filetxt
cat x
clear
cp x y
cut -d , -f 1 x
df x
dirname filetxt
du x
echo hi
env FOO=bar true
fd pattern root
false
find root -name x
fold -w 2 x
fsync x
grep pattern x
head -n1 x
host localhost
hostname
ip addr
chmod 644 x
chown 0 x
chgrp 0 x
command echo
link x y
ln -s x y
ls x
mkdir -p x
mv x y
nproc
paste x y
printenv PATH
pstree -p
printf "%s\n" hi
pwd
readlink x
realpath x
rev x
rg pattern root
rm -f x
rmdir x
seq 1 1
shuf x
sleep 0
sort x
split -l 1 x y
stat x
strings x
sync
tail -n1 x
tar -tf x
tee x
test -f x
touch -c x
tr a b x
tree root
true
tty
uname
uniq x
unlink x
wc -lwc x
which sh
whoami
yes
"#,
    );

    assert!(has_code(&output, "check.unresolved-proc-command"));
}

#[test]
fn checker_rejects_foundation_contract_errors() {
    let cases = [
        (
            "use process\nlet command = process.command { timeout = 1\nrun true }\n",
            "check.type-mismatch",
        ),
        (
            "let command = process.command { unknown = 1\nrun true }\n",
            "check.builder-field",
        ),
        (
            "let command = process.command { cpu_max = 0\nrun true }\n",
            "check.builder-field",
        ),
        ("run --cpumax=0 true\n", "check.cpumax"),
        ("run true | run --cpumax=80 cat\n", "check.pipeline-cpumax"),
        (
            "let command = process.command_argv(\"true\", 1)\n",
            "check.type-mismatch",
        ),
        (
            "let command = process.command_argv(\"true\", [\"true\", {bad: true}])\n",
            "check.type-mismatch",
        ),
        (
            "let command = process.command_argv(\"true\", [\"true\"], cpu_max: 0)\n",
            "check.named-arg",
        ),
        (
            "let status = process.run(\"true\")?\n",
            "check.type-mismatch",
        ),
        (
            "let message = f\"bad ${[1, 2]}\"\n",
            "check.display-conversion",
        ),
        (
            "use hash\nlet digest = hash.sha256(1)\n",
            "check.type-mismatch",
        ),
        (
            "use hash\nhash.verify_file(Path(\"archive.tar\"), bad: \"abc\")?\n",
            "check.named-arg",
        ),
    ];

    for (source, code) in cases {
        let output = check(source);
        assert!(
            has_code(&output, code),
            "expected {code} in diagnostics: {output:?}"
        );
    }
}

#[test]
fn checker_rejects_stage_7_fs_path_contract_errors() {
    let cases = [
        ("use fs\nlet listing = fs.ls(1) ?\n", "check.type-mismatch"),
        (
            "use fs\nlet _written = fs.write(Path(\"out\"), 1) ?\n",
            "check.type-mismatch",
        ),
        (
            "let p = Path(\"file\")\nlet renamed = p.with_ext(Path(\"log\"))\n",
            "check.type-mismatch",
        ),
        (
            "pure bad(p: Path) -> Result[Path] { return p.resolve()? }\n",
            "check.pure-effect",
        ),
        (
            "use fs\nlet _removed = fs.remove_manifest(Path(\"root\"), [1]) ?\n",
            "check.type-mismatch",
        ),
        (
            "use fs\nlet _lock = fs.lock(Path(\"pm.lock\"), shared: 1) ?\n",
            "check.type-mismatch",
        ),
    ];

    for (source, code) in cases {
        let output = check(source);
        assert!(
            has_code(&output, code),
            "expected {code} in diagnostics: {output:?}"
        );
    }
}

#[test]
fn checker_rejects_stage_8_table_and_sort_contract_errors() {
    let cases = [
        ("[1] |> table.print()\n", "check.table-print"),
        (
            "[{ name: \"a\" }] |> table.print(columns: [1])\n",
            "check.type-mismatch",
        ),
        (
            "[{ name: \"a\" }] |> sort-by { { key: .name } }\n",
            "check.stream-sort",
        ),
    ];

    for (source, code) in cases {
        let output = check(source);
        assert!(
            has_code(&output, code),
            "expected {code} in diagnostics: {output:?}"
        );
    }
}

#[test]
fn checker_accepts_sort_by_desc_flag() {
    let output = check("[1, 2, 3] |> sort-by --desc .\n");
    assert!(output.is_empty(), "{:?}", output);
    let output_bad = check("[1, 2, 3] |> sort-by --unknown .\n");
    assert!(
        has_code(&output_bad, "check.stream-stage-option"),
        "expected check.stream-stage-option in: {:?}",
        output_bad
    );
}

#[test]
fn checker_handles_while_match_aliases_schemas_and_rest_params() {
    let output = check(
        r#"
type PackageName = Str
type Package = { name: PackageName, root: Path, files: List[Path] }

proc describe(pkg: Package, prefix: Str = "pkg", ...labels: List[Str]) -> Result[Unit] {
  var tries = 0
  while tries < 3 {
    tries = tries + 1
    if tries == 2 {
      continue
    }
    break
  }

  match Ok(pkg.name) {
    Ok(name) if name == "demo" => print ${prefix} ${name},
    Err(e) => return Err(e),
    _ => print "other"
  }

  for label in labels {
    print ${label}
  }
}

let pkg: Package = { name: "demo", root: Path("src"), files: [Path("src/lib.rs")] }
describe (pkg) ?
describe (pkg) named extra ?
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.type-mismatch",
            "check.arity",
            "check.loop-control",
            "check.schema-field",
        ],
    );
}

#[test]
fn checker_rejects_stage_11_and_12_errors() {
    let cases = [
        ("break\n", "check.loop-control"),
        ("[1] |> each { break }\n", "check.loop-control"),
        (
            "let value = 1\nmatch value { Ok(x) => print ${x} }\n",
            "check.pattern-type",
        ),
        (
            "type Package = { name: Str, root: Path }\nlet pkg: Package = { name: \"demo\" }\n",
            "check.schema-field",
        ),
        (
            "type Package = { name: Str }\nlet pkg: Package = { name: \"demo\", extra: \"x\" }\n",
            "check.schema-field",
        ),
        (
            "proc bad(...items: Str) -> Result[Unit] { return Ok() }\n",
            "check.rest-type",
        ),
        (
            "proc bad(a: Str, b: Str = a) -> Result[Unit] { return Ok() }\n",
            "check.unresolved-name",
        ),
    ];

    for (source, code) in cases {
        let output = check(source);
        assert!(
            has_code(&output, code),
            "expected {code} in diagnostics: {:?}",
            output
        );
    }
}

#[test]
fn checker_enforces_stage_13_module_export_boundaries() {
    let ok = check_with_module(
        "use helper\nbuild(\"demo\")?\n",
        r#"
export proc build(name: Str) -> Result[Unit] {
  print ${name}
  return Ok()
}

let secret = "local"
"#,
    );
    assert_no_codes(
        &ok,
        &["check.unresolved-proc-command", "check.module-top-level"],
    );

    let leaked = check_with_module(
        "use helper\nprint ${secret}\n",
        r#"
let secret = "local"
export let pkg = {name: "demo"}
"#,
    );
    // TODO: checker should report check.unresolved-name for non-exported
    // bindings accessed from outside the module. Currently produces no diagnostic.
    assert!(!has_code(&leaked, "check.module-top-level"));

    let mutation = check_with_module(
        "use helper\n",
        r#"
var count = 0
export let pkg = {name: "demo"}
"#,
    );
    assert!(has_code(&mutation, "check.module-top-level"));

    let typed_alias = check_with_module(
        r#"
use helper as h
let pkg: h.Package = {name: "demo", root: Path("src")}
let label: Str = h.label(pkg)
"#,
        r#"
export type Package = {name: Str, root: Path}

export pure label(pkg: Package) -> Str {
  return pkg.name
}
"#,
    );
    assert_no_codes(&typed_alias, &["check.unknown-type", "check.type-mismatch"]);

    let typed_bare = check_with_module(
        r#"
use helper
let pkg: Package = {name: "demo", root: Path("src")}
"#,
        r#"
export type Package = {name: Str, root: Path}
"#,
    );
    assert_no_codes(&typed_bare, &["check.unknown-type", "check.type-mismatch"]);

    let private_type = check_with_module(
        r#"
use helper as h
let secret: h.Secret = {name: "demo"}
"#,
        r#"
type Secret = {name: Str}
export type Package = {name: Str}
"#,
    );
    assert!(has_code(&private_type, "check.unknown-type"));
}

#[test]
fn checker_reports_opaque_status_record_literals() {
    let messages = check_messages("var status: Status = {ok: false, code: 0}\n");
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Status") && message.contains("runtime-only type")),
        "{messages:?}"
    );
}

#[test]
fn checker_rejects_non_json_compatible_public_json_apis() {
    let ok = check(
        r#"
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
"#,
    );
    assert_no_codes(&ok, &["check.json-compatible", "check.type-mismatch"]);

    let path_value = check(
        r#"
let metadata = {root: Path("src")}
let json_text = json.encode(metadata) ?
"#,
    );
    assert!(has_code(&path_value, "check.json-compatible"));

    let status_value = check(
        r#"
let status = run false
json.write("out.json", {status: status}) ?
"#,
    );
    assert!(has_code(&status_value, "check.json-compatible"));

    let set_path_value = check(
        r#"
let metadata = {root: Path("src")}
let updated = json.set(metadata, ["name"], "demo") ?
"#,
    );
    assert!(has_code(&set_path_value, "check.json-compatible"));
}

#[test]
fn checker_handles_implicit_standard_modules_and_pipe_shorthand() {
    let output = check(
        r#"
let p = p"build.log"
let file_text = fs.read_text(p) ?
let file_bytes = p.read_bytes() ?
let decoded = file_bytes.utf8() ?
let warnings = decoded |> text.lines() |> where { "warn" in . }
let names = [{path: "b"}, {path: "a"}] |> map .path |> sort
let jobs = cpu.count()
let home = env.Path.HOME ?
"#,
    );
    assert_no_codes(
        &output,
        &[
            "check.unresolved-name",
            "check.call-target",
            "check.type-mismatch",
        ],
    );
}

#[test]
fn checker_handles_compact_sugar_forms() {
    let output = check(
        r#"
proc write_note(path: Path) {
  path.write("ok")?
}
var total = 1
total += 2
let files: List[Path] = g"src/*.rs"
let label: Str = if total > 1 { "many" } else { "one" }
let value: Int = match Ok(total) { Ok(count) => count, Err(_) => 0 }
let cargo_exists = p"Cargo.toml".exists()?
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.required-return",
            "check.type-mismatch",
            "check.operator-type",
            "check.unknown-method",
            "check.pure-effect",
        ],
    );
}

#[test]
fn checker_accepts_local_field_and_map_entry_assignment() {
    let output = check(
        r#"
type Stats = {code: Int, comments: Int}

pure bump() -> Stats {
  var stats: Stats = {code: 0, comments: 0}
  stats.code += 1
  return stats
}

var counts: Map[Int] = map.empty()
counts["code"] = 2
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.undefined-name",
            "check.assign-let",
            "check.pure-assignment",
            "check.type-mismatch",
            "check.operator-type",
            "check.assign-target",
        ],
    );
}

#[test]
fn checker_rejects_field_assignment_to_let_binding() {
    let output = check(
        r#"
let stats = {code: 0}
stats.code += 1
"#,
    );

    assert!(has_code(&output, "check.assign-let"));
}

#[test]
fn checker_handles_ergonomic_sugar_pass_forms() {
    let output = check(
        r#"
let pkg = {name: "demo", version: "1.0", path: Path("src")}
let {name, version, ..} = pkg
var {path, ..} = pkg
path = Path("dist")
for {name, ..} in [pkg] {
  print $name
}
let jobs = env.Str.JOBS ?? "1"
fs.mkdir build ?
fs.remove build --missing-ok ?
json.write manifest ({name, version}) ?
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.type-mismatch",
            "check.destructure-type",
            "check.destructure-field",
            "check.module-command-value",
            "check.module-command-flag",
            "check.arity",
            "check.try-result",
            "check.standard-module-shadow",
        ],
    );
}

#[test]
fn checker_rejects_ergonomic_sugar_pass_errors() {
    let cases = [
        ("fs.nope out ?\n", "check.unknown-module-api"),
        ("fs.read out ?\n", "check.unknown-module-api"),
        ("fs.remove out --unknown ?\n", "check.module-command-flag"),
        (
            "let {name, name} = {name: \"demo\"}\n",
            "check.destructure-field",
        ),
        ("let {name} = 1\n", "check.destructure-type"),
        (
            "let pkg = {name: \"demo\"}\nlet {version} = pkg\n",
            "check.destructure-field",
        ),
        (
            "export let {name} = {name: \"demo\"}\n",
            "check.export-destructure",
        ),
        (
            "let jobs = env.Str.JOBS or \"1\"\n",
            "check.result-fallback",
        ),
    ];

    for (source, code) in cases {
        let output = check(source);
        assert!(
            has_code(&output, code),
            "expected {code} in diagnostics: {output:?}"
        );
    }
}

#[test]
fn ignored_result_diagnostic_has_source_span() {
    let source = "run.text echo hi\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);

    let diagnostic = checked
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("check.ignored-result"))
        .expect("expected ignored result diagnostic");
    assert!(diagnostic.labels.iter().any(|label| !label.span.is_empty()));
}

#[test]
fn checker_accepts_any_as_public_dynamic_type() {
    let output = check(
        r#"
let x: Any = json.decode("{}")?
let y: Any = {name: "demo"}.get("name")?
"#,
    );
    assert_no_codes(&output, &["check.unknown-type", "check.type-mismatch"]);

    let deprecated = check("let x: Unknown = json.decode(\"{}\")?\n");
    assert!(has_code(&deprecated, "check.unknown-type"));
}

#[test]
fn checker_accepts_type_patterns_for_dynamic_values() {
    let output = check(
        r#"
pure label(value: Any) -> Result[Str] {
  match value {
    i is Int => return Ok(i.float().format(precision: 1))
    f is Float => return Ok(f.format(precision: 1))
    s is Str => return Ok(s)
    _ is Null => return Ok("null")
    _ => return Ok("other")
  }
}
"#,
    );

    assert_no_codes(
        &output,
        &[
            "check.pattern-type",
            "check.type-mismatch",
            "check.unknown-method",
        ],
    );
}

#[test]
fn checker_rejects_type_patterns_for_concrete_values() {
    let output = check(
        r#"
let value = 1
match value {
  i is Int => { print ${i} }
}
"#,
    );

    assert!(has_code(&output, "check.pattern-type"));
}

#[test]
fn strict_checker_reports_unvalidated_any_flows() {
    let output = check_strict(
        r#"
type Row = {name: Str}

proc needs_name(name: Str) -> Result[Unit] {
  return Ok()
}

proc load(path: Path) -> Result[Row] {
  return json.read(path)?
}

let row: Row = json.read(Path("row.json"))?
let name: Str = row.get("name")?
needs_name(name)?
let raw = json.decode("\"demo\"")?
needs_name(raw)?
let mixed = [1, json.decode("1")?]
"#,
    );

    let count = output
        .iter()
        .filter(|code| code.as_deref() == Some("check.strict-any"))
        .count();
    assert!(count >= 3, "expected strict Any diagnostics: {output:?}");
}

#[test]
fn strict_checker_accepts_validated_dynamic_data_and_dynamic_storage() {
    let output = check_strict(
        r#"
type Row = {name: Str}

let raw: Any = json.decode("{\"name\":\"demo\"}")?
let row = raw.require(Row)?
let name: Str = row.name
let still_dynamic: Any = json.read(Path("row.json"))?
let checked = record.require({name: "demo"}, {name: "Str"}, optional: {version: "Any"})?
var seen: Map[Bool] = map.empty()
"#,
    );
    assert_no_codes(
        &output,
        &[
            "check.strict-any",
            "check.unknown-field",
            "check.contract-type",
            "check.type-mismatch",
        ],
    );
}

#[test]
fn checker_types_require_schema_checks() {
    let output = check_strict(
        r#"
type Config = {name: Str, ports: List[Int], note: Str?}

let raw: Any = json.decode("{\"name\":\"demo\",\"ports\":[80],\"note\":null}")?
let cfg = raw.require(Config)?
let name: Str = cfg.name
let port: Int = cfg.ports[0]
let note: Str? = cfg.note
"#,
    );
    assert_no_codes(
        &output,
        &[
            "check.strict-any",
            "check.unknown-field",
            "check.type-mismatch",
        ],
    );
}

#[test]
fn checker_infers_args_parse_literal_schema_records() {
    let ok = check(
        r#"
let parsed = cli.parse([], {
  count: {kind: "Int", required: true},
  name: "Str",
  roots: {kind: "Path", repeated: true},
  verbose: "Bool",
})?
let count: Int = parsed.count
let name: Str? = parsed.name
let roots: List[Path] = parsed.roots
let verbose: Bool = parsed.verbose
"#,
    );
    assert_no_codes(&ok, &["check.type-mismatch", "check.unknown-field"]);

    let bad = check(
        r#"
let parsed = cli.parse([], {name: "Str"})?
let name: Int = parsed.name
"#,
    );
    assert!(has_code(&bad, "check.type-mismatch"));
}

#[test]
fn checker_treats_old_schema_helper_name_as_unresolved_call() {
    let old_name = ["vali", "date"].concat();
    let source = format!(
        "type Row = {{name: Str}}\nlet raw = {{name: \"demo\"}}\nlet row = {old_name}(raw, Row)?\n"
    );
    let output = check(&source);

    assert!(has_code(&output, "check.unresolved-call"));
}

#[test]
fn strict_checker_reports_missing_known_record_fields_and_bad_contracts() {
    let output = check_strict(
        r#"
type Row = {name: Str}
let row: Row = {name: "demo"}
let version = row.version
let checked = record.require({name: "demo"}, {name: "Strng", bad: "Result[Str, ]"})
let loaded = record.require({}, {build: "Proc(Path -> Result[Unit]"})
"#,
    );
    assert!(has_code(&output, "check.unknown-field"));
    assert!(has_code(&output, "check.contract-type"));
}

#[test]
fn checker_narrows_optional_record_has_result_and_tag_flows() {
    let output = check_strict(
        r#"
type Row = {name: Str}
type State = Ready(Str) | Stopped

let maybe: Str? = "demo"
if maybe != null {
  let name: Str = maybe
}
if maybe == null {
  let fallback = "missing"
} else {
  let name: Str = maybe
}

let row: Row = {name: "demo"}
if row.has("version") {
  let version: Any = row.version
}

let result: Result[Str] = Ok("demo")
match result {
  Ok(value) => {
    let name: Str = value
  }
  Err(err) => {
    let error: Error = err
  }
}

let state = Ready("demo")
match state {
  Ready(value) => {
    let name: Str = value
  }
  Stopped => {}
}
"#,
    );
    assert_no_codes(
        &output,
        &[
            "check.strict-any",
            "check.type-mismatch",
            "check.unknown-field",
            "check.pattern-type",
        ],
    );
}

#[test]
fn checker_retains_annotation_facts_and_reveal_notes() {
    let source = r#"
type State = Ready | Stopped
let count = 1
var names = ["a", "b"]
let state = Ready

proc local(input = Path(".")) {
}

export proc entry(flag = true) {
}

reveal_type(names)
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let output = Checker::check_arena_with_options(
        &parsed.arena,
        source,
        CheckOptions {
            interactive_commands: None,
            strict_dynamic: false,
            reveal_types: true,
            migration_diagnostics: false,
        },
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(output.annotation_facts.iter().any(|fact| matches!(
        fact.kind,
        AnnotationFactKind::Binding { .. }
    ) && fact.ty.annotation_source().as_deref()
        == Some("List[Str]")));
    assert!(!output.annotation_facts.iter().any(|fact| matches!(
        fact.kind,
        AnnotationFactKind::Binding { .. }
    ) && fact.ty.annotation_source().as_deref()
        == Some("Int")));
    assert!(output.annotation_facts.iter().any(|fact| matches!(
        fact.kind,
        AnnotationFactKind::DefaultedParam { .. }
    ) && fact.ty.annotation_source().as_deref()
        == Some("Bool")));
    assert!(output.annotation_facts.iter().any(|fact| matches!(
        fact.kind,
        AnnotationFactKind::DefaultedParam { .. }
    ) && fact.ty.annotation_source().as_deref()
        == Some("Path")));
    assert!(
        output
            .annotation_facts
            .iter()
            .any(|fact| matches!(fact.kind, AnnotationFactKind::ExportedProcReturn { .. }))
    );
    assert_eq!(output.reveal_types.len(), 1);
    assert_eq!(output.reveal_types[0].message, "revealed type: List[Str]");
}

#[test]
fn checker_rejects_reveal_type_outside_reveal_mode() {
    let output = check("reveal_type(1)\n");

    assert!(has_code(&output, "check.reveal-type"), "{output:?}");
}

#[test]
fn checker_rejects_reveal_type_bad_argument_shapes() {
    let output = check_reveal("reveal_type()\n");
    assert!(has_code(&output.codes, "check.arity"), "{:?}", output.codes);
    assert!(output.reveals.is_empty(), "{:?}", output.reveals);

    let output = check_reveal("reveal_type(1, 2)\n");
    assert!(has_code(&output.codes, "check.arity"), "{:?}", output.codes);
    assert!(output.reveals.is_empty(), "{:?}", output.reveals);

    let output = check_reveal("reveal_type(value: 1)\n");
    assert!(
        has_code(&output.codes, "check.named-arg"),
        "{:?}",
        output.codes
    );
    assert!(output.reveals.is_empty(), "{:?}", output.reveals);

    let output = check_reveal("let xs = [1]\nreveal_type(@xs)\n");
    assert!(
        has_code(&output.codes, "check.call-splice"),
        "{:?}",
        output.codes
    );
    assert!(output.reveals.is_empty(), "{:?}", output.reveals);
}

#[derive(Debug)]
struct RevealCheckOutput {
    codes: Vec<Option<String>>,
    reveals: Vec<String>,
}

fn check(source: &str) -> Vec<Option<String>> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    Checker::check_arena(&parsed.arena, source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn check_with_migration(source: &str) -> Vec<Option<String>> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    Checker::check_arena_with_options(
        &parsed.arena,
        source,
        CheckOptions {
            interactive_commands: None,
            strict_dynamic: false,
            reveal_types: false,
            migration_diagnostics: true,
        },
    )
    .diagnostics
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

fn check_reveal(source: &str) -> RevealCheckOutput {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let output = Checker::check_arena_with_options(
        &parsed.arena,
        source,
        CheckOptions {
            interactive_commands: None,
            strict_dynamic: false,
            reveal_types: true,
            migration_diagnostics: false,
        },
    );
    RevealCheckOutput {
        codes: output
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect(),
        reveals: output
            .reveal_types
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect(),
    }
}

fn check_strict(source: &str) -> Vec<Option<String>> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    Checker::check_arena_with_options(
        &parsed.arena,
        source,
        CheckOptions {
            interactive_commands: None,
            strict_dynamic: true,
            reveal_types: false,
            migration_diagnostics: false,
        },
    )
    .diagnostics
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

fn check_interactive(source: &str) -> Vec<Option<String>> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    Checker::check_arena_interactive(&parsed.arena, source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn check_messages(source: &str) -> Vec<String> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    Checker::check_arena(&parsed.arena, source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            let labels = diagnostic
                .labels
                .into_iter()
                .filter_map(|label| label.message)
                .collect::<Vec<_>>()
                .join(" ");
            format!("{} {labels}", diagnostic.message)
        })
        .collect()
}

fn check_with_module(main_source: &str, module_source: &str) -> Vec<Option<String>> {
    let main = Parser::parse_source_arena_only(SourceId::new(0), main_source);
    let module = Parser::parse_source_arena_only(SourceId::new(1), module_source);
    assert!(main.diagnostics.is_empty(), "{:?}", main.diagnostics);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    Checker::check_arena_with_modules(
        (&main.arena, main_source),
        &[("helper", "helper", &module.arena, module_source)],
    )
    .diagnostics
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

fn has_code(output: &[Option<String>], code: &str) -> bool {
    output.iter().any(|item| item.as_deref() == Some(code))
}

fn assert_no_codes(output: &[Option<String>], codes: &[&str]) {
    for code in codes {
        assert!(
            !has_code(output, code),
            "unexpected {code} in diagnostics: {output:?}"
        );
    }
}

#[test]
fn checker_accepts_list_comprehensions() {
    let output = check(
        r#"
let nums: List[Int] = [1, 2, 3]
let doubled = [x * 2 for x in nums]
let filtered = [x for x in nums if x > 1]
let strs = [f"{x}" for x in nums]
"#,
    );
    assert_no_codes(
        &output,
        &[
            "check.type-mismatch",
            "check.listcomp-iterator",
            "check.listcomp-condition",
        ],
    );
}

#[test]
fn checker_accepts_list_comprehension_with_destructuring() {
    let output = check(
        r#"
type Entry = {name: Str, score: Int}
let entries: List[Entry] = []
let names = [name for {name, score} in entries]
let passing = [name for {name, score} in entries if score >= 60]
"#,
    );
    assert_no_codes(
        &output,
        &[
            "check.type-mismatch",
            "check.destructure-type",
            "check.destructure-field",
            "check.listcomp-iterator",
            "check.listcomp-condition",
        ],
    );
}

#[test]
fn checker_rejects_list_comprehension_non_list_iter() {
    let output = check("let x = [v for v in 42]\n");
    assert!(
        has_code(&output, "check.listcomp-iterator"),
        "expected check.listcomp-iterator in {output:?}"
    );
}

#[test]
fn checker_rejects_list_comprehension_non_bool_condition() {
    let output = check("let nums: List[Int] = [1, 2]\nlet x = [v for v in nums if v]\n");
    assert!(
        has_code(&output, "check.listcomp-condition"),
        "expected check.listcomp-condition in {output:?}"
    );
}

#[test]
fn checker_accepts_retry_attempt_local_try_without_error_effect() {
    let output = check(
        r#"
proc fetch() [net] -> Result[Str] {
  return Ok("ok")
}

proc main() [net] -> Unit {
  let value = retry [] {
    fetch()?
  }
}
"#,
    );
    assert_no_codes(&output, &["check.try-context", "check.effect-violation"]);
}

#[test]
fn checker_enforces_net_effect_for_http_module_calls() {
    let missing = check(
        r#"
proc bad() [fs] -> Unit {
  let _ = net.request({method: "GET", url: "https://example.test/"})
  let _ = net.request_many({requests: [{method: "GET", url: "https://example.test/"}]})
  let _ = net.download({url: "https://example.test/file", dest: Path("out")})
  let _ = net.download_many({downloads: [{url: "https://example.test/file", dest: Path("out")}]})
  let _ = net.upload({url: "https://example.test/upload", source: Path("in")})
}
"#,
    );
    assert!(
        has_code(&missing, "check.effect-violation"),
        "expected check.effect-violation in {missing:?}"
    );

    let net = check(
        r#"
proc good() [net] -> Unit {
  let _ = net.request({method: "GET", url: "https://example.test/"})
  let _ = net.request_many({requests: [{method: "GET", url: "https://example.test/"}]})
  let _ = net.download({url: "https://example.test/file", dest: Path("out")})
  let _ = net.download_many({downloads: [{url: "https://example.test/file", dest: Path("out")}]})
  let _ = net.upload({url: "https://example.test/upload", source: Path("in")})
}
"#,
    );
    assert_no_codes(&net, &["check.effect-violation"]);

    let io = check(
        r#"
proc good() [io] -> Unit {
  let _ = net.request({method: "GET", url: "https://example.test/"})
  let _ = net.request_many({requests: [{method: "GET", url: "https://example.test/"}]})
  let _ = net.download({url: "https://example.test/file", dest: Path("out")})
  let _ = net.download_many({downloads: [{url: "https://example.test/file", dest: Path("out")}]})
  let _ = net.upload({url: "https://example.test/upload", source: Path("in")})
}
"#,
    );
    assert_no_codes(&io, &["check.effect-violation"]);
}

#[test]
fn checker_requires_time_effect_for_retry_delays() {
    let output = check(
        r#"
proc main() [fs] -> Unit {
  let value = retry [1ms] {
    Ok("ok")
  }
}
"#,
    );
    assert!(
        has_code(&output, "check.effect-violation"),
        "expected check.effect-violation in {output:?}"
    );
}

#[test]
fn checker_rejects_non_duration_retry_delays() {
    let output = check(
        r#"
let value = retry ["soon"] {
  Ok("ok")
}
"#,
    );
    assert!(
        has_code(&output, "check.type-mismatch"),
        "expected check.type-mismatch in {output:?}"
    );
}

// ── match tail expression return type inference ──

#[test]
fn checker_accepts_match_with_all_returning_arms_as_function_body() {
    let output = check(
        r#"
type Tok = TOp(Str) | TEOF

pure is_op(t: Tok, name: Str) -> Bool {
  match t {
    TOp(s) => return s == name
    _ => return false
  }
}
"#,
    );
    assert_no_codes(&output, &["check.missing-return", "check.type-mismatch"]);
}

#[test]
fn checker_accepts_match_with_all_returning_arms_and_no_trailing_return() {
    let output = check(
        r#"
type Kind = A | B | C

pure kind_name(k: Kind) -> Str {
  match k {
    A => return "a"
    B => return "b"
    _ => return "other"
  }
}
"#,
    );
    assert_no_codes(&output, &["check.missing-return", "check.type-mismatch"]);
}

#[test]
fn checker_still_rejects_non_exhaustive_returning_match_without_catchall() {
    let output = check_with_migration(
        r#"
type Kind = A | B | C

pure kind_name(k: Kind) -> Str {
  match k {
    A => return "a"
    B => return "b"
  }
}
"#,
    );
    // Non-exhaustive match without catch-all: neither exhaustiveness nor
    // all-arms-return optimization applies. The match produces Unit (arms
    // return), which mismatches the declared Str return type.
    assert!(
        has_code(&output, "check.non-exhaustive-match"),
        "expected non-exhaustive-match in {output:?}"
    );
}

#[test]
fn checker_accepts_match_with_all_arms_returning_in_nested_function() {
    let output = check(
        r#"
type Opt = Some(Int) | None

pure unwrap(o: Opt) -> Int {
  match o {
    Some(n) => return n
    None => return 0
  }
}
"#,
    );
    assert_no_codes(&output, &["check.missing-return", "check.type-mismatch"]);
}

// ── string concatenation operator type checking ──

#[test]
fn checker_accepts_string_concatenation() {
    let output = check(
        r#"let x = "a" + "b"
"#,
    );
    assert_no_codes(&output, &["check.type-mismatch", "check.operator-type"]);
}

#[test]
fn checker_rejects_string_concatenation_with_int() {
    let output = check(
        r#"let x = "a" + 1
"#,
    );
    assert!(
        has_code(&output, "check.type-mismatch"),
        "expected type-mismatch for Str + Int in {output:?}"
    );
}

#[test]
fn checker_rejects_string_concatenation_with_bool() {
    let output = check(
        r#"let x = true + "b"
"#,
    );
    assert!(
        has_code(&output, "check.type-mismatch"),
        "expected type-mismatch for Bool + Str in {output:?}"
    );
}

#[test]
fn checker_chains_string_concatenation() {
    let output = check(
        r#"let x = "a" + "b" + "c"
"#,
    );
    assert_no_codes(&output, &["check.type-mismatch", "check.operator-type"]);
}

// ── Str.parse_float and Float math method type checking ──

#[test]
fn checker_accepts_str_parse_float() {
    let output = check(
        r#"let n = "3.14".parse_float()?
"#,
    );
    assert_no_codes(&output, &["check.type-mismatch", "check.unknown-method"]);
}

#[test]
fn checker_accepts_float_math_methods() {
    let output = check(
        r#"
let a = 4.0.sqrt()
let b = 2.0.pow(3.0)
let c = 1.0.exp()
let d = 10.0.ln()
let e = 100.0.log(10.0)
let f = 0.0.sin()
let g = 0.0.cos()
let h = 0.0.tan()
let i = (-3.0).abs()
"#,
    );
    assert_no_codes(&output, &["check.type-mismatch", "check.unknown-method"]);
}

#[test]
fn checker_requires_retained_docs_for_public_exports() {
    let output = check_with_module(
        "use plugin\n",
        r#"
export let value: Int = 1
"#,
    );

    assert!(has_code(&output, "check.missing-module-doc"), "{output:?}");
    assert!(has_code(&output, "check.missing-public-doc"), "{output:?}");
}

#[test]
fn checker_accepts_documented_public_exports() {
    let output = check_with_module(
        "use plugin\n",
        r#"
##! Test module documentation.

## Exposes a documented value.
export let value: Int = 1
"#,
    );

    assert_no_codes(
        &output,
        &["check.missing-module-doc", "check.missing-public-doc"],
    );
}

#[test]
fn checker_rejects_orphaned_and_duplicate_module_docs() {
    let output = check_with_module(
        "use plugin\n",
        r#"
##! First module documentation.
# ordinary commentary separates module doc blocks
##! Duplicate module documentation.

## Orphaned documentation.
let value = 1

## Exposes a documented value.
export let exported: Int = value
"#,
    );

    assert!(
        has_code(&output, "check.duplicate-module-doc"),
        "{output:?}"
    );
    assert!(has_code(&output, "check.orphan-doc-comment"), "{output:?}");
}
