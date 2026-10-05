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
    Command::new(release_bin!("xsht"))
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
    assert_eq!(methods.len(), 18);
    assert_eq!(
        methods
            .iter()
            .map(|method| method.overloads.len())
            .sum::<usize>(),
        20
    );
    for name in [
        "open_root",
        "tempdir",
        "project_root",
        "user_root",
        "root_install_file",
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
