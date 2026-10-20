use super::{CliOutput, Stage, StageTimings, cancellation_output, text_bytes};
use crate::xsht::config::config_for_file;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use xsh::diagnostic::{Diagnostic, DiagnosticRenderer, Label};
use xsh::execution::evaluator::{CheckedBodyContract, CheckedBodyContractError, checked_body_contracts};
use xsh::frontend::check::{
    BodyContractError, BodyContractOwner, CheckOptions, CheckSession,
    CheckedCallableContract, CheckedCliEntryContract, CheckedFunctionKind,
    CliContractError, ContractComparisonError,
};
use xsh::frontend::load as loader;
use xsh::frontend::source::SourceMap;
use xsh::frontend::syntax::arena::{ArenaProgram, ArenaStmtKind};

enum DeclarationContract {
    Callable { contract: CheckedCallableContract, exported: bool },
    Cli(CheckedCliEntryContract),
}

impl DeclarationContract {
    fn equivalent_to(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Callable { contract: left, exported: left_exported }, Self::Callable { contract: right, exported: right_exported }) => left_exported == right_exported && left.equivalent_to(right),
            (Self::Cli(left), Self::Cli(right)) => left.equivalent_to(right),
            _ => false,
        }
    }
}

type Contracts = BTreeMap<(PathBuf, String), DeclarationContract>;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum BodyOwner {
    Entry,
    Declaration(String),
}

impl BodyOwner {
    fn label(&self) -> &str {
        match self {
            Self::Entry => "<entry>",
            Self::Declaration(name) => name,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BodyKey {
    source_path: PathBuf,
    module_key: Arc<str>,
    owner: BodyOwner,
}

impl BodyKey {
    fn new(module_key: &str, owner: BodyOwner, original_root: &Path) -> Result<Self, String> {
        let physical = module_key.split('\0').next().expect("module key has a physical path");
        Ok(Self {
            source_path: bounded_file(Path::new(physical), original_root)?,
            module_key: Arc::from(module_key),
            owner,
        })
    }
}

type BodyContracts = BTreeMap<BodyKey, CheckedBodyContract>;

struct CheckedProgram {
    interfaces: Contracts,
    bodies: BodyContracts,
}


pub fn compare_paths_timed(
    original_root: &str,
    candidate_root: &str,
    paths: &[String],
    timings: &StageTimings,
) -> CliOutput {
    if let Some(output) = cancellation_output() {
        return output;
    }
    comparison_output(compare_paths(original_root, candidate_root, paths, timings))
}

fn comparison_output(result: Result<(usize, usize), String>) -> CliOutput {
    if let Some(output) = cancellation_output() {
        return output;
    }
    match result {
        Ok((callables, cli_entries)) => output(0, format!("xsht check interface qualification: {callables} callable, {cli_entries} CLI\n"), String::new()),
        Err(message) => output(2, String::new(), format!("xsht: interface qualification failed: {message}\n")),
    }
}

fn output(status: u8, stdout: String, stderr: String) -> CliOutput {
    CliOutput { status, stdout: text_bytes(stdout), stderr: text_bytes(stderr), trace_text: String::new(), syscall_summary: None }
}

fn check_cancellation() -> Result<(), String> {
    if xsh::process::cancellation_requested_signal().is_some() { Err("qualification cancelled".into()) } else { Ok(()) }
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize().map_err(|error| format!("{}: {error}", path.display()))
}

fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn bounded_file(path: &Path, root: &Path) -> Result<PathBuf, String> {
    let path = canonical(path)?;
    if !path.starts_with(root) || !path.is_file() {
        return Err(format!("{} must be a file inside {}", path.display(), root.display()));
    }
    Ok(path)
}

fn candidate_sources(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = BTreeSet::new();
    let mut directories = vec![root.to_path_buf()];
    let mut visited = BTreeSet::new();
    while let Some(directory) = directories.pop() {
        check_cancellation()?;
        let directory = canonical(&directory)?;
        if !directory.starts_with(root) {
            return Err(format!("source directory escapes candidate root: {}", directory.display()));
        }
        if !visited.insert(directory.clone()) { continue; }
        let mut entries = fs::read_dir(&directory).map_err(|error| format!("{}: {error}", directory.display()))?
            .map(|entry| entry.map(|entry| entry.path())).collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        entries.sort();
        for path in entries {
            check_cancellation()?;
            if matches!(path.file_name().and_then(|name| name.to_str()), Some(".git" | ".work" | "target" | "node_modules")) { continue; }
            let metadata = fs::symlink_metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            if metadata.file_type().is_symlink() { continue; }
            if metadata.is_dir() { directories.push(path); }
            else if path.extension().is_some_and(|extension| extension == "xsh") {
                files.insert(bounded_file(&path, root)?);
            }
        }
    }
    Ok(files.into_iter().collect())
}

fn compare_paths(original_root: &str, candidate_root: &str, paths: &[String], timings: &StageTimings) -> Result<(usize, usize), String> {
    let original_root = canonical(Path::new(original_root))?;
    let candidate_root_spelling = std::path::absolute(candidate_root)
        .map_err(|error| format!("{candidate_root}: {error}"))?;
    let candidate_root = canonical(Path::new(candidate_root))?;
    if !original_root.is_dir() || !candidate_root.is_dir() {
        return Err("original and candidate roots must be directories".into());
    }
    if paths.is_empty() { return Err("candidate entry paths are required".into()); }
    let mut overrides = BTreeMap::new();
    for candidate in candidate_sources(&candidate_root)? {
        check_cancellation()?;
        let relative = candidate.strip_prefix(&candidate_root).expect("bounded candidate source");
        let original = bounded_file(&original_root.join(relative), &original_root)?;
        let bytes = fs::read(&candidate).map_err(|error| format!("{}: {error}", candidate.display()))?;
        if overrides.insert(original.clone(), bytes).is_some() {
            return Err(format!("multiple candidate sources map to {}", original.display()));
        }
    }
    let mut original_contracts = Contracts::new();
    let mut candidate_contracts = Contracts::new();
    let mut original_bodies = BodyContracts::new();
    let mut candidate_bodies = BodyContracts::new();
    let mut entries = BTreeSet::new();
    for path in paths {
        check_cancellation()?;
        let selected = std::path::absolute(path).map_err(|error| format!("{path}: {error}"))?;
        let relative = selected.strip_prefix(&candidate_root_spelling)
            .or_else(|_| selected.strip_prefix(&candidate_root))
            .map_err(|_| format!("{} must be an entry inside {}", selected.display(), candidate_root.display()))?;
        let original = bounded_file(&original_root.join(relative), &original_root)?;
        let candidate = bounded_file(&selected, &candidate_root)?;
        if original.strip_prefix(&original_root).expect("bounded original entry")
            != candidate.strip_prefix(&candidate_root).expect("bounded candidate entry")
        {
            return Err(format!("{}: candidate entry resolution differs from the corresponding original entry", selected.display()));
        }
        if !entries.insert(original.clone()) { continue; }
        let config = config_for_file(path_text(&original)?)?;
        let old = check_contracts(&original, &original_root, &candidate_root, None, config.module_roots(), timings)?;
        let new = check_contracts(&original, &original_root, &candidate_root, Some(&overrides), config.module_roots(), timings)?;
        merge_contracts(&mut original_contracts, old.interfaces)?;
        merge_contracts(&mut candidate_contracts, new.interfaces)?;
        merge_body_contracts(&mut original_bodies, old.bodies)?;
        merge_body_contracts(&mut candidate_bodies, new.bodies)?;
    }
    let mut failures = Vec::new();
    if original_contracts.is_empty() || candidate_contracts.is_empty() {
        failures.push("selected programs must contain callable declarations on both sides".into());
    }
    for ((path, name), old) in &original_contracts {
        check_cancellation()?;
        let Some(new) = candidate_contracts.get(&(path.clone(), name.clone())) else {
            failures.push(format!("{}::{name}: missing candidate declaration", path.display()));
            continue;
        };
        match (old, new) {
            (DeclarationContract::Callable { exported: old_exported, .. }, DeclarationContract::Callable { exported: new_exported, .. }) if old_exported != new_exported => {
                failures.push(format!("{}::{name}: export visibility mismatch", path.display()));
            }
            (DeclarationContract::Callable { contract: old, .. }, DeclarationContract::Callable { contract: new, .. }) => {
                let comparison = if old.equivalent_to(new) { Ok(true) } else if old.is_generic() { Ok(false) } else { old.is_instance_of(new) };
                match comparison {
                    Ok(true) => {}
                    Ok(false) => failures.push(format!("{}::{name}: contract mismatch\n  original: {old}\n  candidate: {new}", path.display())),
                    Err(error) => failures.push(format!("{}::{name}: cannot establish interface compatibility: {}", path.display(), comparison_error(error))),
                }
            }
            (DeclarationContract::Cli(old), DeclarationContract::Cli(new)) if old.equivalent_to(new) => {}
            (DeclarationContract::Cli(_), DeclarationContract::Cli(_)) => failures.push(format!("{}::{name}: CLI parsing contract or declared effect bound mismatch", path.display())),
            _ => failures.push(format!("{}::{name}: declaration kind mismatch", path.display())),
        }
    }
    for (path, name) in candidate_contracts.keys() {
        if !original_contracts.contains_key(&(path.clone(), name.clone())) {
            failures.push(format!("{}::{name}: extra candidate declaration", path.display()));
        }
    }
    if original_bodies.is_empty() || candidate_bodies.is_empty() {
        failures.push("selected programs must publish checked body contracts on both sides".into());
    }
    for (key, old) in &original_bodies {
        check_cancellation()?;
        match candidate_bodies.get(key) {
            None => failures.push(format!("{}::{}: missing candidate checked body contract", key.source_path.display(), key.owner.label())),
            Some(new) if old.preserved_by(new) => {}
            Some(_) => failures.push(format!("{}::{}: checked body contract mismatch", key.source_path.display(), key.owner.label())),
        }
    }
    for key in candidate_bodies.keys() {
        if !original_bodies.contains_key(key) {
            failures.push(format!("{}::{}: extra candidate checked body contract", key.source_path.display(), key.owner.label()));
        }
    }
    if failures.is_empty() {
        let callables = original_contracts.values().filter(|contract| matches!(contract, DeclarationContract::Callable { .. })).count();
        Ok((callables, original_contracts.len() - callables))
    } else { Err(failures.join("\n")) }
}

fn comparison_error(error: ContractComparisonError) -> &'static str {
    match error {
        ContractComparisonError::NonConcreteOriginal => "original contract is not concrete",
        ContractComparisonError::UnsupportedContract => "contract shape is unsupported",
        ContractComparisonError::CannotEstablishConstraints => "joint type constraints could not be established",
        ContractComparisonError::UnresolvedInstance => "generic instance remains unresolved",
    }
}

fn merge_contracts(target: &mut Contracts, contracts: Contracts) -> Result<(), String> {
    for (key, contract) in contracts {
        check_cancellation()?;
        if let Some(previous) = target.get(&key) {
            if !previous.equivalent_to(&contract) {
                return Err(format!("{}::{}: inconsistent declaration across entry programs", key.0.display(), key.1));
            }
        } else { target.insert(key, contract); }
    }
    Ok(())
}

fn merge_body_contracts(target: &mut BodyContracts, bodies: BodyContracts) -> Result<(), String> {
    for (key, body) in bodies {
        check_cancellation()?;
        if let Some(previous) = target.get(&key) {
            if !previous.preserved_by(&body) || !body.preserved_by(previous) {
                return Err(format!("{}::{}: inconsistent checked body contract across entry programs", key.source_path.display(), key.owner.label()));
            }
        } else { target.insert(key, body); }
    }
    Ok(())
}

fn body_contract_error(error: CheckedBodyContractError, sources: &SourceMap) -> String {
    let (span, message) = match error {
        CheckedBodyContractError::Preparation(diagnostic) => return DiagnosticRenderer::new().render(&[diagnostic], sources),
        CheckedBodyContractError::Semantic(error) => match error {
            BodyContractError::MissingSource(source) => return format!("cannot capture checked body contract: missing source {source:?}"),
            BodyContractError::UnmappedFact(span) => (span, "checked fact has no canonical body site"),
            BodyContractError::DuplicateOwner(span) => (span, "body owner is duplicated"),
            BodyContractError::MissingReceiver(span) => (span, "selected operation has no checked receiver"),
            BodyContractError::MissingEffectFact(span) => (span, "body has no checked effect fact"),
            BodyContractError::UnsupportedSelection(span) => (span, "checked selection cannot be compared"),
            BodyContractError::AmbiguousSite(span) => (span, "checked fact has an ambiguous body site"),
        },
    };
    DiagnosticRenderer::new().render(&[
        Diagnostic::error("cannot capture checked body contract").with_label(Label::primary(span, message)),
    ], sources)
}

fn validate_candidate_imports(
    program: &ArenaProgram,
    sources: &SourceMap,
    original_root: &Path,
    candidate_root: &Path,
    module_roots: &[PathBuf],
) -> Result<(), String> {
    let candidate_module_roots = module_roots.iter().map(|root| {
        root.strip_prefix(original_root).map(|relative| candidate_root.join(relative)).unwrap_or_else(|_| root.clone())
    }).collect::<Vec<_>>();
    let ranges = std::iter::once(program.statements)
        .chain(program.modules.iter().filter(|module| !module.internal).map(|module| module.statements));
    program.symbol_owner().with_current(|| {
        for range in ranges {
            for statement in program.arena.stmt_ids(range) {
                check_cancellation()?;
                let statement = program.arena.stmt(statement);
                let ArenaStmtKind::Use(import) = statement.kind else { continue; };
                let import = program.arena.use_stmt(import);
                let Some(expected) = &import.resolved else { continue; };
                let importer = sources.get(statement.span.source_id).expect("loaded import source exists");
                let relative = Path::new(importer.name()).strip_prefix(original_root)
                    .map_err(|_| format!("importer is outside original root: {}", importer.name()))?;
                let candidate_importer = candidate_root.join(relative);
                let path = program.arena.names(import.path).collect::<Vec<_>>();
                let label = path.iter().map(|name| name.as_str().to_string()).collect::<Vec<_>>().join(".");
                let mismatch = || format!("{}: candidate import resolution differs for {label}", importer.name());
                let expected = bounded_file(Path::new(expected.as_ref()), original_root)?;
                match loader::resolve_user_module(&candidate_importer, &path, &candidate_module_roots)? {
                    loader::UserModuleResolution::Found { path, .. } => {
                        let candidate = bounded_file(&path, candidate_root)?;
                        let relative = candidate.strip_prefix(candidate_root).expect("bounded candidate import");
                        let mapped = bounded_file(&original_root.join(relative), original_root)?;
                        if mapped != expected { return Err(mismatch()); }
                    }
                    loader::UserModuleResolution::Standard | loader::UserModuleResolution::Missing(_) => return Err(mismatch()),
                }
            }
        }
        Ok(())
    })
}

fn check_contracts(
    entry: &Path,
    original_root: &Path,
    candidate_root: &Path,
    overrides: Option<&BTreeMap<PathBuf, Vec<u8>>>,
    module_roots: Vec<PathBuf>,
    timings: &StageTimings,
) -> Result<CheckedProgram, String> {
    check_cancellation()?;
    let file = path_text(entry)?;
    let bytes = match overrides {
        Some(overrides) => overrides.get(entry).cloned().ok_or_else(|| format!("missing candidate source for {}", entry.display()))?,
        None => fs::read(entry).map_err(|error| format!("{}: {error}", entry.display()))?,
    };
    let source_overrides = overrides.into_iter().flat_map(|overrides| overrides.iter())
        .map(|(path, bytes)| Ok((path_text(path)?.to_owned(), bytes.clone())))
        .collect::<Result<Vec<_>, String>>()?;
    let (sources, parsed) = timings.time(Stage::Load, || loader::parse_load_entry_source_arena_only_with_overrides(
        file, loader::entry_source_from_bytes(file, bytes), module_roots.clone(), loader::StdlibLinkage::Prepare, source_overrides,
        || xsh::process::cancellation_requested_signal().is_some(),
    )).map_err(|loader::LoadCancelled| "qualification cancelled during source loading".to_owned())?;
    check_cancellation()?;
    if !parsed.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new().render(&parsed.diagnostics, &sources));
    }
    for module in parsed.arena.modules.iter().filter(|module| !module.internal) {
        let path = bounded_file(Path::new(module.key.split('\0').next().unwrap_or(&module.key)), original_root)?;
        if overrides.is_some_and(|overrides| !overrides.contains_key(&path)) {
            return Err(format!("missing candidate source for {}", path.display()));
        }
    }
    if overrides.is_some() {
        validate_candidate_imports(&parsed.arena, &sources, original_root, candidate_root, &module_roots)?;
    }
    let source_id = sources.files().first().expect("entry source exists").id();
    let arena = Arc::new(parsed.arena);
    let sources = Arc::new(sources);
    let session = CheckSession::new(Arc::clone(&arena), Arc::clone(&sources));
    let view = session.view_for_root(arena.statements, source_id);
    let checked = timings.time(Stage::Check, || session.check_view(&view, CheckOptions::for_check_cli()));
    check_cancellation()?;
    if !checked.diagnostics.is_empty() {
        return Err(DiagnosticRenderer::new().render(&checked.diagnostics, &sources));
    }
    let mut contracts = Contracts::new();
    let cli_contracts = checked.checked_declarations.cli_entry_contracts(&arena, &sources).map_err(|error| {
        match error {
            CliContractError::MissingSource(span) => format!("missing source for checked CLI declaration at {span:?}"),
            CliContractError::InvalidSourceSpan(span) => format!("invalid source span for checked CLI declaration at {span:?}"),
        }
    })?;
    for contract in cli_contracts {
        let path = bounded_file(Path::new(contract.source_name()), original_root)?;
        let name = arena.symbol_owner().with_current(|| contract.name().as_str().to_string());
        if contracts.insert((path.clone(), name.clone()), DeclarationContract::Cli(contract)).is_some() {
            return Err(format!("{}::{name}: ambiguous declaration key", path.display()));
        }
    }
    for declaration in checked.checked_declarations.functions.values().filter(|declaration| !declaration.internal) {
        let source = sources.get(declaration.body.source_id).expect("checked declaration source exists");
        let path = bounded_file(Path::new(source.name()), original_root)?;
        let name = arena.symbol_owner().with_current(|| declaration.name.as_str().to_string());
        if declaration.kind == CheckedFunctionKind::CliMain {
            if !contracts.contains_key(&(path.clone(), name.clone())) {
                return Err(format!("{}::{name}: missing checked CLI parsing contract", path.display()));
            }
            continue;
        }
        let contract = declaration.callable_contract().ok_or_else(|| format!("{}::{name}: unsupported declaration contract", path.display()))?;
        if contracts.insert((path.clone(), name.clone()), DeclarationContract::Callable { contract, exported: declaration.is_exported() }).is_some() {
            return Err(format!("{}::{name}: ambiguous declaration key", path.display()));
        }
    }
    let captured = timings.time(Stage::Lower, || checked_body_contracts(checked, &arena, &sources, source_id))
        .map_err(|error| body_contract_error(error, &sources))?;
    check_cancellation()?;
    let mut bodies = BodyContracts::new();
    for body in captured {
        check_cancellation()?;
        let owner = arena.symbol_owner().with_current(|| match body.owner() {
            BodyContractOwner::Entry => BodyOwner::Entry,
            BodyContractOwner::Declaration(name) => BodyOwner::Declaration(name.as_str().to_string()),
        });
        let key = BodyKey::new(body.source_name(), owner, original_root)?;
        if bodies.contains_key(&key) {
            return Err(format!("{}::{}: ambiguous checked body owner", key.source_path.display(), key.owner.label()));
        }
        bodies.insert(key, body);
    }
    Ok(CheckedProgram { interfaces: contracts, bodies })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_qualification_preserves_actual_checked_bodies() {
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.work/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let root = tempfile::TempDir::new_in(scratch).unwrap();
        let original = root.path().join("original");
        let candidate = root.path().join("candidate");
        fs::create_dir_all(&original).unwrap();
        fs::create_dir_all(&candidate).unwrap();
        let source = "pure copy(value: Int) -> Int { let copied: Int = value; copied }\n\
proc bounded(value: Int) [] -> Int { let checked: UInt = value; checked }\n\
pure aggregate(items: List[Int]) -> Int { items |> count() }\n\
proc observe() [time] -> Int { let _ = time.now(); 1 }\n";
        let safe = source.replace("let copied: Int = value", "let copied = value");
        fs::write(original.join("main.xsh"), source).unwrap();
        fs::write(candidate.join("main.xsh"), &safe).unwrap();
        let qualify = |old: &Path, new: &Path| {
            let entries = [new.join("main.xsh").to_str().unwrap().to_owned()];
            compare_paths(old.to_str().unwrap(), new.to_str().unwrap(), &entries, &StageTimings::start())
        };
        assert_eq!(qualify(&original, &original), Ok((4, 0)));
        assert_eq!(qualify(&candidate, &candidate), Ok((4, 0)));
        assert_eq!(qualify(&original, &candidate), Ok((4, 0)));
        for (mutated, owner) in [
            (safe.replace("let checked: UInt = value", "let checked = value"), "bounded"),
            (safe.replace("items |> count()", "items |> sum()"), "aggregate"),
            (safe.replace("let _ = time.now(); 1", "1"), "observe"),
        ] {
            fs::write(candidate.join("main.xsh"), mutated).unwrap();
            assert_eq!(qualify(&candidate, &candidate), Ok((4, 0)));
            let diagnostic = qualify(&original, &candidate).unwrap_err();
            assert!(diagnostic.contains("checked body contract mismatch"), "{diagnostic}");
            assert!(diagnostic.contains(&format!("main.xsh::{owner}")), "{diagnostic}");
        }
    }

    #[test]
    fn public_cli_body_contracts_preserve_emitted_header_gates() {
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.work/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let root = tempfile::TempDir::new_in(scratch).unwrap();
        let entry = root.path().join("main.xsh");
        let capture = |source: &str| {
            fs::write(&entry, source).unwrap();
            let file = entry.to_str().unwrap();
            let input = loader::entry_source_from_bytes(file, fs::read(&entry).unwrap());
            let source_id = input.source_id;
            let (sources, parsed) = loader::parse_load_entry_source_arena_only(file, input, Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let program = Arc::new(parsed.arena);
            let sources = Arc::new(sources);
            let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
            let view = session.view_for_root(program.statements, source_id);
            let checked = session.check_view(&view, CheckOptions::for_check_cli());
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let contracts = checked_body_contracts(checked, &program, &sources, source_id)
                .unwrap_or_else(|error| panic!("cannot capture CLI body {source:?}: {error:?}"));
            program.symbol_owner().with_current(|| contracts.into_iter().map(|body| {
                let owner = match body.owner() {
                    BodyContractOwner::Entry => BodyOwner::Entry,
                    BodyContractOwner::Declaration(name) => BodyOwner::Declaration(name.as_str().to_string()),
                };
                ((body.source_name().to_owned(), owner), body)
            }).collect::<BTreeMap<_, _>>())
        };
        let original = capture("cli main build(value: Str) [] {}\n");
        let candidate = capture("cli main build(value: Int) [] {}\n");
        assert!(!original.is_empty());
        assert_eq!(original.keys().collect::<Vec<_>>(), candidate.keys().collect::<Vec<_>>());
        assert!(original.values().all(|body| body.preserved_by(body)));
        assert!(candidate.values().all(|body| body.preserved_by(body)));
        assert!(!original.iter().all(|(owner, body)| body.preserved_by(&candidate[owner])));
    }

    #[test]
    fn checked_body_keys_preserve_loaded_module_resolution_contexts() {
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.work/tmp");
        fs::create_dir_all(&scratch).unwrap();
        let root = tempfile::TempDir::new_in(scratch).unwrap();
        let common = root.path().join("common");
        let first = root.path().join("first");
        let second = root.path().join("second");
        for directory in [&common, &first, &second] { fs::create_dir_all(directory).unwrap(); }
        let original_root = root.path().canonicalize().unwrap();
        let shared = common.join("shared.xsh");
        fs::write(&shared, "use nested\n").unwrap();
        fs::write(first.join("nested.xsh"), "export const value = 1\n").unwrap();
        fs::write(second.join("nested.xsh"), "export const value = 2\n").unwrap();
        let inputs = [
            ("one.xsh", vec![common.clone(), first.clone()]),
            ("two.xsh", vec![common.clone(), first]),
            ("three.xsh", vec![common, second]),
        ].into_iter().map(|(name, module_roots)| loader::ArenaEntryInput {
            file: root.path().join(name).to_str().unwrap().into(),
            bytes: b"use shared\n".to_vec(), module_roots,
        }).collect();
        let loaded = loader::parse_load_entries_shared_arena_only(inputs);
        assert!(loaded.entries.iter().all(|entry| entry.diagnostics.is_empty()));
        let physical_key = loader::module_key(&shared);
        let keys = loaded.program.modules.iter()
            .filter(|module| module.key.split('\0').next() == Some(physical_key.as_str()))
            .map(|module| BodyKey::new(&module.key, BodyOwner::Entry, &original_root).unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().all(|key| key.source_path == shared.canonicalize().unwrap()));
        assert!(keys.iter().any(|key| key.module_key.contains('\0')));
    }

    #[test]
    fn qualification_cancellation_overrides_success() {
        const CHILD: &str = "XSHT_TEST_INTERFACE_CANCELLATION_CHILD";
        if std::env::var_os(CHILD).is_some() {
            xsh::process::clear_cancellation_request();
            let _signals = xsh::process::install_cancellation_signal_handlers().unwrap();
            assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
            let output = comparison_output(Ok((1, 0)));
            assert_eq!(output.status, 130);
            assert!(output.stdout.is_empty());
            assert!(String::from_utf8_lossy(&output.stderr).contains("interrupted by SIGINT"));
            return;
        }
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "xsht::cli::check::compare::tests::qualification_cancellation_overrides_success", "--nocapture"])
            .env(CHILD, "1").output().expect("run isolated cancellation state test");
        assert!(child.status.success(), "{}", String::from_utf8_lossy(&child.stderr));
        assert!(String::from_utf8_lossy(&child.stdout).contains("1 passed"));
    }
}
