use super::common::*;

#[test]
fn fs_walk_streams_lazily_and_short_circuits_take_first_any_and_break() {
    // A flat directory of 50 files. A lazy `fs.walk` must stop pulling entries
    // as soon as the consumer is satisfied, so a `take(3)`/`first`/`any`/`break`
    // touches only a handful of entries rather than the whole tree.
    let root = temp_path("fs-walk-lazy-root");
    std::fs::create_dir_all(&root).expect("create lazy walk root");
    for index in 0..50 {
        std::fs::write(root.join(format!("f{index:02}.txt")), "x").expect("write file");
    }
    let source = format!(
        "\
let root = Path({})

var pulled = 0
let first3 = fs.walk(root)
|> tee {{ |entry| pulled = pulled + 1 }}
|> where .kind == \"file\"
|> take(3)
|> map .name
print f\"take ok=${{pulled < 50 and first3.len() == 3}}\"

var pulled_any = 0
let any_file = fs.walk(root)
|> tee {{ |entry| pulled_any = pulled_any + 1 }}
|> any .kind == \"file\"
print f\"any ok=${{pulled_any < 50 and any_file}}\"

var pulled_break = 0
for entry in fs.walk(root) |> where .kind == \"file\" {{
  pulled_break = pulled_break + 1
  if pulled_break >= 2 {{ break }}
}}
print f\"break seen=${{pulled_break}}\"

let total = fs.walk(root) |> where .kind == \"file\" |> count()
print f\"total=${{total}}\"
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("fs-walk-lazy", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "take ok=true\n\
any ok=true\n\
break seen=2\n\
total=50\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Reading a map must not copy all of its entries.
///
/// `xsh-runtime-stats` owns the counting allocator, so this Rust harness runs
/// the disk-backed XSH fixture at two sizes and read-pass counts.
#[test]
fn map_reads_do_not_copy_the_map() {
    let fixture = Path::new(cargo_env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/runtime/map-read-scaling.xsh");
    let stats = cargo_env!("CARGO_BIN_EXE_xsh-runtime-stats");

    let run = |fields: usize, passes: usize, tag: &str| -> (u64, String) {
        let report = temp_path(&format!("map-read-scaling-{tag}")).with_extension("json");
        let output = std::process::Command::new(stats)
            .args([
                "--json",
                report.to_str().unwrap(),
                fixture.to_str().unwrap(),
            ])
            .env("XSH_MAP_READ_FIELDS", fields.to_string())
            .env("XSH_MAP_READ_PASSES", passes.to_string())
            .output()
            .expect("run xsh-runtime-stats over the record read fixture");
        assert!(
            output.status.success(),
            "fields={fields} passes={passes} stderr: {}",
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
    };

    // The extra sixteen passes isolate the cost of `16 * fields` reads.
    let small_once = run(64, 1, "64-1");
    let small_many = run(64, 17, "64-17");
    let large_once = run(1024, 1, "1024-1");
    let large_many = run(1024, 17, "1024-17");

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
