#![allow(clippy::single_call_fn)]

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn perf_scenarios_run_on_realistic_corpus() {
    let root = temp_root("corpus");
    make_corpus(&root, 3);

    let extension_count = run_xsh("perf/scenarios/extension-count.xsh", &root);
    assert_success(&extension_count, "extension-count");
    assert_eq!(
        String::from_utf8(extension_count.stdout).unwrap(),
        expected_extension_counts(&root)
    );

    let manifest = run_xsh("perf/scenarios/manifest-hash.xsh", &root);
    assert_success(&manifest, "manifest-hash");
    assert_eq!(
        String::from_utf8(manifest.stdout).unwrap(),
        expected_manifest_summary(&root)
    );

    let logs = run_xsh("perf/scenarios/json-log-rollup.xsh", &root);
    assert_success(&logs, "json-log-rollup");
    assert_eq!(
        String::from_utf8(logs.stdout).unwrap(),
        expected_log_rollup(&root)
    );

    let archive = run_xsh("perf/scenarios/archive-package.xsh", &root);
    assert_success(&archive, "archive-package");
    let archive_stdout = String::from_utf8(archive.stdout).unwrap();
    let archive_fields = archive_stdout.split_whitespace().collect::<Vec<_>>();
    assert_eq!(archive_fields.len(), 3, "{archive_stdout}");
    assert!(archive_fields[0].parse::<usize>().unwrap() >= 8);
    assert_eq!(archive_fields[1], "3");
    assert_eq!(archive_fields[2], payload_digest(&root));

    for (scenario, expected) in [
        ("value-churn", "76554\n"),
        ("record-stream", "56276\n"),
        ("stream-heavy", "4610\n"),
        ("parse-check-heavy", "50302\n"),
    ] {
        let output = run_xsh(&format!("perf/scenarios/{scenario}.xsh"), &root);
        assert_success(&output, scenario);
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    }

    let _ = fs::remove_dir_all(root);
}

#[cfg(feature = "perf-metrics")]
#[test]
fn xsh_perf_allocator_metrics_are_emitted_when_requested() {
    let root = temp_root("alloc");
    fs::create_dir_all(&root).expect("create temp root");
    let script = root.join("hello.xsh");
    fs::write(&script, "print \"ok\"\n").expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .env("XSH_PERF_ALLOC", "1")
        .arg(&script)
        .output()
        .expect("run xsh");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ok\n");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("xsh perf: allocation_calls="), "{stderr}");
    assert!(stderr.contains("allocation_bytes="), "{stderr}");

    let _ = fs::remove_dir_all(root);
}

fn make_corpus(root: &Path, scale: usize) {
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg("perf/make-corpus.xsh")
        .arg("--")
        .arg("--root")
        .arg(root)
        .arg("--scale")
        .arg(scale.to_string())
        .output()
        .expect("run corpus generator");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_xsh(script: &str, root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(script)
        .arg("--")
        .arg(root)
        .output()
        .expect("run xsh")
}

fn assert_success(output: &std::process::Output, name: &str) {
    assert!(
        output.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn expected_extension_counts(root: &Path) -> String {
    let mut counts = BTreeMap::<String, usize>::new();
    walk(root, &mut |path| {
        if path.is_file()
            && !relative_has_hidden_component(root, path)
            && let Some(extension) = path.extension().and_then(OsStr::to_str)
            && !extension.is_empty()
        {
            *counts.entry(extension.to_ascii_lowercase()).or_default() += 1;
        }
        Ok(())
    })
    .expect("walk corpus");

    let mut rows = counts.into_iter().collect::<Vec<_>>();
    rows.sort_by(|(left_ext, left_count), (right_ext, right_count)| {
        left_count
            .cmp(right_count)
            .then_with(|| left_ext.cmp(right_ext))
    });

    rows.into_iter()
        .map(|(extension, count)| format!("{} {extension}\n", count_prefix(count)))
        .collect()
}

fn expected_manifest_summary(root: &Path) -> String {
    let pkgroot = root.join("pkgroot");
    let mut rows = Vec::new();
    walk(&pkgroot, &mut |path| {
        if path.is_file() {
            let data = fs::read(path).expect("read manifest file");
            let relative = path
                .strip_prefix(&pkgroot)
                .expect("strip pkgroot")
                .to_string_lossy()
                .into_owned();
            rows.push((relative, data.len(), sha256_hex(&data)));
        }
        Ok(())
    })
    .expect("walk pkgroot");
    rows.sort_by(|left, right| left.0.cmp(&right.0));

    let total_size = rows.iter().map(|(_, size, _)| *size).sum::<usize>();
    let first = rows.first().expect("manifest row");
    format!("{} {total_size} {} {} 1\n", rows.len(), first.0, first.2)
}

fn expected_log_rollup(root: &Path) -> String {
    let mut totals = BTreeMap::<String, (usize, i64)>::new();
    let mut log_files = fs::read_dir(root.join("logs"))
        .expect("read logs")
        .map(|entry| entry.expect("read log entry").path())
        .collect::<Vec<_>>();
    log_files.sort();

    for path in log_files {
        let text = fs::read_to_string(path).expect("read log");
        for line in text.lines() {
            let service = json_field(line, "service");
            let level = json_field(line, "level");
            if level == "debug" {
                continue;
            }
            let duration_ms = json_int_field(line, "duration_ms");
            let entry = totals.entry(format!("{service}:{level}")).or_default();
            entry.0 += 1;
            entry.1 += duration_ms;
        }
    }

    totals
        .into_iter()
        .map(|(key, (count, duration_ms))| format!("{key} {count} {duration_ms}\n"))
        .collect()
}

fn payload_digest(root: &Path) -> String {
    sha256_hex(&fs::read(root.join("pkgroot/usr/share/demo/payload.txt")).expect("read payload"))
}

fn json_field(line: &str, field: &str) -> String {
    let needle = format!("\"{field}\":\"");
    let start = line.find(&needle).expect("field start") + needle.len();
    let end = line[start..].find('"').expect("field end") + start;
    line[start..end].to_string()
}

fn json_int_field(line: &str, field: &str) -> i64 {
    let needle = format!("\"{field}\":");
    let start = line.find(&needle).expect("field start") + needle.len();
    let end = line[start..]
        .find(|ch: char| !ch.is_ascii_digit())
        .map(|offset| start + offset)
        .unwrap_or(line.len());
    line[start..end].parse().expect("parse int field")
}

fn relative_has_hidden_component(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .expect("strip corpus root")
        .components()
        .any(|component| component.as_os_str().as_encoded_bytes().starts_with(b"."))
}

fn walk(path: &Path, visit: &mut dyn FnMut(&Path) -> io::Result<()>) -> io::Result<()> {
    visit(path)?;
    if path.is_dir() {
        let mut children = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        children.sort();
        for child in children {
            walk(&child, visit)?;
        }
    }
    Ok(())
}

fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn count_prefix(count: usize) -> String {
    if count < 10 {
        format!("   {count}")
    } else if count < 100 {
        format!("  {count}")
    } else if count < 1000 {
        format!(" {count}")
    } else {
        count.to_string()
    }
}

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("xsh-perf-{name}-{}-{nanos}", std::process::id()))
}
