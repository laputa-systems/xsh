//! `lint.prefer-typed-callable`: a `Proc` or `Pure` parameter of a private
//! function whose every call passes a function with one signature can state
//! that signature as a callable type, so the call through it is checked.

use rustc_hash::FxHashSet;
use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label};
use xsh::frontend::check::{FunctionEffectFact, Type};
use xsh::frontend::source::Span;
use xsh::frontend::symbols::{Name, Symbol};
use xsh::frontend::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaRange, ArenaTypeExprTag, AstArena, ExprId, FunctionDefId,
    TypeExprId,
};
use xsh::frontend::syntax::node::Effect;

/// What the linter's traversal saw of the linted module's top-level
/// functions: their definitions, the calls that name them, and every other
/// use of their names.
///
/// A parameter is reported only when the traversal accounts for every way
/// its function is reached. A private top-level function is reachable only
/// through its name in this module, so its callers are exactly the calls
/// recorded here unless the name is also used as a value, which hands the
/// function to callers this cannot enumerate. An exported function has
/// callers in other modules and is never a candidate.
///
/// There is no fix. A callable type changes how the body calls the
/// parameter: `callback.call(x)`, a `Result[Any]`, becomes `callback(x)` with
/// the declared result, and what the body did with the dynamic value has to
/// change with it.
#[derive(Default)]
pub(super) struct CallableParameters {
    functions: Vec<Function>,
    calls: Vec<Call>,
    /// Callee expressions of the recorded calls, which are not value uses.
    callees: FxHashSet<ExprId>,
    /// Functions whose name is used other than as a callee.
    values: FxHashSet<Name>,
}

struct Function {
    name: Name,
    definition: FunctionDefId,
    pure: bool,
    /// Private and top-level: every caller is a call in this module.
    enumerable: bool,
}

struct Call {
    callee: Name,
    /// Each argument entry: its label, if named, and the function it names,
    /// if it is a bare name no local hides.
    arguments: Vec<(Option<Name>, Option<Name>)>,
    /// A splice or spread decides the arguments at run time.
    indirect: bool,
}

/// The parts of a function's signature a callable type states.
#[derive(Eq, PartialEq)]
struct Signature {
    pure: bool,
    parameters: Vec<(Name, String)>,
    return_type: String,
}

impl CallableParameters {
    /// A top-level function definition the traversal reached.
    pub(super) fn define(&mut self, name: Name, definition: FunctionDefId, pure: bool, enumerable: bool) {
        self.functions.push(Function {
            name,
            definition,
            pure,
            enumerable,
        });
    }

    /// A call whose callee is a bare name. `hidden` says whether a local
    /// binding hides a name where the call is.
    pub(super) fn call(
        &mut self,
        arena: &AstArena,
        callee: ExprId,
        args: ArenaRange,
        hidden: &dyn Fn(Name) -> bool,
    ) {
        let ArenaExprKind::Ident(name) = arena.expr(callee).kind else {
            return;
        };
        if hidden(name) {
            return;
        }
        self.callees.insert(callee);
        let mut indirect = false;
        let mut arguments = Vec::new();
        for arg in arena.call_args(args) {
            let (label, value) = match arg.kind {
                ArenaCallArgKind::Positional(value) => (None, value),
                ArenaCallArgKind::Named { name, value, .. } => (Some(name), value),
                ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                    indirect = true;
                    continue;
                }
            };
            let function = match arena.expr(value).kind {
                ArenaExprKind::Ident(function) if !hidden(function) => Some(function),
                _ => None,
            };
            arguments.push((label, function));
        }
        self.calls.push(Call {
            callee: name,
            arguments,
            indirect,
        });
    }

    /// A bare name the traversal visited as an expression. A call's callee is
    /// recorded before its children are visited, so it is told apart here.
    pub(super) fn value(&mut self, expression: ExprId, name: Name, hidden: bool) {
        if !hidden && !self.callees.contains(&expression) {
            self.values.insert(name);
        }
    }

    /// A bare name used as a block's value.
    pub(super) fn tail_value(&mut self, name: Name, hidden: bool) {
        if !hidden {
            self.values.insert(name);
        }
    }

    /// The reports, in source order.
    pub(super) fn finish(
        self,
        arena: &AstArena,
        source: &str,
        checked_returns: &BTreeMap<Span, Type>,
        effects_of: &dyn Fn(Span) -> Option<FunctionEffectFact>,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for function in &self.functions {
            if !function.enumerable || self.values.contains(&function.name) {
                continue;
            }
            let calls = self
                .calls
                .iter()
                .filter(|call| call.callee == function.name)
                .collect::<Vec<_>>();
            if calls.is_empty() || calls.iter().any(|call| call.indirect) {
                continue;
            }
            let parameters = arena.params(arena.function_def(function.definition).params);
            for (index, parameter) in parameters.iter().enumerate() {
                let Some(pure) = dynamic_callable_kind(arena, parameter.ty) else {
                    continue;
                };
                if parameter.ty_defaulted || parameter.default.is_some() || parameter.rest {
                    continue;
                }
                let Some(spelled) = self.one_signature(
                    arena,
                    source,
                    checked_returns,
                    effects_of,
                    &calls,
                    index,
                    parameter.name,
                    pure,
                ) else {
                    continue;
                };
                diagnostics.push(
                    Diagnostic::warning(format!(
                        "every call of `{}` passes a function with one signature as `{}`",
                        function.name, parameter.name
                    ))
                    .with_code(DiagnosticCode::LintPreferTypedCallable)
                    .with_label(Label::primary(
                        arena.type_expr_span(parameter.ty),
                        format!("the callable type is `{spelled}`"),
                    ))
                    .with_note(
                        "a callable type checks the arguments, result, and effects of the call through the parameter. The body then calls it directly, `name(...)`, instead of `name.call(...)`, and uses its declared result",
                    ),
                );
            }
        }
        diagnostics.sort_by_key(|diagnostic| {
            diagnostic
                .labels
                .first()
                .map(|label| label.span.start())
        });
        diagnostics
    }

    /// The callable type every call passes for parameter `index`, when each
    /// passes a top-level function and their signatures agree. The effect
    /// clause is the union of what those functions need; without a known set
    /// for each, the type has no clause.
    #[allow(clippy::too_many_arguments)]
    fn one_signature(
        &self,
        arena: &AstArena,
        source: &str,
        checked_returns: &BTreeMap<Span, Type>,
        effects_of: &dyn Fn(Span) -> Option<FunctionEffectFact>,
        calls: &[&Call],
        index: usize,
        parameter: Name,
        pure: bool,
    ) -> Option<String> {
        let mut signature: Option<Signature> = None;
        let mut effects = Some(Vec::<Effect>::new());
        for call in calls {
            let passed = call
                .arguments
                .iter()
                .enumerate()
                .find(|(position, (label, _))| match label {
                    Some(label) => *label == parameter,
                    None => *position == index,
                })?
                .1
                .1?;
            let function = self
                .functions
                .iter()
                .find(|function| function.name == passed)?;
            let definition = arena.function_def(function.definition);
            let body = arena.span(arena.block(definition.body).span);
            let found = Signature {
                pure: function.pure,
                parameters: arena
                    .params(definition.params)
                    .iter()
                    .map(|param| {
                        (!param.ty_defaulted && !param.rest).then(|| {
                            source
                                .get(arena.type_expr_span(param.ty).range())
                                .map(|ty| (param.name, ty.to_string()))
                        })?
                    })
                    .collect::<Option<Vec<_>>>()?,
                return_type: if definition.return_ty_defaulted {
                    checked_returns.get(&body)?.annotation_source()?
                } else {
                    source
                        .get(arena.type_expr_span(definition.return_ty).range())?
                        .to_string()
                },
            };
            if found.pure != pure || signature.as_ref().is_some_and(|first| *first != found) {
                return None;
            }
            signature = Some(found);
            if !pure {
                match effects_of(body).and_then(|fact| fact.effective) {
                    Some(required) => {
                        if let Some(effects) = &mut effects {
                            effects.extend(required);
                        }
                    }
                    None => effects = None,
                }
            }
        }
        let signature = signature?;
        let parameters = signature
            .parameters
            .iter()
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect::<Vec<_>>()
            .join(", ");
        let clause = match effects {
            Some(effects) if !pure => format!(
                " [{}]",
                Effect::ALL
                    .iter()
                    .filter(|effect| effects.contains(effect))
                    .map(Effect::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => String::new(),
        };
        Some(format!(
            "{}({parameters}){clause} -> {}",
            if pure { "pure" } else { "proc" },
            signature.return_type
        ))
    }
}

/// `Some(true)` for a parameter annotated `Pure`, `Some(false)` for `Proc`.
fn dynamic_callable_kind(arena: &AstArena, ty: TypeExprId) -> Option<bool> {
    if arena.type_expr_tags[ty.index()] != ArenaTypeExprTag::Named {
        return None;
    }
    let name = Name::from_symbol(Symbol::from_raw(arena.type_expr_data[ty.index()].lhs));
    match name.as_str().as_str() {
        "Pure" => Some(true),
        "Proc" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
#[path = "lint_prefer_typed_callable_tests.rs"]
mod tests;
