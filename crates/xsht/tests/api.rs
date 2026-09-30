use std::io::Write;
use std::process::{Command, Stdio};

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

#[test]
fn api_fs_root_inventory_exposes_native_receiver_operations_and_retains_factories() {
    let output = xsht(&["api", "method:FsRoot.read_bytes", "method:FsRoot.mkdir", "method:FsRoot.close"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    for fragment in ["method.FsRoot.read_bytes", "method.FsRoot.mkdir", "method.FsRoot.close", "root.write", "payload"] {
        assert!(text.contains(fragment), "{text}");
    }
    let registry = xsh_registry::signature::api_spec();
    let methods = &registry.methods.iter().find(|entry| entry.receiver == xsh_registry::signature::MethodReceiver::FsRoot).unwrap().methods;
    assert_eq!(methods.len(), 18);
    assert_eq!(methods.iter().map(|method| method.overloads.len()).sum::<usize>(), 20);
    for name in ["open_root", "tempdir", "project_root", "user_root", "root_install_file"] {
        assert!(registry.modules.iter().find(|module| module.name == "fs").unwrap().sig.functions.iter().any(|function| function.name == name));
    }
    for name in ["close_root", "root_path", "root", "root_read", "root_mkdir"] {
        assert!(!registry.modules.iter().find(|module| module.name == "fs").unwrap().sig.functions.iter().any(|function| function.name == name));
    }
}

#[test]
fn api_boolean_guards_explains_exits_refinements_and_no_error_input() {
    let output = xsht(&["api", "language:core.boolean-guards"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for fragment in ["Bool", "Status", "evaluates once", "every reachable failure path", "no input parameter or new Result boundary", "mutation invalidation", "Float", "NaN", "guard let"] {
        assert!(stdout.contains(fragment), "{stdout}");
    }
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
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
fn api_error_fail_is_exactly_registered_and_searchable() {
    let output = xsht(&["api", "api:error.fail"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: exact"), "{stdout}");
    assert!(stdout.contains("api: module.error.fail"), "{stdout}");
    assert!(stdout.contains("error.fail"), "{stdout}");
    assert!(stdout.contains("validation"), "{stdout}");
    assert!(stdout.contains("error effect"), "{stdout}");

    let output = xsht(&["api", "search:fail"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("api: module.error.fail"), "{stdout}");
    assert!(stdout.contains("status: matches"), "{stdout}");
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
fn api_filesystem_walk_contract_documents_hidden_default() {
    let output = xsht(&["api", "api:fs.files", "api:fs.walk"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert_eq!(stdout.matches("hidden: false").count(), 2, "{stdout}");
    assert_eq!(
        stdout.matches("dot-prefixed files and directories").count(),
        2,
        "{stdout}"
    );
    assert!(stdout.contains("query: api:fs.files"), "{stdout}");
    assert!(stdout.contains("query: api:fs.walk"), "{stdout}");
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
        stdout.contains("signature: sort-by(desc: Bool = false, block) -> Stream[T]"),
        "{stdout}"
    );
    assert!(
        stdout.contains("|> sort-by(desc: true) { |e| e.size }"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("sort-by(--desc, { |e| e.size })"),
        "{stdout}"
    );
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

/// The public API surface remains unchanged when implementations move into
/// embedded scripts.
///
/// The recorded API summary covers every standard module, function, overload
/// count, method receiver, method, and record. Additions, removals, renames,
/// shape changes, and overload-count changes all fail this check. The behavior
/// of those entries is covered separately; this test guards the public surface.
#[test]
fn api_surface_matches_the_recorded_reference() {
    let expected = std::fs::read_to_string(
        workspace_root().join("tests/fixtures/modules/standard-api-surface.jsonl"),
    )
    .expect("read recorded API surface");

    let output = xsht(&["api", "summary", "--format", "jsonl"]);
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout).expect("utf8 stdout");

    assert_eq!(
        actual.trim_end(),
        expected.trim_end(),
        "the public API surface changed; regenerate the fixture only when the change is intended"
    );
}

#[test]
fn api_core_procs_demonstrates_lexical_named_argument_puns() {
    let output = xsht(&["api", "language:core.procs"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("ordinary lexical value"), "{stdout}");
    assert!(stdout.contains("greet(name:)"), "{stdout}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_core_fallback_explains_error_parameter_and_lexical_targets() {
    let output = xsht(&["api", "language:core.fallback"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for fragment in [
        "{ |failure| statements; tail_value }",
        "exactly one immutable parameter containing the exact error",
        "Handler tails match the success type",
        "lexical return, loop, propagation, and cleanup targets",
        "right-associative",
        "lint.error-fallback-block",
    ] {
        assert!(stdout.contains(fragment), "{stdout}");
    }
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_slicing_documents_bounds_units_and_retained_count_method() {
    let output = xsht(&["api", "language:core.slicing", "method:Bytes.slice"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for fragment in [
        "api: language.core.slicing",
        "negative bounds count from the end",
        "Str counts Unicode scalars; Bytes counts bytes",
        "data[..2]",
        "data[2..]",
        "api: method.Bytes.slice",
        "Uses offset/count with nonnegative bounds",
    ] {
        assert!(stdout.contains(fragment), "missing {fragment}: {stdout}");
    }
}

#[test]
fn api_comprehensions_reference_exposes_order_cleanup_and_example() {
    let output = xsht(&["api", "language:core.comprehensions"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 API output");
    assert!(stdout.contains("later duplicate keys win"), "{stdout}");
    assert!(stdout.contains("Streams are pulled lazily"), "{stdout}");
    assert!(stdout.contains("for package in packages"), "{stdout}");
}

#[test]
fn api_list_splicing_documents_nesting_order_and_explicit_domains() {
    let output = xsht(&["api", "language:core.list-splicing"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for text in ["ordinary List-valued element remains nested", "left to right", "Results require explicit handling", "@flags", "collect"] {
        assert!(stdout.contains(text), "missing {text}: {stdout}");
    }
}

#[test]
fn api_regex_literals_exposes_preparation_raw_syntax_and_dynamic_compile() {
    let output = xsht(&["api", "language:core.regex-literals", "module:regex.compile"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for fragment in ["rx\"", "no escapes or interpolation", "unreachable code", "repeated calls share", "regex.compile(runtime_pattern)", "structured regex-compile errors"] {
        assert!(stdout.contains(fragment), "missing {fragment}: {stdout}");
    }
}

#[test]
fn api_streams_explains_yield_delegation_and_cleanup_order() {
    let output = xsht(&["api", "language:core.streams"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for fragment in ["yield @source", "Results require explicit handling", "closes children before parent cleanup", "yield @rows()"] {
        assert!(stdout.contains(fragment), "missing {fragment}: {stdout}");
    }
}

#[test]
fn api_core_records_demonstrates_schema_owned_defaults_and_constructor_puns() {
    let output = xsht(&["api", "language:core.records"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("bounded literal constants"), "{stdout}");
    assert!(stdout.contains("enabled: Bool = true"), "{stdout}");
    assert!(stdout.contains("Config(name:)"), "{stdout}");
    assert!(stdout.contains("Observation[T]"), "{stdout}");
    assert!(stdout.contains("CountObservation(value: 7)"), "{stdout}");
    assert!(stdout.contains("disjoint existing field paths"), "{stdout}");
    assert!(stdout.contains("{...settings, build.jobs: 4}"), "{stdout}");
}

#[test]
fn api_private_pure_returns_explain_definition_inference_and_explicit_boundaries() {
    let output = xsht(&["api", "language:core.pure-functions"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Private helpers"), "{stdout}");
    assert!(stdout.contains("recursive"), "{stdout}");
    assert!(stdout.contains("pure add(left: Int, right: Int) {"), "{stdout}");
}

#[test]
fn api_field_labels_distinguishes_wire_names_from_lexical_bindings() {
    let output = xsht(&["api", "language:core.field-labels"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for text in ["Keyword spellings", "reserved binding and import names", "cannot be shorthand or puns", "dynamic values retain require validation", "Entry(type:", "type: entry_kind"] {
        assert!(stdout.contains(text), "missing {text}: {stdout}");
    }
}

#[test]
fn api_map_literals_exposes_classification_order_and_boundaries() {
    let output = xsht(&["api", "language:core.map-literals"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for text in ["Computed keys require Str", "spread-only Maps require context", "canonical key order", "[name]", "before its value"] {
        assert!(stdout.contains(text), "missing {text}: {stdout}");
    }
}

#[test]
fn api_core_assert_documents_lazy_context_and_core_error_identity() {
    let output = xsht(&["api", "language:core.assert"]);
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("api: language.core.assert"), "{stdout}");
    assert!(stdout.contains("assert actual == expected"), "{stdout}");
    assert!(stdout.contains("assertion-failed"), "{stdout}");
    assert!(stdout.contains("only on false"), "{stdout}");
}

#[test]
fn api_core_enums_documents_nominal_constructors_aliases_and_singletons() {
    let references = xsh_registry::reference::language_references();
    let reference = references.iter().find(|entry| entry.id == "core.enums").expect("enum inventory entry");
    assert_documented("language.core.enums", &reference.docs);
    let output = xsht(&["api", "language:core.enums"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).expect("API text");
    for fragment in ["nominal", "module namespace", "parse.enum-migration", "enum Token { Present(Str) }", "type SelectedMode = Mode", "enum State: Str", "atomically", "never convert Str"] {
        assert!(stdout.contains(fragment), "{stdout}");
    }
    let source = reference.docs.example.as_deref().expect("enum source example");
    let source_id = xsh::frontend::source::SourceId::new(0);
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
}

#[test]
fn api_path_interpolation_distinguishes_native_bytes_and_human_text() {
    let output = xsht(&["api", "language:core.path-literals"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Path fragments as native bytes"), "{text}");
    assert!(text.contains("F-strings, print"), "{text}");
    assert!(text.contains("${config_path}.sha256"), "{text}");
    let output = xsht(&["api", "language:core.command-interpolation"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Compound process words retain interpolated Path bytes"), "{text}");
}

#[test]
fn api_duration_arithmetic_explains_dimensions_and_adapter_boundaries() {
    let output = xsht(&["api", "language:core.duration-arithmetic"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for fragment in ["nonnegative Int", "interval count", "once left to right", "pure", "clamping and saturation", "250ms * attempt"] {
        assert!(stdout.contains(fragment), "missing {fragment}: {stdout}");
    }
    let output = xsht(&["api", "api:time.millis"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Negative counts clamp to zero"), "{stdout}");
}

#[test]
fn api_block_strings_explains_exact_margin_source_boundaries_and_literal_domains() {
    let output = xsht(&["api", "language:core.block-strings"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for text in ["exact prefix", "no implicit trailing newline", "longest matching", "original source spans", "Bytes, Path, glob, regex", "name=$name"] {
        assert!(stdout.contains(text), "missing {text}: {stdout}");
    }
}

#[test]
fn api_process_commands_document_exact_bytes_stdin_ownership() {
    let output = xsht(&["api", "api:process.command_argv", "api:process.command"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for text in ["stdin: Bytes", "stdin: Path", "temporary file", "empty Bytes", "hello"] {
        assert!(stdout.contains(text), "missing {text}: {stdout}");
    }
}

#[test]
fn api_core_procs_demonstrates_static_named_argument_spreading() {
    let output = xsht(&["api", "language:core.procs"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("checked finite Record fields"), "{stdout}");
    assert!(stdout.contains("greet(...options)"), "{stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn api_lexical_error_context_retains_contract_and_executable_example() {
    let output = xsht(&["api", "language:core.error-context"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: exact"), "{stdout}");
    assert!(stdout.contains("api: language.core.error-context"), "{stdout}");
    assert!(stdout.contains("Stored or directly returned Err data stays unchanged"), "{stdout}");
    assert!(stdout.contains("let count = ctx"), "{stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn api_constants_retains_preparation_contract_and_executable_example() {
    let output = xsht(&["api", "language:core.constants"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: exact"), "{stdout}");
    assert!(stdout.contains("prepared immutable data"), "{stdout}");
    assert!(stdout.contains("const format_version = 1"), "{stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn api_value_pipeline_retains_argument_placement_and_evaluation_contract() {
    let output = xsht(&["api", "language:core.value-pipelines"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: exact"), "{stdout}");
    assert!(stdout.contains("Input evaluates once before"), "{stdout}");
    assert!(stdout.contains("pipeline_join(\"[\", _, \"]\")"), "{stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn api_absence_lookups_preserves_byte_and_result_boundaries() {
    let output = xsht(&["api", "language:core.absence-lookups"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for fragment in ["successful zero", "negative/out-of-range", "Ok(null)", "typed errors", "ordered receiver/index/fallback", "checked lookup origin", "entries.get(\"missing\") ?? 7"] {
        assert!(stdout.contains(fragment), "missing {fragment}: {stdout}");
    }
}

#[test]
fn api_scalar_iteration_keeps_direct_source_and_snapshot_contract() {
    let output = xsht(&["api", "language:core.scalar-iteration"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("api: language.core.scalar-iteration"), "{stdout}");
    assert!(stdout.contains("retains its snapshot and view bounds"), "{stdout}");
    assert!(stdout.contains("for character in \"café\""), "{stdout}");
    assert!(stdout.contains("for octet in b\"\\x00\\xff\""), "{stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn api_process_accept_policy_documents_actual_status_and_completion_boundary() {
    let output = xsht(&["api", "language:run.status"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("--accept=EXPR"), "{stdout}");
    assert!(stdout.contains("ProcessError.UnexpectedExit"), "{stdout}");
    assert!(stdout.contains("actual Status and .ok are unchanged"), "{stdout}");
    let output = xsht(&["api", "api:process.command_argv"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("accept: List[Int] = default"), "{stdout}");
}

#[test]
fn api_context_scopes_describes_restoration_and_demonstrates_value_forms() {
    let output = xsht(&["api", "language:core.context-scopes"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("body ? propagates"), "{stdout}");
    assert!(stdout.contains("env ({CC:"), "{stdout}");
    assert!(stdout.contains("cd (p\".\")"), "{stdout}");
}
