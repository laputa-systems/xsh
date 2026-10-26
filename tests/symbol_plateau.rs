use std::io::Write;
use std::path::Path;

use xsh::execution::script::{RunOptions, run_script};
use xsh::frontend::symbols::dynamic_symbol_stats;

#[cfg(target_os = "linux")]
#[macro_use]
mod test_binary;

fn write_script(dir: &Path, name: &str, source: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).expect("create script");
    file.write_all(source.as_bytes()).expect("write script");
    path
}

#[test]
fn repeated_preparation_returns_to_a_stable_symbol_plateau() {
    let dir = std::env::temp_dir().join(format!("xsh-symbol-plateau-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let script = write_script(
        &dir,
        "plateau.xsh",
        "proc main() [io] {\n  print shlex.quote(\"a b\")\n  print bytes.human(2048)\n}\n",
    );

    // Warm up so one-time process state is not counted as growth.
    for _ in 0..3 {
        let output = run_script(RunOptions {
            script: script.display().to_string(),
            args: Vec::new(),
            coverage_trace_dir: None,
        });
        assert_eq!(output.status, 0);
    }
    let (baseline, _) = dynamic_symbol_stats();

    for _ in 0..20 {
        let output = run_script(RunOptions {
            script: script.display().to_string(),
            args: Vec::new(),
            coverage_trace_dir: None,
        });
        assert_eq!(output.status, 0);
    }
    let (after, _) = dynamic_symbol_stats();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        after <= baseline,
        "twenty prepared runs left {after} live dynamic symbols against a plateau of {baseline}"
    );
}

#[cfg(target_os = "linux")]
mod streaming_keys {
    use std::fs::{self, File};
    use std::io::{BufWriter, Write};
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    struct Running(Child);

    impl Drop for Running {
        fn drop(&mut self) {
            if self.0.try_wait().expect("poll owned streaming child").is_none() {
                self.0.kill().expect("kill owned streaming child");
            }
            self.0.wait().expect("reap owned streaming child");
        }
    }

    fn write_records(path: &Path, distinct: bool) {
        let mut output = BufWriter::new(File::create(path).expect("create JSON Lines input"));
        let prefix = "k".repeat(595);
        for index in 0..50_000 {
            let suffix = if distinct { index } else { 0 };
            writeln!(output, "{{\"{prefix}{suffix:05}\":0}}")
                .expect("write JSON Lines record");
        }
        output.flush().expect("flush JSON Lines input");
    }

    fn peak_rss(directory: &Path, script: &Path, input: &Path, name: &str) -> u64 {
        let stdout = directory.join(format!("{name}.stdout"));
        let stderr = directory.join(format!("{name}.stderr"));
        let mut child = Running(Command::new(test_bin!("xsh"))
            .arg(script)
            .arg(input)
            .stdin(Stdio::piped())
            .stdout(File::create(&stdout).expect("create child stdout"))
            .stderr(File::create(&stderr).expect("create child stderr"))
            .spawn()
            .expect("spawn streaming child"));
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let output = fs::read(&stdout).expect("read child readiness");
            if output == b"READY 50000\n" {
                break;
            }
            assert!(child.0.try_wait().expect("poll streaming child").is_none(),
                "streaming child exited before READY: {}", String::from_utf8_lossy(&fs::read(&stderr).unwrap()));
            assert!(Instant::now() < deadline, "streaming child did not reach READY");
            std::thread::sleep(Duration::from_millis(10));
        }
        let status = fs::read_to_string(format!("/proc/{}/status", child.0.id()))
            .expect("read streaming child memory");
        let peak_kib: u64 = status.lines()
            .find_map(|line| line.strip_prefix("VmHWM:"))
            .expect("child VmHWM")
            .split_whitespace()
            .next()
            .expect("child VmHWM value")
            .parse()
            .expect("numeric child VmHWM");
        child.0.stdin.take().expect("child stdin")
            .write_all(b"continue\n").expect("acknowledge memory sample");
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.0.try_wait().expect("poll acknowledged child") {
                break status;
            }
            assert!(Instant::now() < deadline, "acknowledged child did not exit");
            std::thread::sleep(Duration::from_millis(10));
        };
        let errors = fs::read(&stderr).expect("read child stderr");
        assert!(status.success(), "streaming child failed: {}", String::from_utf8_lossy(&errors));
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        assert_eq!(fs::read(&stdout).expect("read child stdout"), b"READY 50000\nDONE 50000\n");
        peak_kib * 1024
    }

    #[test]
    fn fresh_streaming_record_keys_do_not_accumulate_for_the_program_lifetime() {
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        fs::create_dir_all(&scratch).expect("create repository scratch");
        let directory = tempfile::TempDir::new_in(scratch).expect("create streaming test directory");
        let repeated = directory.path().join("repeated.jsonl");
        let distinct = directory.path().join("distinct.jsonl");
        write_records(&repeated, false);
        write_records(&distinct, true);
        assert_eq!(fs::metadata(&repeated).unwrap().len(), fs::metadata(&distinct).unwrap().len());
        let script = super::write_script(directory.path(), "stream.xsh", r#"cli main(input: Path) {
  var count = 0
  for item in codec.read_all("json", input)? {
    let decoded = item?
    count = count + 1
  }
  print --flush f"READY {count}"
  assert io.stdin_line()? == "continue"
  print f"DONE {count}"
}
"#);
        let repeated_peak = peak_rss(directory.path(), &script, &repeated, "repeated");
        let distinct_peak = peak_rss(directory.path(), &script, &distinct, "distinct");
        assert!(distinct_peak <= repeated_peak + 16 * 1024 * 1024,
            "fresh keys retained memory after their records dropped: repeated {repeated_peak}, distinct {distinct_peak}");
    }
}
