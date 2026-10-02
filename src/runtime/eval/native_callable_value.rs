use super::*;
use super::indexed::generic::{NativeCallableContract, NativeCallableValueId};

/// Native values retain the original program's registry proof independently of user targets.
#[derive(Clone)]
pub struct RuntimeNativeCallableValue(Arc<NativeCallableValueData>);

struct NativeCallableValueData { program: Arc<FullProgram>, id: NativeCallableValueId }

impl RuntimeNativeCallableValue {
    pub(in crate::runtime::eval) fn new(program: Arc<FullProgram>, id: NativeCallableValueId) -> Result<Self, RuntimeError> {
        program.generic_evidence().ok_or_else(|| RuntimeError::new("invalid-native-callable", "native callable has no prepared program proof"))?
            .native_callable_value(id).map_err(|error| RuntimeError::new("invalid-native-callable", error.message))?;
        Ok(Self(Arc::new(NativeCallableValueData { program, id })))
    }
    pub(in crate::runtime::eval) fn program(&self) -> &Arc<FullProgram> { &self.0.program }
    pub(in crate::runtime::eval) fn id(&self) -> NativeCallableValueId { self.0.id }
    pub(in crate::runtime::eval) fn contract(&self) -> &NativeCallableContract {
        &self.0.program.generic_evidence().expect("validated native callable program")
            .native_callable_value(self.0.id).expect("validated immutable native callable identity").contract
    }
    pub(crate) fn kind(&self) -> crate::sema::inference::CallableKind {
        match self.contract().kind { indexed::generic::CallableKind::Pure => crate::sema::inference::CallableKind::Pure,
            indexed::generic::CallableKind::Proc => crate::sema::inference::CallableKind::Proc }
    }
    pub(crate) fn type_name(&self) -> &'static str {
        match self.contract().kind { indexed::generic::CallableKind::Pure => "Pure", indexed::generic::CallableKind::Proc => "Proc" }
    }
}

impl std::fmt::Debug for RuntimeNativeCallableValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("RuntimeNativeCallableValue").field("id", &self.0.id).field("kind", &self.kind()).finish() }
}
impl PartialEq for RuntimeNativeCallableValue { fn eq(&self, other: &Self) -> bool { Arc::ptr_eq(&self.0, &other.0) } }
impl Eq for RuntimeNativeCallableValue {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;

    fn fixture() -> (Arc<FullProgram>, NativeCallableValueId) {
        let source = "let encode = json.encode\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("native-value.xsh", crate::loader::entry_source_from_text("native-value.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = sources.files().first().unwrap().id();
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let solved = Arc::downgrade(&bodies.solved);
        let program = Arc::new(indexed::full::FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap());
        let id = program.generic_evidence().unwrap().native_callable_values().next().unwrap().0;
        drop(parsed); drop(declarations); drop(bodies);
        assert!(solved.upgrade().is_none());
        (program, id)
    }

    #[test]
    fn native_callable_host_bridge_preserves_original_program_identity_and_registry_contract() {
        std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
            let (program, id) = fixture();
            let symbols = program.symbol_owner().clone();
            let _symbols = symbols.enter();
            let handle = RuntimeNativeCallableValue::new(Arc::clone(&program), id).unwrap();
            let contract = handle.contract().clone();
            let weak = Arc::downgrade(&program);
            let host = LoweredValue::List(vec![LoweredValue::ResultOk(Box::new(LoweredValue::NativeCallable(handle.clone())))]).into_value();
            drop(program);
            assert!(weak.upgrade().is_some());
            let LoweredValue::List(values) = lowered_value_from_runtime_any(&host).unwrap() else { panic!() };
            let LoweredValue::ResultOk(value) = &values[0] else { panic!() };
            let LoweredValue::NativeCallable(restored) = value.as_ref() else { panic!() };
            assert_eq!(restored, &handle);
            assert_eq!(restored.id(), id);
            assert_eq!(restored.contract(), &contract);
            let host_handle = Value::NativeCallable(handle.clone());
            let typed = lowered_value_from_runtime(&host_handle, LoweredType::Pure).unwrap();
            assert!(lowered_value_from_runtime(&host_handle, LoweredType::Proc).is_none());
            assert!(value_matches_static_type(&host_handle, &Type::Pure));
            assert!(lowered_value_matches_static_type(&typed, &Type::Pure));
            assert!(modules::test_value_matches_type(&host_handle, &Type::Pure));
            assert_eq!(modules::encode_cache_key_value(&host_handle), Err("Pure"));
            let (foreign, _) = fixture();
            assert!(RuntimeNativeCallableValue::new(foreign, id).is_err());
            drop(restored.clone()); drop(values); drop(host); drop(host_handle); drop(typed); drop(handle);
            assert!(weak.upgrade().is_none(), "the final host handle releases its owning program");
        }).unwrap().join().unwrap();
    }
}
