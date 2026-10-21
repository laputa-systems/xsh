use super::super::*;
use crate::runtime::eval::Evaluator;
use crate::sema::check::Checker;

fn fixture(source: &str) -> FullProgram {
    let name = "native-internal-effects.xsh";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        name, crate::loader::entry_source_from_text(name, source.to_string()), Vec::new(),
    );
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena_with_sources_and_options(&parsed.arena, &sources, source_id,
        crate::sema::check::CheckOptions { reveal_types: true, embedded_bodies: true, ..Default::default() });
    let declarations = Checker::compact_declarations(&parsed.arena, checked);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    FullBuilder::build_compact(&parsed.arena, &declarations, source, Arc::new(sources), source_id).unwrap()
}

fn on_large_stack(test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(test).unwrap().join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}

fn execute_both(program: &FullProgram) {
    let program = Arc::new(program.clone());
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
        evaluator.indexed_program = Some(Arc::clone(&program));
        evaluator.force_frames = force_frames;
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::new(program.store.source_id, 0, 0)).unwrap().unwrap();
        }
    }
}

fn function(program: &FullProgram, name: &str) -> usize {
    program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == name).unwrap()
}

fn without_effect(program: &FullProgram, owner: usize, effect: Effect) -> FullProgram {
    let FullCallableContract::Ground(signature) = program.store.functions[owner].contract else { panic!("ground function") };
    let mut signature = program.store.semantic.to_signature_with_budget(signature, 0,
        &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default()).unwrap();
    signature.effects.as_mut().unwrap().retain(|candidate| *candidate != effect);
    let mut forged = program.clone();
    forged.store.functions[owner].contract = FullCallableContract::Ground(
        SemanticPoolBuilder::default().intern_signature(&mut forged.store.semantic, &signature).unwrap());
    forged
}

#[test]
fn result_unit_tail_forwarding_retains_interior_error_effects() {
    let program = fixture(r#"
pure completed() -> Result[Unit] { Ok() }
error TailFailure = Failed(message: Str)
var order = 0
proc fail_tail() [] -> Result[Unit] {
    order = order * 10 + 1
    Err(TailFailure.Failed("stopped"))
}
proc forward_failure() [] {
    defer { order = order * 10 + 2 }
    errdefer { order = order * 10 + 3 }
    fail_tail()
}
proc forward_success() [] {
    errdefer { order = 99 }
    completed()
}
proc inferred() [] { completed() }
proc explicit() [] -> Result[Unit] { completed() }
proc interior() [error] -> Result[Unit] {
    completed()
    Ok()
}
assert inferred() is Ok(_)
assert explicit() is Ok(_)
assert interior() is Ok(_)
assert forward_failure() is Err(_)
assert order == 132
assert forward_success() is Ok(_)
assert order == 132
"#);
    execute_both(&program);
    let invalid = without_effect(&program, function(&program, "interior"), Effect::Error);
    let error = FullVerifier::verify(&invalid).unwrap_err();
    assert!(error.message.contains("requires effect error"), "{error:?}");
}

fn external_stdlib_fixture(source: &str) -> FullProgram {
    let name = "external-stdlib-effects.xsh";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only_with_linkage(
        name, crate::loader::entry_source_from_text(name, source.to_string()), Vec::new(),
        crate::loader::StdlibLinkage::LinkToPrepared,
    );
    let source_id = sources.files().first().unwrap().id();
    let file = crate::loader::CompactFileUnit::new(name, source_id, parsed);
    assert!(file.parse_diagnostics().is_empty(), "{:?}", file.parse_diagnostics());
    let declarations = Checker::check_compact_declarations(file.program(), &sources, source_id);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    let graph = crate::loader::CompactModuleGraph::from_file_unit(&file, &declarations);
    FullBuilder::build_compact_external_stdlib(file.program(), &declarations, &graph, source, Arc::new(sources), source_id).unwrap()
}

#[test]
fn external_stdlib_calls_authenticate_actual_callee_effects() {
    on_large_stack(|| {
        let program = external_stdlib_fixture("pure style() -> Str { tui.bold() }\nproc verify(file: Path) [fs] -> Result[Unit] { return hash.verify_file(file, sha256: \"00\") }\nproc shadow() [env] -> Result[Str] { return env.get(\"XSH_TEST_CALLEE\") }\nproc plain() [error] -> Unit { assert false }\n");
        FullVerifier::verify(&program).unwrap();
        let denied = without_effect(&program, function(&program, "verify"), Effect::Fs);
        let error = FullVerifier::verify(&denied).unwrap_err();
        assert!(error.message.contains("requires effect fs"), "{error:?}");
        let mut local_shadow = program.clone();
        let shadow = function(&program, "shadow");
        let mut add_string = |text: &str| {
            let id = u32::try_from(local_shadow.store.strings.len() + 1).unwrap();
            let start = u32::try_from(local_shadow.store.string_bytes.len()).unwrap();
            local_shadow.store.string_bytes.extend_from_slice(text.as_bytes());
            local_shadow.store.strings.push(IrRange::new(start, u32::try_from(text.len()).unwrap()));
            id
        };
        let namespace = add_string("<xsh-stdlib:tui>");
        let member = add_string("bold");
        local_shadow.store.functions[shadow].name = member;
        local_shadow.store.function_metadata[shadow].owner = namespace;
        let error = FullVerifier::verify(&local_shadow).unwrap_err();
        assert!(error.message.contains("script implementation kind differs"), "{error:?}");
        let mut finite_shadow = program.clone();
        let start = u32::try_from(finite_shadow.store.string_bytes.len()).unwrap();
        finite_shadow.store.string_bytes.extend_from_slice(b"<xsh-stdlib:hash>verify_file");
        let namespace = u32::try_from(finite_shadow.store.strings.len() + 1).unwrap();
        finite_shadow.store.strings.push(IrRange::new(start, 17));
        let member = u32::try_from(finite_shadow.store.strings.len() + 1).unwrap();
        finite_shadow.store.strings.push(IrRange::new(start + 17, 11));
        finite_shadow.store.functions[shadow].name = member;
        finite_shadow.store.function_metadata[shadow].owner = namespace;
        let error = FullVerifier::verify(&finite_shadow).unwrap_err();
        assert!(error.message.contains("script implementation effects exceed"), "{error:?}");
        let mut plain_shadow = finite_shadow.clone();
        plain_shadow.store.functions[shadow] = program.store.functions[shadow].clone();
        plain_shadow.store.function_metadata[shadow] = program.store.function_metadata[shadow];
        let plain = function(&program, "plain");
        plain_shadow.store.functions[plain].name = member;
        plain_shadow.store.function_metadata[plain].owner = namespace;
        let error = FullVerifier::verify(&plain_shadow).unwrap_err();
        assert!(error.message.contains("script implementation effects exceed"), "{error:?}");
        program.symbol_owner().with_current(|| {
            let owner = function(&program, "style");
            let call = program.store.function_instruction_range(owner).unwrap()
                .find(|instruction| program.store.tags[*instruction] == FullTag::ExprExternalCall).unwrap();
            let words = program.store.data[call].range().bounds(program.store.extra.len()).unwrap();
            for (namespace, member) in [
                ("<xsh-stdlib:hash>", "verify_file"),
                ("<xsh-stdlib:tui>", "missing"),
                ("user-module", "bold"),
            ] {
                let mut forged = program.clone();
                forged.store.extra[words.start] = Name::intern(namespace).symbol().raw();
                forged.store.extra[words.start + 1] = Name::intern(member).symbol().raw();
                let error = FullVerifier::verify(&forged).unwrap_err();
                assert!(error.message.contains("pure executable body contains an impure operation"), "{error:?}");
            }
        });
    });
}

#[cfg(target_os = "linux")]
#[test]
fn prepared_uptime_implementation_requires_filesystem_permission() {
    on_large_stack(|| {
        let program = fixture("proc uptime() [fs] -> Result[Int] { return unix.uptime_seconds() }\n");
        FullVerifier::verify(&program).unwrap();
        let denied = without_effect(&program, function(&program, "uptime"), Effect::Fs);
        let error = FullVerifier::verify(&denied).unwrap_err();
        assert!(error.message.contains("requires effect fs"), "{error:?}");
        let source = "proc uptime() [process] -> Result[Int] { return unix.uptime_seconds() }\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)), "{:?}", checked.diagnostics);
    });
}



#[test]
fn process_handle_cancel_preserves_compacted_named_defaults() {
    let mut program = fixture(r#"
proc close(handle: ProcessHandle) [process] -> Result[Unit] {
    handle.cancel(kill_after: 0ms)
}
proc signal(handle: ProcessHandle) [process] -> Result[Unit] {
    handle.cancel(signal: "TERM")
}
proc both(handle: ProcessHandle) [process] -> Result[Unit] {
    handle.cancel(signal: "TERM", kill_after: 0ms)
}
proc defaults(handle: ProcessHandle) [process] -> Result[Unit] {
    handle.cancel()
}
pure wrong() -> Bool { true }
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    let owner = function(&program, "close");
    let decoder = function_view_decoder(&program.store, owner);
    let method = program.store.function_instruction_range(owner).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprMethod).unwrap();
    let schema::ExprView::Method { mut args, .. } = decoder.expression_view(method).unwrap() else { panic!("method") };
    assert_eq!(args.len(), 1);
    let argument = args.next().unwrap().unwrap().raw() as usize;
    assert!(program.store.function_instruction_range(owner).unwrap().contains(&argument));
    let durations = program.store.function_instruction_range(owner).unwrap().filter(|&instruction|
        matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Duration(_)
            | schema::ExprView::PreparedConstant(PreparedConstantValue(LoweredValue::Duration(_), _))))).collect::<Vec<_>>();
    assert_eq!(durations.len(), 1);
    let duration = durations[0];
    let boolean = program.store.function_instruction_range(function(&program, "wrong")).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
    let mut invalid = program.clone();
    invalid.store.tags[duration] = FullTag::ExprBool;
    invalid.store.data[duration] = program.store.data[boolean];
    let error = FullVerifier::verify(&invalid).unwrap_err();
    assert!(error.message.contains("ground method arguments do not match a registered contract"), "{error:?}");
}

#[test]
fn process_port_rows_preserve_pipeline_predicate_categories() {
    let actual = fixture(r#"
type PortProcess = {
    pid: Int, parent_pid: Int, command: Str, argv: Str, argv0: Str,
    user: Str, uid: Int, protocol: Str, local_address: Str,
    local_port: Int, local: Str, remote_address: Str, remote_port: Int,
    remote: Str, state: Str, fd: Int, inode: Int,
}
let rows: List[PortProcess] = process.port(1)? |> where .pid == 1
let count = rows |> count()
if count == 0 {
    print false
} else {
    let row = rows[0]
    print ${row.pid == 1}
}
"#);
    assert!(actual.store.tags.contains(&FullTag::BoolLiteralCompareSlot));
    let mut program = fixture(r#"
proc flag() [] -> Bool { true }
let count: Int = if flag() { 7 } else { 8 }
let alternate: Int = 9
if count == 7 {
    assert true
} else {
    assert false
}
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let integer = (0..program.store.driver_steps.len()).flat_map(|step| program.store.driver_instruction_range(step).unwrap()).find_map(|instruction| {
        if program.store.tags[instruction].kind() != schema::InstructionKind::Expr { return None; }
        let decoder = driver_projection_decoder(&program, instruction);
        matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Int(7)
            | schema::ExprView::PreparedConstant(PreparedConstantValue(LoweredValue::Int(7), _))))
            .then_some(instruction)
    }).unwrap();
    let boolean = program.store.function_instruction_range(function(&program, "flag")).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
    let mut wrong_writer = program.clone();
    wrong_writer.store.tags[integer] = FullTag::ExprBool;
    wrong_writer.store.data[integer] = program.store.data[boolean];
    let error = FullVerifier::verify(&wrong_writer).unwrap_err();
    assert!(error.message.contains("producer") || error.message.contains("annotation"), "{error:?}");
    let predicate = program.store.tags.iter().position(|tag| *tag == FullTag::BoolLiteralCompareSlot).unwrap();
    let decoder = driver_projection_decoder(&program, predicate);
    let step = program.store.driver_steps[driver_owner_index(decoder.owner).unwrap()];
    let read = step.slots.bounds(program.store.driver_slots.len()).unwrap()
        .find(|&index| program.store.string(program.store.driver_slots[index].name).unwrap() == "count").unwrap();
    let alternate = program.store.binding_identities.iter().position(|identity|
        program.store.string(identity.name).unwrap() == "alternate").unwrap();
    let mut wrong_owner = program.clone();
    assert_ne!(wrong_owner.store.driver_slots[read].binding_identity, alternate as u32);
    wrong_owner.store.driver_slots[read].binding_identity = alternate as u32;
    let error = FullVerifier::verify(&wrong_owner).unwrap_err();
    assert!(error.message.contains("identity") || error.message.contains("identities"), "{error:?}");
}

#[test]
fn checked_field_reads_preserve_terminating_guards() {
    let program = fixture(r#"
type Index = {value: Int?}
type Label = {name: Str?}
pure add(value: Int) -> Int { value + 1 }
pure count(rows: List[Index]) -> Int {
    var total = 0
    for row in rows {
        if row.value == null { continue }
        total += add(row.value)
    }
    total
}
pure names(rows: List[Label]) -> List[Str] {
    let result: List[Str] = collect {
        for row in rows {
            if row.name == null or row.name == "" { continue }
            yield row.name
        }
    }
    result
}
assert count([{value: null}, {value: 4}]) == 5
assert names([{name: null}, {name: ""}, {name: "kept"}]) == ["kept"]
"#);
    execute_both(&program);
}

#[test]
fn checked_mutable_reads_preserve_terminating_guards() {
    let program = fixture(r#"
type Row = {root: Str, point: Str}
pure choose(flag: Bool) -> Str {
    var selected: Row? = null
    if flag { selected = {root: "/", point: "chosen"} }
    if selected == null { return "none" }
    let chosen: Row = selected
    chosen.point
}
pure choose_int(flag: Bool) -> Int {
    var value: Int? = null
    if flag { value = 7 }
    if value == null { return 0 }
    let narrowed: Int = value
    narrowed
}
pure choose_after_write(value: Row?, flag: Bool) -> Str {
    var selected: Row? = null
    if { selected = value; flag } { return "skip" } else if selected == null { return "none" }
    let chosen: Row = selected
    chosen.point
}
assert choose_int(true) == 7
assert choose_int(false) == 0
assert choose_after_write({root: "/", point: "kept"}, false) == "kept"
assert choose_after_write(null, false) == "none"
assert choose(true) == "chosen"
assert choose(false) == "none"
"#);
    let mut program = program;
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    for name in ["choose", "choose_int"] {
        let owner = function(&program, name);
        let decoder = function_view_decoder(&program.store, owner);
        assert!(program.store.function_instruction_range(owner).unwrap().any(|instruction| {
            matches!(decoder.expression_view(instruction), Ok(schema::ExprView::CheckedValue { value, check, .. })
                if check.schema.is_none() && (match check.ty.materialize().unwrap() { Type::Record(_) => name == "choose", Type::Int => name == "choose_int", _ => false })
                    && matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Param(_))))
        }));
    }
}

#[test]
fn checked_prefix_guards_reject_fallthrough_writes() {
    for (index, guard) in [
        "if selected == null { return \"none\" } else if { other = null; flag } { return \"skip\" }",
        "if selected == null { return \"none\" } else if flag { return \"skip\" } else { other = null }",
        "if selected != null { other = null } else { return \"none\" }",
    ].into_iter().enumerate() {
        let source = format!("type Row = {{point: Str}}\npure choose(value: Row?, flag: Bool) -> Str {{\nvar selected: Row? = value\nvar other: Row? = null\n{guard}\nlet chosen = selected\nchosen.point\n}}\nassert choose({{point: \"kept\"}}, false) == \"kept\"\nassert choose(null, false) == \"none\"\n");
        let program = fixture(&source);
        execute_both(&program);
        let owner = function(&program, "choose");
        let decoder = function_view_decoder(&program.store, owner);
        let range = program.store.function_instruction_range(owner).unwrap();
        let selected = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::CheckedValue { value, check, .. }) if check.schema.is_none() && matches!(check.ty.materialize().unwrap(), Type::Record(_)) => match decoder.expression_view(value.raw() as usize).unwrap() {
                schema::ExprView::Param(slot) => Some(slot),
                _ => None,
            },
            _ => None,
        }).unwrap();
        assert!(selected >= program.store.functions[owner].params.len as usize);
        let (condition, is_boolean) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::If { mut branches, .. }) => Some((branches.next().unwrap().unwrap().0.raw() as usize, false)),
            Ok(schema::StmtView::IfBool { mut branches, .. }) => Some((branches.next().unwrap().unwrap().0.raw() as usize, true)),
            _ => None,
        }).unwrap();
        assert!(decoder.checked_slot_null_condition_proves_nonnull_with(condition, index == 2, is_boolean, selected, |_| Ok(())).unwrap());
        let (assignment, original) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Assign { slot, value, .. }) if matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Null)) => Some((instruction, slot)),
            _ => None,
        }).unwrap();
        assert_ne!(original, selected);
        let mut malformed = program.clone();
        malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
        malformed.store.extra[program.store.data[assignment].range().start as usize] = selected as u32;
        assert!(matches!(function_view_decoder(&malformed.store, owner).statement_view(assignment).unwrap(), schema::StmtView::Assign { slot, .. } if slot == selected));
        let error = FullVerifier::verify(&malformed).unwrap_err();
        assert!(error.message.contains("checked value input disagrees") || error.message.contains("non-null branch proof"), "{error:?}");
    }
}

#[test]
fn checked_guarded_operands_preserve_evaluation_order() {
    let program = fixture(r#"
type Row = {value: Int}
var shared: Row? = {value: 7}
proc clear_shared() [] -> Int { shared = null; 0 }
proc read_before_clear() [] -> Int {
    if shared != null {
        let result = shared.value + clear_shared()
        result
    } else { 0 }
}
assert read_before_clear() == 7
assert shared == null
"#);
    execute_both(&program);
    let owner = function(&program, "read_before_clear");
    let setter = function(&program, "clear_shared");
    let capture = program.store.captures[program.store.functions[owner].captures.bounds(program.store.captures.len()).unwrap()].iter()
        .find(|capture| capture.slot_and_flags & (1 << 31) != 0).unwrap();
    assert!(program.store.captures[program.store.functions[setter].captures.bounds(program.store.captures.len()).unwrap()].iter()
        .any(|other| other.slot_and_flags & (1 << 31) != 0 && other.binding_identity == capture.binding_identity));
    let decoder = function_view_decoder(&program.store, owner);
    let (binary, left, right) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
        let schema::ExprView::Binary { op: BinaryOp::Add, left, right, .. } = decoder.expression_view(instruction).ok()? else { return None };
        matches!(decoder.expression_view(right.raw() as usize), Ok(schema::ExprView::Call { function, .. }) if function.id.index() == setter)
            .then_some((instruction, left.raw() as usize, right.raw() as usize))
    }).unwrap();
    assert!(matches!(decoder.expression_view(left).unwrap(), schema::ExprView::Field { .. }));
    let mut malformed = program.clone();
    malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&malformed).unwrap();
    let payload = program.store.data[binary].range().start as usize;
    assert_eq!(malformed.store.extra[payload + 1], left as u32);
    assert_eq!(malformed.store.extra[payload + 2], right as u32);
    malformed.store.extra.swap(payload + 1, payload + 2);
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(binary).unwrap(), schema::ExprView::Binary { left: actual_left, right: actual_right, .. } if actual_left.raw() as usize == right && actual_right.raw() as usize == left));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("non-null branch proof") || error.message.contains("checked value input disagrees"), "{error:?}");
}

#[test]
fn checked_guarded_list_operands_preserve_evaluation_order() {
    let program = fixture(r#"
type Row = {value: Int}
var shared: Row? = {value: 7}
proc clear_shared() [] -> Int { shared = null; 0 }
proc read_before_clear() [] -> List[Int] {
    if shared != null { [shared.value, clear_shared()] } else { [] }
}
assert read_before_clear() == [7, 0]
assert shared == null
"#);
    execute_both(&program);
    let owner = function(&program, "read_before_clear");
    let setter = function(&program, "clear_shared");
    let decoder = function_view_decoder(&program.store, owner);
    let (list, block, left, right) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
        let schema::ExprView::List(mut items) = decoder.expression_view(instruction).ok()? else { return None };
        if items.len() != 2 { return None; }
        let block = items.block_identity_for_test();
        let left = items.next()?.ok()?.raw() as usize;
        let right = items.next()?.ok()?.raw() as usize;
        matches!(decoder.expression_view(right), Ok(schema::ExprView::Call { function, .. }) if function.id.index() == setter)
            .then_some((instruction, block, left, right))
    }).unwrap();
    assert!(matches!(decoder.expression_view(left).unwrap(), schema::ExprView::Field { .. }));
    let words = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
    assert_eq!(&program.store.extra[words.clone()], &[2, left as u32, right as u32]);
    let mut malformed = program.clone();
    malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&malformed).unwrap();
    malformed.store.extra.swap(words.start + 1, words.start + 2);
    let decoder = function_view_decoder(&malformed.store, owner);
    let schema::ExprView::List(mut items) = decoder.expression_view(list).unwrap() else { panic!("expected list"); };
    assert_eq!(items.next().unwrap().unwrap().raw() as usize, right);
    assert_eq!(items.next().unwrap().unwrap().raw() as usize, left);
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("non-null branch proof") || error.message.contains("checked value input disagrees"), "{error:?}");
}

#[test]
fn checked_guarded_interpolations_and_tags_preserve_evaluation_order() {
    for (declaration, result, expression, empty, expected) in [
        ("", "Str", r#"f"{shared.value}:{clear_shared()}""#, r#""""#, r#""7:0""#),
        ("enum Triple { Values(Int, Int, Int) }", "Triple", "Values(0, shared.value, clear_shared())", "Values(0, 0, 0)", "Values(0, 7, 0)"),
    ] {
        let source = format!("type Row = {{value: Int}}\n{declaration}\nvar shared: Row? = {{value: 7}}\nproc clear_shared() [] -> Int {{ shared = null; 0 }}\nproc read_before_clear() [] -> {result} {{ if shared != null {{ {expression} }} else {{ {empty} }} }}\nassert read_before_clear() == {expected}\nassert shared == null\n");
        let program = fixture(&source);
        execute_both(&program);
        let owner = function(&program, "read_before_clear");
        let setter = function(&program, "clear_shared");
        let decoder = function_view_decoder(&program.store, owner);
        let (block, left, right, format) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let (block, values, format) = match decoder.expression_view(instruction).ok()? {
                schema::ExprView::FmtString(parts) => {
                    let block = parts.block_identity_for_test();
                    let values = parts.filter_map(|part| match part.ok()? { decode::FmtPartView::Expr(value, ..) => Some(value.raw() as usize), _ => None }).collect::<Vec<_>>();
                    (block, values, true)
                }
                schema::ExprView::Tag { fields, .. } => {
                    let block = fields.block_identity_for_test();
                    (block, fields.map(|value| value.unwrap().raw() as usize).collect::<Vec<_>>(), false)
                }
                _ => return None,
            };
            let left = *values.iter().find(|&&value| matches!(decoder.expression_view(value), Ok(schema::ExprView::Field { .. })))?;
            let right = *values.iter().find(|&&value| matches!(decoder.expression_view(value), Ok(schema::ExprView::Call { function, .. }) if function.id.index() == setter))?;
            Some((block, left, right, format))
        }).unwrap();
        let words = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let offsets = if format {
            let entries = &program.store.extra[words.clone()];
            let left_offset = entries.windows(2).position(|entry| entry == [1, left as u32]).unwrap() + 1;
            let right_offset = entries.windows(2).position(|entry| entry == [1, right as u32]).unwrap() + 1;
            (words.start + left_offset, words.start + right_offset)
        } else {
            assert_eq!(&program.store.extra[words.clone()][2..], &[left as u32, right as u32]);
            (words.start + 2, words.start + 3)
        };
        let mut malformed = program.clone();
        malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&malformed).unwrap();
        malformed.store.extra.swap(offsets.0, offsets.1);
        assert_eq!(malformed.store.extra[offsets.0], right as u32);
        assert_eq!(malformed.store.extra[offsets.1], left as u32);
        let error = FullVerifier::verify(&malformed).unwrap_err();
        assert!(error.message.contains("non-null branch proof") || error.message.contains("checked value input disagrees"), "{error:?}");
    }
}

#[test]
fn checked_guarded_branch_conditions_preserve_evaluation_order() {
    let program = fixture(r#"
type Row = {value: Int}
var shared: Row? = {value: 7}
proc clear_shared() [] -> Int { shared = null; 0 }
proc read_before_clear() [] -> Int {
    if shared != null {
        let result = if shared.value == 1 { 1 } else if clear_shared() == 0 { 2 } else { 3 }
        result
    } else { 0 }
}
assert read_before_clear() == 2
assert shared == null
"#);
    execute_both(&program);
    let owner = function(&program, "read_before_clear");
    let setter = function(&program, "clear_shared");
    let decoder = function_view_decoder(&program.store, owner);
    let mut field_condition = None;
    let mut call_condition = None;
    for instruction in program.store.function_instruction_range(owner).unwrap() {
        let Ok(schema::ExprView::Binary { op: BinaryOp::Eq, left, .. }) = decoder.expression_view(instruction) else { continue };
        match decoder.expression_view(left.raw() as usize).unwrap() {
            schema::ExprView::Field { .. } => field_condition = Some((instruction, left.raw() as usize)),
            schema::ExprView::Call { function, .. } if function.id.index() == setter => call_condition = Some((instruction, left.raw() as usize)),
            _ => {}
        }
    }
    let (field_condition, field) = field_condition.unwrap();
    let (call_condition, call) = call_condition.unwrap();
    let field_word = program.store.data[field_condition].range().start as usize + 1;
    let call_word = program.store.data[call_condition].range().start as usize + 1;
    assert_eq!(program.store.extra[field_word], field as u32);
    assert_eq!(program.store.extra[call_word], call as u32);
    let mut malformed = program.clone();
    malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&malformed).unwrap();
    malformed.store.extra.swap(field_word, call_word);
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(call_condition).unwrap(), schema::ExprView::Binary { left, .. } if left.raw() as usize == field));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("non-null branch proof") || error.message.contains("checked value input disagrees"), "{error:?}");
}

#[test]
fn checked_guarded_reads_keep_repeated_uses_bounded() {
    for count in [16, 32, 64, 128] {
        let mut body = String::new();
        for _ in 0..count { body.push_str("total += row.value\n"); }
        let source = format!("type Row = {{value: Int?}}\npure sum(row: Row) -> Int {{\nif row.value == null {{ return 0 }}\nvar total = 0\n{body}total\n}}\nassert sum({{value: 2}}) == {}\nassert sum({{value: null}}) == 0\n", count * 2);
        let mut program = fixture(&source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
    }
}

#[test]
fn checked_optional_iteration_preserves_prefix_guards() {
    let program = fixture(r#"
pure sum(values: List[Int]?) -> Int {
    if values == null { return 0 }
    var total = 0
    for value in values { total += value }
    total
}
assert sum([2, 3]) == 5
assert sum(null) == 0
"#);
    execute_both(&program);
}

#[test]
fn checked_pattern_optional_iteration_preserves_prefix_guards() {
    let program = fixture(r#"
enum Action { Delete(List[Int]?), Skip }
pure sum(action: Action) -> Int {
    match action {
        Delete(keys) => {
            if keys == null { return 0 }
            var total = 0
            for key in keys { total += key }
            total
        }
        Skip => 0
    }
}
assert sum(Delete([2, 3])) == 5
assert sum(Delete(null)) == 0
assert sum(Skip) == 0
pure count(value: List[Int]?) -> Int {
    if value == null { return 0 }
    value.len()
}
assert count([2, 3]) == 2
assert count(null) == 0
"#);
    execute_both(&program);
}

#[test]
fn checked_captured_explicit_errors_preserve_transfer_contracts() {
    let program = fixture(r#"
error Stopped = Failed(message: Str)
let result: Result[Unit, Stopped] = try { Err(Stopped.Failed(message: "stopped"))? }
assert result == Err(Stopped.Failed(message: "stopped"))
pure fail_value() -> Result[Unit] { error.fail("stopped") }
assert match fail_value() { Err(_) => true; Ok(_) => false }
"#);
    execute_both(&program);
    let (step, transfer) = (0..program.store.driver_steps.len()).find_map(|step| {
        let decoder = verify_schemes::decoder_for_driver_step(&program.store, step).unwrap();
        decoder.instruction_range.clone().find(|&instruction| decoder.direct_error_transfer(instruction).unwrap().is_some()).map(|instruction| (step, instruction))
    }).unwrap();
    let decoder = verify_schemes::decoder_for_driver_step(&program.store, step).unwrap();
    let (payload, cause) = decoder.direct_error_transfer(transfer).unwrap().unwrap();
    assert!(cause.is_none());
    assert_eq!(program.store.tags[payload], FullTag::ExprMatch);
    let message = decoder.instruction_range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr).unwrap();
    assert!(matches!(decoder.expression_view(message).unwrap(), schema::ExprView::Str(_)));
    let boolean = program.store.tags.iter().position(|tag| *tag == FullTag::ExprBool).unwrap();
    let mut malformed = program.clone();
    malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&malformed).unwrap();
    malformed.store.tags[message] = FullTag::ExprBool;
    malformed.store.data[message] = program.store.data[boolean];
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("contract") || error.message.contains("error") || error.message.contains("Error"), "{error:?}");
    let mut program = fixture(r#"
error First = Failed(message: Str)
error Other = Failed(message: Str)
pure transfer(value: First, other: Other) -> Result[Unit, First] {
    Err(value)?
    Ok()
}
assert match transfer(First.Failed(message: "first"), Other.Failed(message: "other")) { Err(_) => true; Ok(_) => false }
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let owner = function(&program, "transfer");
    let decoder = function_view_decoder(&program.store, owner);
    let (transfer, payload) = decoder.instruction_range.clone().find_map(|instruction| {
        decoder.direct_error_transfer(instruction).unwrap().map(|(payload, cause)| { assert!(cause.is_none()); (instruction, payload) })
    }).unwrap();
    assert_eq!(program.store.tags[transfer], FullTag::ExprTry);
    assert!(matches!(decoder.expression_view(payload).unwrap(), schema::ExprView::Param(0)));
    let mut malformed = program.clone();
    malformed.store.extra[program.store.data[payload].range().start as usize] = 1;
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(payload).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("error") || error.message.contains("contract"), "{error:?}");

}

#[test]
fn checked_snapshot_boolean_aliases_preserve_nullable_fields() {
    let program = fixture(r#"
type Row = {left: Int?, right: Int?}
pure sum(row: Row) -> Int {
    let present = row.left != null and row.right != null
    if ! present { return 0 }
    row.left + row.right
}
assert sum({left: 2, right: 3}) == 5
assert sum({left: null, right: 3}) == 0
pure compare(state: Str, value: Str?, target: Str) -> Int {
    if state != "observed" or value == null { 0 } else if value == target { 1 } else { 2 }
}
assert compare("observed", "kept", "kept") == 1
assert compare("observed", null, "kept") == 0
"#);
    execute_both(&program);
}

#[test]
fn checked_boolean_aliases_authenticate_binding_and_snapshot_identity() {
    let program = fixture(r#"
type Row = {value: Int?}
var shared: Row = {value: 7}
proc clear_shared() [] -> Unit { shared = {value: null} }
pure sum(row: Row, spare: Bool) -> Int {
    let present = row.value != null
    if ! present { return 0 }
    row.value
}
proc read_copy(row: Row) [] -> Int {
    let observed = shared.value
    let present = row.value != null
    clear_shared()
    if ! present { return 0 }
    row.value
}
assert sum({value: 7}, true) == 7
assert sum({value: null}, true) == 0
assert read_copy({value: 7}) == 7
"#);
    execute_both(&program);
    let mut original = program.clone();
    original.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&original).unwrap();
    execute_both(&original);
    let owner = function(&program, "sum");
    let decoder = function_view_decoder(&program.store, owner);
    let reference = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
        let schema::ExprView::IfExpr { mut branches, .. } = decoder.expression_view(instruction).ok()? else { return None };
        let (condition, _) = branches.next()?.ok()?;
        let schema::ExprView::Param(slot) = decoder.expression_view(condition.raw() as usize).ok()? else { return None };
        (slot >= program.store.functions[owner].params.len as usize).then_some(condition.raw() as usize)
    }).unwrap();
    let mut wrong_alias = original.clone();
    wrong_alias.store.extra[program.store.data[reference].range().start as usize] = 1;
    assert!(matches!(function_view_decoder(&wrong_alias.store, owner).expression_view(reference).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&wrong_alias).unwrap_err();
    assert!(error.message.contains("checked value input disagrees") || error.message.contains("non-null branch proof"), "{error:?}");

    let owner = function(&program, "read_copy");
    let setter = function(&program, "clear_shared");
    let captures = program.store.functions[owner].captures;
    let setter_captures = program.store.functions[setter].captures;
    let capture = program.store.captures.iter().skip(captures.start as usize).take(captures.len as usize)
        .find(|capture| program.store.string(capture.name).unwrap() == "shared").copied().unwrap();
    let setter_capture = program.store.captures.iter().skip(setter_captures.start as usize).take(setter_captures.len as usize)
        .find(|capture| program.store.string(capture.name).unwrap() == "shared").unwrap();
    assert_eq!(capture.binding_identity, setter_capture.binding_identity);
    assert_ne!(capture.slot_and_flags & (1 << 31), 0);
    assert_ne!(setter_capture.slot_and_flags & (1 << 31), 0);
    let captured_slot = capture.slot_and_flags & !(1 << 31);
    let decoder = function_view_decoder(&program.store, owner);
    let reads = program.store.function_instruction_range(owner).unwrap().filter(|instruction|
        matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(0)))).collect::<Vec<_>>();
    assert!(reads.len() >= 2);
    let mut shared_alias = original;
    for read in reads {
        shared_alias.store.extra[program.store.data[read].range().start as usize] = captured_slot;
        assert!(matches!(function_view_decoder(&shared_alias.store, owner).expression_view(read).unwrap(), schema::ExprView::Param(slot) if slot == captured_slot as usize));
    }
    let error = FullVerifier::verify(&shared_alias).unwrap_err();
    assert!(error.message.contains("checked value input disagrees") || error.message.contains("non-null branch proof"), "{error:?}");
}

#[test]
fn checked_nullable_conditional_storage_preserves_actual_branch_types() {
    let program = fixture(r#"
type Table = {size: Int, growths: List[Int]}
pure choose(table: Table) -> List[Int]? {
    let events: List[Int]? = if table.size > 0 { table.growths } else { null }
    events
}
assert choose({size: 1, growths: [2, 3]}) == [2, 3]
assert choose({size: 0, growths: [2, 3]}) == null
pure trim(value: Str?) -> Str? { value?.trim() }
assert trim(" kept ") == "kept"
assert trim(null) == null
pure environment(target: Str) -> Record {
    match target {
        "a" => {shared: "value", first: "one"}
        "b" => {shared: "value", second: "two"}
        else => {shared: "fallback"}
    }
}
assert environment("a").shared == "value"
assert environment("b").shared == "value"
"#);
    execute_both(&program);
}

#[test]
fn checked_narrowed_return_values_preserve_result_lifting() {
    let program = fixture(r#"
pure take(value: Int?) -> Result[Int] {
    if value == null { return Ok(0) }
    value
}
assert take(7)? == 7
assert take(null)? == 0
pure mutable_take(flag: Bool) -> Result[Int] {
    var found = null
    if flag { found = 7 }
    guard found != null else { return Ok(0) }
    found
}
assert mutable_take(true)? == 7
assert mutable_take(false)? == 0
"#);
    execute_both(&program);
}

#[test]
fn checked_narrowed_result_tails_preserve_unsigned_domains() {
    let mut program = fixture(r#"
pure keep(valid: UInt?, raw: Int?) -> Result[UInt] {
    if valid == null { return Ok(0) }
    if raw == null { return Ok(0) }
    valid
}
pure widen(value: UInt?) -> Result[Int] {
    if value == null { return Ok(0) }
    value
}
assert keep(7, -1)? == 7
assert widen(7)? == 7
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let owner = function(&program, "keep");
    let decoder = function_view_decoder(&program.store, owner);
    let source = decoder.instruction_range.clone().find_map(|instruction| {
        let schema::StmtView::Return { value } = decoder.statement_view(instruction).ok()? else { return None };
        matches!(decoder.expression_view(value.raw() as usize).ok()?, schema::ExprView::Param(0)).then_some(value.raw() as usize)
    }).unwrap();
    let mut malformed = program.clone();
    malformed.store.extra[program.store.data[source].range().start as usize] = 1;
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(source).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("contract") || error.message.contains("checked value"), "{error:?}");
}

#[test]
fn checked_scalar_initializers_authenticate_every_writer() {
    let program = fixture(r#"
pure count(base: Int, other: Str) -> Int {
    var result = base
    result = base
    result
}
pure flag(base: Bool, other: Str) -> Bool {
    var result = base
    result = base
    result
}
assert count(7, "bad") == 7
assert flag(true, "bad")
"#);
    execute_both(&program);
    let mut original = program.clone();
    original.store.checked_expr_types.retain(|fact| fact.is_contract());
    for name in ["count", "flag"] {
        let owner = function(&program, name);
        let facts = original.store.functions[owner].slot_type_facts;
        original.store.slot_type_facts[facts.start as usize..(facts.start + facts.len) as usize].fill(IR_NONE);
    }
    FullVerifier::verify(&original).unwrap();
    execute_both(&original);
    for name in ["count", "flag"] {
        let owner = function(&program, name);
        let decoder = function_view_decoder(&program.store, owner);
        let value = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::AssignInt { value, .. }) if name == "count" => Some(value.raw() as usize),
            Ok(schema::StmtView::AssignBool { value, .. }) if name == "flag" => Some(value.raw() as usize),
            Ok(schema::StmtView::Assign { value, .. }) => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        assert!(matches!(program.store.tags[value], FullTag::IntSlot | FullTag::BoolSlot | FullTag::ExprParam));
        let mut malformed = original.clone();
        malformed.store.extra[program.store.data[value].range().start as usize] = 1;
        let error = FullVerifier::verify(&malformed).unwrap_err();
        assert!(error.message.contains("contract") || error.message.contains("type") || error.message.contains("scalar"), "{error:?}");
    }
}

#[test]
fn checked_mutable_slices_preserve_initializer_contracts() {
    let program = fixture(r#"
pure clip(data: Bytes, split: Bool) -> Result[Bytes] {
    let records = if split { if data.is_empty() { [] } else { [data] } } else { [data] }
    for raw in records {
        var line = raw
        var end = line.len()
        end = end - 1
        line = line[..end]
        return Ok(line)
    }
    Ok(b"")
}
assert clip(b"ab", false)? == b"a"
assert clip(b"ab", true)? == b"a"
assert clip(b"", true)? == b""
"#);
    execute_both(&program);
}

#[test]
fn checked_collectors_prove_record_writers() {
    let program = fixture(r#"
type Row = {value: Int}
pure gather(items: List[Int]) -> List[Row] {
    let result: List[Row] = collect {
        for item in items { yield {value: item} }
    }
    result
}
pure gather_or_stop(items: List[Int]) -> Result[List[Row], Str] {
    let result: List[Row] = collect {
        for item in items {
            if item < 0 { return Err("stop") }
            yield {value: item}
        }
    }
    Ok(result)
}
assert gather([1, 2]) == [{value: 1}, {value: 2}]
assert gather([]) == []
assert gather_or_stop([1, 2])? == [{value: 1}, {value: 2}]
assert gather_or_stop([1, -1, 2]) == Err("stop")
"#);
    execute_both(&program);
    let owner = function(&program, "gather_or_stop");
    let payload = program.store.function_instruction_range(owner).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr).unwrap();
    let integer = program.store.function_instruction_range(owner).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
    let mut malformed = program.clone();
    malformed.store.tags[payload] = FullTag::ExprInt;
    malformed.store.data[payload] = program.store.data[integer];
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
}


#[test]
fn checked_collectors_preserve_empty_outputs() {
    let program = fixture(r#"
type Row = {value: Int}
pure empty() -> List[Row] {
    let result: List[Row] = collect {}
    result
}
assert empty() == []
"#);
    execute_both(&program);
    let owner = function(&program, "empty");
    let decoder = function_view_decoder(&program.store, owner);
    let (gate, checked) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
        Ok(schema::ExprView::CheckedValue { check, value, .. }) if program.store.tags[value.raw() as usize] == FullTag::ExprValueBlock => Some((instruction, check.ty.id)),
        _ => None,
    }).unwrap();
    let Type::List(item) = program.store.semantic.to_type(checked).unwrap() else { panic!("list gate") };
    let mut malformed = program.clone();
    malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
    let nonempty = SemanticPoolBuilder::default().intern_type(&mut malformed.store.semantic, &Type::non_empty(*item)).unwrap();
    let word = program.store.data[gate].range().start as usize + 1;
    assert_eq!(program.store.extra[word], checked.raw());
    malformed.store.extra[word] = nonempty.raw();
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(gate).unwrap(), schema::ExprView::CheckedValue { check, .. } if check.ty.id == nonempty));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("checked value input has no producer contract"), "{error:?}");
}

#[test]
fn checked_collectors_preserve_heterogeneous_record_shapes() {
    let program = fixture(r#"
pure varied() -> List[Record] {
    let result: List[Record] = collect {
        yield {number: 1}
        yield {text: "two"}
    }
    result
}
assert varied().len() == 2
"#);
    execute_both(&program);
}

#[test]
fn checked_collectors_preserve_unsigned_runtime_boundary() {
    let source = r#"
pure unsigned(value: Int) -> List[UInt] {
    let result: List[UInt] = collect { yield value }
    result
}
"#;
    execute_both(&fixture(&format!("{source}\nassert unsigned(7)[0] == 7\n")));
    let failed = Arc::new(fixture(&format!("{source}\nlet failed = unsigned(-1)\n")));
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*failed.sources).clone());
        evaluator.indexed_program = Some(Arc::clone(&failed));
        evaluator.force_frames = force_frames;
        let error = (0..failed.driver_step_count().unwrap()).find_map(|step| evaluator.eval_indexed_driver_step(step, Span::new(failed.store.source_id, 0, 0)).unwrap().err()).unwrap();
        assert_eq!(error.kind, "type-error");
        assert!(error.message.contains("UInt constraint"), "{error:?}");
    }
}

#[test]
fn checked_collectors_authenticate_writes_and_capture_roots() {
    let program = fixture(r#"
type Row = {value: Int, flag: Bool}
var shared: List[Bool] = [false]
pure gather(items: List[Int], flag: Bool) -> List[Row] {
    if shared.len() != 1 { return [] }
    let result: List[Row] = collect {
        for item in items { yield {value: item, flag: flag} }
    }
    result
}
assert gather([7], true)[0].value == 7
"#);
    execute_both(&program);
    let owner = function(&program, "gather");
    let decoder = function_view_decoder(&program.store, owner);
    let range = program.store.function_instruction_range(owner).unwrap();
    let input = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
        Ok(schema::ExprView::CheckedValue { value, .. }) if program.store.tags[value.raw() as usize] == FullTag::ExprValueBlock => Some(value.raw() as usize),
        _ => None,
    }).unwrap();
    let schema::ExprView::ValueBlock { mut body, .. } = decoder.expression_view(input).unwrap() else { unreachable!() };
    assert_eq!(body.len(), 3);
    let declaration = body.next().unwrap().unwrap().raw() as usize;
    let schema::StmtView::Let { slot: accumulator, .. } = decoder.statement_view(declaration).unwrap() else { panic!("fresh accumulator") };
    body.next().unwrap().unwrap();
    let schema::StmtView::Value { value: result } = decoder.statement_view(body.next().unwrap().unwrap().raw() as usize).unwrap() else { panic!("accumulator result") };
    let result = result.raw() as usize;
    assert!(matches!(decoder.expression_view(result).unwrap(), schema::ExprView::Param(slot) if slot == accumulator));
    let capture = program.store.captures[program.store.functions[owner].captures.bounds(program.store.captures.len()).unwrap()].iter()
        .find(|capture| program.store.string(capture.name).unwrap() == "shared").unwrap();
    assert_ne!(capture.slot_and_flags & (1 << 31), 0);
    let capture_slot = (capture.slot_and_flags & !(1 << 31)) as usize;
    assert_ne!(capture_slot, accumulator);
    assert_ne!(capture.binding_identity, IR_NONE);
    let mut malformed = program.clone();
    malformed.store.extra[program.store.data[result].range().start as usize] = capture_slot as u32;
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(result).unwrap(), schema::ExprView::Param(slot) if slot == capture_slot));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("checked value input disagrees with its contract"), "{error:?}");
    let writer = range.clone().find(|&instruction| matches!(decoder.statement_view(instruction), Ok(schema::StmtView::Assign { slot, op: AssignOp::Add, .. }) if slot == accumulator)).unwrap();
    let mut malformed = program.clone();
    let set = malformed.store.assign_ops.len() as u32;
    malformed.store.assign_ops.push(AssignOp::Set);
    malformed.store.extra[program.store.data[writer].range().start as usize + 1] = set;
    assert!(matches!(function_view_decoder(&malformed.store, owner).statement_view(writer).unwrap(), schema::StmtView::Assign { op: AssignOp::Set, .. }));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("checked value input has no producer contract"), "{error:?}");
    let flag = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(1)))).unwrap();
    let yielded = range.clone().find_map(|instruction| {
        let Ok(schema::ExprView::Record(mut fields)) = decoder.expression_view(instruction) else { return None };
        let super::super::decode::RecordEntryView::Field(_, value) = fields.next()?.ok()? else { return None };
        Some(value.raw() as usize)
    }).unwrap();
    let mut malformed = program.clone();
    malformed.store.data[yielded] = program.store.data[flag];
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(yielded).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
}


#[test]
fn checked_collectors_keep_nested_writer_proofs_bounded() {
    for (writers, depth) in [(16, 4), (32, 8), (64, 16), (128, 32)] {
        let mut body = "if flag {\n".repeat(depth);
        for value in 0..writers { body.push_str(&format!("yield {{value: {value}}}\n")); }
        body.push_str(&"}\n".repeat(depth));
        let source = format!("type Row = {{value: Int}}\npure alternate() -> Bool {{ false }}\npure gather(flag: Bool) -> List[Row] {{ let result: List[Row] = collect {{\n{body}}}\nresult\n}}\nassert gather(true).len() == {writers}\nassert gather(false) == []\n");
        let program = fixture(&source);
        execute_both(&program);
        let owner = function(&program, "gather");
        let range = program.store.function_instruction_range(owner).unwrap();
        assert!(range.clone().filter(|&instruction| matches!(program.store.tags[instruction], FullTag::StmtIf | FullTag::StmtIfBool)).count() >= depth);
        let scalar = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
        let boolean = program.store.function_instruction_range(function(&program, "alternate")).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
        let mut malformed = program.clone();
        malformed.store.tags[scalar] = FullTag::ExprBool;
        malformed.store.data[scalar] = program.store.data[boolean];
        let error = FullVerifier::verify(&malformed).unwrap_err();
        assert!(error.message.contains("contract"), "{error:?}");
    }
}

#[test]
fn checked_collectors_preserve_lexical_shadow_boundaries() {
    let program = fixture(r#"
type Row = {value: Int}
pure gather(value: Int) -> List[Row] {
    let result: List[Row] = collect {
        yield {value: value}
        if value > 0 {
            let value = value + 1
            yield {value: value}
        }
        for value in [value] { yield {value: value} }
        yield {value: value}
    }
    result
}
assert gather(3) == [{value: 3}, {value: 4}, {value: 3}, {value: 3}]
"#);
    execute_both(&program);
    let owner = function(&program, "gather");
    let decoder = function_view_decoder(&program.store, owner);
    let range = program.store.function_instruction_range(owner).unwrap();
    let local = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
        Ok(schema::StmtView::LetInt { slot, .. }) => Some(slot),
        _ => None,
    }).unwrap();
    let before = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(0)))).unwrap();
    let mut malformed = program.clone();
    malformed.store.extra[program.store.data[before].range().start as usize] = local as u32;
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(before).unwrap(), schema::ExprView::Param(slot) if slot == local));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("visible lexical declaration"), "{error:?}");
    let (loop_slot, iter) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
        Ok(schema::StmtView::For { slot, iter, .. }) => Some((slot, iter.raw() as usize)),
        _ => None,
    }).unwrap();
    let schema::ExprView::List(mut items) = decoder.expression_view(iter).unwrap() else { panic!("literal loop input") };
    assert_eq!(items.len(), 1);
    let before_loop = items.next().unwrap().unwrap().raw() as usize;
    assert!(matches!(decoder.expression_view(before_loop).unwrap(), schema::ExprView::Param(0)));
    let mut malformed = program.clone();
    malformed.store.extra[program.store.data[before_loop].range().start as usize] = loop_slot as u32;
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(before_loop).unwrap(), schema::ExprView::Param(slot) if slot == loop_slot));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("visible lexical declaration") || error.message.contains("active lexical slot"), "{error:?}");
}

#[test]
fn checked_comprehensions_preserve_nonempty_mapping() {
    let program = fixture(r#"
pure copy_nonempty(items: NonEmpty[Int]) {
    [item for item in items]
}
pure filter_nonempty(items: NonEmpty[Int]) {
    [item for item in items if item > 0]
}
pure nested_nonempty(items: NonEmpty[Int]) {
    [item for item in items for item in {if item > 0 {[item]} else {[]}}]
}
pure through_splice(items: NonEmpty[Int]) -> Int {
    copy_nonempty([@items]).first()
}
let copied: NonEmpty[Int] = copy_nonempty([4, 5])
let filtered: List[Int] = filter_nonempty([-1])
let nested: List[Int] = nested_nonempty([-1])
assert through_splice([6]) == 6
assert copied.first() == 4
assert copied.last() == 5
assert filtered == []
assert nested == []
"#);
    execute_both(&program);
}

#[test]
fn checked_nonempty_literals_authenticate_members_and_cardinality() {
    let program = fixture(r#"
pure first(items: NonEmpty[Int]) -> Int { items.first() }
pure boolean() -> Bool { true }
pure concrete() -> Int {
    let spare: List[Int] = []
    first([7])
}
pure spliced(items: NonEmpty[Int], plain: List[Int]) -> Int {
    let _ = plain
    first([@items])
}
assert concrete() == 7
assert spliced([8], []) == 8
"#);
    execute_both(&program);
    let owner = function(&program, "concrete");
    let range = program.store.function_instruction_range(owner).unwrap();
    let decoder = function_view_decoder(&program.store, owner);
    let mut empty = None;
    let mut nonempty = None;
    let mut member = None;
    for instruction in range {
        if let Ok(schema::ExprView::List(mut items)) = decoder.expression_view(instruction) {
            if items.len() == 0 { empty = Some(instruction); }
            else {
                nonempty = Some(instruction);
                member = Some(items.next().unwrap().unwrap().raw() as usize);
            }
        }
    }
    let (empty, nonempty, member) = (empty.unwrap(), nonempty.unwrap(), member.unwrap());
    let mut malformed = program.clone();
    malformed.store.data.swap(empty, nonempty);
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(nonempty).unwrap(), schema::ExprView::List(items) if items.len() == 0));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("ground list producer has no item contract"), "{error:?}");
    let mut malformed = program.clone();
    malformed.store.tags[member] = FullTag::ExprBool;
    let boolean = program.store.function_instruction_range(function(&program, "boolean")).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
    malformed.store.data[member] = program.store.data[boolean];
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
    let owner = function(&program, "spliced");
    let decoder = function_view_decoder(&program.store, owner);
    let splice = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
        let Ok(schema::ExprView::ListBuild(mut entries)) = decoder.expression_view(instruction) else { return None };
        let (true, value, _) = entries.next().unwrap().unwrap() else { panic!("splice") };
        Some(value.raw() as usize)
    }).unwrap();
    assert!(matches!(decoder.expression_view(splice).unwrap(), schema::ExprView::Param(0)));
    let mut malformed = program.clone();
    let plain = program.store.function_instruction_range(owner).unwrap().find(|&instruction|
        matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(1)))).unwrap();
    malformed.store.data[splice] = program.store.data[plain];
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(splice).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("contract") || error.message.contains("non-empty producer"), "{error:?}");
}

#[test]
fn checked_comprehensions_authenticate_nonempty_mapping_source() {
    let program = fixture(r#"
pure mapped(items: NonEmpty[Int], plain: List[Int]) -> NonEmpty[Int] {
    let _ = plain
    [item for item in items]
}
assert mapped([9], []).first() == 9
"#);
    execute_both(&program);
    let owner = function(&program, "mapped");
    let decoder = function_view_decoder(&program.store, owner);
    let range = program.store.function_instruction_range(owner).unwrap();
    let comprehension = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprListComp).unwrap();
    let schema::ExprView::ListComp { mut qualifiers, .. } = decoder.expression_view(comprehension).unwrap() else { unreachable!() };
    let super::super::decode::CompQualifierView::For { iter, .. } = qualifiers.next().unwrap().unwrap() else { panic!("mapping source") };
    let iter = iter.raw() as usize;
    assert!(matches!(decoder.expression_view(iter).unwrap(), schema::ExprView::Param(0)));
    let plain = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(1)))).unwrap();
    let mut malformed = program.clone();
    malformed.store.data[iter] = program.store.data[plain];
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(iter).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&malformed).expect_err("mapping a possibly empty source cannot retain NonEmpty");
    assert!(error.message.contains("ground list comprehension does not preserve its validation"), "{error:?}");
}

#[test]
fn physical_process_spawn_and_wait_contracts_preserve_results() {
    on_large_stack(|| {
        fixture(r#"
proc wrapped_launch() [process] -> Result[Result[ProcessHandle, ProcessError]] {
    let started = spawn run true
    Ok(started)
}
"#);
        let source = r#"
proc launch() [process] -> Result[ProcessHandle, ProcessError] {
    spawn run true
}
proc launch_plan(plan: Command) [process] -> Result[ProcessHandle, ProcessError] {
    spawn plan
}
pure build_argv(target: Str, words: List[Str]) -> Command {
    process.command_argv(target, words)
}
proc launch_builder() [process, error] -> Result[ProcessHandle, ProcessError] {
    spawn process.command { timeout = 2s; run true }
}
let child = launch()?
let status = wait child?
assert status.ok
let plan = build_argv("sh", ["sh", "-c", "exit 1"])
let planned = launch_plan(plan)?
let built = launch_builder()?
let statuses = wait [planned, built]?
assert !statuses[0].ok
assert statuses[1].ok
let empty = wait []?
assert empty.len() == 0
"#;
        let mut program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        let owner = function(&program, "build_argv");
        let function = program.store.functions[owner];
        for parameter in 0..function.params.len as usize {
            program.store.slot_type_facts[function.slot_type_facts.start as usize + parameter] = IR_NONE;
        }
        let range = program.store.function_instruction_range(owner).unwrap();
        program.store.checked_expr_types.retain(|fact| fact.is_contract() || !range.contains(&(fact.instruction as usize)));
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
    });
}

fn process_parameter_mutation(program: &FullProgram, owner: &str, from: u32, to: u32) -> FullProgram {
    let owner = function(program, owner);
    let decoder = function_view_decoder(&program.store, owner);
    let instruction = program.store.function_instruction_range(owner).unwrap().find(|&instruction| {
        matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(slot)) if slot == from as usize)
    }).unwrap();
    let mut forged = program.clone();
    forged.store.extra[program.store.data[instruction].range().start as usize] = to;
    forged
}

#[test]
fn physical_process_producers_reject_wrong_handles_commands_and_run_operands() {
    on_large_stack(|| {
        let source = r#"
proc launch_words(target: Str, words: List[Str], input: Bytes, limit: Duration, cpus: Int, accepted: List[Int], paths: List[Path], wrong: List[Bytes], erased: Any) [process, error] -> Result[ProcessHandle, ProcessError] {
    spawn run --timeout=limit --cpumax=cpus --accept=accepted XSH_PROCESS_PATH=(paths) $target @words < (input)
}
proc launch_command(plan: Command, wrong: Str, erased: Any) [process] -> Result[ProcessHandle, ProcessError] {
    spawn plan
}
proc await_one(handle: ProcessHandle, wrong: Str, erased: Any) [process] -> Result[Status, ProcessError] {
    wait handle
}
proc await_many(handles: List[ProcessHandle], wrong: List[Int], erased: Any) [process] -> Result[List[Status], ProcessError] {
    wait handles
}
proc build_plan(limit: Duration, wrong: Bool) [process, error] -> Result[ProcessHandle, ProcessError] {
    spawn process.command { timeout = limit; run true }
}
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        for (name, from, to, message) in [
            ("launch_words", 0, 8, "generic call substitution contains dynamic type"),
            ("launch_words", 1, 7, "process word disagrees with its physical conversion role"),
            ("launch_words", 2, 7, "process word disagrees with its physical conversion role"),
            ("launch_words", 3, 4, "ground producer value disagrees with its checked contract"),
            ("launch_words", 4, 3, "ground producer value disagrees with its checked contract"),
            ("launch_words", 5, 7, "ground producer value disagrees with its checked contract"),
            ("launch_words", 6, 7, "process word disagrees with its physical conversion role"),
            ("launch_command", 0, 1, "ground producer value disagrees with its checked contract"),
            ("launch_command", 0, 2, "ground producer value disagrees with its checked contract"),
            ("await_one", 0, 1, "wait target is neither ProcessHandle nor List[ProcessHandle]"),
            ("await_one", 0, 2, "generic call substitution contains dynamic type"),
            ("await_many", 0, 1, "wait target is neither ProcessHandle nor List[ProcessHandle]"),
            ("await_many", 0, 2, "generic call substitution contains dynamic type"),
            ("build_plan", 0, 1, "process option disagrees with its canonical contract"),
        ] {
            let error = FullVerifier::verify(&process_parameter_mutation(&program, name, from, to)).unwrap_err();
            assert!(error.message.contains(message), "{name} parameter {from}->{to}: {error:?}");
        }
        for name in ["launch_words", "launch_command", "await_one", "await_many", "build_plan"] {
            let error = FullVerifier::verify(&without_effect(&program, function(&program, name), Effect::Process)).unwrap_err();
            assert!(error.message.contains("requires effect process"), "{name}: {error:?}");
        }
        let error = FullVerifier::verify(&without_effect(&program, function(&program, "launch_words"), Effect::Error)).unwrap_err();
        assert!(error.message.contains("requires effect error"), "{error:?}");
    });
}

#[test]
fn physical_process_results_match_uncalled_owner_contracts() {
    on_large_stack(|| {
        let source = r#"
proc await_one(handle: ProcessHandle) [process] -> Result[Status, ProcessError] { wait handle }
proc await_many(handles: List[ProcessHandle]) [process] -> Result[List[Status], ProcessError] { wait handles }
proc launch_command(plan: Command) [process] -> Result[ProcessHandle, ProcessError] { spawn plan }
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        for name in ["await_one", "await_many", "launch_command"] {
            let owner = function(&program, name);
            let FullCallableContract::Ground(id) = program.store.functions[owner].contract else { panic!("ground process function") };
            let mut signature = program.store.semantic.to_signature_with_budget(id, 0,
                &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default()).unwrap();
            signature.return_ty = Box::new(Type::Result(Box::new(Type::Bool), Box::new(Type::ProcessError)));
            let mut forged = program.clone();
            let signature = SemanticPoolBuilder::default().intern_signature(&mut forged.store.semantic, &signature).unwrap();
            forged.store.functions[owner].contract = FullCallableContract::Ground(signature);
            forged.store.functions[owner].checked_return_type = forged.store.semantic.signature_return_type(signature).unwrap().raw();
            let error = FullVerifier::verify(&forged).unwrap_err();
            assert_eq!(error.message, "ground producer value disagrees with its checked contract", "{name}: {error:?}");
        }
    });
}

#[test]
fn physical_process_builder_preserves_the_canonical_record_environment_boundary() {
    on_large_stack(|| {
        let source = r#"
proc launch_with_env(environment: Record, erased: Any, wrong: Bool) [process, error] -> Result[ProcessHandle, ProcessError] {
    spawn process.command {
        env = environment
        run sh -c r"""test "$XSH_RECORD_ENV" = preserved"""
    }
}
let child = launch_with_env({XSH_RECORD_ENV: "preserved"}, {}, false)?
let status = wait child?
assert status.ok
"#;
        let program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        for slot in [1, 2] {
            let error = FullVerifier::verify(&process_parameter_mutation(&program, "launch_with_env", 0, slot)).unwrap_err();
            assert_eq!(error.message, "process option disagrees with its canonical contract", "{error:?}");
        }
    });
}

#[test]
fn environment_assignment_proves_native_effects_and_closed_value_domain() {
    on_large_stack(|| {
        let source = r#"
proc configure(text: Str, paths: List[Path], wrong: List[Int], erased: Any) [env, error] -> Result[Str] {
    let _ = wrong
    let _ = erased
    e"XSH_NATIVE_TEXT" = text
    e"XSH_NATIVE_NUMBER" = 8080
    e"XSH_NATIVE_PATHS" = paths
    Ok(e"XSH_NATIVE_TEXT"?)
}
assert configure("kept", [p"/one", p"/two"], [1], {})? == "kept"
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "configure");
        let decoder = function_view_decoder(&program.store, owner);
        let value = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::ExprView::ModuleCall { op: RuntimeOp::EnvSet, mut args, .. } = decoder.expression_view(instruction).ok()? else { return None };
            args.next().unwrap().unwrap().unwrap();
            Some(args.next().unwrap().unwrap().unwrap().raw() as usize)
        }).unwrap();
        assert!(matches!(decoder.expression_view(value).unwrap(), schema::ExprView::Param(0)));
        for slot in [2, 3] {
            let mut forged = program.clone();
            forged.store.extra[program.store.data[value].range().start as usize] = slot;
            let error = FullVerifier::verify(&forged).unwrap_err();
            assert!(error.message.contains("environment assignment value has no closed conversion domain"), "{error:?}");
        }
        let error = FullVerifier::verify(&without_effect(&program, owner, Effect::Env)).unwrap_err();
        assert!(error.message.contains("requires effect env"), "{error:?}");
    });
}

#[test]
fn managed_release_proves_canonical_resource_and_native_effects() {
    on_large_stack(|| {
        let source = r#"
proc publish(site: Path) [fs, error] -> Result[Int] {
    with root = fs.open_root(site)?, held = fs.lock(fp"{site}/.lock")? {
        root.write(p"value", "kept")
    }
    let text = with root = fs.open_root(site)? { assert root.read_text(p"value")? == "kept"; "kept" }?
    Ok(text.byte_len())
}
tempdir site { assert publish(site)? == 4 }
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "publish");
        let decoder = function_view_decoder(&program.store, owner);
        let payload = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::ExprView::Ok(value) = decoder.expression_view(instruction).ok()? else { return None };
            (program.store.tags[value.raw() as usize] == FullTag::ExprStr).then_some(value.raw() as usize)
        }).unwrap();
        let integer = program.store.tags.iter().position(|tag| *tag == FullTag::ExprInt).unwrap();
        let mut wrong_payload = program.clone();
        wrong_payload.store.tags[payload] = FullTag::ExprInt;
        wrong_payload.store.data[payload] = program.store.data[integer];
        let error = FullVerifier::verify(&wrong_payload).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees") || error.message.contains("storage contract"), "{error:?}");
        let handover = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::StmtView::Assign { value, .. } = decoder.statement_view(instruction).ok()? else { return None };
            (program.store.tags[value.raw() as usize] == FullTag::ExprBool).then_some(value.raw() as usize)
        }).unwrap();
        let mut wrong_handover = program.clone();
        wrong_handover.store.tags[handover] = FullTag::ExprInt;
        wrong_handover.store.data[handover] = program.store.data[integer];
        let error = FullVerifier::verify(&wrong_handover).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees") || error.message.contains("storage contract"), "{error:?}");
        for (from, to) in [(RuntimeOp::FsCloseRootIfOpen, RuntimeOp::FsUnlockIfHeld), (RuntimeOp::FsUnlockIfHeld, RuntimeOp::FsCloseRootIfOpen)] {
            let instruction = program.store.function_instruction_range(owner).unwrap().find(|&instruction| {
                matches!(decoder.expression_view(instruction), Ok(schema::ExprView::ModuleCall { op, .. }) if op == from)
            }).unwrap();
            let mut forged = program.clone();
            let pool = program.store.extra[program.store.data[instruction].range().start as usize] as usize;
            forged.store.runtime_ops[pool] = to;
            let error = FullVerifier::verify(&forged).unwrap_err();
            assert!(error.message.contains("native operation arguments have no matching contract"), "{error:?}");
        }
        let error = FullVerifier::verify(&without_effect(&program, owner, Effect::Fs)).unwrap_err();
        assert!(error.message.contains("requires effect fs"), "{error:?}");
    });
}

#[test]
fn typed_module_dynamic_calls_preserve_export_contracts() {
    on_large_stack(|| {
        let work = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&work).unwrap();
        let directory = tempfile::tempdir_in(work).unwrap();
        let module = directory.path().join("calculator.xsh");
        std::fs::write(&module, r#"##! Checked dynamic module calls.
## Adds a defaulted amount.
export pure add(value: Int = 4, by: Int = 2) -> Int { value + by }
## Reads an environment value before observing the input.
export proc observe(value: Int) [env] -> Int { let _ = e"PATH"; value + 1 }
"#).unwrap();
        let source = format!(r#"
type Calculator = module {{
    export pure add(value: Int = 4, by: Int = 2) -> Int
    export proc observe(value: Int) [env] -> Int
}}
proc apply(plugin: Calculator, value: Int, wrong: Str) [env] -> Int {{
    let _ = wrong
    let added = plugin.add(by: value)
    plugin.observe(added)
}}
pure apply_pure(plugin: Calculator) -> Int {{ plugin.add(4, by: 3) }}
pure apply_trailing(plugin: Calculator) -> Int {{ plugin.add(5) }}
let plugin = module.load(p{module:?})?.require(Calculator)?
assert apply(plugin, 4, "wrong") == 9
assert apply_pure(plugin) == 7
assert apply_trailing(plugin) == 7
"#);
        let program = fixture(&source);
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "apply");
        let decoder = function_view_decoder(&program.store, owner);
        let default_args = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::ExprView::DynamicCall { args, .. } = decoder.expression_view(instruction).ok()? else { return None };
            let block = args.block_identity_for_test();
            args.into_iter().any(|argument| matches!(argument, Ok(super::super::decode::CallArgView::Default(0)))).then_some(block)
        }).unwrap();
        let words = program.store.blocks[default_args.index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[words.start + 1..words.start + 3], [2, 0]);
        let mut wrong_default = program.clone();
        wrong_default.store.extra[words.start + 2] = 1;
        let error = FullVerifier::verify(&wrong_default).unwrap_err();
        assert!(error.message.contains("default argument has no parameter default"), "{error:?}");
        let error = FullVerifier::verify(&without_effect(&program, owner, Effect::Env)).unwrap_err();
        assert!(error.message.contains("requires effect env"), "{error:?}");
        let observe = program.store.function_instruction_range(owner).unwrap().find(|&instruction| {
            matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Field { name: "observe", .. }))
        }).unwrap();
        let pure_owner = function(&program, "apply_pure");
        let pure_decoder = function_view_decoder(&program.store, pure_owner);
        let pure_field = program.store.function_instruction_range(pure_owner).unwrap().find_map(|instruction| {
            let schema::ExprView::DynamicCall { callee, .. } = pure_decoder.expression_view(instruction).ok()? else { return None };
            Some(callee.raw() as usize)
        }).unwrap();
        let mut impure = program.clone();
        impure.store.extra[program.store.data[pure_field].range().start as usize + 1] =
            program.store.extra[program.store.data[observe].range().start as usize + 1];
        let error = FullVerifier::verify(&impure).unwrap_err();
        assert!(error.message.contains("pure executable body contains an impure operation"), "{error:?}");
    });
}

#[test]
fn typed_module_dynamic_calls_prove_arguments_without_result_demand() {
    on_large_stack(|| {
        let mut program = fixture(r#"
type Calculator = module { export pure add(value: Int) -> Int; export pure wrong(value: Int) -> Str }
proc apply(plugin: Calculator, value: Int, wrong: Str) [] -> Int { let _ = wrong; let _ = "wrong"; plugin.add(value) }
"#);
        assert!(program.store.registered_calls.is_empty());
        assert!(!program.store.tags.iter().any(|tag| matches!(tag, FullTag::ExprOk | FullTag::ExprErr | FullTag::ExprTypedCall)));
        program.store.checked_expr_types.clear();
        FullVerifier::verify(&program).unwrap();
        let owner = function(&program, "apply");
        let decoder = function_view_decoder(&program.store, owner);
        let (arguments, callee, argument) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::ExprView::DynamicCall { callee, mut args, .. } = decoder.expression_view(instruction).ok()? else { return None };
            let block = args.block_identity_for_test();
            let super::super::decode::CallArgView::Single(value) = args.next().unwrap().unwrap() else { panic!("single argument") };
            Some((block, callee.raw() as usize, value.raw() as usize))
        }).unwrap();
        assert!(matches!(decoder.expression_view(argument).unwrap(), schema::ExprView::Param(1)));
        let mut wrong = program.clone();
        wrong.store.extra[program.store.data[argument].range().start as usize] = 2;
        let error = FullVerifier::verify(&wrong).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees"), "{error:?}");
        let words = program.store.blocks[arguments.index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[words.start..words.start + 2], [1, 0]);
        let mut wrong_splice = program.clone();
        wrong_splice.store.extra[words.start + 1] = 1;
        let error = FullVerifier::verify(&wrong_splice).unwrap_err();
        assert!(error.message.contains("splice has no rest parameter"), "{error:?}");
        let schema::ExprView::Field { base, .. } = decoder.expression_view(callee).unwrap() else { panic!("module field") };
        let mut nonmodule = program.clone();
        nonmodule.store.extra[program.store.data[base.raw() as usize].range().start as usize] = 2;
        let error = FullVerifier::verify(&nonmodule).unwrap_err();
        assert!(error.message.contains("unknown effect contract"), "{error:?}");
        let words = program.store.data[callee].range().bounds(program.store.extra.len()).unwrap();
        let name = program.store.strings.iter().position(|range| program.store.string_bytes[range.bounds(program.store.string_bytes.len()).unwrap()] == *b"wrong").unwrap();
        let mut wrong_result = program.clone();
        wrong_result.store.extra[words.start + 1] = IrStringId::new(name).unwrap().raw();
        let error = FullVerifier::verify(&wrong_result).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees"), "{error:?}");
    });
}

#[test]
fn unknown_dynamic_calls_keep_unknown_effect_contracts() {
    on_large_stack(|| {
        let mut program = fixture("proc erased(action: Proc) -> Result[Any] { action.call() }\n");
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        let owner = function(&program, "erased");
        let FullCallableContract::Ground(signature) = program.store.functions[owner].contract else { panic!("ground function") };
        let mut signature = program.store.semantic.to_signature_with_budget(signature, 0,
            &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default()).unwrap();
        signature.effects = Some(Vec::new());
        program.store.functions[owner].contract = FullCallableContract::Ground(
            SemanticPoolBuilder::default().intern_signature(&mut program.store.semantic, &signature).unwrap());
        let error = FullVerifier::verify(&program).unwrap_err();
        assert!(error.message.contains("unknown effect contract"), "{error:?}");
    });
}


#[test]
fn driver_projections_derive_wait_results_from_actual_writers() {
    on_large_stack(|| {
        let source = r#"
let started = spawn run true
let child = started?
let statuses = wait [child]?
assert statuses[0].ok
"#;
        let mut program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let wait = program.store.tags.iter().position(|tag| *tag == FullTag::ExprWait).unwrap();
        let decoder = driver_projection_decoder(&program, wait);
        let schema::ExprView::Wait { target, .. } = decoder.expression_view(wait).unwrap() else { panic!("wait producer") };
        let schema::ExprView::List(mut values) = decoder.expression_view(target.raw() as usize).unwrap() else { panic!("wait target list") };
        let argument = values.next().unwrap().unwrap().raw() as usize;
        assert!(matches!(decoder.expression_view(argument).unwrap(), schema::ExprView::Param(_)));
        let mut wrong_handle = program.clone();
        wrong_handle.store.extra[program.store.data[argument].range().start as usize] = driver_projection_slot(&program, &decoder, "started");
        let error = FullVerifier::verify(&wrong_handle).unwrap_err();
        assert_eq!(error.message, "wait target is neither ProcessHandle nor List[ProcessHandle]", "{error:?}");
        let index = program.store.tags.iter().position(|tag| *tag == FullTag::ExprIndex).unwrap();
        let decoder = driver_projection_decoder(&program, index);
        let schema::ExprView::Index { base, .. } = decoder.expression_view(index).unwrap() else { panic!("status readback index") };
        let base = base.raw() as usize;
        assert!(matches!(decoder.expression_view(base).unwrap(), schema::ExprView::Param(_)));
        let mut wrong_readback = program.clone();
        wrong_readback.store.extra[program.store.data[base].range().start as usize] = driver_projection_slot(&program, &decoder, "child");
        let error = FullVerifier::verify(&wrong_readback).unwrap_err();
        assert_eq!(error.message, "callable index producer has no checked collection element type", "{error:?}");
    });
}

fn driver_projection_decoder(program: &FullProgram, instruction: usize) -> FullDecoder<'_> {
    (0..program.store.driver_steps.len()).map(|step| verify_schemes::decoder_for_driver_step(&program.store, step).unwrap())
        .find(|decoder| decoder.instruction_range.contains(&instruction)).unwrap()
}

fn driver_projection_slot(program: &FullProgram, decoder: &FullDecoder<'_>, name: &str) -> u32 {
    let driver = program.store.driver_steps[driver_owner_index(decoder.owner).unwrap()];
    let range = driver.slots.bounds(program.store.driver_slots.len()).unwrap();
    program.store.driver_slots[range].iter().find(|slot| program.store.string(slot.name).unwrap() == name).unwrap().slot
}

#[test]
fn driver_projections_authenticate_common_value_roots_and_all_writers() {
    on_large_stack(|| {
        let source = r#"
type DriverRow = {value: Int}
var state = {rows: [{value: 1}], enabled: true, kind: "stable", unused: "unused"}
state = {rows: [{value: 2}], enabled: true, kind: "stable", unused: "unused"}
proc change(value: Int, wrong: Bool) [] {
    state.rows[0].value = value
}
pure project(rows: List[DriverRow], wrong: List[Str]) -> List[DriverRow] { rows }
change(3, false)
let alias = state
let {rows, enabled, kind, unused} = alias
let projected = project(rows, ["wrong"])
guard let {values, active} = Ok({values: projected, active: enabled}) else { exit 3 }
assert values[0].value == 3
assert active
assert state.enabled
assert kind == state.kind
"#;
        let mut program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let definition = |name: &str| program.store.driver_slots.iter().position(|slot|
            slot.flags & DRIVER_SLOT_DEFINITION != 0 && program.store.string(slot.name).unwrap() == name).unwrap();
        let mut wrong_unused = program.clone();
        wrong_unused.store.driver_slots[definition("unused")].type_id = program.store.driver_slots[definition("enabled")].type_id;
        let error = FullVerifier::verify(&wrong_unused).unwrap_err();
        assert_eq!(error.message, "storage declaration disagrees with its read contract", "{error:?}");
        let alias_identity = program.store.binding_identities.iter().position(|identity| program.store.string(identity.name).unwrap() == "alias").unwrap() as u32;
        let alias_step = program.store.driver_steps.iter().find(|step| {
            let range = step.writers.bounds(program.store.driver_writers.len()).unwrap();
            program.store.driver_writers[range].iter().any(|writer| writer.binding_identity == alias_identity)
        }).unwrap();
        let range = alias_step.slots.bounds(program.store.driver_slots.len()).unwrap();
        let read = range.into_iter().find(|slot| program.store.driver_slots[*slot].flags & DRIVER_SLOT_READ != 0).unwrap();
        let mut cyclic = program.clone();
        cyclic.store.driver_slots[read].binding_identity = alias_identity;
        cyclic.store.driver_slots[read].name = program.store.binding_identities[alias_identity as usize].name;
        let error = FullVerifier::verify(&cyclic).unwrap_err();
        assert_eq!(error.message, "driver sync type identities conflict", "{error:?}");
        for name in ["change", "project"] {
            let error = FullVerifier::verify(&process_parameter_mutation(&program, name, 0, 1)).unwrap_err();
            assert_eq!(error.message, "ground producer value disagrees with its checked contract", "{name}: {error:?}");
        }
        let mut fields = Vec::new();
        for step in 0..program.store.driver_steps.len() {
            let decoder = verify_schemes::decoder_for_driver_step(&program.store, step).unwrap();
            for instruction in decoder.instruction_range.clone() {
                if let Ok(schema::ExprView::Field { name, .. }) = decoder.expression_view(instruction) {
                    fields.push((name.to_string(), instruction));
                }
            }
        }
        let boolean = fields.iter().find(|(name, _)| name == "enabled").unwrap().1;
        let text = fields.iter().find(|(name, _)| name == "kind").unwrap().1;
        let mut wrong_field = program.clone();
        wrong_field.store.extra[program.store.data[boolean].range().start as usize + 1] = program.store.extra[program.store.data[text].range().start as usize + 1];
        let error = FullVerifier::verify(&wrong_field).unwrap_err();
        assert_eq!(error.message, "condition producer disagrees with its control role", "{error:?}");
    });
}

#[test]
fn driver_projections_contextual_argument_carriers_prove_actual_words() {
    on_large_stack(|| {
        let source = r#"
pure descriptor(word: Path, wrong: Bool, more: List[Str], erased: Any, erased_more: List[Any]) -> Command {
    let stored: List[Any] = ["true", word]
    process.command_argv("true", ["true", word, @more, @[]], env: {})
}
let plan = descriptor(p"/bin/true", false, ["argument"], p"/bin/true", ["argument"])
"#;
        let mut program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "descriptor");
        let decoder = function_view_decoder(&program.store, owner);
        let call = decoder.instruction_range.clone().find(|instruction| program.store.tags[*instruction] == FullTag::ExprProcessCommandArgv).unwrap();
        let schema::ExprView::ProcessCommandArgv(command) = decoder.expression_view(call).unwrap() else { panic!("command argv") };
        let argument = command.argv.raw() as usize;
        let stored = decoder.instruction_range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, annotation: Some(_), .. }) => Some(slot),
            _ => None,
        }).unwrap();
        for slot in [stored as u32, 1] {
            let mut noncarrier = program.clone();
            noncarrier.store.extra[program.store.data[argument].range().start as usize] = slot;
            let error = FullVerifier::verify(&noncarrier).unwrap_err();
            assert!(error.message.starts_with("command argv has no concrete word list contract"), "{error:?}");
        }
        for (from, to) in [(0, 1), (0, 3), (2, 4)] {
            let mut wrong_word = program.clone();
            for instruction in decoder.instruction_range.clone() {
                if matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(slot)) if slot == from) {
                    wrong_word.store.extra[program.store.data[instruction].range().start as usize] = to as u32;
                }
            }
            let error = FullVerifier::verify(&wrong_word).unwrap_err();
            assert_eq!(error.message, "command argv element is not a concrete process word", "{from}->{to}: {error:?}");
        }
    });
}

#[test]
fn driver_projections_authenticate_zero_output_sources_and_envelopes() {
    on_large_stack(|| {
        let source = r#"
var state = {value: 1}
let alias = state
let {} = alias
guard let _ = Ok(alias) else { exit 3 }
"#;
        let mut program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let step = program.store.driver_steps.iter().find(|step| step.tag == FullDriverTag::LetRecord).unwrap();
        let source = program.store.payload(step.data.range()).unwrap()[0] as usize;
        assert!(matches!(program.store.tags[source], FullTag::ExprParam));
        let mut wrong_source = program.clone();
        wrong_source.store.tags[source] = FullTag::ExprBool;
        wrong_source.store.extra[program.store.data[source].range().start as usize] = 1;
        let error = FullVerifier::verify(&wrong_source).unwrap_err();
        assert_eq!(error.message, "driver record source field path is not a record", "{error:?}");
        let guard = program.store.tags.iter().position(|tag| *tag == FullTag::StmtGuard).unwrap();
        let mut wrong_envelope = program.clone();
        let range = program.store.data[guard].range().bounds(program.store.extra.len()).unwrap();
        wrong_envelope.store.extra[range.end - 1] = 1;
        let error = FullVerifier::verify(&wrong_envelope).unwrap_err();
        assert_eq!(error.message, "driver guard writer has no success payload", "{error:?}");
    });
}

#[test]
fn driver_projections_contextual_lists_reject_guarded_multiarm_carriers() {
    on_large_stack(|| {
        let source = r#"
pure choose(flag: Bool) -> Command {
    let replacement = p"/bin/true"
    match ["true", "argument"] {
        values if flag => process.command_argv("true", values, env: {})
        values => process.command_argv("true", values, env: {})
    }
}
let plan = choose(false)
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "choose");
        let decoder = function_view_decoder(&program.store, owner);
        let path = decoder.instruction_range.clone().find(|instruction| program.store.tags[*instruction] == FullTag::ExprPath).unwrap();
        let subject = decoder.instruction_range.clone().find_map(|instruction| {
            let schema::ExprView::MatchExpr { value, mut arms, .. } = decoder.expression_view(instruction).ok()? else { return None };
            let (_, guard, _) = arms.next()?.ok()?;
            (guard.is_some() && arms.next().is_some()).then_some(value.raw() as usize)
        }).unwrap();
        let schema::ExprView::List(mut values) = decoder.expression_view(subject).unwrap() else { panic!("match list subject") };
        let word = values.next().unwrap().unwrap().raw() as usize;
        let mut mixed = program.clone();
        mixed.store.tags[word] = program.store.tags[path];
        mixed.store.data[word] = program.store.data[path];
        let error = FullVerifier::verify(&mixed).unwrap_err();
        assert!(error.message.starts_with("command argv has no concrete word list contract"), "{error:?}");
    });
}

#[test]
fn driver_projections_wide_unused_destructuring_keeps_bounded_proof_work() {
    on_large_stack(|| {
        let fields = (0..256).map(|index| format!("field{index}: {index}")).collect::<Vec<_>>().join(", ");
        let names = (0..256).map(|index| format!("field{index}")).collect::<Vec<_>>().join(", ");
        let source = format!("var state = {{{fields}}}\nlet alias = state\nlet {{{names}}} = alias\nassert field255 == 255\n");
        let mut program = fixture(&source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
    });
}

#[test]
fn physical_optional_record_lifts_validate_nested_fields() {
    on_large_stack(|| {
        let mut program = fixture(r#"
type Row = {value: Int, items: List[Int]}
pure foreign() -> Str { "bad" }
pure keep() -> Row? {
    var active: Row? = null
    active = {value: 7, items: []}
    active
}
assert keep() != null
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "keep");
        let scalar = program.store.function_instruction_range(owner).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
        let foreign = program.store.tags.iter().position(|tag| *tag == FullTag::ExprStr).unwrap();
        let mut wrong_field = program.clone();
        wrong_field.store.tags[scalar] = FullTag::ExprStr;
        wrong_field.store.data[scalar] = program.store.data[foreign];
        let error = FullVerifier::verify(&wrong_field).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
    });
}

#[test]
fn physical_optional_pattern_reads_require_a_dominating_exit() {
    on_large_stack(|| {
        let mut program = fixture(r#"
enum Node { Item(Int?) }
pure take(value: Int) -> Result[Int] { Ok(value) }
pure initial() -> Int? { null }
pure keep(node: Node) -> Result[Int] {
    match node {
        Item(child) => {
            var scratch = initial()
            if child == null { return Ok(0) }
            scratch = null
            take(child)
        }
    }
}
assert keep(Item(7))? == 7
assert keep(Item(null))? == 0
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "keep");
        let range = program.store.function_instruction_range(owner).unwrap();
        let guard = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::BoolLiteralCompareSlot).unwrap();
        let condition = program.store.data[guard].range().start as usize;
        let guarded_slot = program.store.extra[condition + 1];
        let mut wrong_guard = program.clone();
        let op = program.store.extra[condition] as usize;
        wrong_guard.store.binary_ops[op] = BinaryOp::Ne;
        let error = FullVerifier::verify(&wrong_guard).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
        let writer = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::StmtAssign).unwrap();
        let mut stale_read = program.clone();
        stale_read.store.extra[program.store.data[writer].range().start as usize] = guarded_slot;
        let error = FullVerifier::verify(&stale_read).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
        let mut no_hints = program.clone();
        let facts = program.store.functions[owner].slot_type_facts;
        no_hints.store.slot_type_facts[facts.start as usize..(facts.start + facts.len) as usize].fill(IR_NONE);
        FullVerifier::verify(&no_hints).unwrap();
        execute_both(&no_hints);
        let source = r#"
enum Node { Item(Int?, Int) }
pure take(value: Int) -> Result[Int] { Ok(value) }
pure keep(node: Node) -> Result[Int] {
    match node {
        Item(child, checked) => {
            if GUARD { return Ok(0) }
            let _ = take(checked)
            let _ = take(checked)
            let _ = take(checked)
            take(checked)
        }
    }
}
assert keep(Item(7, 7))? == 7
assert keep(Item(null, 7))? == 0
"#.replace("GUARD", &format!("child == null{}", " == false".repeat(32)));
        let mut deep = fixture(&source);
        execute_both(&deep);
        let owner = function(&deep, "keep");
        let range = deep.store.function_instruction_range(owner).unwrap();
        assert_eq!(range.clone().filter(|&instruction| deep.store.tags[instruction] == FullTag::BoolNot).count(), 32);
        let decoder = function_view_decoder(&deep.store, owner);
        let comparison = range.clone().find(|&instruction| deep.store.tags[instruction] == FullTag::BoolLiteralCompareSlot).unwrap();
        let slot = deep.store.extra[deep.store.data[comparison].range().start as usize + 1];
        let target = function(&deep, "take");
        let arguments = range.filter_map(|instruction| {
            let schema::ExprView::DirectPureCall { function, mut args, .. } = decoder.expression_view(instruction).ok()? else { return None };
            if function.id.index() != target { return None; }
            let decode::CallArgView::Single(value) = args.next()?.unwrap() else { panic!("single argument") };
            Some(value.raw() as usize)
        }).collect::<Vec<_>>();
        assert_eq!(arguments.len(), 4);
        for argument in arguments {
            deep.store.extra[deep.store.data[argument].range().start as usize] = slot;
        }
        deep.store.checked_expr_types.retain(|fact| fact.is_contract());
        let facts = deep.store.functions[owner].slot_type_facts;
        deep.store.slot_type_facts[facts.start as usize..(facts.start + facts.len) as usize].fill(IR_NONE);
        FullVerifier::verify(&deep).unwrap();
        execute_both(&deep);
    });
}

#[test]
fn physical_optional_aggregate_shapes_do_not_establish_validation() {
    on_large_stack(|| {
        let mut program = fixture(r#"
pure empty() -> List[Int]? { [] }
assert (empty() ?? [1]).is_empty()
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "empty");
        let FullCallableContract::Ground(signature) = program.store.functions[owner].contract else { panic!("ground function") };
        let mut signature = program.store.semantic.to_signature_with_budget(signature, 0,
            &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default()).unwrap();
        signature.return_ty = Box::new(Type::Optional(Box::new(Type::non_empty(Type::Int))));
        let signature = SemanticPoolBuilder::default().intern_signature(&mut program.store.semantic, &signature).unwrap();
        program.store.functions[owner].contract = FullCallableContract::Ground(signature);
        program.store.functions[owner].checked_return_type = program.store.semantic.signature_return_type(signature).unwrap().raw();
        let error = FullVerifier::verify(&program).unwrap_err();
        assert!(error.message.contains("ground list producer has no item contract"), "{error:?}");
    });
}

#[test]
fn physical_optional_pattern_active_branch_preserves_narrowing() {
    on_large_stack(|| {
        for body in [
            "if child != null { take(child) } else { return Ok(0) }",
            "if child != null { take(child) } else { Ok(0) }",
            "if child != null { return take(child) }; Ok(0)",
            "while child != null { return take(child) }; Ok(0)",
        ] {
            let source = r#"
enum Node { Item(Int?) }
pure take(value: Int) -> Result[Int] { Ok(value) }
pure keep(node: Node) -> Result[Int] {
    match node { Item(child) => { BODY } }
}
assert keep(Item(7))? == 7
assert keep(Item(null))? == 0
"#.replace("BODY", body);
            let mut program = fixture(&source);
            program.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let owner = function(&program, "keep");
            let guard = program.store.function_instruction_range(owner).unwrap().find(|&instruction| matches!(program.store.tags[instruction], FullTag::BoolLiteralCompareSlot | FullTag::ExprBinary)).unwrap();
            let op = program.store.extra[program.store.data[guard].range().start as usize] as usize;
            let mut wrong_arm = program.clone();
            wrong_arm.store.binary_ops[op] = BinaryOp::Eq;
            let error = FullVerifier::verify(&wrong_arm).unwrap_err();
            assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
            let facts = program.store.functions[owner].slot_type_facts;
            program.store.slot_type_facts[facts.start as usize..(facts.start + facts.len) as usize].fill(IR_NONE);
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
        }
    });
}

#[test]
fn physical_optional_pattern_null_guards_do_not_validate_unsigned() {
    on_large_stack(|| {
        for source in [r#"
enum Node { Item(Int?, UInt) }
pure take(value: UInt) -> Result[UInt] { Ok(value) }
pure keep(node: Node) -> Result[UInt] {
    match node {
        Item(child, checked) => {
            if child == null { return Ok(0) }
            take(checked)
        }
    }
}
assert keep(Item(-1, 7))? == 7
"#, r#"
type Signed = {value: Int}
type Unsigned = {value: UInt}
enum Node { Item(Signed?, Unsigned) }
pure take(value: Unsigned) -> Result[UInt] { Ok(value.value) }
pure keep(node: Node) -> Result[UInt] {
    match node {
        Item(child, checked) => {
            if child == null { return Ok(0) }
            take(checked)
        }
    }
}
assert keep(Item({value: -1}, {value: 7}))? == 7
"#] {
            let mut program = fixture(source);
            program.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let owner = function(&program, "keep");
            let decoder = function_view_decoder(&program.store, owner);
            let range = program.store.function_instruction_range(owner).unwrap();
            let guard = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::BoolLiteralCompareSlot).unwrap();
            let slot = program.store.extra[program.store.data[guard].range().start as usize + 1];
            let target = function(&program, "take");
            let argument = range.clone().find_map(|instruction| {
                let schema::ExprView::DirectPureCall { function, mut args, .. } = decoder.expression_view(instruction).ok()? else { return None };
                if function.id.index() != target { return None; }
                let decode::CallArgView::Single(value) = args.next()?.unwrap() else { panic!("single argument") };
                Some(value.raw() as usize)
            }).unwrap();
            assert!(matches!(decoder.expression_view(argument).unwrap(), schema::ExprView::Param(_)));
            let mut forged = program.clone();
            forged.store.extra[program.store.data[argument].range().start as usize] = slot;
            let error = FullVerifier::verify(&forged).unwrap_err();
            assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
        }
        let mut lawful = fixture(r#"
enum Node { Item(UInt?) }
pure take(value: Int) -> Result[Int] { Ok(value) }
pure keep(node: Node) -> Result[Int] {
    match node {
        Item(child) => {
            if child == null { return Ok(0) }
            take(child)
        }
    }
}
assert keep(Item(7))? == 7
assert keep(Item(null))? == 0
"#);
        lawful.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&lawful).unwrap();
        execute_both(&lawful);
    });
}

#[test]
fn producer_corpus_reduced_empty_list_argument_carrier() {
    on_large_stack(|| {
        let program = fixture(r#"
type Row = {items: List[Str]}
let row = Row(items: [])
assert row.items.is_empty()
"#);
        execute_both(&program);
    });
}


#[test]
fn producer_corpus_reduced_empty_map_argument_carrier() {
    on_large_stack(|| {
        let mut program = fixture(r#"
type Row = {before: Int, items: Map[Str, Int], after: Int}
var counter = 0
proc next() [] -> Int { counter += 1; counter }
let row = Row(after: next(), items: {}, before: next())
assert row.items.is_empty()
assert row.after == 1
assert row.before == 2
assert counter == 2
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
    });
}



#[test]
fn producer_context_immutable_branch_splice() {
    on_large_stack(|| {
        let mut program = fixture(r#"
pure scalar() -> Int { 7 }
pure words(flag: Bool) -> List[Str] {
    let prefix = if flag { [] } else { ["a"] }
    [@prefix, "b"]
}
assert words(true) == ["b"]
assert words(false) == ["a", "b"]
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "words");
        let item = program.store.function_instruction_range(owner).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr).unwrap();
        let scalar = program.store.tags.iter().position(|tag| *tag == FullTag::ExprInt).unwrap();
        let mut wrong_item = program.clone();
        wrong_item.store.tags[item] = FullTag::ExprInt;
        wrong_item.store.data[item] = program.store.data[scalar];
        let error = FullVerifier::verify(&wrong_item).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract") || error.message.contains("fresh list splice disagrees"), "{error:?}");
    });
}

#[test]
fn producer_context_carrier_contracts_require_unique_consumers() {
    on_large_stack(|| {
        for fields in ["left: List[Str], right: List[Str]", "left: List[Str], right: List[Int]"] {
            let source = format!("type Pair = {{{fields}}}\nlet pair = Pair(left: [], right: [])\nassert pair.left.is_empty()\nassert pair.right.is_empty()\n");
            let mut program = fixture(&source);
            program.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let mut fields = Vec::new();
            for step in 0..program.store.driver_steps.len() {
                let mut decoder = stage_view_decoder(&program.store);
                let row = program.store.driver_steps[step];
                decoder.owner = driver_owner(step).unwrap();
                decoder.slot_count = row.slot_count;
                decoder.instruction_range = program.store.driver_instruction_range(step).unwrap();
                for instruction in decoder.instruction_range.clone() {
                    if program.store.tags[instruction] != FullTag::ExprRecord { continue; }
                    let schema::ExprView::Record(mut entries) = decoder.expression_view(instruction).unwrap() else { unreachable!() };
                    let writers = decoder.checked_record_field_writers(&mut entries, |_| unreachable!()).unwrap();
                    for writer in writers.values() {
                        if let CheckedRecordFieldWriter::Value(value) = writer {
                            if program.store.tags[*value] == FullTag::ExprParam { fields.push(*value); }
                        }
                    }
                }
            }
            assert_eq!(fields.len(), 2);
            let mut wrong_binder = program.clone();
            let first = program.store.data[fields[0]].range().start as usize;
            let second = program.store.data[fields[1]].range().start as usize;
            wrong_binder.store.extra[second] = program.store.extra[first];
            let error = FullVerifier::verify(&wrong_binder).unwrap_err();
            assert!(error.message.contains("contextual aggregate subject has no authenticated consumer contract") || error.message.contains("conflicting consumer contracts"), "{error:?}");
        }
    });
}

#[test]
fn producer_context_overwritten_record_entries_keep_their_obligations() {
    on_large_stack(|| {
        let mut program = fixture(r#"
type Row = {value: Int}
pure earlier() -> Int { 7 }
pure alternate() -> Bool { false }
pure keep() -> Result[Row] { Ok({value: earlier(), ...{value: 9}}) }
assert keep()?.value == 9
var counter = 0
proc next() [] -> Int { counter += 1; counter }
let observed = {value: next(), ...{value: next()}}
assert observed.value == 2
assert counter == 2
let failed = try { {value: "bad".parse_int()?, ...{value: next()}} }
assert match failed {
    Ok(_) => false
    Err(_) => true
}
assert counter == 2
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let earlier = function(&program, "earlier");
        let scalar = program.store.function_instruction_range(earlier).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
        let alternate = function(&program, "alternate");
        let boolean = program.store.function_instruction_range(alternate).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
        let mut wrong_child = program.clone();
        wrong_child.store.tags[scalar] = FullTag::ExprBool;
        wrong_child.store.data[scalar] = program.store.data[boolean];
        let error = FullVerifier::verify(&wrong_child).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");

    });
}

#[test]
fn producer_context_checked_records_preserve_source_domains() {
    on_large_stack(|| {
        for body in ["Row(value: 7)", "{...{value: 7}}"] {
            let source = format!("type Row = {{value: Int}}\npure alternate() -> Bool {{false}}\npure keep() -> Result[Row] {{ let row: Row = {body}; Ok(row) }}\nassert keep()?.value == 7\n");
            let mut program = fixture(&source);
            program.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let keep = function(&program, "keep");
            let scalar = program.store.function_instruction_range(keep).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
            let alternate = function(&program, "alternate");
            let boolean = program.store.function_instruction_range(alternate).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
            let mut wrong_field = program.clone();
            wrong_field.store.tags[scalar] = FullTag::ExprBool;
            wrong_field.store.data[scalar] = program.store.data[boolean];
            let error = FullVerifier::verify(&wrong_field).unwrap_err();
            assert!(error.message.contains("checked record field disagrees with its contract") || error.message.contains("checked record spread field disagrees with its contract") || error.message.contains("checked value input disagrees with its contract"), "{error:?}");
        }
        let program = fixture(r#"
type Row = {value: UInt, items: NonEmpty[Int]}
pure make(value: Int) -> Result[Row] { Ok(Row(value: value, items: [7])) }
assert make(7)?.value == 7
"#);
        execute_both(&program);
        let failed = Arc::new(fixture(r#"
type Row = {value: UInt, items: NonEmpty[Int]}
pure make(value: Int) -> Result[Row] { Ok(Row(value: value, items: [7])) }
let failed = make(-1)?
"#));
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*failed.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&failed));
            evaluator.force_frames = force_frames;
            let error = (0..failed.driver_step_count().unwrap()).find_map(|step| evaluator.eval_indexed_driver_step(step, Span::new(failed.store.source_id, 0, 0)).unwrap().err()).unwrap();
            assert_eq!(error.kind, "type-error");
            assert!(error.message.contains("UInt constraint"), "{error:?}");
        }
        {
            let source = "type Row = {count: UInt}\npure alternate() -> Bool { false }\npure keep() -> Row? { let row: Row? = {count: 7}; row }\nassert keep() != null\n";
            let mut optional = fixture(&source);
            optional.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&optional).unwrap();
            execute_both(&optional);
            let keep = function(&optional, "keep");
            let scalar = optional.store.function_instruction_range(keep).unwrap().find(|&instruction| matches!(optional.store.tags[instruction], FullTag::ExprInt | FullTag::ExprUInt)).unwrap();
            let alternate = function(&optional, "alternate");
            let boolean = optional.store.function_instruction_range(alternate).unwrap().find(|&instruction| optional.store.tags[instruction] == FullTag::ExprBool).unwrap();
            let mut wrong_field = optional.clone();
            wrong_field.store.tags[scalar] = FullTag::ExprBool;
            wrong_field.store.data[scalar] = optional.store.data[boolean];
            let error = FullVerifier::verify(&wrong_field).unwrap_err();
            assert!(error.message.contains("checked record field disagrees with its contract") || error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
        }
        let mut nested = fixture(r#"
type Inner = {items: NonEmpty[Int]}
type Row = {nested: Inner}
pure alternate_nested() -> Bool { false }
pure make(flag: Bool) -> Result[Row] {
    let row: Row = if flag { {nested: {items: [7]}} } else { {nested: {items: [8]}} }
    Ok(row)
}
assert make(true)?.nested.items[0] == 7
assert make(false)?.nested.items[0] == 8
"#);
        nested.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&nested).unwrap();
        execute_both(&nested);
        let make = function(&nested, "make");
        let scalars = nested.store.function_instruction_range(make).unwrap().filter(|&instruction| nested.store.tags[instruction] == FullTag::ExprInt).collect::<Vec<_>>();
        assert_eq!(scalars.len(), 2);
        let alternate = function(&nested, "alternate_nested");
        let boolean = nested.store.function_instruction_range(alternate).unwrap().find(|&instruction| nested.store.tags[instruction] == FullTag::ExprBool).unwrap();
        let mut wrong_nested = nested.clone();
        for scalar in scalars {
            wrong_nested.store.tags[scalar] = FullTag::ExprBool;
            wrong_nested.store.data[scalar] = nested.store.data[boolean];
        }
        let error = FullVerifier::verify(&wrong_nested).unwrap_err();
        assert!(error.message.contains("checked value input disagrees with its contract") || error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
    });
}

#[test]
fn checked_trivial_flow_census_keeps_all_returns_and_writers() {
    let program = fixture(r#"
pure choose(flag: Bool) -> Int {
    var result = 0
    if flag { result = 7; return result } else { result = 3; return result }
}
assert choose(true) == 7
assert choose(false) == 3
"#);
    execute_both(&program);
    let owner = function(&program, "choose");
    program.symbol_owner().with_current(|| {
        let mut decoder = function_view_decoder(&program.store, owner);
        let range = decoder.instruction_range.clone();
        assert!(range.clone().filter(|&instruction| program.store.tags[instruction] == FullTag::StmtReturn).count() >= 2);
        decoder.instruction_states = Some(RefCell::new(vec![0; range.len()]));
        decoder.instruction_proof = Some(RefCell::new(in_place::InstructionProof::new(range.clone())));
        decoder.block_states = Some(OwnedBlockStates::new(decoder.owner, &owned_block_counts(&program.store)));
        let mut index = verify_schemes::GroundProducerIndex::new(&program.store).unwrap();
        decoder.producer_demand = index.demand();
        let body = [program.store.functions[owner].body];
        let mut cursor = FullCursor::new(&body);
        Vec::<BuildStmtId>::verify(&decoder, &mut cursor).unwrap();
        cursor.finish().unwrap();
        decoder.finish_function().unwrap();
        index.record(&decoder).unwrap();
        let statements = range.clone().filter(|&instruction| program.store.tags[instruction].kind() == schema::InstructionKind::Stmt).count();
        assert_eq!(index.census_instruction_work(), (range.len() + statements, range.len()));
        assert!(index.census_matches_depth_first(&decoder).unwrap());
    });
}

#[test]
fn producer_context_census_bounds_mixed_instruction_patterns() {
    on_large_stack(|| {
        for locals in [64, 128, 256, 512] {
            let declarations = (0..locals).map(|index| format!("let local_{index} = {index}; ")).collect::<String>();
            let source = format!("enum Node {{ Item(Int) }}\npure alternate() -> Int {{ 0 }}\npure keep(node: Node) -> Result[Int] {{ {declarations} match node {{ Item(value) => Ok(value) }} }}\nassert keep(Item(7))? == 7\n");
            let program = fixture(&source);
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let keep = function(&program, "keep");
            program.symbol_owner().with_current(|| {
                let mut decoder = function_view_decoder(&program.store, keep);
                let range = decoder.instruction_range.clone();
                decoder.instruction_states = Some(RefCell::new(vec![0; range.len()]));
                decoder.instruction_proof = Some(RefCell::new(in_place::InstructionProof::new(range)));
                decoder.block_states = Some(OwnedBlockStates::new(decoder.owner, &owned_block_counts(&program.store)));
                let mut index = verify_schemes::GroundProducerIndex::new(&program.store).unwrap();
                decoder.producer_demand = index.demand();
                let body = [program.store.functions[keep].body];
                let mut cursor = FullCursor::new(&body);
                Vec::<BuildStmtId>::verify(&decoder, &mut cursor).unwrap();
                cursor.finish().unwrap();
                decoder.finish_function().unwrap();
                index.record(&decoder).unwrap();
                let (visits, instructions) = index.census_instruction_work();
                let statements = decoder.instruction_range.clone().filter(|&instruction| program.store.tags[instruction].kind() == schema::InstructionKind::Stmt).count();
                assert_eq!(visits, instructions + statements + 1);
                assert!(index.census_matches_depth_first(&decoder).unwrap());
            });
            let decoder = function_view_decoder(&program.store, keep);
            let matched = decoder.instruction_range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprMatch).unwrap();
            let schema::ExprView::MatchExpr { mut arms, .. } = decoder.expression_view(matched).unwrap() else { unreachable!() };
            let (pattern, _, _) = arms.next().unwrap().unwrap();
            let binds = decoder.pattern_capture_slots(pattern.raw() as usize).unwrap();
            assert_eq!(binds.len(), 1);
            assert!(matches!(decoder.checked_pattern_view(pattern.raw()).unwrap(), schema::PatternView::Tag { .. }));
            let payload = program.store.pattern_data[pattern.raw() as usize].range();
            let slot_word = (payload.start + payload.len) as usize - 1;
            assert_eq!(program.store.extra[slot_word], *binds.first().unwrap() as u32);
            let mut wrong_pattern = program.clone();
            wrong_pattern.store.extra[slot_word] = 0;
            let error = FullVerifier::verify(&wrong_pattern).unwrap_err();
            assert!(error.message.contains("slot") || error.message.contains("capture"), "{error:?}");
            let mut wrong_owner = program.clone();
            let alternate = function(&program, "alternate");
            wrong_owner.store.function_instruction_starts[keep] = program.store.function_instruction_starts[alternate];
            let error = FullVerifier::verify(&wrong_owner).unwrap_err();
            assert!(error.message.contains("instruction") || error.message.contains("owner"), "{error:?}");
        }
    });
}

#[test]
fn producer_context_native_arguments_keep_nested_producer_proofs() {
    on_large_stack(|| {
        let mut program = fixture(r#"
pure source() -> Bytes { b"." }
pure alternate() -> Str { "." }
pure keep() -> Result[Path] { Ok(source() as Path) }
assert keep()? == p"."
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let source = function(&program, "source");
        let bytes = program.store.function_instruction_range(source).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBytes).unwrap();
        let alternate = function(&program, "alternate");
        let text = program.store.function_instruction_range(alternate).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr).unwrap();
        let mut wrong_leaf = program.clone();
        wrong_leaf.store.tags[bytes] = FullTag::ExprStr;
        wrong_leaf.store.data[bytes] = program.store.data[text];
        let error = FullVerifier::verify(&wrong_leaf).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
        let keep = function(&program, "keep");
        let decoder = function_view_decoder(&program.store, keep);
        assert!(decoder.instruction_range.clone().any(|instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::ModuleCall { op: RuntimeOp::PathParseBytes, .. }))));
    });
}

#[test]
fn producer_context_carrier_hints_do_not_replace_consumer_contracts() {
    on_large_stack(|| {
        let mut program = fixture(r#"
type Row = {items: List[Str]}
pure alternate() -> Bool { false }
pure keep() -> Result[Row] { Ok(Row(items: [])) }
assert keep()?.items.is_empty()
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let keep = function(&program, "keep");
        let empty = program.store.function_instruction_range(keep).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprList).unwrap();
        let owner = ExecutableOwner::Function(IrFunctionId::new(keep).unwrap());
        let mut wrong_hint = program.clone();
        let ty = SemanticPoolBuilder::default().intern_type(&mut wrong_hint.store.semantic, &Type::List(Box::new(Type::Int))).unwrap();
        wrong_hint.store.checked_expr_types.push(FullCheckedExprType { instruction: empty as u32, type_id: ty, purpose: FullCheckedExprPurpose::ProducerContext(owner) });
        wrong_hint.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
        let error = FullVerifier::verify(&wrong_hint).unwrap_err();
        assert!(error.message.contains("checked record field disagrees with its contract") || error.message.contains("contextual aggregate hint disagrees"), "{error:?}");
        let mut coordinated = program.clone();
        let ty = SemanticPoolBuilder::default().intern_type(&mut coordinated.store.semantic, &Type::List(Box::new(Type::Str))).unwrap();
        coordinated.store.checked_expr_types.push(FullCheckedExprType { instruction: empty as u32, type_id: ty, purpose: FullCheckedExprPurpose::ProducerContext(owner) });
        coordinated.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
        let alternate = function(&program, "alternate");
        let boolean = program.store.function_instruction_range(alternate).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
        coordinated.store.tags[empty] = FullTag::ExprBool;
        coordinated.store.data[empty] = program.store.data[boolean];
        let error = FullVerifier::verify(&coordinated).unwrap_err();
        assert_eq!(error.message, "full IR contains an instruction or block not owned by the function body");
        let mut typed_items = fixture(r#"
type Row = {items: List[Str]}
pure alternate() -> Bool { false }
pure keep() -> Result[Row] { Ok(Row(items: ["word"])) }
assert keep()?.items[0] == "word"
"#);
        typed_items.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&typed_items).unwrap();
        execute_both(&typed_items);
        let item_owner = function(&typed_items, "keep");
        let list = typed_items.store.function_instruction_range(item_owner).unwrap().find(|&instruction| typed_items.store.tags[instruction] == FullTag::ExprList).unwrap();
        let item = typed_items.store.function_instruction_range(item_owner).unwrap().find(|&instruction| typed_items.store.tags[instruction] == FullTag::ExprStr).unwrap();
        let alternate_items = function(&typed_items, "alternate");
        let boolean_item = typed_items.store.function_instruction_range(alternate_items).unwrap().find(|&instruction| typed_items.store.tags[instruction] == FullTag::ExprBool).unwrap();
        let ty = SemanticPoolBuilder::default().intern_type(&mut typed_items.store.semantic, &Type::List(Box::new(Type::Str))).unwrap();
        typed_items.store.checked_expr_types.push(FullCheckedExprType { instruction: list as u32, type_id: ty, purpose: FullCheckedExprPurpose::ProducerContext(ExecutableOwner::Function(IrFunctionId::new(item_owner).unwrap())) });
        typed_items.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
        typed_items.store.tags[item] = FullTag::ExprBool;
        typed_items.store.data[item] = typed_items.store.data[boolean_item];
        let error = FullVerifier::verify(&typed_items).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract") || error.message.contains("checked record field disagrees with its contract"), "{error:?}");
        let mut wrong_owner = wrong_hint.clone();
        wrong_owner.store.checked_expr_types.iter_mut().find(|fact| fact.instruction == empty as u32).unwrap().purpose = FullCheckedExprPurpose::ProducerContext(ExecutableOwner::Function(IrFunctionId::new(alternate).unwrap()));
        assert!(FullVerifier::verify(&wrong_owner).is_err());
        let mut wrong_instruction = wrong_hint;
        wrong_instruction.store.checked_expr_types.iter_mut().find(|fact| fact.instruction == empty as u32).unwrap().instruction = boolean as u32;
        wrong_instruction.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
        assert!(FullVerifier::verify(&wrong_instruction).is_err());
    });
}

#[test]
fn producer_context_native_constructor_preparation_stays_within_work_bound() {
    on_large_stack(|| {
        let program = fixture(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/xsh/generic-constructors.xsh")));
        FullVerifier::verify(&program).unwrap();
    });
}
