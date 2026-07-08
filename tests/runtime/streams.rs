use super::common::*;

#[test]
fn projected_reduce_by_preserves_duplicate_output_field_behavior() {
    let output = run_temp_script(
        "projected-reduce-by-duplicate-output-field",
        "\
let rows = [
  {key: \"g\", a: 1, b: 10},
  {key: \"g\", a: 2, b: 20},
  {key: \"g\", a: 3, b: 30},
]
let reduced = (rows)
  |> reduce-by --sum { |row|
    {key: row.key, value: {x: row.a, x: row.b}}
  }
let g = reduced.get(\"g\", {x: 0})
print f\"x=${g.x}\"
",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "x=10\n");
}

#[test]
fn live_stream_par_map_flat_map_reduce_by_matches_collected_rows() {
    let root = temp_path("live-stream-flat-map-reduce-root");
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).expect("create flat-map reduce fixture dirs");
    std::fs::write(root.join("a.txt"), "abc").expect("write a");
    std::fs::write(nested.join("b.txt"), "de").expect("write b");
    std::fs::write(nested.join("c.md"), "fghi").expect("write c");
    let source = format!(
        "\
let root = Path({})
let streamed = fs.walk(root)
  |> where .kind == \"file\"
  |> par-map --jobs=4 {{ |entry|
    [{{ext: entry.ext, count: 1, size: entry.size}}]
  }}
  |> flat-map {{ |rows| rows }}
  |> reduce-by --sum {{ |row|
    {{key: row.ext, value: {{count: row.count, size: row.size}}}}
  }}
let collected = fs.walk(root)
  |> where .kind == \"file\"
  |> collect()
  |> par-map --jobs=4 {{ |entry|
    {{ext: entry.ext, count: 1, size: entry.size}}
  }}
  |> reduce-by --sum {{ |row|
    {{key: row.ext, value: {{count: row.count, size: row.size}}}}
  }}
let st = streamed.get(\"txt\", {{count: 0, size: 0}})
let sm = streamed.get(\"md\", {{count: 0, size: 0}})
let ct = collected.get(\"txt\", {{count: 0, size: 0}})
let same = st.count == ct.count and st.size == ct.size and streamed == collected
print f\"same=${{same}} txt=${{st.count}}/${{st.size}} md=${{sm.count}}/${{sm.size}}\"
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("live-stream-flat-map-reduce", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "same=true txt=2/5 md=1/4\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn live_stream_par_map_for_loop_matches_collected_rows() {
    let root = temp_path("live-stream-par-map-for-root");
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).expect("create par-map for fixture dirs");
    std::fs::write(root.join("a.txt"), "abc").expect("write a");
    std::fs::write(nested.join("b.txt"), "de").expect("write b");
    std::fs::write(nested.join("c.md"), "fghi").expect("write c");
    let source = format!(
        "\
let root = Path({})
var streamed_txt_count = 0
var streamed_txt_size = 0
var streamed_md_count = 0
var streamed_md_size = 0
for row in fs.walk(root)
  |> where .kind == \"file\"
  |> par-map --jobs=4 {{ |entry|
    {{ext: entry.ext, count: 1, size: entry.size}}
  }}
  |> where .ext != \"\" {{
  match row.ext {{
    \"txt\" => {{
      streamed_txt_count += row.count
      streamed_txt_size += row.size
    }}
    \"md\" => {{
      streamed_md_count += row.count
      streamed_md_size += row.size
    }}
    _ => {{}}
  }}
}}
let collected = fs.walk(root)
  |> where .kind == \"file\"
  |> collect()
  |> par-map --jobs=4 {{ |entry|
    {{ext: entry.ext, count: 1, size: entry.size}}
  }}
  |> reduce-by --sum {{ |row|
    {{key: row.ext, value: {{count: row.count, size: row.size}}}}
  }}
let ct = collected.get(\"txt\", {{count: 0, size: 0}})
let cm = collected.get(\"md\", {{count: 0, size: 0}})
let same = streamed_txt_count == ct.count and streamed_txt_size == ct.size and streamed_md_count == cm.count and streamed_md_size == cm.size
print f\"same=${{same}} txt=${{streamed_txt_count}}/${{streamed_txt_size}} md=${{streamed_md_count}}/${{streamed_md_size}}\"
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("live-stream-par-map-for", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "same=true txt=2/5 md=1/4\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn user_stream_producers_yield_lazily_and_run_defers_on_stop() {
    let root = temp_path("stream-producer-root");
    std::fs::create_dir_all(&root).expect("create producer root");
    let marker = root.join("marker.txt");
    let source = format!(
        "\
stream nums(marker: Path) [fs, error] -> Stream[Int] {{
  defer marker.write(\"closed\")?
  for n in range(5) {{
    yield n
  }}
}}

let first = nums(Path({})) |> first()?
print ${{first}}
print ${{Path({}).read_text()?}}
",
        xsh_string_literal(&marker.to_string_lossy()),
        xsh_string_literal(&marker.to_string_lossy())
    );
    let output = run_temp_script("stream-producer-lazy-defer", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "0\nclosed\n");
}

#[test]
fn text_module_handles_fixed_string_fields_replacement_and_counts() {
    let output = run_temp_script(
        "text-fixed-string",
        "\
let row = \" alpha::beta::gamma \"
let fields = row.trim().fields(delimiter: \"::\")
let joined = fields.join(separator: \"/\")
let replaced = joined.replace(\"beta\", \"B\")
let scalars = \"hé\".split(\"\")
let wrapped = \"alpha beta gamma\".wrap(10)
let slug = \"alpha beta_gamma\".translate(\" _\", \"--\")
let deleted = \"a-b_c\".delete(\"-_\")
let squeezed = \"nooo   way\".squeeze(chars: \" o\")
print ${fields[0]} ${fields[2]} ${joined} ${replaced} ${\"desserts\".reverse()}
print ${\"one\\ntwo\\n\".count_lines()} ${\"one two\".count_words()} ${\"hé\".count_chars()} ${\"hé\".count_bytes()} ${scalars[1]}
print ${wrapped[0]} ${wrapped[1]} ${slug} ${deleted} ${squeezed}
",
    );

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "alpha gamma alpha/beta/gamma alpha/B/gamma stressed\n2 2 2 3 é\nalpha beta gamma alpha-beta-gamma abc no way\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn args_and_regex_modules_cover_cli_records_and_explicit_patterns() {
    let output = run_temp_script(
        "args-regex",
        r###"
let parsed = cli.parse([
  "--root", "pkg root",
  "-vj4",
  "-D", "A=1",
  "--define=B=2",
  "-n", "demo",
], {
  root: {kind: "Path", required: true},
  jobs: {kind: "Int", short: "j", default: 1},
  define: {kind: "Str", short: "D", repeated: true},
  verbose: {kind: "Bool", short: "v", default: false},
  name: {kind: "Str", short: "n", required: true},
})?
let missing = cli.parse([], {name: "Str"})?
let line = "WARN build.rs: unused value"
let level = regex.compile("WARN|ERROR")?
let parts = regex.compile("^(\\w+) ([^:]+): (.*)$")?
let whitespace = regex.compile("\\s+")?
let warn_unused = regex.compile("WARN.*unused")?
let found = level.find(line)
let captures = parts.captures(line)
let rewritten = whitespace.replace(line, "|")
let compiled_found = level.find(line)
let compiled_captures = parts.captures(line)
let compiled_rewritten = whitespace.replace(line, "|")
print ${parsed.root.name} ${parsed.jobs} ${parsed.define.len()} ${parsed.verbose} ${parsed.name} ${missing.name == null}
print ${warn_unused.matches(line)} ${found[0].text} ${captures[2]} ${rewritten}
print ${level.matches(line)} ${compiled_found[0].text} ${compiled_captures[2]} ${compiled_rewritten} ${level.pattern}
match cli.parse(["--jobs", "NaN"], {jobs: "Int"}) {
  Err(e) => print ${e.kind} ${"argv[1]" in e.message}
}
match cli.parse(["--unknown"], {jobs: "Int"}) {
  Err(e) => print ${e.kind} ${"unknown argument" in e.message}
}
match regex.compile("[") {
  Err(e) => print ${e.kind}
}
"###,
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "pkg root 4 2 true demo true\ntrue WARN build.rs WARN|build.rs:|unused|value\ntrue WARN build.rs WARN|build.rs:|unused|value WARN|ERROR\ncli-parse true\ncli-parse true\nregex-compile\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn diff_and_patch_modules_apply_text_and_reject_escapes() {
    let root = temp_path("diff-patch");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create diff root");

    let script = format!(
        "\
let root = Path({})
let original = fp\"${{root}}/original.txt\"
let modified = fp\"${{root}}/modified.txt\"
original.write(\"\"\"alpha
beta
\"\"\")?
modified.write(\"\"\"alpha
BETA
gamma
\"\"\")?
let d = diff.unified(original, modified, context: 1)?
print ${{d.files}} ${{d.hunks}} ${{\"BETA\" in d.text}}
let apply_root = fp\"${{root}}/apply\"
fs.mkdir(apply_root)?
fp\"${{apply_root}}/original.txt\".write(\"\"\"alpha
beta
\"\"\")?
let patch_text = \"\"\"--- original.txt
+++ original.txt
@@ -1,2 +1,3 @@
 alpha
-beta
+BETA
+gamma
\"\"\"
let applied = patch.apply(apply_root, patch_text)?
print ${{applied.files}} ${{applied.hunks}} ${{fp\"${{apply_root}}/original.txt\".read_text()?.trim()}}
let escape_patch = \"\"\"--- /dev/null
+++ ../escape.txt
@@ -0,0 +1 @@
+bad
\"\"\"
match patch.apply(apply_root, escape_patch) {{
  Err(e) => print ${{e.kind}}
}}
fs.mkdir(fp\"${{apply_root}}/safe\", parents: true)?
fs.symlink(Path(\"target.txt\"), fp\"${{apply_root}}/safe/link\")?
let symlink_patch = \"\"\"--- safe/link
+++ safe/link
@@ -1 +1 @@
-old
+new
\"\"\"
match patch.apply(apply_root, symlink_patch) {{
  Err(e) => print ${{e.kind}}
}}
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("diff-patch", &script);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "1 1 true\n1 1 alpha\nBETA\ngamma\npatch-path\npatch-escape\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn json_lines_adapter_fails_fast_with_source_context() {
    let output = run_temp_script(
        "json-lines-error",
        "\"{\\\"ok\\\":true}\\nnot json\\n\" |> json.lines() |> count()\n",
    );

    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("json-lines")
    );
}

#[test]
fn batch_max_argv_splits_long_path_lists_before_running_commands() {
    let mut source = String::from("let files = [");
    for index in 0..300 {
        if index > 0 {
            source.push_str(", ");
        }
        source.push_str("Path(\"");
        source.push_str(&"a".repeat(900));
        source.push_str(&index.to_string());
        source.push_str("\")");
    }
    source.push_str(
        "]\n(files) |> batch --max-argv |> each { |chunk|\n  run true @chunk ?\n}\nprint \"ok\"\n",
    );

    let output = run_temp_script("stream-batch-max-argv", &source);

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ok\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn sigterm_cancels_parallel_stream_process_work_without_losing_trace_context() {
    let root = temp_path("cancel-parallel-stream-root");
    std::fs::create_dir_all(&root).unwrap();
    let ready = root.join("ready");
    let shell = "printf ready >> \"$1\"; sleep 10";
    let source = format!(
        "\
let ready = Path({})
[\"one\", \"two\"] |> each --jobs=2 {{ |item|
  let _status = run sh -c {} sh (ready) ?
}}
",
        xsh_string_literal(ready.to_str().unwrap()),
        xsh_string_literal(shell)
    );

    let output = run_cancelable_temp_script(
        "cancel-parallel-stream",
        &source,
        ["--trace", "--raw"],
        &ready,
        libc::SIGTERM,
    );

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("canceled"));
    assert!(stderr.contains("kind=parallel.job.start"));
    assert!(stderr.contains("kind=parallel.cancel"));
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn signal_hook_runs_from_parallel_stream_parent_checkpoint() {
    let source = "\
on USR1 [] {
  print \"hook\"
  abort(0)
}

let _sender = process.spawn(process.command_argv(\"sh\", [\"sh\", \"-c\", r\"sleep 0.05; kill -USR1 $PPID\"]))?
let values = [1, 2, 3] |> par-map --jobs=2 { |value|
  time.sleep(1s)?
  value
}
print \"after\"
";

    let output = run_temp_script("signal-hook-parallel-stream", source);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hook\n");
}

#[test]
fn par_map_errors_become_empty_lists() {
    let output = run_temp_script(
        "par-map-error-empty-lists",
        "\
error DivError = DivisionByZero(message: Str) : InvalidData

proc safe_div(x: Int) [error] -> Result[Int] {
  if x == 0 {
    return Err(DivError.DivisionByZero(\"division by zero\"))
  }
  Ok(10 / x)
}

let results = [1, 2, 0, 4]
  |> par-map { |x| safe_div(x) }

print ${results.len()}
print ${results[0]}
print ${results[1]}
print ${results[3]}
",
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout, "4\n10\n5\n2\n");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("par-map error"));
}
