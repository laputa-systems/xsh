use super::{Checker, Type};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{ArenaFunctionDef, ArenaProgram, ArenaStmtKind, StmtId};

impl Checker {
    pub(super) fn infer_default_parameter_types(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        statements: &[StmtId],
    ) {
        let count = statements.len();
        for _ in 0..=count {
            let before = self.parameter_types.len();
            let mut probe = self.constraint_probe();
            for &id in statements {
                let kind = match program.arena.stmt(id).kind {
                    ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                    kind => kind,
                };
                match kind {
                    ArenaStmtKind::ProcDef(id)
                    | ArenaStmtKind::PureDef(id)
                    | ArenaStmtKind::StreamDef(id) => {
                        let def = program.arena.function_def(id);
                        probe.boundary.in_pure = matches!(kind, ArenaStmtKind::PureDef(_));
                        probe.boundary.current_return = Some(if matches!(kind, ArenaStmtKind::StreamDef(_)) {
                            super::ReturnContext::Producer
                        } else {
                            super::ReturnContext::Function(probe.type_from_arena(program, def.return_ty))
                        });
                        probe.boundary.current_effects = def
                            .effects
                            .map(|effects| program.arena.effects(effects).collect());
                        for param in program.arena.params(def.params) {
                            let span = program.arena.span(param.span);
                            if let Some(annotation) = param.annotation {
                                let ty = probe.type_from_arena(program, annotation);
                                self.parameter_types.insert(span, ty.clone());
                                probe.parameter_types.insert(span, ty);
                            } else if let Some(default) = param.default
                                && let std::collections::btree_map::Entry::Vacant(entry) =
                                    self.parameter_types.entry(span)
                            {
                                let actual = probe.check_expr_arena(program, source, default, None);
                                let actual = probe
                                    .type_constraints
                                    .resolve(&actual)
                                    .unwrap_or(Type::Invalid);
                                if parameter_type_is_concrete(&actual) {
                                    entry.insert(actual.clone());
                                    probe.parameter_types.insert(span, actual);
                                }
                            }
                        }
                        self.publish_parameter_types(program, def);
                        probe.publish_parameter_types(program, def);
                    }
                    ArenaStmtKind::Let { .. }
                    | ArenaStmtKind::Var { .. }
                    | ArenaStmtKind::Const { .. }
                    | ArenaStmtKind::Use(_) => {
                        probe.boundary.current_return = None;
                        probe.boundary.in_pure = false;
                        probe.check_stmt_arena(program, source, id);
                    }
                    _ => {}
                }
            }
            if before == self.parameter_types.len() {
                break;
            }
        }
    }

    pub(super) fn checked_parameter_type(
        &self,
        program: &ArenaProgram,
        param: &crate::syntax::arena::ArenaParam,
    ) -> Option<Type> {
        self.parameter_types
            .get(&program.arena.span(param.span))
            .cloned()
    }

    pub(super) fn publish_parameter_types(
        &mut self,
        program: &ArenaProgram,
        def: &ArenaFunctionDef,
    ) {
        let declaration = super::FunctionDeclarationId::new(self.effect_declaration_id(
            program,
            def.body,
        ));
        if let Some(signature) = self.declaration_inference.pending_mut(declaration)
            && let Some(shape) = signature.provisional_mut()
        {
            for (param, slot) in program.arena.params(def.params).iter().zip(&mut shape.params) {
                if let Some(ty) = self.parameter_types.get(&program.arena.span(param.span)) {
                    slot.signature.ty = ty.clone();
                    slot.authority = if param.annotation.is_some() {
                        super::TypeAuthority::Explicit
                    } else if param.default.is_some() {
                        super::TypeAuthority::Default
                    } else {
                        super::TypeAuthority::Inferred
                    };
                }
            }
        }
    }

    pub(super) fn infer_checked_parameter(
        &mut self,
        program: &ArenaProgram,
        source: &str,
        param: &crate::syntax::arena::ArenaParam,
    ) -> Type {
        let span = program.arena.span(param.span);
        if let Some(annotation) = param.annotation {
            let ty = self.type_from_arena(program, annotation);
            self.parameter_types.insert(span, ty.clone());
            return ty;
        }
        if let Some(ty) = self.checked_parameter_type(program, param) {
            return match self.type_constraints.resolve(&ty) {
                Ok(resolved) => resolved,
                Err(crate::sema::constraints::ConstraintResolutionError::ForeignVariable) => {
                    panic!("parameter inference belongs to another checker")
                }
                Err(crate::sema::constraints::ConstraintResolutionError::TypeDepth) => {
                    self.error(
                        span,
                        "parameter inference exceeds the type depth limit",
                        DiagnosticCode::CheckLocalInference,
                    );
                    Type::Invalid
                }
            };
        }
        if let Some(default) = param.default {
            let actual = self.check_expr_arena(program, source, default, None);
            let actual = self
                .type_constraints
                .resolve(&actual)
                .unwrap_or(Type::Invalid);
            if parameter_type_is_concrete(&actual) {
                self.parameter_types.insert(span, actual.clone());
                return actual;
            }
        }
        if param.default.is_none() {
            if self.boundary.in_pure {
                let ty = self.type_constraints.fresh_at_level(span, 1);
                self.parameter_types.insert(span, ty.clone());
                return ty;
            }
            self.parameter_types.insert(span, Type::Unknown);
            Type::Unknown
        } else {
            if self.inferred_returns.is_none() {
                self.error(span, "default does not establish a concrete parameter type; declare an annotation (including in cyclic declaration dependencies)", DiagnosticCode::CheckInferParam);
            }
            Type::Invalid
        }
    }
}

pub(super) fn parameter_type_is_concrete(ty: &Type) -> bool {
    match ty {
        Type::BuiltinParameter(_)
        | Type::Inference(_)
        | Type::Any
        | Type::Unknown
        | Type::Invalid
        | Type::Null
        | Type::DynamicModule => false,
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) | Type::Set(inner) => {
            parameter_type_is_concrete(inner)
        }
        Type::Map(key, value) => {
            parameter_type_is_concrete(key) && parameter_type_is_concrete(value)
        }
        Type::Result(ok, err) => parameter_type_is_concrete(ok) && parameter_type_is_concrete(err),
        Type::Module(exports) => {
            !exports.is_empty()
                && exports.values().all(|export| match export {
                    crate::sema::types::ModuleExportType::Value { ty, .. } => {
                        parameter_type_is_concrete(ty)
                    }
                    crate::sema::types::ModuleExportType::Pure { sig, .. }
                    | crate::sema::types::ModuleExportType::Proc { sig, .. } => {
                        parameter_type_is_concrete(&sig.return_ty)
                            && sig
                                .params
                                .iter()
                                .all(|param| parameter_type_is_concrete(&param.ty))
                    }
                    crate::sema::types::ModuleExportType::GenericPure { .. } => false,
                })
        }
        Type::Record(fields) => {
            !fields.is_empty() && fields.values().all(parameter_type_is_concrete)
        }
        Type::Validated(validated) => parameter_type_is_concrete(validated.base()),
        _ => true,
    }
}

#[cfg(test)]
mod host_anchor_tests {
    use super::super::{CheckOptions, Checker, MethodReceiver};
    use crate::diagnostic::DiagnosticCode;
    use crate::sema::types::Type;
    use crate::source::{SourceId, Span};
    use crate::syntax::parser::Parser;

    #[test]
    fn inferred_host_facts_follow_late_parameter_constraints() {
        let source = "pure anchor(value: Str) -> Bool { value == \"x\" }\n\
                      pure inspect(text) -> Int {\n\
                      let changed = text.replace(\"\\t\", with: \" \")\n\
                      var count = 0\n\
                      for line in text.split(\"\\n\") {\n\
                      for word in line.split(\"\\t\") { count += 1 }\n}\n\
                      let anchored = anchor(text)\n\
                      count\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let mut counts = Vec::new();
            for _ in 0..2 {
                let checked = Checker::check_arena_with_options(
                    &parsed.arena,
                    source,
                    CheckOptions::for_execution(),
                );
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                assert_eq!(checked.api_calls.len(), 3);
                for call in checked.api_calls.values() {
                    assert_eq!(call.receiver, Some(MethodReceiver::Str));
                    match call.sig.op {
                        crate::modules::signature::RuntimeOp::TextReplace => {
                            assert_eq!(call.params, vec![Type::Str, Type::Str]);
                            assert_eq!(call.argument_slots, vec![0, 1]);
                        }
                        crate::modules::signature::RuntimeOp::TextSplit => {
                            assert_eq!(call.params, vec![Type::Str, Type::Int]);
                            assert_eq!(call.argument_slots, vec![0]);
                        }
                        operation => panic!("unexpected registered operation {operation:?}"),
                    }
                }
                let start = source.find("line.split").expect("nested receiver");
                assert_eq!(checked.expr_types.get(&Span::new(SourceId::new(0), start, start + 4)), Some(&Type::Str));
                assert_eq!(checked.function_body_check_entries, 2);
                counts.push(checked.function_body_check_entries);
            }
            assert_eq!(counts[0], counts[1]);
        });
    }

    #[test]
    fn host_receiver_domains_preserve_record_map_and_signed_unsigned_alternatives() {
        for source in [
            "type Row = {value: Int}\npure keys(value: Row) -> List[Str] { value.keys() }\n",
            "pure keys(value: Map[Str, Int]) -> List[Str] { value.keys() }\n",
            "pure signed(value: Int) -> Float { value.float() }\npure unsigned(value: UInt) -> Float { value.float() }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        }
        for (source, code) in [
            ("pure keys(value) -> List[Str] { value.keys() }\nlet output = keys({value: 1})\n", DiagnosticCode::CheckAmbiguousOverload),
            ("pure number(value) -> Float { value.float() }\nlet output = number(1)\n", DiagnosticCode::CheckLocalInference),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(code)), "{source}: {:?}", checked.diagnostics);
            assert!(checked.diagnostics.iter().all(|diagnostic| diagnostic.code != Some(DiagnosticCode::CheckUnknownMethod)), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn inferred_host_obligations_publish_indirect_chain_and_reordered_facts() {
        for (bindings, operations) in [
            ("let flat = text.trim()", vec![crate::modules::signature::RuntimeOp::TextTrim]),
            ("let first = text.trim(); let flat = first.trim()", vec![crate::modules::signature::RuntimeOp::TextTrim; 2]),
            ("let flat = text.replace(\"\\t\", with: \" \"); let clean = text.trim(); let proof = anchor(clean)", vec![crate::modules::signature::RuntimeOp::TextReplace, crate::modules::signature::RuntimeOp::TextTrim]),
            ("let clean = text.trim(); let proof = anchor(clean); let flat = text.replace(\"\\t\", with: \" \")", vec![crate::modules::signature::RuntimeOp::TextTrim, crate::modules::signature::RuntimeOp::TextReplace]),
        ] {
            for anchor_first in [false, true] {
                for receiver in ["text", "input"] {
                    let anchor = "pure anchor(value: Str) -> Bool { value == \"x\" }";
                    let body = format!("pure inspect({receiver}) -> Int {{\n{}\nlet anchored = anchor(flat)\nvar count = 0\nfor line in flat.split(\"\\n\") {{ for word in line.split(\" \") {{ count += 1 }} }}\ncount\n}}", bindings.replace("text", receiver));
                    let source = if anchor_first { format!("{anchor}\n{body}\n") } else { format!("{body}\n{anchor}\n") };
                    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                    parsed.arena.symbol_owner().with_current(|| {
                        let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
                        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                        let mut expected = operations.clone();
                        expected.extend([crate::modules::signature::RuntimeOp::TextSplit; 2]);
                        assert_eq!(checked.api_calls.values().map(|call| call.sig.op).collect::<Vec<_>>(), expected);
                        for call in checked.api_calls.values() {
                            assert_eq!(call.receiver, Some(MethodReceiver::Str));
                            assert!(call.sig.pure);
                            assert!(call.sig.effects.is_empty());
                            match call.sig.op {
                                crate::modules::signature::RuntimeOp::TextReplace => {
                                    assert_eq!(call.params, vec![Type::Str, Type::Str]);
                                    assert_eq!(call.argument_slots, vec![0, 1]);
                                }
                                crate::modules::signature::RuntimeOp::TextSplit => {
                                    assert_eq!(call.params, vec![Type::Str, Type::Int]);
                                    assert_eq!(call.argument_slots, vec![0]);
                                }
                                crate::modules::signature::RuntimeOp::TextTrim => {
                                    assert!(call.params.is_empty());
                                    assert!(call.argument_slots.is_empty());
                                }
                                operation => panic!("unexpected registered operation {operation:?}"),
                            }
                        }
                        let start = source.find("line.split").unwrap();
                        assert_eq!(checked.expr_types.get(&Span::new(SourceId::new(0), start, start + 4)), Some(&Type::Str));
                        assert!(checked.parameter_types.values().all(|ty| *ty == Type::Str));
                        assert!(checked.expr_types.values().all(|ty| !ty.contains_inference()));
                        assert_eq!(checked.function_body_check_entries, 2);
                    });
                }
            }
        }
    }

    #[test]
    fn inferred_host_obligations_reject_actual_ambiguity_without_caller_evidence() {
        for caller in ["\"a\\tb\"", "rx\"a\""] {
            let source = format!("pure anchor(value: Str) -> Bool {{ value == \"x\" }}\npure ambiguous(text) -> Str {{ let flat = text.replace(\"\\t\", with: \" \"); let anchored = anchor(flat); flat }}\nlet output = ambiguous({caller})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckAmbiguousOverload)), "{:?}", checked.diagnostics);
        }
        let source = "pure ambiguous(raw) -> Int? { raw.byte_at(0) }\nlet output = ambiguous(b\"A\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckAmbiguousOverload)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn inferred_host_anchors_preserve_ambiguity_arguments_effects_and_annotations() {
        for (source, code) in [
            (
                "pure invalid(text) -> Str { text.replace(1, with: \" \") }\n",
                DiagnosticCode::CheckTypeMismatch,
            ),
            (
                "pure invalid(text) -> Str { text.unregistered_operation() }\n",
                DiagnosticCode::CheckUnknownMethod,
            ),
            (
                "pure first_byte(raw) -> Int { raw.byte_at(0) ?? 0 }\nlet byte: Int = first_byte(b\"A\")\n",
                DiagnosticCode::CheckAmbiguousOverload,
            ),
            (
                "pure anchor(value: Str) -> Bool { value == \"x\" }\npure invalid(text) -> Str { let changed = text.replace(1, with: \" \"); let anchored = anchor(text); changed }\n",
                DiagnosticCode::CheckTypeMismatch,
            ),
            (
                "pure anchor(value: Path) -> Bool { value == p\"x\" }\npure invalid(path) -> Str { let text = path.read_text() ?? \"\"; let anchored = anchor(path); text }\n",
                DiagnosticCode::CheckPureEffect,
            ),
            (
                "pure anchor(value: Str) -> Bool { value == \"x\" }\npure invalid(text) -> Int { let changed = text.replace(\"\\t\", with: \" \"); let anchored = anchor(text); changed }\n",
                DiagnosticCode::CheckTypeMismatch,
            ),
            (
                "pure anchor(value: Str) -> Bool { value == \"x\" }\npure invalid(text: Any) -> Str { let changed = text.replace(\"\\t\", with: \" \"); let anchored = anchor(text); changed }\n",
                DiagnosticCode::CheckDynamicBoundary,
            ),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(
                &parsed.arena,
                source,
                CheckOptions::for_execution(),
            );
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(code)), "{code:?}: {:?}", checked.diagnostics);
        }
    }
}
