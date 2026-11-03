use crate::diagnostic::{Diagnostic, DiagnosticCode, Label};
use crate::sema::check::CheckBundleView;
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

/// The wire mappings of the standard modules' enums that declare wire strings.
pub fn builtin_wire_mappings() -> &'static [Arc<WireEnumMapping>] {
    static MAPPINGS: std::sync::OnceLock<Vec<Arc<WireEnumMapping>>> = std::sync::OnceLock::new();
    MAPPINGS.get_or_init(|| {
        xsh_registry::enums::builtin_enums()
            .iter()
            .filter(|entry| entry.variants.iter().all(|variant| variant.wire.is_some()))
            .map(|entry| {
                Arc::new(WireEnumMapping {
                    type_name: Name::intern(entry.name),
                    variants: entry
                        .variants
                        .iter()
                        .filter_map(|variant| Some((Name::intern(variant.name), Arc::from(variant.wire?))))
                        .collect(),
                })
            })
            .collect()
    })
}

pub fn builtin_wire_mapping(name: &str) -> Arc<WireEnumMapping> {
    let name = Name::intern(name);
    builtin_wire_mappings()
        .iter()
        .find(|mapping| mapping.type_name == name)
        .unwrap_or_else(|| panic!("`{name}` is not a standard enum with wire strings"))
        .clone()
}

pub fn nominal_enum_name(namespace: Option<Name>, name: Name) -> Name {
    namespace.map_or(name, |namespace| {
        Name::intern(format!("{namespace}.{name}"))
    })
}

impl PreparedWireEnums {
    pub(super) fn merge_visible(&mut self, other: &Self) {
        for (&name, mapping) in &other.mappings {
            if let Some(existing) = self.mappings.get(&name) {
                assert_eq!(
                    existing, mapping,
                    "wire enum `{name}` has conflicting visible owner mappings"
                );
            } else {
                self.mappings.insert(name, Arc::clone(mapping));
            }
        }
    }

    pub fn prepare(
        program: &ArenaProgram,
        analyze: impl Fn(ExprId) -> Option<LiteralConstant>,
    ) -> (Self, Vec<Diagnostic>) {
        Self::prepare_scopes(&CheckBundleView::whole_program(program), analyze)
    }

    pub(super) fn prepare_bundle(
        view: &CheckBundleView<'_>,
        analyze: impl Fn(ExprId) -> Option<LiteralConstant>,
    ) -> (Self, Vec<Diagnostic>) {
        Self::prepare_scopes(view, analyze)
    }

    fn prepare_scopes(
        view: &CheckBundleView<'_>,
        analyze: impl Fn(ExprId) -> Option<LiteralConstant>,
    ) -> (Self, Vec<Diagnostic>) {
        let program = view.program;
        let mut prepared = Self::default();
        for mapping in builtin_wire_mappings() {
            prepared.mappings.insert(mapping.type_name, Arc::clone(mapping));
        }
        let mut diagnostics = Vec::new();
        let scopes = std::iter::once((program.root_nominal_namespace, view.statement_ids().collect::<Vec<_>>()))
        .chain(view.modules.iter().copied().map(|module| {
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
