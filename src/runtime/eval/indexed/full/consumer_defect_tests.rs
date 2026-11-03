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
type Installed = {version: Str, ordering: Ordering?}\n\
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
    let null = instruction_with_tag(&program, owner, FullTag::ExprNull, 0);
    let mut forged = program.clone();
    forged.store.tags[null] = FullTag::ExprUnit;
    assert_eq!(
        FullVerifier::verify(&forged).unwrap_err().message,
        "ground producer value disagrees with its checked contract",
    );
}
