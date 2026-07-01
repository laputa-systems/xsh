use super::common::*;

#[test]
fn table_and_sort_stream_stages_are_trace_observable() {
    let output = run_temp_script_with_args(
        "table-trace",
        "\
let rows = [{name: \"b\", size: 2}, {name: \"a\", size: 1}]
(rows) |> sort-by { .size } |> table.print(columns: [\"name\", \"size\"])
",
        ["--trace", "--raw"],
    );

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "┌──────┬──────┐\n│ name │ size │\n├──────┼──────┤\n│ a    │    1 │\n├──────┼──────┤\n│ b    │    2 │\n└──────┴──────┘\n"
    );
    let trace = String::from_utf8(output.stderr).unwrap();
    assert!(trace.contains("kind=stream.stage.enter"));
    assert!(trace.contains("kind=stream.stage.exit"));
    assert!(trace.contains("name=\"sort-by\""));
    assert!(trace.contains("stage=b\"sort-by\""));
    assert!(trace.contains("name=\"table.print\""));
    assert!(trace.contains("stage=b\"table.print\""));
    assert!(trace.contains("item_count=2"));
}

#[test]
fn reduce_by_streams_grouped_aggregates_with_sum_min_and_max() {
    // `reduce-by` keeps one accumulator per key (no group-by materialization).
    // --sum folds field-wise over records, exercising the ecount2 shape.
    let output = run_temp_script(
        "reduce-by",
        "\
let nums = [1, 2, 3, 4, 5, 6]
let agg = (nums) |> reduce-by --sum { |n| {key: if n % 2 == 0 { \"even\" } else { \"odd\" }, value: {count: 1, total: n}} }
let lo = (nums) |> reduce-by --min { |n| {key: \"all\", value: n} }
let hi = (nums) |> reduce-by --max { |n| {key: \"all\", value: n} }
let even = agg.get(\"even\", {count: 0, total: 0})
let odd = agg.get(\"odd\", {count: 0, total: 0})
let lo_all = lo.get(\"all\", 0)
let hi_all = hi.get(\"all\", 0)
print f\"even=${even.count}/${even.total} odd=${odd.count}/${odd.total} min=${lo_all} max=${hi_all}\"
",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "even=3/12 odd=3/9 min=1 max=6\n"
    );
}

#[test]
fn reduce_by_parallel_jobs_matches_serial() {
    // `--jobs=N` folds partitions on worker threads and merges the associative
    // partials; the result must equal the serial fold exactly. The input must
    // exceed the parallel threshold for `--jobs` to actually engage.
    let output = run_temp_script(
        "reduce-by-jobs",
        "\
let nums = [0] |> range(0, 50000)
let serial = (nums) |> reduce-by --sum { |n| {key: if n % 3 == 0 { \"a\" } else if n % 3 == 1 { \"b\" } else { \"c\" }, value: {count: 1, total: n}} }
let par = (nums) |> reduce-by --sum --jobs=8 { |n| {key: if n % 3 == 0 { \"a\" } else if n % 3 == 1 { \"b\" } else { \"c\" }, value: {count: 1, total: n}} }
var ok = true
for k in serial.keys() {
  let s = serial.get(k, {count: 0, total: 0})
  let p = par.get(k, {count: 0, total: 0})
  if s.count != p.count or s.total != p.total { ok = false }
}
let lo = ((nums) |> reduce-by --min --jobs=8 { |n| {key: \"all\", value: n} }).get(\"all\", -1)
let hi = ((nums) |> reduce-by --max --jobs=8 { |n| {key: \"all\", value: n} }).get(\"all\", -1)
print f\"match=${ok} keys=${par.keys().len()} min=${lo} max=${hi}\"
",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "match=true keys=3 min=0 max=49999\n"
    );
}

#[test]
fn par_map_reduce_by_fuses_to_local_worker_aggregation() {
    let output = run_temp_script(
        "par-map-reduce-by-fused",
        "\
let nums = [0] |> range(0, 50000)
let fused = (nums)
  |> par-map --jobs=8 { |n|
    {bucket: if n % 4 == 0 { \"a\" } else if n % 4 == 1 { \"b\" } else if n % 4 == 2 { \"c\" } else { \"d\" }, doubled: n * 2, count: 1}
  }
  |> reduce-by --sum { |row|
    {key: row.bucket, value: {count: row.count, total: row.doubled}}
  }
let unfused = (nums)
  |> par-map --jobs=8 { |n|
    {bucket: if n % 4 == 0 { \"a\" } else if n % 4 == 1 { \"b\" } else if n % 4 == 2 { \"c\" } else { \"d\" }, doubled: n * 2, count: 1}
  }
  |> reduce-by --sum --jobs=1 { |row|
    {key: row.bucket, value: {count: row.count, total: row.doubled}}
  }
var ok = true
for k in unfused.keys() {
  let left = fused.get(k, {count: 0, total: 0})
  let right = unfused.get(k, {count: 0, total: 0})
  if left.count != right.count or left.total != right.total { ok = false }
}
let a = fused.get(\"a\", {count: 0, total: 0})
print f\"match=${ok} keys=${fused.keys().len()} a=${a.count}/${a.total}\"
",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "match=true keys=4 a=12500/624950000\n"
    );
}

#[test]
fn parallel_count_and_group_by_match_serial_including_order() {
    // count{block} and group-by parallelize by default above the threshold;
    // results must equal `--jobs=1` exactly — and group-by must preserve the
    // encounter order of items within each group (contiguous-chunk merge).
    let output = run_temp_script(
        "parallel-count-group",
        "\
let nums = [0] |> range(0, 20000)
let cpar = (nums) |> count { if . % 2 == 0 { \"even\" } else { \"odd\" } }
let cser = (nums) |> count --jobs=1 { if . % 2 == 0 { \"even\" } else { \"odd\" } }
let count_ok = cpar.get(\"even\", 0) == cser.get(\"even\", 0) and cpar.get(\"odd\", 0) == cser.get(\"odd\", 0)
let even = cpar.get(\"even\", 0)
let gpar = (nums) |> group-by { . % 3 } |> sort-by .key |> map { |g| g.items }
let gser = (nums) |> group-by --jobs=1 { . % 3 } |> sort-by .key |> map { |g| g.items }
let group_ok = gpar == gser
let groups = gpar.len()
print f\"count_ok=${count_ok} even=${even} group_ok=${group_ok} groups=${groups}\"
",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "count_ok=true even=10000 group_ok=true groups=3\n"
    );
}

#[test]
fn adapters_bridge_text_bytes_and_json_lines_explicitly() {
    let output = run_temp_script(
        "stream-adapters",
        "\
let captured = run.text printf \"%s\\n\" \"a.txt\" \"b.log\" ?
let paths = captured |> text.lines() |> map { |line| Path(line) }
let chunks = b\"abcde\" |> bytes.chunks(2)
let rows = \"{\\\"name\\\":\\\"a\\\",\\\"size\\\":1}\\n{\\\"name\\\":\\\"b\\\",\\\"size\\\":2}\\n\"
|> json.lines()
|> sort-by { .size }
let streamed = \"{\\\"name\\\":\\\"c\\\",\\\"size\\\":3}\\n\" |> json.stream()
let words = \"one two\".words()
print ${paths[0].ext} ${paths[1].name}
print ${chunks[0] == b\"ab\"} ${chunks[2] == b\"e\"}
print ${rows[1].name} ${rows[0].size}
print ${streamed[0].name} ${words[1]}
",
    );

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "txt b.log\ntrue true\nb 1\nc two\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn lines_methods_and_adapter_are_lazy_sources() {
    let root = temp_path("stream-lines-root");
    std::fs::create_dir_all(&root).expect("create stream lines root");
    let file = root.join("input.txt");
    std::fs::write(&file, "alpha\r\nbeta\ngamma\n").expect("write stream lines fixture");
    let source = format!(
        "\
let file_path = Path({})
let first = file_path.lines()? |> first()?
let second = \"one\\ntwo\\n\".lines() |> drop(1) |> first()?
let third = \"red\\nblue\\n\" |> text.lines() |> take(1)
let collected = \"x\\ny\\n\".lines().collect()
let byte_lines = b\"a\\nb\\n\".lines().collect()
let file_byte_lines = file_path.bytes_lines()?.collect()
print ${{first}} ${{second}} ${{third[0]}} ${{collected[1]}} ${{byte_lines.len()}} ${{file_byte_lines[1] == b\"beta\"}}
",
        xsh_string_literal(&file.to_string_lossy())
    );
    let output = run_temp_script("lines-methods", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "alpha two red y 2 true\n"
    );
}

#[test]
fn flat_map_consumes_live_streams_returned_by_blocks() {
    let root = temp_path("flat-map-live-stream-root");
    std::fs::create_dir_all(&root).expect("create flat-map root");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    std::fs::write(&left, "a\nb\n").expect("write left fixture");
    std::fs::write(&right, "c\nd\n").expect("write right fixture");
    let source = format!(
        "\
let paths = [Path({}), Path({})]
let lines = paths |> flat-map {{ |pth| pth.lines()? }}
print ${{lines.join(\",\")}}
",
        xsh_string_literal(&left.to_string_lossy()),
        xsh_string_literal(&right.to_string_lossy())
    );
    let output = run_temp_script("flat-map-live-stream", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "a,b,c,d\n");
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
fn sort_by_desc_reverses_sort_order() {
    let output = run_temp_script(
        "sort-by-desc",
        r#"
let nums = [3, 1, 4, 1, 5, 9, 2, 6]
let asc = nums |> sort-by .
let desc = nums |> sort-by --desc .
print ${asc[0]} ${asc[7]}
print ${desc[0]} ${desc[7]}
let words = ["banana", "apple", "cherry"]
let desc_words = words |> sort-by --desc .
print ${desc_words[0]} ${desc_words[2]}
"#,
    );

    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "1 9\n9 1\ncherry apple\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
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
fn adapter_stream_stages_are_trace_observable() {
    let output = run_temp_script_with_args(
        "stream-adapter-trace",
        "\"a\\nb\\n\" |> text.lines()\n",
        ["--trace", "--raw"],
    );

    assert!(output.status.success());
    let trace = String::from_utf8(output.stderr).unwrap();
    assert!(trace.contains("kind=stream.stage.enter"));
    assert!(trace.contains("kind=stream.stage.exit"));
    assert!(trace.contains("name=\"text.lines\""));
    assert!(trace.contains("stage=b\"text.lines\""));
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
fn structured_stream_errors_include_stage_item_and_trace_context() {
    let output = run_temp_script_with_args(
        "stream-error",
        "\
let xs = [\"only\"]
let values = [1] |> map { |index| xs[index] }
print ${values[0]}
",
        ["--trace", "--raw"],
    );

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("stream stage `map` item 0 failed"));
    assert!(stderr.contains("index-out-of-range"));
    assert!(stderr.contains("kind=stream.item.error"));
    assert!(stderr.contains("item_index=0"));
}

#[test]
fn structured_stream_batch_count_and_argv_limits() {
    let output = run_temp_script(
        "stream-batch",
        "\
let by_count = [1, 2, 3, 4, 5] |> batch --count=2
print ${by_count[0][0]} ${by_count[0][1]} ${by_count[1][0]} ${by_count[2][0]}

let by_size = [Path(\"aaaa\"), Path(\"bbbb\"), Path(\"cccc\")]
|> batch --max-bytes=10
print ${by_size[0][0]} ${by_size[0][1]}
print ${by_size[1][0]}

[Path(\"one\"), Path(\"two\")] |> batch --max-argv |> each { |files|
  run true @files ?
}
print \"argv ok\"

let empty = [1] |> where { false } |> batch --count=2 |> count()
print ${empty}
",
    );

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "1 2 3 5\naaaa bbbb\ncccc\nargv ok\n0\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn batch_stream_stage_is_trace_observable() {
    let output = run_temp_script_with_args(
        "batch-trace",
        "[1, 2, 3] |> batch --count=2\n",
        ["--trace", "--raw"],
    );

    assert!(output.status.success());
    let trace = String::from_utf8(output.stderr).unwrap();
    assert!(trace.contains("kind=stream.stage.enter"));
    assert!(trace.contains("kind=stream.stage.exit"));
    assert!(trace.contains("name=\"batch\""));
    assert!(trace.contains("stage=b\"batch\""));
    assert!(trace.contains("item_count=3"));
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
fn parallel_stream_stages_are_bounded_and_deterministic() {
    let output = run_temp_script(
        "parallel-streams",
        "\
let values = [1, 2, 3, 4] |> par-map { |x| x * 2 }
print ${values[0]} ${values[1]} ${values[2]} ${values[3]}
[\"a\", \"b\"] |> each --jobs=2 { |x|
  print ${x}
}
",
    );

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "2 4 6 8\na\nb\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn parallel_stream_preserves_filtered_order() {
    let output = run_temp_script(
        "parallel-stream-filtered-order",
        "\
let values = [0, 1, 2, 3, 4, 5] |> where { |x| x >= 2 } |> par-map --jobs=3 { |x| x * 10 }
print ${values[0]} ${values[1]} ${values[2]} ${values[3]}
",
    );

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "20 30 40 50\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn parallel_stream_failures_are_aggregate_errors_with_item_trace() {
    let output = run_temp_script_with_args(
        "parallel-stream-error",
        "\
let xs = [\"only\"]
let values = [1, 2, 3] |> par-map --jobs=1 { |index| xs[index] }
",
        ["--trace", "--raw"],
    );

    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("par-map error"));
    assert!(stderr.contains("kind=parallel.job.start"));
    assert!(stderr.contains("kind=parallel.job.end"));
    assert!(stderr.contains("item_index=0"));
}

#[test]
fn parallel_stream_failure_with_idle_workers_exits() {
    let output = run_temp_script_with_args(
        "parallel-stream-error-idle-workers",
        "\
let xs = [\"only\"]
let values = [1, 2, 3] |> par-map --jobs=8 { |index| xs[index] }
",
        ["--trace", "--raw"],
    );

    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("par-map error"));
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
