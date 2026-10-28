use super::common::*;

const OFFSET_SECONDS: i64 = 1_000_000;

#[cfg(target_os = "linux")]
#[test]
fn boottime_follows_a_time_namespace_offset_while_wall_time_stays_put() {
    let path = write_temp_script(
        "boottime-timens",
        "print ${time.boottime()?} ${time.now()}\n",
    );
    let baseline = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&path)
        .output()
        .expect("run baseline");
    assert!(baseline.status.success(), "{:?}", baseline);
    let shifted = Command::new("unshare")
        .args(["--user", "--map-root-user", "--time", &format!("--boottime={OFFSET_SECONDS}"), "--fork"])
        .arg(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&path)
        .output()
        .expect("run unshare");
    let _ = std::fs::remove_file(&path);
    if !shifted.status.success() {
        let stderr = String::from_utf8_lossy(&shifted.stderr);
        if stderr.contains("unshare failed") {
            eprintln!("time namespace unavailable: {stderr}");
            return;
        }
        panic!("{:?}", shifted);
    }
    let parse = |output: &std::process::Output| -> (i64, i64) {
        let text = String::from_utf8(output.stdout.clone()).expect("utf-8 output");
        let mut fields = text.split_whitespace().map(|field| field.parse::<i64>().expect("integer"));
        (fields.next().unwrap(), fields.next().unwrap())
    };
    let (base_boot, base_wall) = parse(&baseline);
    let (shifted_boot, shifted_wall) = parse(&shifted);
    let boot_shift = shifted_boot - base_boot;
    assert!(
        (boot_shift - OFFSET_SECONDS * 1_000_000_000).abs() < 5_000_000_000,
        "boottime moved by {boot_shift} ns"
    );
    assert!((shifted_wall - base_wall).abs() < 5_000, "wall time moved by {} ms", shifted_wall - base_wall);
}
