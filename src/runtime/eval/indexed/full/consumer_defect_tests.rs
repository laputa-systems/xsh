use super::noncompletion_tests::{checked_program, function_index};
use super::*;

fn reissued(program: &FullProgram, owner: usize, edit: impl FnOnce(&mut crate::sema::types::CallableType)) -> FullProgram {
    let FullCallableContract::Ground(signature) = program.store.functions[owner].contract else { panic!("ground function") };
    let mut callable = program.store.semantic.to_signature_with_budget(
        signature,
        0,
        &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default(),
    ).unwrap();
    edit(&mut callable);
    let mut forged = program.clone();
    let mut pool = SemanticPoolBuilder::default();
    forged.store.functions[owner].contract = FullCallableContract::Ground(
        pool.intern_signature(&mut forged.store.semantic, &callable).unwrap(),
    );
    if forged.store.functions[owner].checked_return_type != IR_NONE {
        forged.store.functions[owner].checked_return_type = pool.intern_type(&mut forged.store.semantic, &callable.return_ty).unwrap().raw();
    }
    forged
}

const OPTIONAL_LIST_ARGUMENT: &str = "pure first_word(words: List[Str]? = null) -> Bool { words != null }\n\
pure named() -> Bool { first_word(words: []) }\n\
assert named()\n";

#[test]
fn empty_list_reaches_an_optional_list_parameter_only_through_a_list_contract() {
    let program = checked_program("optional-list-argument.xsh", OPTIONAL_LIST_ARGUMENT);
    FullVerifier::verify(&program).unwrap();
    let owner = function_index(&program, "first_word");
    let forged = reissued(&program, owner, |callable| {
        callable.params[0].ty = Type::Optional(Box::new(Type::Map(Box::new(Type::Str), Box::new(Type::Str))));
    });
    assert_eq!(
        FullVerifier::verify(&forged).unwrap_err().message,
        "contextual aggregate argument has an incompatible consumer contract",
    );
}

#[test]
fn empty_list_reaches_an_optional_record_field_only_through_a_record_contract() {
    let program = checked_program(
        "optional-record-argument.xsh",
        "type Holder = {words: List[Str]}\n\
         pure held(holder: Holder?) -> Bool { holder != null }\n\
         pure named() -> Bool { held(holder: {words: []}) }\n\
         assert named()\n",
    );
    FullVerifier::verify(&program).unwrap();
    let owner = function_index(&program, "held");
    let forged = reissued(&program, owner, |callable| {
        callable.params[0].ty = Type::Optional(Box::new(Type::List(Box::new(Type::Str))));
    });
    assert_eq!(
        FullVerifier::verify(&forged).unwrap_err().message,
        "contextual aggregate argument has an incompatible consumer contract",
    );
}

fn instruction_with_tag(program: &FullProgram, owner: usize, tag: FullTag, nth: usize) -> usize {
    function_view_decoder(&program.store, owner)
        .instruction_range
        .clone()
        .filter(|&instruction| program.store.tags[instruction] == tag)
        .nth(nth)
        .expect("fixture has the instruction")
}

const NARROWED_ENUM_FIELD: &str = "enum Ordering: Str { Older = \"lt\", Same = \"eq\" }\n\
type Installed = {previous: Ordering?, ordering: Ordering?}\n\
pure earlier(installed: Installed) -> Bool { installed.previous == null }\n\
enum State { Gone, There(Installed) }\n\
pure holds(o: Ordering) -> Bool { o == .Same }\n\
pure classify(state: State) -> Bool {\n\
  match state {\n\
    Gone => false\n\
    There(installed) => {\n\
      if installed.ordering == null { false } else { holds(installed.ordering) }\n\
    }\n\
  }\n\
}\n\
assert classify(.Gone) == false\n";

#[test]
fn narrowed_enum_field_read_needs_its_dominating_null_test() {
    let program = checked_program("narrowed-enum-field.xsh", NARROWED_ENUM_FIELD);
    FullVerifier::verify(&program).unwrap();
    let owner = function_index(&program, "classify");
    let tested = instruction_with_tag(&program, owner, FullTag::ExprField, 0);
    let words = program.store.data[tested].range().bounds(program.store.extra.len()).unwrap();
    let ordering = program.store.extra[words.start + 1];
    let previous = (0..program.store.extra.len() as u32)
        .find(|&id| program.store.string(id.into()).ok() == Some("previous"))
        .expect("the fixture reads the previous field elsewhere");
    assert_eq!(program.store.string(ordering.into()).unwrap(), "ordering");
    let mut forged = program.clone();
    forged.store.extra[words.start + 1] = previous;
    assert_eq!(
        FullVerifier::verify(&forged).unwrap_err().message,
        "ground producer value disagrees with its checked contract",
    );
}

const EMPTY_MAP_CONDITIONAL: &str = "pure pick(cond: Bool, table: Map[Str, Any]) -> Map[Str, Any] {\n\
  let chosen = if cond { table } else { {} }\n\
  chosen\n\
}\n\
assert pick(false, {}).len() == 0\n";

#[test]
fn conditional_with_an_empty_map_arm_keeps_dynamic_values_for_empty_maps_only() {
    let program = checked_program("empty-map-conditional.xsh", EMPTY_MAP_CONDITIONAL);
    FullVerifier::verify(&program).unwrap();
    let owner = function_index(&program, "pick");
    let empty = function_view_decoder(&program.store, owner)
        .instruction_range
        .clone()
        .find(|&instruction| matches!(program.store.tags[instruction], FullTag::ExprMapLiteral | FullTag::ExprEmptyMap))
        .expect("the else arm holds an empty map");
    let mut forged = program.clone();
    forged.store.tags[empty] = FullTag::ExprList;
    assert_eq!(
        FullVerifier::verify(&forged).unwrap_err().message,
        "generic call substitution contains dynamic type Map[Any]",
    );
}

#[test]
fn named_map_literals_take_a_dynamic_map_contract_like_positional_ones() {
    for (name, source) in [
        ("named-empty-map-argument.xsh", "pure size(vars: Map[Str, Any]) -> Int { vars.len() }\npure named() -> Int { size(vars: {}) }\nassert named() == 0\n"),
        ("named-populated-map-argument.xsh", "pure size(vars: Map[Str, Any]) -> Int { vars.len() }\npure named() -> Int { size(vars: {[\"k\"]: 1}) }\nassert named() == 1\n"),
        ("named-nested-dynamic-map-argument.xsh", "pure size(vars: Map[Str, List[Any]]) -> Int { vars.len() }\npure named() -> Int { size(vars: {[\"k\"]: []}) }\nassert named() == 1\n"),
        ("positional-populated-map-argument.xsh", "pure size(vars: Map[Str, Any]) -> Int { vars.len() }\npure named() -> Int { size({[\"k\"]: 1}) }\nassert named() == 1\n"),
    ] {
        FullVerifier::verify(&checked_program(name, source)).unwrap();
    }
}

const NAMED_POPULATED_MAP: &str = "pure size(vars: Map[Str, Any]) -> Int { vars.len() }\n\
pure named() -> Int { size(vars: {[\"k\"]: 1}) }\n\
assert named() == 1\n";

#[test]
fn named_populated_map_reaches_a_dynamic_map_only_through_a_map_contract() {
    let program = checked_program("named-populated-map.xsh", NAMED_POPULATED_MAP);
    FullVerifier::verify(&program).unwrap();
    let owner = function_index(&program, "size");
    let forged = reissued(&program, owner, |callable| {
        callable.params[0].ty = Type::List(Box::new(Type::Any));
    });
    assert_eq!(
        FullVerifier::verify(&forged).unwrap_err().message,
        "ground producer value disagrees with its checked contract",
    );
}

#[test]
fn named_populated_map_source_must_stay_a_map_literal() {
    let program = checked_program("named-populated-map-source.xsh", NAMED_POPULATED_MAP);
    let owner = function_index(&program, "named");
    let literal = instruction_with_tag(&program, owner, FullTag::ExprMapLiteral, 0);
    let mut forged = program.clone();
    forged.store.tags[literal] = FullTag::ExprList;
    assert!(FullVerifier::verify(&forged).is_err());
}

#[test]
fn populated_map_arguments_cannot_forge_key_or_value_types() {
    for argument in ["vars: {[\"k\"]: 1}", "{[\"k\"]: 1}"] {
        let source = format!("pure size(vars: Map[Str, Any]) -> Int {{ vars.len() }}\npure named() -> Int {{ size({argument}) }}\nassert named() == 1\n");
        let program = checked_program("forged-map-contents.xsh", &source);
        let owner = function_index(&program, "size");
        for (key, value) in [(Type::Int, Type::Any), (Type::Str, Type::Str)] {
            let forged = reissued(&program, owner, |callable| {
                callable.params[0].ty = Type::Map(Box::new(key), Box::new(value));
            });
            assert!(FullVerifier::verify(&forged).is_err());
        }
    }
}

#[test]
fn named_map_binding_cannot_forge_its_value_type() {
    let program = checked_program(
        "forged-map-binding.xsh",
        "pure size(vars: Map[Str, Any]) -> Int { vars.len() }\n\
         pure named() -> Int { let vars: Map[Str, Any] = {[\"k\"]: 1}; size(vars: vars) }\n\
         assert named() == 1\n",
    );
    FullVerifier::verify(&program).unwrap();
    let owner = function_index(&program, "size");
    let forged = reissued(&program, owner, |callable| {
        callable.params[0].ty = Type::Map(Box::new(Type::Str), Box::new(Type::Str));
    });
    assert!(FullVerifier::verify(&forged).is_err());
}

const FINISHING_ARMS: &str = "proc ready(x: Int) -> Result[Int, Error] { Ok(x) }\n\
proc finish(x: Int) -> Result[Unit, Error] {\n\
  match ready(x) {\n\
    Err(error) => Err(error)\n\
    Ok(value) => {\n\
      guard value > 0 else { return error.fail(\"not positive\") }\n\
    }\n\
  }\n\
}\n\
assert finish(2) is Ok(_)\n";

#[test]
fn match_arm_blocks_finish_a_unit_result_function() {
    let program = checked_program("finishing-arms.xsh", FINISHING_ARMS);
    FullVerifier::verify(&program).unwrap();
    let owner = function_index(&program, "finish");
    let mut forged = reissued(&program, owner, |callable| {
        *callable.return_ty = Type::Result(Box::new(Type::Int), Box::new(Type::Error));
    });
    let mut pool = SemanticPoolBuilder::default();
    let unit_result = pool.intern_type(&mut forged.store.semantic, &Type::Result(Box::new(Type::Unit), Box::new(Type::Error))).unwrap();
    let int_result = pool.intern_type(&mut forged.store.semantic, &Type::Result(Box::new(Type::Int), Box::new(Type::Error))).unwrap();
    for fact in &mut forged.store.checked_expr_types {
        if fact.type_id == unit_result {
            fact.type_id = int_result;
        }
    }
    assert_eq!(
        FullVerifier::verify(&forged).unwrap_err().message,
        "ground producer value disagrees with its checked contract",
    );
}

fn loaded_module_program(source: &str) -> FullProgram {
    let path = "loaded-deployment.xsh";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only_with_linkage(
        path,
        crate::loader::entry_source_from_text(path, source.to_owned()),
        Vec::new(),
        crate::loader::StdlibLinkage::LinkToPrepared,
    );
    let source_id = sources.files().first().map(crate::source::SourceFile::id).expect("entry source is present");
    let file = crate::loader::CompactFileUnit::new(path, source_id, parsed);
    assert!(file.parse_diagnostics().is_empty(), "{:?}", file.parse_diagnostics());
    let declarations = crate::sema::check::Checker::check_compact_declarations(file.program(), &sources, source_id);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    let graph = crate::loader::CompactModuleGraph::from_file_unit(&file, &declarations);
    FullBuilder::build_compact_external_stdlib(file.program(), &declarations, &graph, source, Arc::new(sources), source_id)
        .expect("loaded module should pass full IR verification")
}

const RENDERED_PAGE: &str = "##! A loaded module that renders before it uses the page.\n\
pure keep(text: Str) -> Str { text }\n\
## Renders one page.\n\
export proc page() [error] -> Result[Str, Error] {\n\
  let rendered = template.render(\"a\", 1)?\n\
  Ok(keep(rendered))\n\
}\n";

#[test]
fn external_script_call_is_typed_by_the_registry_contract() {
    let program = loaded_module_program(RENDERED_PAGE);
    FullVerifier::verify(&program).unwrap();
    let owner = function_index(&program, "keep");
    let forged = reissued(&program, owner, |callable| callable.params[0].ty = Type::Int);
    assert_eq!(
        FullVerifier::verify(&forged).unwrap_err().message,
        "ground producer value disagrees with its checked contract",
    );
}

const TESTED_ERROR_PARAMETER: &str = "error BuildError = Missing(message: Str) : NotFound\n\
pure error_message(failure: Error) -> Str { failure.message }\n\
pure describe(value: Any) -> Str { if value is NotFound { error_message(value) } else { \"other\" } }\n\
assert describe(BuildError.Missing(message: \"absent\")) == \"absent\"\n";

const TESTED_ERROR_DRIVER: &str = "error BuildError = Missing(message: Str) : NotFound\n\
pure dynamic_failure() -> Any { BuildError.Missing(message: \"absent\") }\n\
pure error_message(failure: Error) -> Str { failure.message }\n\
let failure = dynamic_failure()\n\
if failure is NotFound {\n  print ${error_message(failure)}\n}\n";

const TESTED_ERROR_FAMILY: &str = "error BuildError = Missing(message: Str) : NotFound\n\
pure family_message(failure: BuildError) -> Str { failure.message }\n\
pure describe(value: Any) -> Str { if value is BuildError { family_message(value) } else { \"other\" } }\n\
assert describe(BuildError.Missing(message: \"absent\")) == \"absent\"\n";

#[test]
fn type_tested_error_facet_reaches_an_error_parameter() {
    FullVerifier::verify(&checked_program("tested-error-facet.xsh", TESTED_ERROR_PARAMETER)).unwrap();
}

#[test]
fn type_tested_error_facet_of_an_immutable_driver_binding_reaches_an_error_parameter() {
    FullVerifier::verify(&checked_program("tested-error-driver.xsh", TESTED_ERROR_DRIVER)).unwrap();
}

#[test]
fn type_tested_error_family_reaches_a_family_parameter() {
    FullVerifier::verify(&checked_program("tested-error-family.xsh", TESTED_ERROR_FAMILY)).unwrap();
}

#[test]
fn type_tested_error_reaches_only_the_error_types_it_satisfies() {
    for (name, source, narrower) in [
        ("tested-error-facet-forged.xsh", TESTED_ERROR_PARAMETER, "error_message"),
        ("tested-error-family-forged.xsh", TESTED_ERROR_FAMILY, "family_message"),
    ] {
        let program = checked_program(name, source);
        FullVerifier::verify(&program).unwrap();
        let _symbols = program.symbol_owner().enter();
        let owner = function_index(&program, narrower);
        for forged_parameter in [Type::Str, Type::Int, Type::ErrorFacet(Name::intern("Unrelated")), Type::ErrorFamily(Name::intern("Unrelated"))] {
            let forged = reissued(&program, owner, |callable| callable.params[0].ty = forged_parameter.clone());
            assert_eq!(
                FullVerifier::verify(&forged).unwrap_err().message,
                "ground producer value disagrees with its checked contract",
                "{name}: {forged_parameter:?}",
            );
        }
    }
}

const TESTED_ERROR_DRIVER_VAR: &str = "error BuildError = Missing(message: Str) : NotFound\n\
pure dynamic_failure() -> Any { BuildError.Missing(message: \"absent\") }\n\
pure seen(failure: Error) -> Str { \"seen\" }\n\
var failure = dynamic_failure()\n\
if failure is NotFound {\n  assert seen(failure) == \"seen\"\n}\n\
failure = 1\n";

const TESTED_ERROR_DRIVER_VAR_LOOP: &str = "error BuildError = Missing(message: Str) : NotFound\n\
pure dynamic_failure() -> Any { BuildError.Missing(message: \"absent\") }\n\
pure seen(failure: Error) -> Str { \"seen\" }\n\
var failure = dynamic_failure()\n\
var rounds = 0\n\
while rounds < 2 {\n  if failure is NotFound {\n    assert seen(failure) == \"seen\"\n  }\n  failure = dynamic_failure()\n  rounds += 1\n}\n";

#[test]
fn type_tested_error_of_a_driver_var_reaches_an_error_parameter() {
    FullVerifier::verify(&checked_program("tested-error-driver-var.xsh", TESTED_ERROR_DRIVER_VAR)).unwrap();
    FullVerifier::verify(&checked_program("tested-error-driver-var-loop.xsh", TESTED_ERROR_DRIVER_VAR_LOOP)).unwrap();
}

#[test]
fn type_tested_error_of_a_driver_var_reaches_only_the_error_types_it_satisfies() {
    let program = checked_program("tested-error-driver-var-forged.xsh", TESTED_ERROR_DRIVER_VAR);
    FullVerifier::verify(&program).unwrap();
    let _symbols = program.symbol_owner().enter();
    let owner = function_index(&program, "seen");
    for forged_parameter in [Type::Str, Type::Int, Type::ErrorFacet(Name::intern("Unrelated")), Type::ErrorFamily(Name::intern("Unrelated"))] {
        let forged = reissued(&program, owner, |callable| callable.params[0].ty = forged_parameter.clone());
        assert_eq!(
            FullVerifier::verify(&forged).unwrap_err().message,
            "ground producer value disagrees with its checked contract",
            "{forged_parameter:?}",
        );
    }
}

const TESTED_ERROR_DRIVER_VAR_REWRITTEN_AFTER_USE: &str = "error BuildError = Missing(message: Str) : NotFound\n\
pure dynamic_failure() -> Any { BuildError.Missing(message: \"absent\") }\n\
pure seen(failure: Error) -> Str { \"seen\" }\n\
var failure = dynamic_failure()\n\
if failure is NotFound {\n  assert seen(failure) == \"seen\"\n  failure = 1\n}\n";

#[test]
fn a_driver_var_rewritten_before_its_use_loses_its_type_test_narrowing() {
    let program = checked_program("tested-error-driver-var-rewrite.xsh", TESTED_ERROR_DRIVER_VAR_REWRITTEN_AFTER_USE);
    FullVerifier::verify(&program).unwrap();
    let assertion = program.store.tags.iter().position(|tag| *tag == FullTag::StmtAssert).expect("the branch asserts");
    let write = program.store.tags.iter().position(|tag| format!("{tag:?}").starts_with("StmtAssign")).expect("the branch rewrites the var");
    let block = program.store.blocks.iter().find(|block| {
        let Some(words) = block.instructions.bounds(program.store.extra.len()) else { return false };
        block.flags & BLOCK_STATEMENTS != 0 && program.store.extra[words.start + 1..words.end].contains(&(assertion as u32))
    }).expect("the assertion sits in a statement block");
    let words = block.instructions.bounds(program.store.extra.len()).unwrap();
    let position = |instruction: usize| (words.start + 1..words.end).find(|&at| program.store.extra[at] == instruction as u32).expect("the statement is in the block");
    let mut forged = program.clone();
    forged.store.extra.swap(position(assertion), position(write));
    assert_eq!(
        FullVerifier::verify(&forged).unwrap_err().message,
        "ground producer value disagrees with its checked contract",
    );
}

const TESTED_ERROR_VARIANT: &str = "error BuildError = Missing(message: Str) : NotFound\n\
pure variant_seen(failure: BuildError) -> Str { \"seen\" }\n\
pure error_seen(failure: Error) -> Str { \"seen\" }\n\
pure describe(value: Any) -> Str { if value is BuildError.Missing { variant_seen(value) + error_seen(value) } else { \"other\" } }\n\
assert describe(BuildError.Missing(message: \"absent\")) == \"seenseen\"\n";

const TESTED_ERROR_VARIANT_DRIVER_VAR: &str = "error BuildError = Missing(message: Str) : NotFound\n\
pure dynamic_failure() -> Any { BuildError.Missing(message: \"absent\") }\n\
pure variant_seen(failure: BuildError) -> Str { \"seen\" }\n\
var failure = dynamic_failure()\n\
if failure is BuildError.Missing {\n  assert variant_seen(failure) == \"seen\"\n}\n";

#[test]
fn type_tested_error_variant_reaches_family_and_error_parameters() {
    FullVerifier::verify(&checked_program("tested-error-variant.xsh", TESTED_ERROR_VARIANT)).unwrap();
    FullVerifier::verify(&checked_program("tested-error-variant-driver-var.xsh", TESTED_ERROR_VARIANT_DRIVER_VAR)).unwrap();
}

#[test]
fn type_tested_error_variant_reaches_only_the_error_types_it_satisfies() {
    let program = checked_program("tested-error-variant-forged.xsh", TESTED_ERROR_VARIANT);
    FullVerifier::verify(&program).unwrap();
    let _symbols = program.symbol_owner().enter();
    for (callee, forged_parameters) in [
        ("variant_seen", vec![Type::Str, Type::Int, Type::ErrorFamily(Name::intern("Unrelated")), Type::ErrorFacet(Name::intern("Unrelated")),
            Type::ErrorVariant { family: Name::intern("BuildError"), variant: Name::intern("Unrelated") }]),
        ("error_seen", vec![Type::Str, Type::Int, Type::ErrorFacet(Name::intern("Unrelated")),
            Type::ErrorVariant { family: Name::intern("BuildError"), variant: Name::intern("Unrelated") }]),
    ] {
        let owner = function_index(&program, callee);
        for forged_parameter in forged_parameters {
            let forged = reissued(&program, owner, |callable| callable.params[0].ty = forged_parameter.clone());
            assert_eq!(
                FullVerifier::verify(&forged).unwrap_err().message,
                "ground producer value disagrees with its checked contract",
                "{callee}: {forged_parameter:?}",
            );
        }
    }
}
