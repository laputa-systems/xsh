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
