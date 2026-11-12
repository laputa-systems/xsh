//! Checks of `xsht api` that read the registry directly: what the command
//! prints is compared with, or counted against, the tables it is generated
//! from. Assertions on the rendered reference alone are native tests in
//! `tests/xsh/api-tool.xsh`.

use std::process::Command;

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn xsht(args: &[&str]) -> std::process::Output {
    Command::new(test_bin!("xsht"))
        .args(args)
        .current_dir(workspace_root())
        .output()
        .expect("run xsht")
}

#[test]
fn fs_root_registry_holds_native_receiver_operations_and_retains_factories() {
    let registry = xsh_registry::signature::api_spec();
    let methods = &registry
        .methods
        .iter()
        .find(|entry| entry.receiver == xsh_registry::signature::MethodReceiver::FsRoot)
        .unwrap()
        .methods;
    assert_eq!(methods.len(), 26);
    assert_eq!(
        methods
            .iter()
            .map(|method| method.overloads.len())
            .sum::<usize>(),
        28
    );
    let readlink_result = methods.iter().find(|method| method.name == "readlink_result").unwrap();
    assert_eq!(readlink_result.overloads.len(), 1);
    assert_eq!(readlink_result.overloads[0].sig.op,
        xsh_registry::signature::RuntimeOp::FsRootReadlinkResult);
    for name in [
        "open_root",
        "tempdir",
        "project_root",
        "user_root",
        "root_install_file",
        "root_install_file_sha256",
    ] {
        assert!(
            registry
                .modules
                .iter()
                .find(|module| module.name == "fs")
                .unwrap()
                .sig
                .functions
                .iter()
                .any(|function| function.name == name)
        );
    }
    for name in ["close_root", "root_path", "root", "root_read", "root_mkdir"] {
        assert!(
            !registry
                .modules
                .iter()
                .find(|module| module.name == "fs")
                .unwrap()
                .sig
                .functions
                .iter()
                .any(|function| function.name == name)
        );
    }
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

fn assert_contains_in_order(text: &str, fragments: &[String]) {
    let mut start = 0;
    for fragment in fragments {
        let Some(offset) = text[start..].find(fragment) else {
            panic!("missing {fragment:?} after byte {start} in:\n{text}");
        };
        start += offset + fragment.len();
    }
}

fn xsht_text(args: &[&str]) -> String {
    let output = xsht(args);
    assert!(
        output.status.success(),
        "xsht {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn enum_query_lists_each_builtin_variant_with_its_purpose() {
    for entry in xsh_registry::enums::builtin_enums() {
        let query = format!("enum:{}", entry.name);
        let text = xsht_text(&["api", &query]);
        let mut fragments = vec![
            format!("query: {query}\n"),
            "status: exact\n".to_string(),
            format!("api: enum.{}\n", entry.name),
            "kind: enum\n".to_string(),
            format!("purpose: {}\n", entry.summary),
            format!("variants: {}\n", entry.variants.len()),
        ];
        for variant in entry.variants {
            fragments.push(format!("variant: {}\n", variant.name));
            fragments.push(format!("variant-purpose: {}\n", variant.summary));
        }
        assert_contains_in_order(&text, &fragments);
    }
}

#[test]
fn enum_module_selector_lists_that_modules_builtin_enums() {
    let text = xsht_text(&["api", "enum:archive"]);
    assert!(text.contains("status: matches\n"), "{text}");
    for entry in xsh_registry::enums::builtin_enums() {
        let listed = text.contains(&format!("api: enum.{}\n", entry.name));
        assert_eq!(
            listed,
            entry.name.starts_with("archive."),
            "{} listed={listed}\n{text}",
            entry.name
        );
    }
}

#[test]
fn enum_jsonl_reports_every_variant_with_its_purpose() {
    let text = xsht_text(&["api", "--format", "jsonl", "enum:archive.Format"]);
    assert!(text.contains("\"id\":\"enum.archive.Format\""), "{text}");
    assert!(text.contains("\"kind\":\"enum\""), "{text}");
    let entry = xsh_registry::enums::builtin_enum(xsh_registry::enums::ARCHIVE_FORMAT).unwrap();
    for variant in entry.variants {
        let object = format!(
            "{{\"name\":\"{}\",\"summary\":\"{}\"}}",
            variant.name, variant.summary
        );
        assert!(text.contains(&object), "missing {object} in {text}");
    }
}

#[test]
fn enum_query_without_a_match_is_missing() {
    let output = xsht(&["api", "--strict", "enum:archive.Nope"]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("status: missing\n"), "{text}");
}

#[test]
fn typed_codec_api_renders_schema_operands_separately_from_values() {
    let output = xsht(&["api", "api:codec.decode_as", "api:codec.read_as"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("codec.decode_as(format: Str, text: Str, T) -> Result[T, Error]"), "{text}");
    assert!(text.contains("codec.read_as(format: Str, path: Path, T) -> Result[T, Error]"), "{text}");
    assert!(!text.contains("T: Any"), "{text}");
}
