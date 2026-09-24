use super::common::*;

// `xsh-runtime-stats` owns the counting allocator. The disk-backed fixture
// separates construction from read passes; subtracting one pass from seventeen
// isolates repeated reads at the same collection size.
fn read_scaling_allocations(
    collection: &str,
    fields: usize,
    passes: usize,
) -> (u64, String) {
    let fixture = Path::new(cargo_env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/runtime/{collection}-read-scaling.xsh"));
    let stats = cargo_env!("CARGO_BIN_EXE_xsh-runtime-stats");
    let report = temp_path(&format!("{collection}-read-scaling-{fields}-{passes}"))
        .with_extension("json");
    let output = std::process::Command::new(stats)
        .args([
            "--json",
            report.to_str().unwrap(),
            fixture.to_str().unwrap(),
        ])
        .env(format!("XSH_{}_READ_FIELDS", collection.to_uppercase()), fields.to_string())
        .env(format!("XSH_{}_READ_PASSES", collection.to_uppercase()), passes.to_string())
        .output()
        .expect("run xsh-runtime-stats over the collection read fixture");
    assert!(
        output.status.success(),
        "{collection} fields={fields} passes={passes} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report_text = std::fs::read_to_string(&report).expect("read runtime stats report");
    let report_json = json_parse(&report_text);
    let allocations = json_u64(json_field(
        json_field(&report_json, "execution"),
        "alloc_count",
    ));
    let _ = std::fs::remove_file(&report);
    (
        allocations,
        String::from_utf8(output.stdout).expect("fixture stdout is UTF-8"),
    )
}

/// Reading a map must not copy all of its entries.
///
/// `xsh-runtime-stats` runs the disk-backed XSH fixture at two sizes and read-pass counts.
#[test]
fn map_reads_do_not_copy_the_map() {
    let run = |fields, passes| read_scaling_allocations("map", fields, passes);

    // The extra sixteen passes isolate the cost of `16 * fields` reads.
    let small_once = run(64, 1);
    let small_many = run(64, 17);
    let large_once = run(1024, 1);
    let large_many = run(1024, 17);

    assert_eq!(small_once.1, "64 1 2016\n");
    assert_eq!(small_many.1, "64 17 34272\n");
    assert_eq!(large_once.1, "1024 1 523776\n");
    assert_eq!(large_many.1, "1024 17 8904192\n");

    let per_read =
        |once: u64, many: u64, fields: u64| (many - once) as f64 / (16.0 * fields as f64);
    let small_per_read = per_read(small_once.0, small_many.0, 64);
    let large_per_read = per_read(large_once.0, large_many.0, 1024);

    // A full copy per read would grow roughly sixteenfold with the map size.
    assert!(
        small_per_read < 8.0 && large_per_read < 8.0,
        "per-read allocations: 64-key {small_per_read:.2}, 1024-key {large_per_read:.2}"
    );
    assert!(
        large_per_read <= small_per_read * 2.0 + 1.0,
        "per-read allocations grew with container size: \
         64-key {small_per_read:.2}, 1024-key {large_per_read:.2}"
    );

    // A full copy per `set` would make construction quadratic.
    let construction_ratio = large_once.0 as f64 / small_once.0 as f64;
    assert!(
        construction_ratio < 40.0,
        "constructing a 16x larger map cost {construction_ratio:.1}x the allocations"
    );
}

/// Repeated dynamic `Record` field reads must not copy the other fields.
#[test]
fn record_reads_do_not_copy_the_record() {
    let run = |fields, passes| read_scaling_allocations("record", fields, passes);
    let small_once = run(64, 1);
    let small_many = run(64, 17);
    let large_once = run(1024, 1);
    let large_many = run(1024, 17);

    assert_eq!(small_once.1, "64 1 2016\n");
    assert_eq!(small_many.1, "64 17 34272\n");
    assert_eq!(large_once.1, "1024 1 523776\n");
    assert_eq!(large_many.1, "1024 17 8904192\n");

    let per_read =
        |once: u64, many: u64, fields: u64| (many - once) as f64 / (16.0 * fields as f64);
    let small_per_read = per_read(small_once.0, small_many.0, 64);
    let large_per_read = per_read(large_once.0, large_many.0, 1024);
    assert!(
        small_per_read < 16.0 && large_per_read < 16.0,
        "per-read allocations: 64-field {small_per_read:.2}, 1024-field {large_per_read:.2}"
    );
    assert!(
        large_per_read <= small_per_read * 2.0 + 1.0,
        "per-read allocations grew with record size: \
         64-field {small_per_read:.2}, 1024-field {large_per_read:.2}"
    );
}
