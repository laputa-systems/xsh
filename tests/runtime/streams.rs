use super::common::*;

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
fn sigterm_cancels_traced_par_map_process_work_without_losing_trace_context() {
    let root = temp_path("cancel-parallel-stream-root");
    std::fs::create_dir_all(&root).unwrap();
    let ready = root.join("ready");
    let shell = "printf ready >> \"$1\"; sleep 10";
    let source = format!(
        "\
let ready = Path({})
[\"one\", \"two\"] |> par-map --jobs=2 {{ |item|
  let _status = run sh -c {} sh (ready) ?
  item
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
    assert!(stderr.contains("kind=parallel.job.start"), "{stderr}");
    assert!(stderr.contains("kind=parallel.job.end"), "{stderr}");
    assert!(stderr.contains("kind=stream.item.error"), "{stderr}");
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
