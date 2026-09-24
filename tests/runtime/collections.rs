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

/// Measure a complete accumulation with the counting allocator. The map
/// fixture uses zero read passes so the result covers construction only.
fn accumulation_traffic(collection: &str, size: usize) -> (u64, u64, String) {
    let fixture_name = match collection {
        "list" => "list-append-scaling",
        "map" => "map-read-scaling",
        _ => unreachable!("known accumulation fixture"),
    };
    let fixture = Path::new(cargo_env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/runtime/{fixture_name}.xsh"));
    let report = temp_path(&format!("{collection}-accumulation-{size}"))
        .with_extension("json");
    let mut command = std::process::Command::new(cargo_env!("CARGO_BIN_EXE_xsh-runtime-stats"));
    command.args(["--json", report.to_str().unwrap(), fixture.to_str().unwrap()]);
    match collection {
        "list" => {
            command.env("XSH_LIST_APPEND_SIZE", size.to_string());
        }
        "map" => {
            command
                .env("XSH_MAP_READ_FIELDS", size.to_string())
                .env("XSH_MAP_READ_PASSES", "0");
        }
        _ => unreachable!("known accumulation fixture"),
    }
    let output = command.output().expect("run accumulation fixture");
    assert!(
        output.status.success(),
        "{collection} size={size} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report_text = std::fs::read_to_string(&report).expect("read runtime stats report");
    let report_json = json_parse(&report_text);
    let execution = json_field(&report_json, "execution");
    let allocation_count = json_u64(json_field(execution, "alloc_count"));
    let allocated_bytes = json_u64(json_field(execution, "alloc_bytes"));
    let _ = std::fs::remove_file(&report);
    (
        allocation_count,
        allocated_bytes,
        String::from_utf8(output.stdout).expect("fixture stdout is UTF-8"),
    )
}

/// Sixteen times as many appended items or inserted map entries must not
/// multiply allocation traffic quadratically. Alias behavior is covered by
/// native tests in `tests/xsh/stdlib/methods.xsh` and `map.xsh`.
#[test]
fn list_and_map_accumulation_allocation_traffic_is_not_quadratic() {
    for collection in ["list", "map"] {
        let small = accumulation_traffic(collection, 128);
        let large = accumulation_traffic(collection, 2048);
        let expected_small = if collection == "list" { "128 127\n" } else { "128 0 0\n" };
        let expected_large = if collection == "list" { "2048 2047\n" } else { "2048 0 0\n" };
        assert_eq!(small.2, expected_small);
        assert_eq!(large.2, expected_large);

        let count_ratio = large.0 as f64 / small.0 as f64;
        let bytes_ratio = large.1 as f64 / small.1 as f64;
        assert!(
            count_ratio < 40.0 && bytes_ratio < 40.0,
            "{collection} accumulation traffic grew too quickly: \
             allocation count {count_ratio:.1}x, bytes {bytes_ratio:.1}x"
        );
    }
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
