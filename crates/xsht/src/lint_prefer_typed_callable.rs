//! `lint.prefer-typed-callable`: a `Proc` or `Pure` parameter of a private
//! function whose every call passes a function with one signature can state
//! that signature as a callable type, so the call through it is checked.

use rustc_hash::FxHashSet;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label};
use xsh::frontend::check::{CheckOutput, CheckedCallableContract, CheckedFunctionKind};
use xsh::frontend::source::Span;
use xsh::frontend::symbols::{Name, Symbol};
use xsh::frontend::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaRange, ArenaTypeExprTag, AstArena, ExprId,
    FunctionDefId, TypeExprId,
};

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
    arguments: Vec<(Option<Name>, Span)>,
    /// A splice or spread decides the arguments at run time.
    indirect: bool,
}

impl CallableParameters {
    /// A top-level function definition the traversal reached.
    pub(super) fn define(
        &mut self,
        name: Name,
        definition: FunctionDefId,
        pure: bool,
        enumerable: bool,
    ) {
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
            arguments.push((label, arena.expr(value).span));
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
        checked: Option<&CheckOutput>,
    ) -> Vec<Diagnostic> {
        let Some(checked) = checked else {
            return Vec::new();
        };
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
                let Some(annotation) = parameter.annotation else {
                    continue;
                };
                let Some(pure) = dynamic_callable_kind(arena, annotation) else {
                    continue;
                };
                if parameter.default.is_some() || parameter.rest {
                    continue;
                }
                let Some(spelled) = Self::one_signature(
                    checked,
                    &calls,
                    index,
                    parameter.name,
                    pure,
                ) else {
                    continue;
                };
                diagnostics.push(
                    Diagnostic::note(format!(
                        "every call of `{}` passes a function with one signature as `{}`",
                        function.name, parameter.name
                    ))
                    .with_code(DiagnosticCode::LintPreferTypedCallable)
                    .with_label(Label::primary(
                        arena.type_expr_span(annotation),
                        format!("the callable type is `{spelled}`"),
                    ))
                    .with_note(
                        "a callable type checks the arguments, result, and effects of the call through the parameter. The body then calls it directly, `name(...)`, instead of `name.call(...)`, and uses its declared result",
                    ),
                );
            }
        }
        diagnostics
            .sort_by_key(|diagnostic| diagnostic.labels.first().map(|label| label.span.start()));
        diagnostics
    }

    fn one_signature(
        checked: &CheckOutput,
        calls: &[&Call],
        index: usize,
        parameter: Name,
        pure: bool,
    ) -> Option<String> {
        let expected_kind = if pure {
            CheckedFunctionKind::Pure
        } else {
            CheckedFunctionKind::Proc
        };
        let mut contract: Option<CheckedCallableContract> = None;
        for call in calls {
            let span = call
                .arguments
                .iter()
                .enumerate()
                .find(|(position, (label, _))| match label {
                    Some(label) => *label == parameter,
                    None => *position == index,
                })?
                .1
                .1;
            let found = checked.callable_contract_at(span)?;
            if found.kind() != expected_kind {
                return None;
            }
            contract = Some(match contract {
                Some(previous) => previous.join_effects(&found)?,
                None => found,
            });
        }
        contract?.source_annotation()
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
