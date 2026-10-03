use super::*;
use super::super::super::generic::{PreparedOperationAuthority, is_fs_root_method_owner};
use super::super::tests::{fixture, program_name, run_with_large_stack};
use crate::runtime::eval::Evaluator;
use crate::runtime::value::{ResultValue, RuntimeError, Value};
use std::os::unix::ffi::OsStringExt;

const SOURCE: &str = r#"
proc allocate() [fs] -> Result[FsRoot] { return fs.tempdir() }
proc host_path(root: FsRoot) [fs] -> Result[Path] { return root.host_path() }
proc write_root(root: FsRoot) [fs, error] -> Result[Str] {
  root.mkdir(p"child", parents: true)?
  root.write(data: "payload", path: p"guard")?
  return root.read_text(p"guard")
}
proc read_root(root: FsRoot) [fs] -> Result[Str] { return root.read_text(p"guard") }
proc close_root(root: FsRoot) [fs] -> Result[Unit] { return root.close() }
"#;

const OPTIONAL_SOURCE: &str = r#"
proc optional_children(root: FsRoot?) [fs] -> Bool { root?.children(p".") != null }
proc other_optional_children(root: FsRoot?) [fs] -> Bool { root?.children(p".") != null }
proc rejected_optional_path() [error] -> Path { error.fail("optional argument reached")?; p"guard" }
proc optional_read(root: FsRoot?) [fs, error] -> Any { root?.read_bytes(rejected_optional_path()) }
"#;

fn invoke(evaluator: &mut Evaluator, program: &FullProgram, name: &str, arguments: &[Value], recursive: bool) -> Result<Value, RuntimeError> {
    let function = LoweredFunctionKey::Name(program_name(program, name));
    crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, || {
        evaluator.call_indexed_direct(function, LoweredFunctionKind::Proc, arguments,
            Span::new(program.store.source_id, 0, 0)).expect("prepared filesystem worker exists")
    })
}

fn allocate(evaluator: &mut Evaluator, program: &FullProgram, recursive: bool) -> Value {
    let Value::Result(ResultValue::Ok(value)) = invoke(evaluator, program, "allocate", &[], recursive).unwrap() else { panic!("filesystem root allocation succeeds"); };
    assert!(matches!(value.as_ref(), Value::FsRoot(_)));
    *value
}

fn host_path(evaluator: &mut Evaluator, program: &FullProgram, root: &Value, recursive: bool) -> std::path::PathBuf {
    let Value::Result(ResultValue::Ok(value)) = invoke(evaluator, program, "host_path", std::slice::from_ref(root), recursive).unwrap() else { panic!("live capability host path succeeds"); };
    let Value::Path(path) = *value else { panic!("root host path preserves Path carrier"); };
    std::path::PathBuf::from(std::ffi::OsString::from_vec(path.bytes))
}

#[test]
fn fs_root_prepared_packets_keep_opaque_receivers_named_order_and_interior_defaults_after_frontend_drop() {
    run_with_large_stack(|| {
        let program = Arc::new(fixture("fs-root-prepared-packets.xsh", SOURCE));
        program.symbol_owner().with_current(|| {
            let (_, allocation) = program.generic_evidence().unwrap().ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::FsTempDir, .. })).expect("opaque root allocation retains its selected native source proof");
            let TypeRef::Ground(result) = allocation.contract.result else { panic!("root allocation result is closed"); };
            assert_eq!(program.store.semantic.to_type(result).unwrap(), Type::Result(Box::new(Type::FsRoot), Box::new(Type::Error)));
            assert!(allocation.contract.receiver.is_none());
            assert!(allocation.contract.arguments.is_empty());
            assert!(allocation.contract.binding.default_slots.is_empty());
            let (_, mkdir) = program.generic_evidence().unwrap().ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::FsRootMkdir, .. })).expect("root mkdir is independently prepared");
            assert!(mkdir.contract.verify_fs_root_method(&program.store.semantic).unwrap());
            assert_eq!(mkdir.contract.binding.supplied_slots.as_ref(), &[1, 3]);
            assert_eq!(mkdir.contract.binding.default_slots.as_ref(), &[2]);
            assert!(mkdir.contract.argument_sources[2].is_none());
            let (_, write) = program.generic_evidence().unwrap().ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::FsRootWrite, .. })).unwrap();
            assert_eq!(write.contract.binding.supplied_slots.as_ref(), &[2, 1]);
            assert_eq!(write.contract.arguments[0].original.name, Some(Name::intern("data")));
            assert_eq!(write.contract.arguments[1].original.name, Some(Name::intern("path")));
        });
        for recursive in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(program.clone());
            let root = allocate(&mut evaluator, &program, recursive);
            let path = host_path(&mut evaluator, &program, &root, recursive);
            assert_eq!(invoke(&mut evaluator, &program, "write_root", std::slice::from_ref(&root), recursive).unwrap(), Value::ok(Value::Str(Arc::from("payload"))));
            assert_eq!(std::fs::read(path.join("guard")).unwrap(), b"payload");
            assert!(path.join("child").is_dir());
            assert_eq!(invoke(&mut evaluator, &program, "read_root", std::slice::from_ref(&root), recursive).unwrap(), Value::ok(Value::Str(Arc::from("payload"))));
            assert_eq!(invoke(&mut evaluator, &program, "close_root", &[root], recursive).unwrap(), Value::ok(Value::Unit));
        }
    });
}

#[test]
fn fs_root_prepared_packets_reject_missing_foreign_default_and_receiver_rewrites() {
    run_with_large_stack(|| {
        let program = fixture("fs-root-prepared-packets.xsh", SOURCE);
        let foreign = fixture("fs-root-prepared-packets.xsh", SOURCE);
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::FsRootMkdir, .. })).unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let mut other = program.store.clone();
            other.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = foreign.generic_evidence().unwrap().ground_native_calls().find(|(_, proof)| is_fs_root_method_owner(proof.contract.registry_owner)).unwrap().1.source;
            assert!(FullVerifier::verify_generic_evidence(&other).is_err());
            let mut defaults = program.store.clone();
            defaults.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.binding.default_slots[0] = 3;
            assert!(FullVerifier::verify_generic_evidence(&defaults).is_err());
            let mut absent = program.store.clone();
            let packet = absent.payload(absent.data[source.instruction as usize].range()).unwrap();
            let block = IrBlockId::from_raw(packet[2]).unwrap();
            let entries = absent.blocks[block.index()].instructions;
            assert_eq!(absent.extra[entries.start as usize + 5], 0);
            absent.extra[entries.start as usize + 5] = 2;
            assert!(FullVerifier::verify_generic_evidence(&absent).is_err(), "an omitted native default keeps its original absence marker");
            let mut changed = program.store.clone();
            let receiver = proof.contract.receiver.as_ref().unwrap();
            let slot = receiver.saved.as_ref().unwrap().slot;
            let read = changed.data[receiver.instruction as usize].range();
            changed.extra[read.start as usize] = slot + 1;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let (_, other_write) = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::FsRootWrite, .. })).unwrap();
            let mut replaced = program.store.clone();
            replaced.extra[entries.start as usize + 2] = other_write.contract.receiver.as_ref().unwrap().instruction;
            assert!(FullVerifier::verify_generic_evidence(&replaced).is_err(), "another same-typed receiver read cannot replace the original capability allocation");
            let mut erased = program.store.clone();
            let any = SemanticPoolBuilder::default().intern_type(&mut erased.semantic, &Type::Any).unwrap();
            let evidence = erased.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver.as_mut().unwrap().ty = TypeRef::Ground(any);
            evidence.test_native_call_source_mut(proof.source).unwrap().expected.receiver.as_mut().unwrap().ty = TypeRef::Ground(any);
            assert!(FullVerifier::verify_generic_evidence(&erased).is_err());
            assert_eq!(encoded_fs_root_method_arguments(&program.store, source.instruction, proof.contract.argument_sources.len()).unwrap().1.as_slice(), proof.contract.argument_sources.as_ref());
        });
    });
}

#[test]
fn fs_root_prepared_workers_refuse_foreign_capabilities_before_host_writes_on_both_routes() {
    run_with_large_stack(|| {
        let program = Arc::new(fixture("fs-root-prepared-packets.xsh", SOURCE));
        for recursive in [false, true] {
            let mut owner = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            owner.indexed_program = Some(program.clone());
            let root = allocate(&mut owner, &program, recursive);
            let path = host_path(&mut owner, &program, &root, recursive);
            let mut foreign = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            foreign.indexed_program = Some(program.clone());
            let local = allocate(&mut foreign, &program, recursive);
            let local_path = host_path(&mut foreign, &program, &local, recursive);
            let result = invoke(&mut foreign, &program, "write_root", std::slice::from_ref(&root), recursive).unwrap();
            let Value::Result(ResultValue::Err(error)) = result else { panic!("foreign opaque owner is refused"); };
            let Value::Error(error) = *error else { panic!("foreign root error preserves its error carrier"); };
            assert_eq!(error.kind, "fs-root");
            assert!(!path.join("guard").exists());
            assert!(!path.join("child").exists());
            assert!(!local_path.join("guard").exists());
            assert!(!local_path.join("child").exists());
            assert_eq!(invoke(&mut owner, &program, "close_root", &[root], recursive).unwrap(), Value::ok(Value::Unit));
            assert_eq!(invoke(&mut foreign, &program, "close_root", &[local], recursive).unwrap(), Value::ok(Value::Unit));
        }
    });
}

#[test]
fn fs_root_prepared_result_receivers_keep_original_propagation_and_refuse_wrapper_rewrites() {
    run_with_large_stack(|| {
        let source = format!("{SOURCE}\nproc close_result(root: Result[FsRoot]) [fs, error] -> Result[Unit] {{ return root?.close() }}\n");
        let program = fixture("fs-root-result-receiver.xsh", &source);
        program.symbol_owner().with_current(|| {
            let (_, proof) = program.generic_evidence().unwrap().ground_native_calls().find(|(_, proof)| proof.contract.receiver.as_ref().is_some_and(|receiver|
                receiver.source_wrappers.first().is_some_and(|wrapper| wrapper.kind == ValueInitializerWrapperKind::FsRootReceiverTry))).expect("Result receiver propagation is independently retained");
            let receiver = proof.contract.receiver.as_ref().unwrap();
            let TypeRef::Ground(source) = receiver.source_type else { panic!("original Result receiver remains closed"); };
            assert_eq!(program.store.semantic.to_type(source).unwrap(), Type::Result(Box::new(Type::FsRoot), Box::new(Type::Error)));
            let TypeRef::Ground(actual) = receiver.ty else { panic!("host receiver remains closed"); };
            assert_eq!(program.store.semantic.to_type(actual).unwrap(), Type::FsRoot);
            let wrapper = receiver.source_wrappers.first().unwrap();
            let mut changed = program.store.clone();
            changed.tags[wrapper.instruction as usize] = FullTag::ExprCheckedValue;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let mut operand = program.store.clone();
            let range = operand.data[wrapper.instruction as usize].range();
            operand.extra[range.start as usize] = receiver.instruction;
            assert!(FullVerifier::verify_generic_evidence(&operand).is_err());
        });
        let program = Arc::new(program);
        for recursive in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(program.clone());
            let root = allocate(&mut evaluator, &program, recursive);
            assert_eq!(invoke(&mut evaluator, &program, "close_result", &[Value::ok(root)], recursive).unwrap(), Value::ok(Value::Unit));
        }
    });
}

#[test]
fn fs_root_optional_receivers_keep_opaque_present_capability_and_absent_arguments_lazy_after_frontend_drop() {
    run_with_large_stack(|| {
        let source = format!("{SOURCE}\n{OPTIONAL_SOURCE}");
        let program = Arc::new(fixture("fs-root-optional-receiver.xsh", &source));
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            assert_eq!(generic.original_optional_receiver_guards().count(), 3);
            for (_, proof) in generic.ground_native_calls().filter(|(_, proof)| proof.contract.receiver.as_ref()
                .is_some_and(|receiver| receiver.saved.as_ref().is_some_and(|saved| saved.guarded_read.is_some()))) {
                assert!(proof.contract.verify_fs_root_method(&program.store.semantic).unwrap());
                let receiver = proof.contract.receiver.as_ref().unwrap();
                let TypeRef::Ground(source) = receiver.source_type else { panic!("original optional source remains closed"); };
                let TypeRef::Ground(actual) = receiver.ty else { panic!("host capability remains closed"); };
                assert_eq!(program.store.semantic.to_type(source).unwrap(), Type::Optional(Box::new(Type::FsRoot)));
                assert_eq!(program.store.semantic.to_type(actual).unwrap(), Type::FsRoot);
                assert!(receiver.source_wrappers.iter().all(|wrapper| wrapper.kind != ValueInitializerWrapperKind::FsRootReceiverTry));
            }
        });
        for recursive in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(program.clone());
            let root = allocate(&mut evaluator, &program, recursive);
            assert_eq!(invoke(&mut evaluator, &program, "optional_children", &[Value::Null], recursive).unwrap(), Value::Bool(false));
            assert_eq!(invoke(&mut evaluator, &program, "optional_children", std::slice::from_ref(&root), recursive).unwrap(), Value::Bool(true));
            assert_eq!(invoke(&mut evaluator, &program, "optional_read", &[Value::Null], recursive).unwrap(), Value::Null);
            let error = invoke(&mut evaluator, &program, "optional_read", std::slice::from_ref(&root), recursive)
                .expect_err("present receivers evaluate their authored argument before the host read");
            assert!(error.message.contains("optional argument reached"));
            assert!(evaluator.call_stack.is_empty());
            assert_eq!(invoke(&mut evaluator, &program, "close_root", &[root], recursive).unwrap(), Value::ok(Value::Unit));
        }
    });
}

#[test]
fn fs_root_optional_receivers_refuse_missing_foreign_and_joint_guard_or_source_substitution() {
    run_with_large_stack(|| {
        let source = format!("{SOURCE}\n{OPTIONAL_SOURCE}");
        let program = fixture("fs-root-optional-receiver.xsh", &source);
        let foreign_program = fixture("fs-root-optional-receiver.xsh", &source);
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let guards = generic.original_optional_receiver_guards().collect::<Vec<_>>();
            let guard = guards[0];
            let other = guards.iter().find(|other| other.owner != guard.owner).unwrap();
            let (id, proof) = generic.ground_native_calls().find(|(_, proof)| proof.contract.receiver.as_ref()
                .is_some_and(|receiver| receiver.saved.as_ref().and_then(|saved| saved.guarded_read) == Some(guard.read))).unwrap();
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_original_compiler_argument_wrappers();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let mut foreign = program.store.clone();
            foreign.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = foreign_program.generic_evidence().unwrap()
                .ground_native_calls().find(|(_, proof)| proof.contract.receiver.as_ref().is_some_and(|receiver|
                    receiver.saved.as_ref().is_some_and(|saved| saved.guarded_read.is_some()))).unwrap().1.source;
            assert!(FullVerifier::verify_generic_evidence(&foreign).is_err());
            let mut carrier = program.store.clone();
            let range = carrier.data[guard.wrapper as usize].range();
            carrier.extra[range.start as usize] = other.carrier;
            let receipt = carrier.generic.as_deref_mut().unwrap().test_original_compiler_argument_wrapper_mut(guard.wrapper).unwrap();
            receipt.initializer = other.carrier;
            receipt.payload[0] = other.carrier;
            receipt.optional_receiver_guard.as_mut().unwrap().carrier = other.carrier;
            assert!(FullVerifier::verify_generic_evidence(&carrier).is_err(), "a same-typed carrier cannot replace the original opaque optional root");
            let mut rewritten = program.store.clone();
            let evidence = rewritten.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver.as_mut().unwrap().source_instruction = other.carrier;
            evidence.test_native_call_source_mut(proof.source).unwrap().expected.receiver.as_mut().unwrap().source_instruction = other.carrier;
            assert!(FullVerifier::verify_generic_evidence(&rewritten).is_err(), "joint public contract edits cannot replace the original root source");
        });
    });
}
