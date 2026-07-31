use super::common::*;

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
fn par_map_filesystem_reads_preserve_all_results() {
    let root = temp_path("par-map-filesystem-reads");
    std::fs::create_dir_all(&root).expect("create par-map filesystem root");
    for index in 0..32 {
        std::fs::write(root.join(format!("entry-{index:02}.txt")), format!("{index}\n"))
            .expect("write par-map filesystem entry");
    }
    let source = format!(
        "\
let root = Path({})
let entries = fs.files(root, stat: false)? |> collect()
let lengths = entries |> par-map --jobs=8 {{ |entry|
  entry.path.read_text()?.count_chars()
}}
print f\"count=${{lengths.len()}} total=${{lengths |> sum()}}\"
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("par-map-filesystem-reads", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "count=32 total=86\n"
    );
    let _ = std::fs::remove_dir_all(root);
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
