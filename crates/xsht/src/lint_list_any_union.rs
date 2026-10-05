//! `lint.list-any-union`: a binding annotated `List[Any]` that is only ever
//! filled from list literals of a few concrete element types has the closed
//! type `List[Union[A, B]]`.

use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, Label};
use xsh::frontend::check::{Type, union_member_error};
use xsh::frontend::source::Span;
use xsh::frontend::symbols::{Name, Symbol};
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaTypeExprTag, AssignTargetId,
    AstArena, TypeExprId,
};
use xsh::frontend::syntax::node::AssignOp;

/// More members than this is a sign the list is heterogeneous on purpose.
const MAX_MEMBERS: usize = 4;

/// The `List[Any]` bindings of the linted module whose every element is still
/// known to come from a typed list literal.
///
/// The linter's statement traversal reports each declaration and each
/// assignment it visits, so only the linted module is examined. A list is a
/// value: its elements change only through an assignment that names the
/// binding, and the traversal reaches every one of those, including the ones
/// in nested blocks and callables. That is what makes the element types
/// provable. A parameter or a record field is filled from places this cannot
/// enumerate and is never a candidate.
///
/// There is no fix. The union changes the binding's type, so a use that
/// passes it where `List[Any]` is expected stops checking (lists are
/// invariant), and reads of its elements have to narrow.
#[derive(Default)]
pub(super) struct ListAnyBindings {
    candidates: Vec<Candidate>,
}

struct Candidate {
    /// The span the linter's scope records for the binding, which tells this
    /// binding from another of the same name.
    definition: Span,
    annotation: Span,
    /// Element types in the order first written, or `None` once a write was
    /// seen whose elements are not typed literal elements.
    members: Option<Vec<Type>>,
}

impl ListAnyBindings {
    /// A `let`, `const`, or `var` the traversal has just defined.
    pub(super) fn declare(
        &mut self,
        arena: &AstArena,
        definition: Span,
        ty: Option<TypeExprId>,
        initializer: &ArenaExprOrRun,
        expr_types: &BTreeMap<Span, Type>,
    ) {
        let Some(ty) = ty else {
            return;
        };
        if !is_list_of_any(arena, ty)
            || self
                .candidates
                .iter()
                .any(|candidate| candidate.definition == definition)
        {
            return;
        }
        let mut members = Some(Vec::new());
        // The binding is not in scope in its own initializer.
        add_literal_elements(arena, initializer, expr_types, &|_| false, &mut members);
        self.candidates.push(Candidate {
            definition,
            annotation: arena.type_expr_span(ty),
            members,
        });
    }

    /// An assignment whose target's root name the linter's scopes resolve to
    /// the binding defined at `definition`. A name is declared before any
    /// assignment to it checks, so the declaration was reported first.
    /// `definition_of` resolves a name where the assignment is.
    pub(super) fn assign(
        &mut self,
        arena: &AstArena,
        definition: Span,
        target: AssignTargetId,
        op: AssignOp,
        value: &ArenaExprOrRun,
        expr_types: &BTreeMap<Span, Type>,
        definition_of: &dyn Fn(Name) -> Option<Span>,
    ) {
        let Some(candidate) = self
            .candidates
            .iter_mut()
            .find(|candidate| candidate.definition == definition)
        else {
            return;
        };
        // `xs = [...]` and `xs += [...]` write typed literal elements. An
        // element or field assignment, or any other operator, does not.
        let whole_list = matches!(
            arena.assign_target(target).kind,
            ArenaAssignTargetKind::Name(_)
        ) && matches!(op, AssignOp::Set | AssignOp::Add);
        if whole_list {
            // `xs = [@xs, ...]` keeps the elements it already has.
            let is_binding = |name| definition_of(name) == Some(definition);
            add_literal_elements(
                arena,
                value,
                expr_types,
                &is_binding,
                &mut candidate.members,
            );
        } else {
            candidate.members = None;
        }
    }

    /// The reports for the candidates that stayed provable, in source order.
    pub(super) fn finish(self) -> Vec<Diagnostic> {
        let mut candidates = self.candidates;
        candidates.sort_by_key(|candidate| candidate.annotation.start());
        candidates
            .into_iter()
            .filter_map(|candidate| {
                let mut members = candidate.members?;
                // Sites that hold the same types get the same suggestion,
                // whichever element happened to be written first.
                members.sort_by_key(member_rank);
                // One element type is `List[T]`, which the annotation could
                // simply say; that is a different rewrite from a union.
                if members.len() < 2
                    || members.len() > MAX_MEMBERS
                    || union_member_error(&members).is_some()
                {
                    return None;
                }
                let spelled = members
                    .iter()
                    .map(spelled_member)
                    .collect::<Option<Vec<_>>>()?
                    .join(", ");
                Some(
                    Diagnostic::warning(format!(
                        "every element of this `List[Any]` is one of {} types",
                        members.len()
                    ))
                    .with_code(DiagnosticCode::LintListAnyUnion)
                    .with_label(Label::primary(
                        candidate.annotation,
                        format!("the closed type is `List[Union[{spelled}]]`"),
                    ))
                    .with_note(
                        "a union keeps the element types checked: reads narrow with `is` or a type pattern, and a value of any other type is rejected. Uses that pass this list where `List[Any]` is expected need the same union type",
                    ),
                )
            })
            .collect()
    }
}

/// The root name an assignment target writes through: `xs` for `xs`,
/// `xs[0]`, and `xs.field`.
pub(super) fn assigned_root(arena: &AstArena, mut target: AssignTargetId) -> Option<Name> {
    loop {
        match arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(name) => return Some(name),
            ArenaAssignTargetKind::Field { base, .. }
            | ArenaAssignTargetKind::Index { base, .. } => target = base,
            ArenaAssignTargetKind::Env(_) => return None,
        }
    }
}

/// Adds the element types of the list literal `value` to `members`, or
/// clears `members` when `value` is not a list literal whose every element
/// has a checked concrete type. A spliced `List[T]` contributes `T`, and a
/// splice of the binding being filled (`is_binding`) contributes nothing.
fn add_literal_elements(
    arena: &AstArena,
    value: &ArenaExprOrRun,
    expr_types: &BTreeMap<Span, Type>,
    is_binding: &dyn Fn(Name) -> bool,
    members: &mut Option<Vec<Type>>,
) {
    let Some(known) = members else {
        return;
    };
    let ArenaExprOrRun::Expr(value) = *value else {
        *members = None;
        return;
    };
    let ArenaExprKind::List(items) = arena.expr(value).kind else {
        *members = None;
        return;
    };
    for item in arena.list_elements(items) {
        if item.splice_span.is_some()
            && matches!(arena.expr(item.value).kind, ArenaExprKind::Ident(name) if is_binding(name))
        {
            continue;
        }
        let element = match expr_types.get(&arena.expr(item.value).span) {
            Some(Type::List(spliced)) if item.splice_span.is_some() => (**spliced).clone(),
            Some(element) if item.splice_span.is_none() => element.clone(),
            _ => {
                *members = None;
                return;
            }
        };
        if !known.contains(&element) {
            known.push(element);
        }
    }
}

fn is_list_of_any(arena: &AstArena, ty: TypeExprId) -> bool {
    if arena.type_expr_tags[ty.index()] != ArenaTypeExprTag::List {
        return false;
    }
    let item = TypeExprId::from_index(arena.type_expr_data[ty.index()].lhs as usize);
    arena.type_expr_tags[item.index()] == ArenaTypeExprTag::Named
        && Name::from_symbol(Symbol::from_raw(arena.type_expr_data[item.index()].lhs)) == "Any"
}

/// The position of a member in a suggested union: text, then paths, then the
/// other scalars, then collections.
fn member_rank(member: &Type) -> usize {
    match member {
        Type::Str => 0,
        Type::Path => 1,
        Type::Int => 2,
        Type::UInt => 3,
        Type::Float => 4,
        Type::Bool => 5,
        Type::Duration => 6,
        Type::Bytes => 7,
        _ => 8,
    }
}

/// A member the suggestion can spell without knowing which declarations are
/// in scope: a scalar, or a list or map of such. Records print as `Record`
/// and enums by a name that may need a qualifier, so neither is suggested,
/// and a dynamic element has no member at all.
fn spelled_member(member: &Type) -> Option<String> {
    match member {
        Type::Bool
        | Type::Int
        | Type::UInt
        | Type::Float
        | Type::Duration
        | Type::Str
        | Type::Bytes
        | Type::Path => Some(member.to_string()),
        Type::List(item) => Some(format!("List[{}]", spelled_member(item)?)),
        Type::Map(key, value) if **key == Type::Str => {
            Some(format!("Map[{}]", spelled_member(value)?))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "lint_list_any_union_tests.rs"]
mod tests;
