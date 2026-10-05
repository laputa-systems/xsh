use super::common::*;

#[test]
fn sigterm_cancels_traced_par_map_process_work_without_losing_trace_context() {
    let root = temp_path("cancel-parallel-stream-root");
    std::fs::create_dir_all(&root).unwrap();
    let ready = root.join("ready");
    let shell = "printf ready >> \"$1\"; sleep 10";
    let source = format!(
        "\
let ready = Path({})
let _ = [\"one\", \"two\"] |> par-map(jobs: 2) {{ |item|
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
    assert!(!stderr.contains("return-outside-function"), "{stderr}");
    assert!(stderr.contains("kind=parallel.job.start"), "{stderr}");
    assert!(stderr.contains("kind=parallel.job.end"), "{stderr}");
    // The canceled run fails the callback's `?`, which propagates out of the
    // stage as an `Err`, as it would from `map`.
    assert!(stderr.contains("kind=result.propagate"), "{stderr}");
    assert!(!stderr.contains("kind=stream.item.error"), "{stderr}");
    let _ = std::fs::remove_dir_all(root);
}
