use crate::diagnostic::{Diagnostic, DiagnosticCode, Label};
use crate::sema::constants::LiteralConstant;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaProgram, ArenaStmtKind, ArenaTypeDefBody, ExprId};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A declaring enum's validated wire mapping, shared by constructors and schemas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireEnumMapping {
    pub type_name: Name,
    pub variants: BTreeMap<Name, Arc<str>>,
}

#[derive(Clone, Debug, Default)]
pub struct PreparedWireEnums {
    pub mappings: FxHashMap<Name, Arc<WireEnumMapping>>,
}

pub fn nominal_enum_name(namespace: Option<Name>, name: Name) -> Name {
    namespace.map_or(name, |namespace| {
        Name::intern(format!("{namespace}.{name}"))
    })
}

pub fn declaring_enum_name(program: &ArenaProgram, id: crate::syntax::arena::TypeDefId) -> Name {
    let namespace = program
        .modules
        .iter()
        .find(|module| {
            program.module_statements(module).any(|statement| {
                let kind = match program.arena.stmt(statement).kind {
                    ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                    kind => kind,
                };
                matches!(kind, ArenaStmtKind::TypeDef(candidate) if candidate == id)
            })
        })
        .map(|module| module.name);
    nominal_enum_name(
        namespace.or(program.root_nominal_namespace),
        program.arena.type_def(id).name,
    )
}

impl PreparedWireEnums {
    pub fn prepare(
        program: &ArenaProgram,
        analyze: impl Fn(ExprId) -> Option<LiteralConstant>,
    ) -> (Self, Vec<Diagnostic>) {
        let mut prepared = Self::default();
        let mut diagnostics = Vec::new();
        let scopes = std::iter::once((
            program.root_nominal_namespace,
            program.statement_ids().collect::<Vec<_>>(),
        ))
        .chain(program.modules.iter().map(|module| {
            (
                Some(module.name),
                program.module_statements(module).collect(),
            )
        }));
        for (namespace, statements) in scopes {
            for statement in statements {
                let kind = match program.arena.stmt(statement).kind {
                    ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                    kind => kind,
                };
                let ArenaStmtKind::TypeDef(id) = kind else {
                    continue;
                };
                let definition = program.arena.type_def(id);
                let ArenaTypeDefBody::TagUnion(range) = definition.body else {
                    continue;
                };
                let variants = program.arena.tag_variants(range);
                if !variants.iter().any(|variant| variant.wire_value.is_some()) {
                    continue;
                }
                let mut mapping = BTreeMap::new();
                let mut strings = BTreeMap::new();
                let mut valid = true;
                for variant in variants {
                    let value = variant.wire_value.and_then(&analyze);
                    let message = if !variant.fields.is_empty() {
                        Some("Str-backed enum variants cannot have payload fields")
                    } else if let Some(LiteralConstant::Str(value)) = value {
                        if strings.insert(value.clone(), variant.name).is_some() {
                            Some("Str-backed enum wire strings must be unique")
                        } else {
                            mapping.insert(variant.name, value);
                            None
                        }
                    } else {
                        Some(
                            "every Str-backed enum variant requires a bounded constant Str mapping",
                        )
                    };
                    if let Some(message) = message {
                        valid = false;
                        diagnostics.push(
                            Diagnostic::error(message)
                                .with_code(DiagnosticCode::CheckEnumWireMapping)
                                .with_label(Label::primary(
                                    program.arena.span(variant.span),
                                    message,
                                )),
                        );
                    }
                }
                if valid {
                    let type_name = nominal_enum_name(namespace, definition.name);
                    prepared.mappings.insert(
                        type_name,
                        Arc::new(WireEnumMapping {
                            type_name,
                            variants: mapping,
                        }),
                    );
                }
            }
        }
        (prepared, diagnostics)
    }
}
