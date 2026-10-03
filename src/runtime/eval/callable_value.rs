use super::*;
use super::indexed::generic::{CallableValueId, UserCallableContract};
use crate::sema::inference::CallableKind;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct RuntimeCallableCapture {
    pub slot: usize,
    pub value: Value,
    pub host_binding: Option<super::host_environment::CapturedHostBinding>,
    pub live_cell: Option<super::lowered_run::indexed_run::live_capture_cells::LiveCaptureCell>,
}

impl RuntimeCallableCapture {
    pub(in crate::runtime::eval) fn captured_value(&self) -> Value {
        if let Some(cell) = &self.live_cell { return cell.value().into_value(); }
        self.host_binding.as_ref().map_or_else(|| self.value.clone(), |host| host.value())
    }
}

// The program owns the callable proof and symbols. Captures belong to this
// creation, so transporting a callable never substitutes a current name lookup.
#[derive(Clone)]
pub struct RuntimeCallableValue(Arc<RuntimeCallableValueData>);

struct RuntimeCallableValueData {
    program: Arc<FullProgram>,
    id: CallableValueId,
    contract: UserCallableContract,
    captures: Arc<[RuntimeCallableCapture]>,
}

impl RuntimeCallableValue {
    pub(in crate::runtime::eval) fn new(program:Arc<FullProgram>,id:CallableValueId,captures:Vec<RuntimeCallableCapture>)->Result<Self,RuntimeError> {
        let invalid = |error: indexed::IrVerifyError| RuntimeError::new("invalid-callable-value",error.message);
        let proof = program.generic_evidence().ok_or_else(|| RuntimeError::new("invalid-callable-value","callable program has no prepared evidence"))?;
        let contract = proof.callable_value(id).map_err(invalid)?.contract;
        let header = program.function_view_by_id(contract.target).map_err(invalid)?.header().map_err(invalid)?;
        // Mutable bindings require a shared live cell. An immutable snapshot
        // would change what the callable observes after its creation.
        if captures.len()!=header.captures.len() {
            return Err(RuntimeError::new("invalid-callable-value","callable creation captures do not match the prepared header"));
        }
        for (actual,expected) in captures.iter().zip(&header.captures) {
            if actual.slot!=expected.slot {
                return Err(RuntimeError::new("invalid-callable-value","callable creation capture slots differ from the prepared header"));
            }
            match (expected.mutable, &actual.live_cell) {
                (true, Some(cell)) => {
                    if expected.host_binding.is_some() || actual.host_binding.is_some() { return Err(RuntimeError::new("invalid-callable-value", "mutable lexical capture cannot substitute a host binding")); }
                    cell.validate_capture(&program, contract.target, actual.slot, Span::new(crate::source::SourceId::new(0), 0, 0))?;
                }
                (true, None) => return Err(RuntimeError::new("unsupported-callable-value","mutable callable captures require a live binding environment")),
                (false, Some(_)) => return Err(RuntimeError::new("invalid-callable-value", "immutable callable capture cannot substitute a live binding")),
                (false, None) => {},
            }
            match (expected.host_binding, &actual.host_binding) {
                (Some(binding), Some(host)) => {
                    let (id, source) = proof.host_binding_capture_for_slot(contract.target, expected.slot as u32).map_err(invalid)?
                        .ok_or_else(|| RuntimeError::new("invalid-callable-value", "host capture lacks its original allocation receipt"))?;
                    if id != host.capture_id() || source.binding != binding || actual.value != host.value() {
                        return Err(RuntimeError::new("invalid-callable-value", "host capture changes its original creation environment or allocation"));
                    }
                }
                (None, None) => {},
                _ => return Err(RuntimeError::new("invalid-callable-value", "host capture requires its authenticated creation environment")),
            }
            let value = lowered_value_from_runtime_any(&actual.captured_value()).ok_or_else(|| RuntimeError::new("invalid-callable-value","callable capture cannot cross the runtime value boundary"))?;
            if !lowered_ops::lowered_value_matches(expected.kind,&value) {
                return Err(RuntimeError::new("invalid-callable-value",format!("callable capture slot {} requires {}, found {}",actual.slot,lowered_ops::lowered_type_name(expected.kind),actual.value.type_name())));
            }
        }
        Ok(Self(Arc::new(RuntimeCallableValueData{program,id,contract,captures:captures.into()})))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_with_program(&self, program: Arc<FullProgram>) -> Self {
        Self(Arc::new(RuntimeCallableValueData { program, id: self.0.id, contract: self.0.contract, captures: self.0.captures.clone() }))
    }
    pub(in crate::runtime::eval) fn program(&self)->&Arc<FullProgram> {&self.0.program}
    pub(in crate::runtime::eval) fn id(&self)->CallableValueId {self.0.id}
    pub(in crate::runtime::eval) fn contract(&self)->UserCallableContract {self.0.contract}
    pub(in crate::runtime::eval) fn captures(&self)->&[RuntimeCallableCapture] {&self.0.captures}
    pub(crate) fn kind(&self)->CallableKind {match self.0.contract.kind {indexed::generic::CallableKind::Pure=>CallableKind::Pure,indexed::generic::CallableKind::Proc=>CallableKind::Proc}}
    pub(crate) fn type_name(&self)->&'static str {match self.kind(){CallableKind::Pure=>"Pure",CallableKind::Proc=>"Proc",CallableKind::Stream=>"Stream"}}
}

impl std::fmt::Debug for RuntimeCallableValue {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.debug_struct("RuntimeCallableValue").field("id",&self.0.id).field("kind",&self.kind()).field("capture_count",&self.0.captures.len()).finish()}
}
impl PartialEq for RuntimeCallableValue {fn eq(&self,other:&Self)->bool {Arc::ptr_eq(&self.0,&other.0)}}
impl Eq for RuntimeCallableValue {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;

    fn fixture(source:&str)->(Arc<FullProgram>,CallableValueId) {
        let (sources,parsed)=crate::loader::parse_load_entry_source_arena_only("callable-value.xsh",crate::loader::entry_source_from_text("callable-value.xsh",source.to_owned()),Vec::new());
        assert!(parsed.diagnostics.is_empty(),"{:?}",parsed.diagnostics);
        let source_id=sources.files().first().unwrap().id();
        let declarations=Checker::check_compact_declarations(&parsed.arena);let bodies=Checker::probe_compact_bodies(&parsed.arena,&declarations);
        assert!(bodies.diagnostics.is_empty(),"{:?}",bodies.diagnostics);
        let solved=Arc::downgrade(&bodies.solved);
        let program=Arc::new(indexed::full::FullBuilder::build_compact(&parsed.arena,&declarations,&bodies,source,Arc::new(sources),source_id).unwrap());
        let id=program.generic_evidence().unwrap().callable_values().next().unwrap().0;
        drop(parsed);drop(declarations);drop(bodies);assert!(solved.upgrade().is_none());(program,id)
    }

    fn captures(program:&FullProgram,id:CallableValueId)->Vec<RuntimeCallableCapture> {
        let target=program.generic_evidence().unwrap().callable_value(id).unwrap().contract.target;
        let environment = super::super::host_environment::HostBindingEnvironment::new(Vec::new());
        program.function_view_by_id(target).unwrap().header().unwrap().captures.iter().map(|capture|RuntimeCallableCapture {
            slot:capture.slot,
            live_cell: None,
            host_binding: capture.host_binding.map(|_| {
                let (id, _) = program.generic_evidence().unwrap().host_binding_capture_for_slot(target, capture.slot as u32).unwrap().unwrap();
                super::super::host_environment::CapturedHostBinding::new(&environment, id)
            }),
            value:match capture.name.as_str().as_str() {
                "base"=>Value::Int(3),
                "args"=>Value::List(Vec::new()),
                other=>panic!("unexpected fixture capture {other}"),
            },
        }).collect()
    }

    #[test]
    fn prepared_callable_survives_container_and_host_value_round_trips_with_its_creation() {
        std::thread::Builder::new().stack_size(64*1024*1024).spawn(|| {
            for (source,kind,storage,wrong_storage,semantic_type) in [
                ("let base: Int = 3\npure plus(value: Int) -> Int { value + base }\nlet alias = plus\n",CallableKind::Pure,LoweredType::Pure,LoweredType::Proc,Type::Pure),
                ("let base: Int = 3\nproc plus(value: Int) [] -> Int { value + base }\nlet alias = plus\n",CallableKind::Proc,LoweredType::Proc,LoweredType::Pure,Type::Proc),
            ] {
                let (program,id)=fixture(source);
                let contract=program.generic_evidence().unwrap().callable_value(id).unwrap().contract;
                let header=program.function_view_by_id(contract.target).unwrap().header().unwrap();
                let base_slot=header.captures.iter().find(|capture|capture.name.as_str().as_str()=="base").unwrap().slot;
                let handle=RuntimeCallableValue::new(Arc::clone(&program),id,captures(&program,id)).unwrap();
                assert_eq!(handle.contract(),contract);
                assert!(Arc::ptr_eq(handle.program(),&program));
                let weak=Arc::downgrade(&program);drop(program);
                let nested=LoweredValue::List(vec![LoweredValue::ResultOk(Box::new(LoweredValue::Callable(handle.clone())))]).into_value();
                let restored=lowered_value_from_runtime_any(&nested).unwrap();
                let LoweredValue::List(list)=restored else{panic!()};
                let LoweredValue::ResultOk(value)=&list[0] else{panic!()};
                let LoweredValue::Callable(restored)=value.as_ref() else{panic!()};
                assert_eq!(restored,&handle);
                assert_eq!(restored.id(),id);
                assert_eq!(restored.captures().iter().find(|capture|capture.slot==base_slot).unwrap().value,Value::Int(3));
                assert!(weak.upgrade().is_some());
                assert_eq!(restored.kind(),kind);
                let host=Value::Callable(handle);
                let typed=lowered_value_from_runtime(&host,storage).unwrap();
                assert!(lowered_value_from_runtime(&host,wrong_storage).is_none());
                assert!(value_matches_static_type(&host,&semantic_type));
                assert!(lowered_value_matches_static_type(&typed,&semantic_type));
                assert!(modules::test_value_matches_type(&host,&semantic_type));
                assert_eq!(modules::encode_cache_key_value(&host),Err(host.type_name()));
            }
        }).unwrap().join().unwrap();
    }

    #[test]
    fn prepared_callable_refuses_foreign_identity_and_incomplete_or_wrong_creation_captures() {
        std::thread::Builder::new().stack_size(64*1024*1024).spawn(|| {
            let source="let base: Int = 3\npure plus(value: Int) -> Int { value + base }\nlet alias = plus\n";
            let (program,id)=fixture(source);
            let actual=captures(&program,id);
            let valid=RuntimeCallableValue::new(Arc::clone(&program),id,actual.clone()).unwrap();
            let another_creation=RuntimeCallableValue::new(Arc::clone(&program),id,actual.clone()).unwrap();
            assert_ne!(valid,another_creation);
            let mut missing=actual.clone();missing.pop();
            assert!(RuntimeCallableValue::new(Arc::clone(&program),id,missing).is_err());
            assert!(actual.len()>1);
            let mut duplicate=actual.clone();duplicate[1].slot=duplicate[0].slot;
            assert!(RuntimeCallableValue::new(Arc::clone(&program),id,duplicate).is_err());
            let target=program.generic_evidence().unwrap().callable_value(id).unwrap().contract.target;
            let header=program.function_view_by_id(target).unwrap().header().unwrap();
            let base=header.captures.iter().position(|capture|capture.name.as_str().as_str()=="base").unwrap();
            for replacement in [Value::Str("wrong".into()),Value::Null,Value::Result(crate::runtime::value::ResultValue::Ok(Box::new(Value::Int(3))))] {
                let mut wrong=actual.clone();wrong[base].value=replacement;
                assert!(RuntimeCallableValue::new(Arc::clone(&program),id,wrong).is_err());
            }
            let (_,foreign)=fixture(source);
            assert!(RuntimeCallableValue::new(program,foreign,actual).is_err());
        }).unwrap().join().unwrap();
    }

    #[test]
    fn prepared_callable_refuses_to_snapshot_a_mutable_lexical_capture() {
        std::thread::Builder::new().stack_size(64*1024*1024).spawn(|| {
            let (program,id)=fixture("var base: Int = 3\nproc plus(value: Int) [] -> Int { value + base }\nlet alias = plus\n");
            let target=program.generic_evidence().unwrap().callable_value(id).unwrap().contract.target;
            let header=program.function_view_by_id(target).unwrap().header().unwrap();
            assert!(header.captures.iter().any(|capture|capture.mutable));
            let error=RuntimeCallableValue::new(Arc::clone(&program),id,captures(&program,id)).unwrap_err();
            assert_eq!(error.kind,"unsupported-callable-value");
            assert!(error.message.contains("live binding environment"));
        }).unwrap().join().unwrap();
    }
}
