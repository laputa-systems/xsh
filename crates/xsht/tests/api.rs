use std::io::Write;
use std::process::{Command, Stdio};

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn xsht(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(args)
        .current_dir(workspace_root())
        .output()
        .expect("run xsht")
}

#[test]
fn api_mixed_batch_preserves_query_order() {
    let output = xsht(&[
        "api",
        "api:json.read",
        "method:Path.read_text",
        "record:FsEntry",
        "language:run.status",
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("api: module.json.read"), "{stdout}");
    assert!(stdout.contains("api: method.Path.read_text"), "{stdout}");
    assert!(stdout.contains("api: record.FsEntry"), "{stdout}");
    assert!(stdout.contains("api: language.run.status"), "{stdout}");
    assert!(
        stdout.find("query: api:json.read") < stdout.find("query: method:Path.read_text")
            && stdout.find("query: method:Path.read_text") < stdout.find("query: record:FsEntry")
            && stdout.find("query: record:FsEntry") < stdout.find("query: language:run.status"),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_without_query_is_a_standalone_onboarding_guide() {
    let output = xsht(&["api"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    for fragment in [
        "XSH API getting started",
        "proc main(...argv: List[Str])",
        "xsht check hello.xsh",
        "xsht fmt hello.xsh",
        "xsht lint hello.xsh",
        "xsht api module:fs",
        "xsht api api:fs.read_text",
    ] {
        assert!(stdout.contains(fragment), "{stdout}");
    }
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_without_query_jsonl_is_a_valid_guide_object() {
    let output = xsht(&["api", "--format", "jsonl"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    let parsed = xsh::host::json::parse_raw_json(stdout.trim()).expect("parse guide JSON");
    assert_eq!(
        xsh::host::json::raw_json_get(&parsed, "kind").and_then(xsh::host::json::raw_json_as_str),
        Some("guide")
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_onboarding_script_passes_xsht_check() {
    let output = xsht(&["api"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let start = stdout.find("proc main(...argv: List[Str])").unwrap();
    let end = stdout[start..].find("\n\nBasic development loop:").unwrap() + start;
    let root = tempfile::tempdir().expect("tempdir");
    let script = root.path().join("hello.xsh");
    std::fs::write(&script, &stdout[start..end]).expect("write onboarding script");

    let checked = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", script.to_str().expect("script path")])
        .current_dir(workspace_root())
        .output()
        .expect("run xsht check");
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

#[test]
fn api_module_query_lists_the_module_and_its_members() {
    let output = xsht(&["api", "module:fs"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: matches"), "{stdout}");
    assert!(stdout.contains("api: module.fs\n"), "{stdout}");
    assert!(stdout.contains("api: module.fs.read_text\n"), "{stdout}");
    assert!(stdout.contains("purpose:"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_exact_item_explains_effects_and_contract() {
    let output = xsht(&["api", "api:fs.read_text"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("contract:"), "{stdout}");
    assert!(stdout.contains("effects: fs"), "{stdout}");
    assert!(stdout.contains("signature: fs.read_text"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_language_group_includes_the_language_contract() {
    let output = xsht(&["api", "language:effect"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: matches"), "{stdout}");
    assert!(stdout.contains("api: language.effect.fs"), "{stdout}");
    assert!(stdout.contains("contract:"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_print_builtin_is_indexed_with_signature_effects_and_example() {
    let output = xsht(&["api", "language:core.print"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: exact"), "{stdout}");
    assert!(stdout.contains("api: language.core.print"), "{stdout}");
    assert!(stdout.contains("effects: none"), "{stdout}");
    assert!(
        stdout.contains("signature: print [--flush] ARG..."),
        "{stdout}"
    );
    assert!(stdout.contains("separated by a single space"), "{stdout}");
    assert!(stdout.contains("expression string literals"), "{stdout}");
    assert!(stdout.contains("example:"), "{stdout}");
    assert!(stdout.contains("print \"hello\" $name"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_print_builtin_is_discoverable_by_search() {
    let output = xsht(&["api", "search:print"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: matches"), "{stdout}");
    assert!(stdout.contains("api: language.core.print"), "{stdout}");
    assert!(
        stdout.contains("Prints values to standard output."),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_print_builtin_is_found_by_output_and_builtin_terms() {
    let output = xsht(&["api", "search:builtin"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: matches"), "{stdout}");
    assert!(stdout.contains("api: language.core.print"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let output = xsht(&["api", "search:output"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: matches"), "{stdout}");
    assert!(stdout.contains("api: language.core.print"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_summary_reports_the_complete_queryable_surface() {
    let output = xsht(&["api", "summary"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.starts_with("XSH API summary\n"), "{stdout}");
    for label in [
        "standard modules:",
        "module functions:",
        "module overloads:",
        "method receivers:",
        "methods:",
        "method overloads:",
        "standard records:",
        "language reference items:",
        "total queryable items:",
        "documented items:",
    ] {
        assert!(stdout.contains(label), "{stdout}");
    }
    assert!(stdout.contains("\nmodules\n"), "{stdout}");
    for (module, signatures) in xsh::api::api_spec().module_entries() {
        assert!(stdout.contains(&format!("── {module} (")), "{stdout}");
        for function in &signatures.functions {
            assert!(
                stdout.contains(&format!("── {} (", function.name)),
                "{stdout}"
            );
        }
    }
    assert!(stdout.contains("\nmethods\n"), "{stdout}");
    assert!(stdout.contains("\nrecords\n"), "{stdout}");
    for record in xsh_registry::records::record_schemas().keys() {
        assert!(stdout.contains(&format!("── {record}\n")), "{stdout}");
    }
    assert!(stdout.contains("\nlanguage\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_summary_jsonl_is_one_structured_response() {
    let output = xsht(&["api", "summary", "--format", "jsonl"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    assert!(stdout.contains("\"kind\":\"summary\""), "{stdout}");
    assert!(stdout.contains("\"total_queryable_items\":"), "{stdout}");
    assert!(stdout.contains("\"documented_items\":"), "{stdout}");
    assert!(stdout.contains("\"modules\":["), "{stdout}");
    assert!(stdout.contains("\"method_receivers\":["), "{stdout}");
    assert!(stdout.contains("\"records\":["), "{stdout}");
    assert!(stdout.contains("\"language_groups\":["), "{stdout}");
    let parsed = xsh::host::json::parse_raw_json(stdout.trim()).expect("parse summary JSON");
    assert!(xsh::host::json::raw_json_get(&parsed, "modules").is_some());
    assert!(xsh::host::json::raw_json_get(&parsed, "method_receivers").is_some());
    assert!(xsh::host::json::raw_json_get(&parsed, "records").is_some());
    assert!(xsh::host::json::raw_json_get(&parsed, "language_groups").is_some());
}

#[test]
fn api_summary_rejects_selectors() {
    let output = xsht(&["api", "summary", "api:json.read"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cannot be combined with selectors"),
    );
}

#[test]
fn api_jsonl_has_one_response_per_selector() {
    let output = xsht(&[
        "api",
        "--format",
        "jsonl",
        "api:json.read",
        "language:effect.process",
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains("\"schema_version\":1"), "{}", lines[0]);
    assert!(
        lines[0].contains("\"query\":\"api:json.read\""),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].contains("\"query\":\"language:effect.process\""),
        "{}",
        lines[1]
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_strict_renders_all_queries_before_failing() {
    let output = xsht(&["api", "--strict", "api:json.read", "api:json.missing"]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("query: api:json.read\nstatus: exact"),
        "{stdout}"
    );
    assert!(
        stdout.contains("query: api:json.missing\nstatus: missing"),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_combines_query_file_and_argv_queries() {
    let root = tempfile::tempdir().expect("tempdir");
    let query_file = root.path().join("queries.txt");
    std::fs::write(&query_file, "api:json.read\nlanguage:effect.fs\n").expect("write query file");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "api",
            "--query-file",
            query_file.to_str().expect("query path"),
            "record:FsEntry",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("run xsht");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.find("query: record:FsEntry") < stdout.find("query: api:json.read")
            && stdout.find("query: api:json.read") < stdout.find("query: language:effect.fs"),
        "{stdout}"
    );
}

#[test]
fn api_inventory_is_standalone_and_documented() {
    let mut ids = Vec::new();
    for (id, docs) in xsh::api::api_spec().docs_entries() {
        ids.push(id.to_string());
        assert_documented(id, docs);
    }
    for name in xsh_registry::records::record_schemas().keys() {
        let id = format!("record.{name}");
        ids.push(id.clone());
        assert_documented(&id, &xsh_registry::signature::record_docs(name));
    }
    for reference in xsh_registry::reference::language_references() {
        let id = format!("language.{}", reference.id);
        ids.push(id.clone());
        assert_documented(&id, &reference.docs);
    }

    let mut sorted_ids = ids.clone();
    sorted_ids.sort();
    sorted_ids.dedup();
    assert_eq!(sorted_ids.len(), ids.len(), "API item IDs must be unique");
}

fn assert_documented(id: &str, docs: &xsh_registry::api_docs::ApiDocs) {
    assert!(!docs.summary.trim().is_empty(), "{id} has no purpose");
    assert!(
        docs.tags.iter().all(|tag| !tag.trim().is_empty()),
        "{id} has an empty tag"
    );
    if let Some(example) = &docs.example {
        assert!(!example.trim().is_empty(), "{id} has an empty example");
    }
}

#[test]
fn api_stdin_queries_join_argv_batch_in_request_order() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["api", "record:FsEntry", "--stdin"])
        .current_dir(workspace_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start xsht");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"api:json.read\nlanguage:effect.fs\n")
        .expect("write queries");
    let output = child.wait_with_output().expect("wait xsht");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.find("query: record:FsEntry") < stdout.find("query: api:json.read")
            && stdout.find("query: api:json.read") < stdout.find("query: language:effect.fs"),
        "{stdout}"
    );
}

#[test]
fn api_search_is_local_and_deterministic() {
    let output = xsht(&["api", "search:rooted"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: matches"), "{stdout}");
    assert!(
        stdout.contains("api: module.archive.tar_create"),
        "{stdout}"
    );
    assert!(stdout.contains("api: module.patch.apply"), "{stdout}");
}

#[test]
fn api_defaulted_parameters_explain_positional_only_calls() {
    let output = xsht(&["api", "api:fs.files"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Function arguments are positional-only; parameters marked `= default` may be omitted, but cannot be supplied as `name = value`."),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_stream_sort_by_shows_options_before_block() {
    let output = xsht(&["api", "language:stream.sort-by"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: exact"), "{stdout}");
    assert!(
        stdout.contains("signature: sort-by(--desc: Bool = false, block) -> Stream[T]"),
        "{stdout}"
    );
    assert!(
        stdout.contains("|> sort-by --desc { |e| e.size }"),
        "{stdout}"
    );
    assert!(!stdout.contains("sort-by(--desc, { |e| e.size })"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_stream_stage_group_by_shows_signature_and_record_shape() {
    let output = xsht(&["api", "language:stream.group-by"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: exact"), "{stdout}");
    assert!(stdout.contains("api: language.stream.group-by"), "{stdout}");
    assert!(stdout.contains("signature: "), "{stdout}");
    assert!(stdout.contains("Stream[{key, items: List[T]}]"), "{stdout}");
    assert!(stdout.contains("items"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_stream_stages_carry_a_signature_in_jsonl() {
    let output = xsht(&[
        "api",
        "--format",
        "jsonl",
        "language:stream.map",
        "language:stream.where",
        "language:stream.sort-by",
        "language:stream.fold",
        "language:stream.each",
        "language:stream.collect",
        "language:stream.unique-by",
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 7, "{lines:?}");
    for query in [
        "map",
        "where",
        "sort-by",
        "fold",
        "each",
        "collect",
        "unique-by",
    ] {
        let id = format!("language:stream.{query}");
        let line = lines
            .iter()
            .find(|line| line.contains(&format!("\"query\":\"{id}\"")))
            .unwrap_or_else(|| panic!("missing {id} in {lines:?}"));
        assert!(
            !line.contains("\"signatures\":[]"),
            "{id} has an empty signature list: {line}"
        );
        assert!(line.contains("\"signatures\":["), "{id}: {line}");
    }
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_module_member_text_shows_the_signature() {
    let output = xsht(&["api", "module:tui.left_pad"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: exact"), "{stdout}");
    assert!(stdout.contains("api: module.tui.left_pad"), "{stdout}");
    assert!(
        stdout.contains("signature: tui.left_pad(text: Str, width: Int) -> Str"),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_module_member_jsonl_matches_text_signature() {
    let output = xsht(&["api", "--format", "jsonl", "module:tui.left_pad"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("\"signatures\":["), "{stdout}");
    assert!(
        stdout.contains("tui.left_pad(text: Str, width: Int) -> Str"),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_module_overview_stays_concise() {
    let output = xsht(&["api", "module:env"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: matches"), "{stdout}");
    assert!(stdout.contains("api: module.env\n"), "{stdout}");
    // An overview lists members by purpose, not by dumping every signature.
    assert!(!stdout.contains("signature: env."), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_method_receiver_query_lists_every_method_of_a_type() {
    let output = xsht(&["api", "method:Str"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: matches"), "{stdout}");
    // A bare receiver query lists the receiver's methods by id without error.
    assert!(stdout.contains("api: method.Str.lower\n"), "{stdout}");
    assert!(stdout.contains("purpose:"), "{stdout}");
    // Like a module overview, a receiver overview stays concise: no full signature dump.
    assert!(!stdout.contains("signature: Str.lower"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_map_receiver_query_discloses_its_constructor() {
    let output = xsht(&["api", "method:Map"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("api: method.Map.constructor\n"), "{stdout}");
    assert!(stdout.contains("map.empty()"), "{stdout}");
    assert!(stdout.contains("`{}` is an empty Record"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_map_summary_discloses_its_constructor() {
    let output = xsht(&["api", "summary"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let map = stdout.find("── Map (").expect("Map receiver in summary");
    let tail = &stdout[map..];
    assert!(tail.contains("module.map.empty"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_method_receiver_query_keeps_exact_member_lookup() {
    let output = xsht(&["api", "method:Str.lower"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: exact"), "{stdout}");
    assert!(stdout.contains("api: method.Str.lower\n"), "{stdout}");
    assert!(stdout.contains("contract:"), "{stdout}");
    assert!(stdout.contains("signature: Str.lower"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_method_receiver_works_for_path_constructor_receiver() {
    // The Path constructor receiver shares the "Path" receiver name, so a bare
    // receiver query lists its methods alongside the path methods.
    let output = xsht(&["api", "method:Path"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: matches"), "{stdout}");
    assert!(stdout.contains("api: method.Path.ext\n"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_core_bindings_names_var_and_let_immutability() {
    let output = xsht(&["api", "language:core.bindings"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: exact"), "{stdout}");
    assert!(stdout.contains("api: language.core.bindings\n"), "{stdout}");
    // The mutable-binding token must be discoverable from the reference, and
    // `let` must be described as immutable, so a first-time agent writing a
    // mutable counter does not have to guess `let mut` / `mut` / `let var`.
    assert!(stdout.contains("var"), "{stdout}");
    assert!(
        stdout.contains("let") && stdout.contains("immutable"),
        "{stdout}"
    );
    assert!(stdout.contains("let mut"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}
