use super::CommandSignature;
use super::super::{LintOptions, Linter};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Severity};
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

fn sha256sum(complete: bool, pinned: &str) -> CommandSignature {
    CommandSignature {
        name: "sha256sum".to_owned(),
        reviewed_version: "9.5".to_owned(),
        pinned_version: pinned.to_owned(),
        complete,
        flags: ["-b", "-c", "-t", "-z", "--binary", "--check", "--tag", "--text", "--zero", "--quiet", "--status", "--strict", "--warn", "--ignore-missing"]
            .map(str::to_owned)
            .to_vec(),
        value_flags: Vec::new(),
    }
}

fn findings(source: &str, signatures: Vec<CommandSignature>) -> Vec<Diagnostic> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    Linter::lint(
        &parsed.arena,
        source,
        LintOptions {
            checked_output: Some(std::sync::Arc::new(checked.clone())),
            expr_types: checked.expr_types,
            command_signatures: signatures,
            ..LintOptions::default()
        },
    )
    .diagnostics
    .into_iter()
    .filter(|diagnostic| matches!(diagnostic.code, Some(DiagnosticCode::LintCommandSignature | DiagnosticCode::LintCommandSignatureAdvisory)))
    .collect()
}

#[test]
fn listed_flags_pass_including_clusters_and_attached_values() {
    let source = "proc main() {\n  run sha256sum -bc --quiet --check -- --not-a-flag\n}\n";
    assert!(findings(source, vec![sha256sum(true, "9.5")]).is_empty());
}

#[test]
fn complete_matching_signature_rejects_a_misspelled_flag() {
    let found = findings("proc main() {\n  run sha256sum --quite file\n}\n", vec![sha256sum(true, "9.5")]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].severity, Severity::Error);
    assert!(found[0].message.contains("--quite"));
}

#[test]
fn partial_signature_never_hard_fails() {
    let found = findings("proc main() {\n  run sha256sum --anything -q\n}\n", vec![sha256sum(false, "9.5")]);
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|finding| finding.severity == Severity::Warning));
}

#[test]
fn version_mismatch_downgrades_a_complete_signature_to_a_warning() {
    let found = findings("proc main() {\n  run sha256sum --quite\n}\n", vec![sha256sum(true, "9.6")]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, Severity::Warning);
    let advice = found[0].labels[0].message.as_deref().unwrap();
    assert!(advice.contains("9.5") && advice.contains("9.6"));
}

#[test]
fn dynamic_heads_and_arguments_stop_checking() {
    let source = "proc probe(tool: Str, command: List[Str], extra: List[Str]) {\n  run @command --bogus\n  run sha256sum @extra --bogus\n  run sha256sum (tool) --bogus\n  run other --bogus\n}\nproc main() {}\n";
    assert!(findings(source, vec![sha256sum(true, "9.5")]).is_empty());
}

#[test]
fn value_flags_consume_their_value() {
    let mut signature = sha256sum(true, "9.5");
    signature.value_flags = vec!["-o".to_owned(), "--output".to_owned()];
    let source = "proc main() {\n  run sha256sum -o -weird --output=-weird --output -weird -bo -weird file\n}\n";
    assert!(findings(source, vec![signature]).is_empty());
}

#[test]
fn checking_a_command_that_does_not_exist_needs_no_host() {
    let mut signature = sha256sum(true, "1");
    signature.name = "xsh-46-absent-tool".to_owned();
    signature.reviewed_version = "1".to_owned();
    let found = findings("proc main() {\n  run xsh-46-absent-tool --bad\n}\n", vec![signature]);
    assert_eq!(found.len(), 1);
}

#[test]
fn no_signatures_leaves_the_rule_off() {
    assert!(findings("proc main() {\n  run sha256sum --quite\n}\n", Vec::new()).is_empty());
}
