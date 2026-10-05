use super::common::*;

// Copying both executables into an isolated directory exercises their embedded
// linkage and process output; a native script cannot relocate either product.
#[test]
fn copied_products_check_and_run_script_backed_calls_in_static_and_loaded_modules() {
    let root = temp_path("copied-products-stdlib-linkage");
    std::fs::create_dir_all(&root).expect("create isolated script root");
    let xsh = root.join("xsh");
    let xsht = root.join("xsht");
    std::fs::copy(cargo_env!("CARGO_BIN_EXE_xsh"), &xsh).expect("copy xsh");
    std::fs::copy(cargo_env!("CARGO_BIN_EXE_xsht"), &xsht).expect("copy xsht");
    std::fs::write(
        root.join("helper.xsh"),
        "##! Static helper.\n## Return a terminal sequence.\nexport pure color() -> Str { return tui.red() }\n",
    )
    .expect("write static module");
    std::fs::write(
        root.join("dynamic.xsh"),
        "##! Dynamic helper.\n## Return a terminal sequence.\nexport pure color() -> Str { return tui.bold() }\n",
    )
    .expect("write loaded module");
    std::fs::write(
        root.join("main.xsh"),
        "use helper\ntype Loaded = module { export pure color() -> Str }\nproc main() [fs, io, error] {\n  let loaded = module.load(p\"dynamic.xsh\")?.require(Loaded)?\n  let both = helper.color() + loaded.color()\n  print $both\n}\n",
    )
    .expect("write entry script");

    let checked = Command::new(&xsht)
        .args(["check", "main.xsh"])
        .current_dir(&root)
        .env_remove("XSH_MODULE_PATH")
        .output()
        .expect("check with copied xsht");
    assert_eq!(
        checked.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(checked.stdout.is_empty());

    let executed = Command::new(&xsh)
        .arg("main.xsh")
        .current_dir(&root)
        .env_remove("XSH_MODULE_PATH")
        .output()
        .expect("run with copied xsh");
    assert_eq!(
        executed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    assert_eq!(executed.stdout, b"\x1b[31m\x1b[1m\n");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn runtime_stats_preserves_parallel_script_output_and_reports_worker_traffic() {
    let script = write_temp_script(
        "runtime-stats-par-map",
        r#"
let values = [0] |> range(0, 100)
let total = values |> par-map(jobs: 2) { |value| value * 2 } |> sum
print $total
"#,
    );
    let report = temp_path("runtime-stats-par-map-report").with_extension("json");
    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
        .args([
            "runtime-stats",
            "--json",
            report.to_str().unwrap(),
            script.to_str().unwrap(),
        ])
        .output()
        .expect("run xsht runtime-stats");

    assert!(output.status.success());
    assert_eq!(output.stdout, b"9900\n");
    assert_eq!(output.stderr, b"");
    let report_text = std::fs::read_to_string(&report).expect("read runtime stats report");
    let report_json = json_parse(&report_text);
    assert!(json_bool(json_field(&report_json, "tracking_active")));
    let workers = json_field(&report_json, "workers");
    assert_eq!(json_u64(json_field(workers, "stage_count")), 2);
    assert!(json_u64(json_field(workers, "alloc_count")) > 0);
    assert!(json_u64(json_field(workers, "alloc_bytes")) > 0);
    let attributions = json_array(json_field(workers, "attributions"));
    assert_eq!(attributions.len(), 4);
    let item = attributions
        .iter()
        .find(|attribution| json_str(json_field(attribution, "scope")) == "par_map_item")
        .expect("par-map attribution");
    assert!(json_u64(json_field(item, "alloc_count")) > 0);
    assert!(json_u64(json_field(item, "alloc_bytes")) > 0);

    std::fs::remove_file(script).expect("remove runtime stats script");
    std::fs::remove_file(report).expect("remove runtime stats report");
}