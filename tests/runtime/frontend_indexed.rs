#[test]
fn stream_producers_are_lazy_and_stop_where_the_consumer_stops() {
    // The declared `stream` contract at its boundaries: the call does not run
    // the body, a pull runs it to the next `yield`, a consumer that stops early
    // leaves the later rows unexecuted, and every way a producer can end runs
    // its `defer` exactly once.
    let dir = std::env::temp_dir().join(format!(
        "xsh-lazy-stream-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let run = |expect_failure: bool| {
        std::process::Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
            .arg("tests/fixtures/runtime/lazy-stream-producers.xsh")
            .env("XSH_LAZY_STREAM_DIR", &dir)
            .env(
                "XSH_LAZY_STREAM_EXPECT_FAILURE",
                if expect_failure { "1" } else { "0" },
            )
            .output()
            .expect("run the lazy producer fixture")
    };

    let output = run(false);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            // The wrapper read its text at the call; the body had not run.
            "body started at call=false\n",
            // The rows come from the text the call retained, not from the file.
            "rows=3 values=3\n",
            // An early stop ran one row and the defer; the later rows never ran.
            "first=3 rows=1 closed=true\n",
            // A body that never started has no defers to run.
            "never started closed=false other=3\n",
            // Returning out of the consuming loop stops the producer there.
            "return stopped=3 closed=true\n",
            // `take` keeps its items and stops; `break` stops on its item.
            "taken=2 rows=2 closed=true\n",
            "break item=3\n",
            "break rows=1 closed=true\n",
            // The malformed third row is not reached by the early stop.
            "checked first=3 rows=1\n",
            // Unreadable text fails at the call, before any row is interpreted.
            "missing input rejected\n",
        )
    );
    assert!(!dir.join("closed-dropped").exists());

    // Consuming past the malformed row fails the consumer with the failure the
    // row declared, and the producer's defer still ran exactly once.
    let output = run(true);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("RowError.Malformed"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.join("closed-failing").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn zero_argument_stream_producers_run_from_every_call_position() {
    // The frame engine resolves a call with no arguments without walking an
    // argument list, so it has to make the same producer decision the argument
    // path makes. When it did not, the body ran with nowhere for `yield` to
    // report and the call failed as a function that did not return.
    let log = std::env::temp_dir().join(format!(
        "xsh-zero-argument-stream-{}-{:?}.log",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let output = std::process::Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg("tests/fixtures/runtime/zero-argument-stream.xsh")
        .env("XSH_ZERO_ARGUMENT_STREAM_LOG", &log)
        .output()
        .expect("run the zero-argument producer fixture");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Direct `for`, a binding, a call from inside a proc's loop, a call from
    // inside another producer, two pipeline stages, and a bounded terminal
    // that stops the producer early — which still runs its `defer`.
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "direct=3\nbound=2\ntotal=3\ndoubled=2\nmapped=2\ntaken=1\nfirst=1\nstopped=1\nclosed\n"
    );
    assert!(output.stderr.is_empty());
    let _ = std::fs::remove_file(&log);
}

#[test]
fn worker_stages_consume_a_producer_through_the_producer_machinery() {
    // The worker stages are consumers too: `par-map` and the fused
    // `par-map | flat-map | reduce-by` path pull a producer's rows through the
    // same machinery as any other consumer, so the body runs at consumption,
    // the stage maps the rows the body yielded, a mid-stream failure is the
    // failure the row declared, and the producer's `defer` runs exactly once
    // on every path.
    let dir = std::env::temp_dir().join(format!(
        "xsh-worker-stage-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let run = |expect_failure: bool| {
        std::process::Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
            .arg("tests/fixtures/runtime/worker-stage-producers.xsh")
            .env("XSH_WORKER_STAGE_DIR", &dir)
            .env(
                "XSH_WORKER_STAGE_EXPECT_FAILURE",
                if expect_failure { "1" } else { "0" },
            )
            .output()
            .expect("run the worker-stage producer fixture")
    };

    let output = run(false);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            // The stage mapped every row the producer yielded, and the defer ran.
            "mapped=3 first=6 rows=3 closed=true\n",
            // A bounded terminal after the stage: the stage consumed the
            // producer, and it was still stopped exactly once.
            "taken=6 rows=3 closed=true\n",
            // The fused worker path: three rows of 3, 3, and 5 bytes.
            "fused=11 rows=3 closed=true\n",
        )
    );

    // Consuming past the malformed row fails the stage with the failure the row
    // declared; the producer stopped at that row, and its defer ran once.
    let output = run(true);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("RowError.Bad"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.join("closed-check").exists());
    assert!(!dir.join("row-check-three").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
