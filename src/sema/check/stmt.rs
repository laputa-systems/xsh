#![allow(clippy::single_call_fn)]

use super::TagVariantInfo;
use super::expr::expr_or_run_span_arena;
use super::{
    AnnotationFact, AnnotationFactKind, BinaryOp, Checker, Diagnostic, FixHint, FxHashSet, Label,
    Name, Span, Type, UnaryOp, command_stmt_asserts_success_arena, command_ty_auto_propagates,
    expr_ty_auto_propagates, normalize_hook_signal, signal_rejection_message,
};
use super::{Binding, TypeDefBody, tail_type_matches_expected};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaFunctionDef,
    ArenaProgram, ArenaRange, ArenaSignalHook, ArenaStmtKind, AssignTargetId, BindingTargetId,
    BlockId, ExprId, StmtId, TypeExprId,
};
use crate::syntax::node::AssignOp;
use rustc_hash::FxHashMap;

pub(super) use super::proof::{ConditionNarrowings, Narrowing};

fn annotation_type_is_nontrivial(ty: &Type) -> bool {
    matches!(
        ty,
        Type::List(_)
            | Type::Map(_, _)
            | Type::Result(_, _)
            | Type::Optional(_)
            | Type::Command
            | Type::Pure
            | Type::Proc
            | Type::Tag(_)
    )
}

/// Arena-native mirror of `block_always_returns`/`stmt_always_returns` — a
/// pure structural walk, same pattern as `block_has_exit_point_arena`.
#[allow(dead_code)]
pub(super) fn block_always_returns_arena(arena: &ArenaProgram, block_id: BlockId) -> bool {
    let block = arena.arena.block(block_id);
    arena
        .arena
        .stmt_ids(block.statements)
        .any(|id| stmt_always_returns_arena(arena, id))
}

#[allow(dead_code)]
pub(super) fn stmt_always_returns_arena(arena: &ArenaProgram, id: StmtId) -> bool {
    match arena.arena.stmt(id).kind {
        ArenaStmtKind::Sugar { expansion, .. } => stmt_always_returns_arena(arena, expansion),
        ArenaStmtKind::Return(_) => true,
        ArenaStmtKind::Expr(expr) => match arena.arena.expr(expr).kind {
            ArenaExprKind::ErrorContext { block, .. } => block_always_returns_arena(arena, block),
            _ => false,
        },

        ArenaStmtKind::If {
            branches,
            else_block: Some(else_block),
        } => {
            arena
                .arena
                .if_branches(branches)
                .iter()
                // A branch whose condition is the literal `false` never runs.
                .filter(|branch| {
                    !matches!(
                        arena.arena.expr(branch.condition).kind,
                        ArenaExprKind::Bool(false)
                    )
                })
                .all(|branch| block_always_returns_arena(arena, branch.block))
                && block_always_returns_arena(arena, else_block)
        }
        ArenaStmtKind::Match { arms, .. } => arena
            .arena
            .match_arms(arms)
            .iter()
            .all(|arm| block_always_returns_arena(arena, arm.block)),
        ArenaStmtKind::With {
            body, else_block, ..
        } => {
            block_always_returns_arena(arena, body) && block_always_returns_arena(arena, else_block)
        }
        _ => false,
    }
}

/// Whether a tail `if` can supply its block's value: some branch ends in a
/// statement that has one. An `if` whose branches all end in a control
/// transfer, a binding, or nothing completes as a statement wherever it
/// stands, so `if done { return x }` may end a block that has no value.
pub(super) fn if_stmt_may_produce_value(arena: &ArenaProgram, id: StmtId) -> bool {
    let ArenaStmtKind::If {
        branches,
        else_block,
    } = arena.arena.stmt(id).kind
    else {
        return false;
    };
    let ends_in_value = |block: BlockId| {
        arena
            .arena
            .stmt_ids(arena.arena.block(block).statements)
            .last()
            .is_some_and(|tail| {
                let tail = arena.arena.core_stmt_id(tail);
                match arena.arena.stmt(tail).kind {
                    ArenaStmtKind::Expr(_)
                    | ArenaStmtKind::Command(_)
                    | ArenaStmtKind::TailBareIdent(_)
                    | ArenaStmtKind::Match { .. } => true,
                    ArenaStmtKind::If { .. } => if_stmt_may_produce_value(arena, tail),
                    _ => false,
                }
            })
    };
    arena
        .arena
        .if_branches(branches)
        .iter()
        .any(|branch| ends_in_value(branch.block))
        || else_block.is_some_and(ends_in_value)
}

/// Returns true if the block contains any `break` or `return` statement that
/// is not inside a nested `while`/`for`/`loop`. A loop body with at least one
/// such exit point is not statically infinite.
/// Arena-native mirror of `block_has_exit_point`/`stmt_has_exit_point` — a
/// pure structural walk (no type-checking), so it's independent of which
/// `ArenaStmtKind` variants `check_stmt_arena` has native coverage for.
#[allow(dead_code)]
pub(super) fn block_has_exit_point_arena(arena: &ArenaProgram, block_id: BlockId) -> bool {
    let block = arena.arena.block(block_id);
    arena
        .arena
        .stmt_ids(block.statements)
        .any(|id| stmt_has_exit_point_arena(arena, id))
}

#[allow(dead_code)]
pub(super) fn stmt_has_exit_point_arena(arena: &ArenaProgram, id: StmtId) -> bool {
    match &arena.arena.stmt(id).kind {
        ArenaStmtKind::Break { .. } | ArenaStmtKind::Return(_) => true,
        ArenaStmtKind::While { .. } | ArenaStmtKind::For { .. } | ArenaStmtKind::Loop { .. } => {
            false
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            arena
                .arena
                .if_branches(*branches)
                .iter()
                .any(|b| block_has_exit_point_arena(arena, b.block))
                || else_block.is_some_and(|b| block_has_exit_point_arena(arena, b))
        }
        ArenaStmtKind::Match { arms, .. } => arena
            .arena
            .match_arms(*arms)
            .iter()
            .any(|a| block_has_exit_point_arena(arena, a.block)),
        ArenaStmtKind::With {
            body, else_block, ..
        } => {
            block_has_exit_point_arena(arena, *body)
                || block_has_exit_point_arena(arena, *else_block)
        }
        ArenaStmtKind::Guard { else_block, .. } => block_has_exit_point_arena(arena, *else_block),
        ArenaStmtKind::Expr(expr) => match arena.arena.expr(*expr).kind {
            ArenaExprKind::ErrorContext { block, .. } => block_has_exit_point_arena(arena, block),
            _ => false,
        },
        ArenaStmtKind::Sugar { expansion, .. } => stmt_has_exit_point_arena(arena, *expansion),
        _ => false,
    }
}

/// The checker facts that decide which values a set of patterns covers.
#[derive(Clone, Copy)]
pub(super) struct ExhaustivenessFacts<'a> {
    pub(super) type_defs: &'a FxHashMap<Name, TypeDefBody>,
    pub(super) tag_variants: &'a FxHashMap<Name, TagVariantInfo>,
    pub(super) pattern_types: &'a FxHashMap<crate::syntax::arena::PatternId, Type>,
    pub(super) inferred_variant_patterns:
        &'a std::collections::BTreeMap<Span, super::InferredVariantPattern>,
    pub(super) error_families: &'a FxHashMap<Name, super::ErrorFamilyInfo>,
}

impl ExhaustivenessFacts<'_> {
    /// The variant names of the enum `type_name`, or `None` when no
    /// declaration of it is visible here. An enum declared in this module is
    /// read from its definition; one imported under a namespace is read from
    /// its constructors, which an import brings in all together.
    pub(super) fn enum_variants(&self, type_name: Name) -> Option<Vec<Name>> {
        let declared = self.type_defs.get(&type_name).or_else(|| {
            self.type_defs.values().find(|body| {
                matches!(body, TypeDefBody::TagUnion(variants)
                    if variants.first().is_some_and(|variant| variant.type_name == type_name))
            })
        });
        if let Some(TypeDefBody::TagUnion(variants)) = declared {
            return Some(variants.iter().map(|variant| variant.name).collect());
        }
        let mut names: Vec<Name> = self
            .tag_variants
            .iter()
            .filter(|(_, info)| info.type_name == type_name)
            .map(|(spelling, _)| unqualified_name(*spelling))
            .collect();
        names.sort_by_key(|name| name.as_str().to_string());
        names.dedup();
        (!names.is_empty()).then_some(names)
    }
}

/// The last segment of a constructor or facet spelling: `Binary` for both
/// `Binary` and `kinds.Binary`.
fn unqualified_name(name: Name) -> Name {
    match name.as_str().rsplit_once('.') {
        Some((_, member)) => Name::intern(member),
        None => name,
    }
}

/// What the unguarded patterns of a match cover of an enum or error family
/// subject.
#[derive(Default)]
pub(super) struct VariantCoverage {
    /// A wildcard, a binding, or a test for the whole family.
    pub(super) catch_all: bool,
    /// Enum variants and `Ok`/`Err`, by unqualified name, matched whatever
    /// their payload is.
    pub(super) constructors: FxHashSet<Name>,
    /// Error variants matched whatever their payload is.
    pub(super) error_variants: FxHashSet<Name>,
    /// Facets tested for, by unqualified name.
    pub(super) facets: FxHashSet<Name>,
    booleans: [bool; 2],
}

impl VariantCoverage {
    /// Whether some pattern matches every value of `variant` of an error
    /// family.
    pub(super) fn covers_error_variant(
        &self,
        name: Name,
        variant: &super::ErrorVariantInfo,
    ) -> bool {
        self.error_variants.contains(&name)
            || variant
                .facets
                .iter()
                .any(|facet| self.facets.contains(&unqualified_name(*facet)))
    }
}

/// Whether `pattern` matches every value of its position's type.
fn pattern_is_irrefutable(
    arena: &ArenaProgram,
    pattern: crate::syntax::arena::PatternId,
    variants: &FxHashMap<Name, TagVariantInfo>,
) -> bool {
    use crate::syntax::arena::ArenaPatternKind;
    match arena.arena.pattern(pattern).kind {
        ArenaPatternKind::Group(child) | ArenaPatternKind::Alias { pattern: child, .. } => {
            pattern_is_irrefutable(arena, child, variants)
        }
        ArenaPatternKind::Alternation(items) => arena
            .arena
            .pattern_ids(items)
            .any(|child| pattern_is_irrefutable(arena, child, variants)),
        ArenaPatternKind::Wildcard => true,
        ArenaPatternKind::Binding(name) => !variants.contains_key(&name),
        ArenaPatternKind::Tuple(items) => arena
            .arena
            .pattern_ids(items)
            .all(|item| pattern_is_irrefutable(arena, item, variants)),
        // A record shape can reject dynamic payloads even when all fields bind.
        ArenaPatternKind::Record { .. } => false,
        _ => false,
    }
}

/// Adds what `pattern` covers to `coverage`. A target-typed `.Name` pattern
/// counts as the qualified pattern the checker resolved it to.
pub(super) fn collect_variant_coverage(
    arena: &ArenaProgram,
    pattern: crate::syntax::arena::PatternId,
    facts: &ExhaustivenessFacts<'_>,
    coverage: &mut VariantCoverage,
) {
    use crate::syntax::arena::ArenaPatternKind;
    let node = arena.arena.pattern(pattern);
    let kind = if node.kind.is_inferred_variant() {
        match facts
            .inferred_variant_patterns
            .get(&arena.arena.span(node.span))
        {
            Some(resolved) => resolved.qualify(&node.kind),
            // An unresolved `.Name` already has its own diagnostic.
            None => return,
        }
    } else {
        node.kind.clone()
    };
    match kind {
        ArenaPatternKind::Group(child) | ArenaPatternKind::Alias { pattern: child, .. } => {
            collect_variant_coverage(arena, child, facts, coverage);
        }
        ArenaPatternKind::Alternation(items) => {
            for item in arena.arena.pattern_ids(items) {
                collect_variant_coverage(arena, item, facts, coverage);
            }
        }
        ArenaPatternKind::Wildcard => coverage.catch_all = true,
        ArenaPatternKind::Binding(name) if !facts.tag_variants.contains_key(&name) => {
            coverage.catch_all = true;
        }
        ArenaPatternKind::Binding(name) => {
            coverage.constructors.insert(unqualified_name(name));
        }
        ArenaPatternKind::Constructor { name, arg }
            if arg.is_none_or(|arg| pattern_is_irrefutable(arena, arg, facts.tag_variants)) =>
        {
            coverage.constructors.insert(unqualified_name(name));
        }
        ArenaPatternKind::ErrorVariant {
            family,
            variant,
            fields,
        } => {
            // `namespace.Variant` of an imported enum parses as this shape.
            if fields.len == 0
                && facts
                    .tag_variants
                    .contains_key(&Name::intern(format!("{family}.{variant}")))
            {
                coverage.constructors.insert(variant);
            } else if arena
                .arena
                .pattern_fields(fields)
                .iter()
                .all(|field| pattern_is_irrefutable(arena, field.pattern, facts.tag_variants))
            {
                coverage.error_variants.insert(variant);
            }
        }
        ArenaPatternKind::Facet(name) => {
            coverage.facets.insert(unqualified_name(name));
        }
        ArenaPatternKind::TestName { .. } | ArenaPatternKind::Type { .. } => {
            match facts.pattern_types.get(&pattern) {
                Some(Type::ErrorVariant { variant, .. }) => {
                    coverage.error_variants.insert(*variant);
                }
                Some(Type::ErrorFacet(facet)) => {
                    coverage.facets.insert(unqualified_name(*facet));
                }
                Some(Type::ErrorFamily(_) | Type::Error) => coverage.catch_all = true,
                _ => {}
            }
        }
        ArenaPatternKind::Literal(expr) => {
            if let ArenaExprKind::Bool(value) = arena.arena.expr(expr).kind {
                coverage.booleans[usize::from(value)] = true;
            }
        }
        _ => {}
    }
}

/// Returns true if a match on `value_ty` with the given arms is exhaustive —
/// i.e. every possible value is matched. This is true when the match has a
/// catch-all (wildcard or non-tag-variant binding) or, for an enum or an
/// error family, when every variant is explicitly covered.
fn match_is_exhaustive_arena(
    arena: &ArenaProgram,
    value_ty: &Type,
    arms: &[crate::syntax::arena::ArenaMatchArm],
    facts: &ExhaustivenessFacts<'_>,
) -> bool {
    patterns_are_exhaustive_arena(
        arena,
        value_ty,
        arms.iter()
            .filter(|arm| arm.guard.is_none())
            .map(|arm| arm.pattern),
        facts,
    )
}

/// The types the type tests of a pattern check for, over every alternative.
/// `pattern_types` holds the checked type of each type-test pattern.
pub(super) fn collect_tested_types_arena(
    arena: &ArenaProgram,
    pattern: crate::syntax::arena::PatternId,
    pattern_types: &FxHashMap<crate::syntax::arena::PatternId, Type>,
    tested: &mut Vec<Type>,
) {
    use crate::syntax::arena::ArenaPatternKind;
    match arena.arena.pattern(pattern).kind {
        ArenaPatternKind::Group(child) | ArenaPatternKind::Alias { pattern: child, .. } => {
            collect_tested_types_arena(arena, child, pattern_types, tested)
        }
        ArenaPatternKind::Alternation(items) => {
            for item in arena.arena.pattern_ids(items) {
                collect_tested_types_arena(arena, item, pattern_types, tested);
            }
        }
        ArenaPatternKind::TestName { .. } | ArenaPatternKind::Type { .. } => {
            tested.extend(pattern_types.get(&pattern).cloned());
        }
        _ => {}
    }
}

pub(super) fn patterns_are_exhaustive_arena(
    arena: &ArenaProgram,
    value_ty: &Type,
    patterns: impl Iterator<Item = crate::syntax::arena::PatternId>,
    facts: &ExhaustivenessFacts<'_>,
) -> bool {
    use crate::syntax::arena::ArenaPatternKind;
    // Patterns match the value, which is a value of the base type.
    let value_ty = value_ty.unvalidated();
    let tag_variants = facts.tag_variants;
    let mut pending: Vec<_> = patterns.collect();
    let mut patterns = Vec::new();
    while let Some(pattern) = pending.pop() {
        match arena.arena.pattern(pattern).kind {
            ArenaPatternKind::Group(child) | ArenaPatternKind::Alias { pattern: child, .. } => {
                pending.push(child)
            }
            ArenaPatternKind::Alternation(items) => pending.extend(arena.arena.pattern_ids(items)),
            _ => patterns.push(pattern),
        }
    }
    let mut coverage = VariantCoverage::default();
    let mut empty_list = false;
    let mut nonempty_list = false;
    let mut tested = Vec::new();
    for pattern in patterns {
        if matches!(value_ty, Type::Union(_)) {
            collect_tested_types_arena(arena, pattern, facts.pattern_types, &mut tested);
        }
        // A type test covers only the values of the tested type, so it is a
        // catch-all for an error family subject and for nothing else.
        let type_test = matches!(
            arena.arena.pattern(pattern).kind,
            ArenaPatternKind::TestName { .. } | ArenaPatternKind::Type { .. }
        );
        if !type_test || matches!(value_ty, Type::ErrorFamily(_)) {
            collect_variant_coverage(arena, pattern, facts, &mut coverage);
        }
        if coverage.catch_all {
            return true;
        }
        if matches!(value_ty, Type::List(_))
            && let ArenaPatternKind::List { elements, rest } = arena.arena.pattern(pattern).kind
        {
            let count = arena.arena.pattern_ids(elements).count();
            if count == 0 {
                if rest.is_some() { return true; }
                empty_list = true;
            } else if count == 1 && rest.is_some() && arena.arena.pattern_ids(elements).all(|child| matches!(arena.arena.pattern(child).kind, ArenaPatternKind::Wildcard) || matches!(arena.arena.pattern(child).kind, ArenaPatternKind::Binding(name) if !tag_variants.contains_key(&name))) {
                nonempty_list = true;
            }
        }
    }
    match value_ty {
        Type::List(_) => empty_list && nonempty_list,
        Type::Bool => coverage.booleans.iter().all(|value| *value),
        // A union is covered when each member has a type test that accepts it.
        Type::Union(members) => members
            .iter()
            .all(|member| tested.iter().any(|tested| member.matches_expected(tested))),
        Type::Result(_, _) => {
            coverage.constructors.contains(&Name::intern("Ok"))
                && coverage.constructors.contains(&Name::intern("Err"))
        }
        Type::Tag(name) => facts.enum_variants(*name).is_some_and(|variants| {
            variants
                .iter()
                .all(|variant| coverage.constructors.contains(variant))
        }),
        // A family whose declaration is visible is closed: its variants are
        // all the values there are.
        Type::ErrorFamily(family) => facts.error_families.get(family).is_some_and(|info| {
            info.variants
                .iter()
                .all(|(name, variant)| coverage.covers_error_variant(*name, variant))
        }),
        _ => false,
    }
}

/// Arena-native mirror of `check_stmt` and the block/binding/assignment
/// machinery it depends on.
#[allow(dead_code)]
impl Checker {
    pub(super) fn define_binding_target_arena(
        &mut self,
        arena: &ArenaProgram,
        target: BindingTargetId,
        ty: &Type,
        mutable: bool,
        span: Span,
    ) {
        match &arena.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => {
                if name.as_str() == "_" {
                    return;
                }
                if self.current_scope().contains_key(name) || self.tag_variants.contains_key(name) {
                    self.error(
                        span,
                        "duplicate name in scope",
                        DiagnosticCode::CheckDuplicateName,
                    );
                }
                self.define(
                    *name,
                    if self.in_pure && mutable {
                        Binding::pure_local_var(ty.clone())
                    } else {
                        Binding::new(ty.clone(), mutable)
                    },
                    span,
                );
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                let record_fields = match ty {
                    Type::Record(fields) => Some(fields),
                    Type::Unknown => None,
                    _ => {
                        self.error(
                            span,
                            "record destructuring requires a record value",
                            DiagnosticCode::CheckDestructureType,
                        );
                        None
                    }
                };
                let mut names = FxHashSet::default();
                for field in arena.arena.destructure_fields(*fields) {
                    let field_span = arena.arena.span(field.span);
                    if !names.insert(field.name) {
                        self.error(
                            field_span,
                            "duplicate destructured field",
                            DiagnosticCode::CheckDestructureField,
                        );
                    }
                    let field_ty = record_fields
                        .and_then(|fields| fields.get(&field.name))
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    if let Some(record_fields) = record_fields
                        && !record_fields.is_empty()
                        && !record_fields.contains_key(&field.name)
                    {
                        self.error(
                            field_span,
                            "unknown destructured field",
                            DiagnosticCode::CheckDestructureField,
                        );
                    }
                    self.define_binding_target_arena(
                        arena,
                        field.target,
                        &field_ty,
                        mutable,
                        field_span,
                    );
                }
            }
        }
    }

    pub(super) fn check_compound_assignment_op(
        &mut self,
        op: AssignOp,
        left: &Type,
        right: &Type,
        op_span: Span,
        rhs_span: Span,
    ) -> Type {
        if left == &Type::Duration {
            let valid = matches!(
                (op, right),
                (AssignOp::Add | AssignOp::Sub, Type::Duration)
                    | (AssignOp::Mul | AssignOp::Div, Type::Int)
            );
            if !valid {
                self.error(
                    rhs_span,
                    "invalid Duration compound assignment dimensions",
                    DiagnosticCode::CheckOperatorType,
                );
            }
            return Type::Duration;
        }
        // Appending reads both operands as lists. The target keeps its type,
        // so a validated target must be one that survives concatenation.
        if op == AssignOp::Add
            && matches!(left.unvalidated(), Type::List(_) | Type::Str)
            && left
                .validated()
                .is_none_or(|validated| validated.validation().survives_concatenation())
        {
            self.expect_type(left.unvalidated(), right, rhs_span);
            return left.clone();
        }
        if matches!(left, Type::Float) && op != AssignOp::Rem {
            if !matches!(right, Type::Float | Type::Unknown) {
                self.error(
                    rhs_span,
                    "compound assignment requires Float operands",
                    DiagnosticCode::CheckOperatorType,
                );
            }
            return Type::Float;
        }
        if !matches!(left, Type::Int | Type::UInt | Type::Unknown) {
            // The target keeps its type, so this one report is the whole
            // mistake: neither the operand nor the result is reported again.
            let symbol = match op {
                AssignOp::Add => "+=",
                AssignOp::Sub => "-=",
                AssignOp::Mul => "*=",
                AssignOp::Div => "/=",
                _ => "%=",
            };
            let mut diagnostic = Diagnostic::error(format!("`{symbol}` is not defined for {left}"))
                .with_code(DiagnosticCode::CheckOperatorType)
                .with_label(Label::primary(
                    op_span,
                    "compound assignment requires Int or Float operands",
                ));
            if *left.unvalidated() == Type::Path {
                diagnostic = diagnostic.with_note(
                    "operators never join paths; build the path with an `fp\"...\"` literal",
                );
            }
            self.diagnostics.push(diagnostic);
            return left.clone();
        }
        if !matches!(right, Type::Int | Type::UInt | Type::Unknown) {
            self.error(
                rhs_span,
                "compound assignment requires Int operands",
                DiagnosticCode::CheckOperatorType,
            );
        }
        Type::Int
    }

    pub(super) fn apply_narrowings(&mut self, narrowings: &[Narrowing]) {
        for narrowing in narrowings {
            let Some(binding) = self.lookup(narrowing.name).cloned() else {
                continue;
            };
            if !binding.proof.accepts(narrowing) {
                continue;
            }
            let mut narrowed = binding;
            if narrowed.unrefined_ty.is_none() {
                narrowed.unrefined_ty = Some(narrowed.ty.clone());
            }
            if !super::proof::replace_projection(
                &mut narrowed.ty,
                &narrowing.path,
                narrowing.ty.clone(),
            ) {
                continue;
            }
            self.current_scope_mut().insert(narrowing.name, narrowed);
        }
    }

    pub(super) fn check_loop_control(&mut self, span: Span, is_break: bool) {
        if self.loop_depth > 0 {
            return;
        }
        if self.in_defer_block {
            self.error(
                span,
                "loop control cannot leave a deferred cleanup block",
                DiagnosticCode::CheckDeferControlFlow,
            );
            return;
        }
        let message = if !self.item_frames.iter().any(super::ItemFrame::is_stage) {
            if is_break {
                "`break` is valid only inside while or for loops"
            } else {
                "`continue` is valid only inside while or for loops"
            }
        } else if is_break {
            "`break` cannot target a structured stream stage"
        } else {
            "`continue` cannot target a structured stream stage"
        };
        self.error(span, message, DiagnosticCode::CheckLoopControl);
    }

    /// Assertion is explicit, so a Bool in statement position is rejected
    /// rather than asserted or silently discarded. Statement use is decided
    /// from the checked type and its consumer, never from a runtime value.
    pub(super) fn reject_bool_statement(
        &mut self,
        source: &str,
        ty: &Type,
        statement: Span,
    ) -> bool {
        if *ty != Type::Bool {
            return false;
        }
        let mut diagnostic = Diagnostic::error("Bool expression statement is not an assertion")
            .with_code(DiagnosticCode::CheckBoolStatement)
            .with_label(Label::primary(
                statement,
                "use `assert <expr>` to assert it, or `let _ = <expr>` to discard it",
            ));
        if let Some(fix) = bool_statement_assert_fix(source, statement) {
            diagnostic = diagnostic.with_fix_hint(fix);
        }
        self.diagnostics.push(diagnostic);
        true
    }

    /// A statement keeps no value: `Unit` is discarded, `Result[Unit]`
    /// propagates its failure, and Bool has its own diagnostic. Any other
    /// value would be lost silently, so it must be bound, returned, used as a
    /// tail, or discarded with `let _ = ...`. The one consumer at statement
    /// level is the script exit status, which takes the final top-level
    /// statement's `Int`.
    ///
    /// `let _ = ` keeps the evaluation exactly, so it is offered as a safe fix
    /// when the statement is valid as an initializer (`discard_fix`). It is
    /// withheld from a `Result`, whose failure should be handled rather than
    /// dropped, and from a discarded copy update such as `items.push(x)`,
    /// whose only effect is the value it returns. The edit needs no source
    /// text, so it stays exact for statements of imported modules, which are
    /// checked against the entry script's text.
    pub(super) fn reject_discarded_value(
        &mut self,
        ty: &Type,
        statement: Span,
        value: Span,
        discard_fix: bool,
        copy_update: Option<CopyUpdateMistake>,
    ) {
        if *ty == Type::Bool
            || ty.is_result_unit()
            || ty.matches_expected(&Type::Unit)
            || self.is_inert_expression_discard(value)
        {
            return;
        }
        if matches!(ty, Type::Int | Type::UInt) && self.exit_status_statement == Some(statement) {
            return;
        }
        let diagnostic = if let Some(mistake) = copy_update {
            Diagnostic::error(mistake.message.clone())
                .with_label(Label::primary(value, mistake.message))
                .with_note(mistake.repair)
        } else if ty.is_result() {
            Diagnostic::error("ignored Result value")
                .with_label(Label::primary(value, format!("this {ty} is dropped; handle it with `?` or `??`, or discard it with `let _ = ...`")))
        } else {
            let diagnostic =
                Diagnostic::error(format!("ignored `{ty}` value")).with_label(Label::primary(
                    value,
                    "bind it, return it, or discard it with `let _ = ...`",
                ));
            if discard_fix {
                diagnostic.with_fix_hint(FixHint::replacement(
                    Span::at(statement.source_id, statement.start()),
                    "discard with `let _ =`",
                    "let _ = ",
                ))
            } else {
                diagnostic
            }
        };
        self.diagnostics
            .push(diagnostic.with_code(DiagnosticCode::CheckIgnoredResult));
    }

    /// XSH has no truthiness, so a condition names the type it found. A
    /// fallible Bool or Status (`fs.exists(path)`) is the usual cause, and
    /// the hint offers `?`, which propagates the failure instead of guessing.
    /// It changes failure behavior, so `lint --fix` never applies it.
    pub(super) fn report_non_bool_condition(
        &mut self,
        ty: &Type,
        span: Span,
        message: &str,
        code: DiagnosticCode,
    ) {
        let mut diagnostic = Diagnostic::error(message)
            .with_code(code)
            .with_label(Label::primary(span, format!("found {ty}")));
        if let Some((Type::Bool | Type::Status, _)) = super::types::result_types(ty) {
            diagnostic = diagnostic.with_fix_hint(
                FixHint::replacement(
                    Span::at(span.source_id, span.end()),
                    "propagate the failure with `?`, or choose a fallback with `??`",
                    "?",
                )
                .dangerous(),
            );
        } else if ty.is_result() {
            diagnostic =
                diagnostic.with_note("unwrap the Result with `?` or `??`, then compare its value");
        } else if matches!(ty, Type::Union(_)) {
            diagnostic = diagnostic
                .with_note("narrow the union with `value is Member` before using it as a condition");
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn check_stmt_arena(&mut self, arena: &ArenaProgram, source: &str, id: StmtId) {
        let stmt = arena.arena.stmt(id);
        self.statement_positions
            .entry(stmt.span)
            .or_insert(super::StatementPosition::Statement);
        // Declarations bind module-level names; inside a body or block nothing
        // could resolve them and preparation has no form for them.
        if matches!(
            stmt.kind,
            ArenaStmtKind::Use(_)
                | ArenaStmtKind::Export(_)
                | ArenaStmtKind::TypeDef(_)
                | ArenaStmtKind::ErrorDef(_)
                | ArenaStmtKind::ProcDef(_)
                | ArenaStmtKind::PureDef(_)
                | ArenaStmtKind::StreamDef(_)
        ) && (self.block_depth > 0 || self.current_return.is_some())
        {
            self.error(
                stmt.span,
                "declarations are allowed only at the top level of a script or module",
                DiagnosticCode::CheckNestedDeclaration,
            );
        }
        match stmt.kind {
            ArenaStmtKind::Sugar { expansion, .. } => {
                self.check_stmt_arena(arena, source, expansion);
            }
            ArenaStmtKind::Use(use_id) => {
                let use_stmt = arena.arena.use_stmt(use_id);
                self.check_use_arena(
                    arena,
                    use_stmt.path,
                    use_stmt.alias,
                    use_stmt.resolved.as_deref(),
                    stmt.span,
                );
            }
            ArenaStmtKind::Export(inner_id) => {
                let inner = arena.arena.stmt(inner_id);
                if let ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Const { target, .. } =
                    inner.kind
                    && matches!(
                        arena.arena.binding_target(target).kind,
                        ArenaBindingTargetKind::Record { .. }
                    )
                {
                    self.error(
                        inner.span,
                        "destructured exports are not supported",
                        DiagnosticCode::CheckExportDestructure,
                    );
                }
                let previous_exported = self.current_exported;
                self.current_exported = true;
                self.check_stmt_arena(arena, source, inner_id);
                self.current_exported = previous_exported;
            }
            ArenaStmtKind::TypeDef(def_id) => {
                let def = arena.arena.type_def(def_id);
                self.check_type_def_arena(arena, source, def, stmt.span);
            }
            ArenaStmtKind::ErrorDef(def_id) => {
                self.check_error_def_arena(arena, source, def_id);
            }
            ArenaStmtKind::ProcDef(def_id) | ArenaStmtKind::CliMain(def_id) => {
                let def = arena.arena.function_def(def_id).clone();
                self.check_function_arena(arena, source, &def, false);
            }
            ArenaStmtKind::PureDef(def_id) => {
                let def = arena.arena.function_def(def_id).clone();
                self.check_function_arena(arena, source, &def, true);
            }
            ArenaStmtKind::StreamDef(def_id) => {
                let def = arena.arena.function_def(def_id).clone();
                self.check_stream_function_arena(arena, source, &def);
            }
            ArenaStmtKind::SignalHook(hook_id) => {
                let hook = arena.arena.signal_hook(hook_id).clone();
                self.check_signal_hook_arena(arena, source, &hook, stmt.span);
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer,
            } => {
                self.check_binding_arena(arena, source, target, ty, initializer, false, stmt.span);
            }
            ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                self.check_binding_arena(arena, source, target, ty, initializer, true, stmt.span);
            }
            ArenaStmtKind::Assign { target, op, value } => {
                self.check_assignment_arena(arena, source, target, op, value, stmt.span);
            }
            ArenaStmtKind::Return(value) => {
                self.check_return_arena(arena, source, value, stmt.span);
            }
            ArenaStmtKind::YieldDelegate(value) => {
                self.check_yield_delegation_arena(arena, source, value, stmt.span);
            }
            ArenaStmtKind::Exit(status) => {
                let actual = self.check_expr_arena(arena, source, status, Some(&Type::Int));
                self.expect_type(&Type::Int, &actual, arena.arena.expr(status).span);
            }
            ArenaStmtKind::Yield(value) => {
                self.check_yield_arena(arena, source, value, stmt.span);
            }
            ArenaStmtKind::Defer(value, _) => {
                self.check_defer_arena(arena, source, value, stmt.span);
            }
            ArenaStmtKind::Break { value } => {
                if self.in_signal_hook {
                    self.error(
                        stmt.span,
                        "`break` is not allowed in signal hooks",
                        DiagnosticCode::CheckSignalHook,
                    );
                }
                self.check_loop_control(stmt.span, true);
                if let Some(expr_id) = value {
                    self.check_expr_arena(arena, source, expr_id, None);
                }
            }
            ArenaStmtKind::Continue => {
                if self.in_signal_hook {
                    self.error(
                        stmt.span,
                        "`continue` is not allowed in signal hooks",
                        DiagnosticCode::CheckSignalHook,
                    );
                }
                self.check_loop_control(stmt.span, false);
            }
            ArenaStmtKind::Assert { condition, message } => {
                if self.retry_attempt_depth == 0 {
                    self.assertion_effect_spans.insert(stmt.span);
                }
                self.check_propagation(
                    &Type::Result(
                        Box::new(Type::Unit),
                        Box::new(Type::ErrorFamily(Name::intern("AssertionError"))),
                    ),
                    stmt.span,
                );
                let condition_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(condition),
                    Some(&Type::Bool),
                    None,
                );
                if condition_ty == Type::Any {
                    self.expect_type(&Type::Bool, &condition_ty, arena.arena.expr(condition).span);
                } else if condition_ty != Type::Bool
                    && !matches!(condition_ty, Type::Unknown | Type::Invalid)
                {
                    self.report_non_bool_condition(
                        &condition_ty,
                        arena.arena.expr(condition).span,
                        "assert condition requires Bool",
                        DiagnosticCode::CheckAssertCondition,
                    );
                }
                if let Some(message) = message {
                    let message_ty = self.check_expr_with_schema_arena(
                        arena,
                        source,
                        ArenaExprOrRun::Expr(message),
                        Some(&Type::Str),
                        None,
                    );
                    if message_ty != Type::Str
                        && !matches!(message_ty, Type::Unknown | Type::Invalid)
                    {
                        self.error(
                            arena.arena.expr(message).span,
                            "assert message requires Str",
                            DiagnosticCode::CheckAssertMessage,
                        );
                    }
                }
                let facts = self.infer_condition_narrowings_arena(arena, condition);
                self.apply_narrowings(&facts.when_true);
            }
            ArenaStmtKind::Expr(expr_id)
                if matches!(
                    arena.arena.expr(expr_id).kind,
                    ArenaExprKind::ErrorContext { .. }
                ) =>
            {
                let ArenaExprKind::ErrorContext { message, block } = arena.arena.expr(expr_id).kind
                else {
                    unreachable!()
                };
                let ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(message),
                    Some(&Type::Str),
                    None,
                );
                self.expect_type(&Type::Str, &ty, arena.arena.expr(message).span);
                self.check_block_arena(arena, source, block);
                self.expr_types
                    .insert(arena.arena.expr(expr_id).span, Type::Unit);
            }
            ArenaStmtKind::Expr(expr_id) => {
                self.statement_root = Some(arena.arena.expr(expr_id).span);
                self.statement_expression_spans
                    .insert(arena.arena.expr(expr_id).span);
                self.propagating_statements.insert(stmt.span);
                let ty = if let ArenaExprKind::ValueBlock(block) = arena.arena.expr(expr_id).kind {
                    self.check_block_arena(arena, source, block);
                    self.expr_types
                        .insert(arena.arena.expr(expr_id).span, Type::Unit);
                    Type::Unit
                } else {
                    self.check_expr_with_schema_arena(
                        arena,
                        source,
                        ArenaExprOrRun::Expr(expr_id),
                        None,
                        None,
                    )
                };
                self.record_inert_expression_discard(arena, ArenaExprOrRun::Expr(expr_id));
                if !self.reject_bool_statement(source, &ty, stmt.span) {
                    self.record_statement_error(&ty, stmt.span);
                }
                self.reject_discarded_value(
                    &ty,
                    stmt.span,
                    arena.arena.expr(expr_id).span,
                    true,
                    copy_update_mistake(arena, expr_id, &ty),
                );
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => self.check_if_arena(arena, source, branches, else_block),
            ArenaStmtKind::While { condition, block } => {
                self.check_while_arena(arena, source, condition, block);
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                self.check_for_arena(arena, source, target, iter, block, stmt.span);
            }
            ArenaStmtKind::Loop { block } => {
                self.loop_depth += 1;
                self.check_block_arena(arena, source, block);
                self.loop_depth -= 1;
                if !block_has_exit_point_arena(arena, block) {
                    self.error(
                        stmt.span,
                        "`loop` has no `break` — will run forever",
                        DiagnosticCode::CheckLoopNoBreak,
                    );
                }
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
            } => {
                self.check_with_arena(arena, source, bindings, body, else_block, stmt.span);
            }
            ArenaStmtKind::Guard {
                target,
                ty,
                initializer,
                else_block,
            } => {
                self.check_guard_arena(
                    arena,
                    source,
                    target,
                    ty,
                    initializer,
                    else_block,
                    stmt.span,
                );
            }
            ArenaStmtKind::Match { value, arms } => {
                self.check_match_arena(arena, source, value, arms);
            }
            ArenaStmtKind::Command(command_id) => {
                self.check_command_stmt_arena(arena, source, command_id);
            }
            ArenaStmtKind::TailBareIdent(name) => {
                let ty = self.check_tail_bare_ident_arena(arena, source, name, stmt.span);
                // `let _ = name` would bind a proc instead of calling it, so
                // a bare word gets no mechanical discard.
                self.reject_discarded_value(&ty, stmt.span, stmt.span, false, None);
                self.reject_bool_statement(source, &ty, stmt.span);
            }
        }
    }

    pub(super) fn bind_pattern_condition_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        condition: ExprId,
    ) {
        if let ArenaExprKind::PatternCondition { value, arms } = arena.arena.expr(condition).kind {
            let ty = self
                .expr_types
                .get(&arena.arena.expr(value).span)
                .cloned()
                .unwrap_or(Type::Unknown);
            // An optional binding's pattern sees the non-null value.
            let ty = match ty {
                Type::Optional(present)
                    if self
                        .optional_binding_spans
                        .contains(&arena.arena.expr(condition).span) =>
                {
                    *present
                }
                ty => ty,
            };
            self.check_pattern_arena(
                arena,
                source,
                arena.arena.match_expr_arms(arms)[0].pattern,
                &ty,
            );
        }
    }

    pub(super) fn check_condition_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        condition: ExprId,
        code: DiagnosticCode,
    ) -> ConditionNarrowings {
        self.control_condition = true;
        let condition_ty = self.check_expr_with_schema_arena(
            arena,
            source,
            ArenaExprOrRun::Expr(condition),
            Some(&Type::Bool),
            None,
        );
        self.control_condition = false;
        let condition_span = arena.arena.expr(condition).span;
        if condition_ty == Type::Any {
            self.expect_type(&Type::Bool, &condition_ty, condition_span);
            return ConditionNarrowings::default();
        }
        if matches!(condition_ty, Type::Bool | Type::Status | Type::Unknown) {
            return self.infer_condition_narrowings_arena(arena, condition);
        }
        self.report_non_bool_condition(
            &condition_ty,
            condition_span,
            "condition must be Bool or Status",
            code,
        );
        ConditionNarrowings::default()
    }

    pub(super) fn infer_condition_narrowings_arena(
        &self,
        arena: &ArenaProgram,
        condition: ExprId,
    ) -> ConditionNarrowings {
        let mut facts = self
            .condition_proofs
            .get(&condition)
            .map(|facts| facts.as_ref().clone())
            .unwrap_or_else(|| self.infer_condition_proof_arena(arena, condition));
        let valid = |fact: &Narrowing| {
            self.lookup(fact.name)
                .is_some_and(|binding| binding.proof.accepts(fact))
        };
        facts.when_true.retain(valid);
        facts.when_false.retain(valid);
        facts
    }

    pub(super) fn proof_subject_arena(
        &self,
        arena: &ArenaProgram,
        mut expr: ExprId,
    ) -> Option<(Name, Vec<Name>, Type)> {
        let mut path = Vec::new();
        loop {
            let subject = match arena.arena.expr(expr).kind {
                ArenaExprKind::Ident(name) => Some(name),
                ArenaExprKind::Item
                    if matches!(self.item_frames.last(), Some(super::ItemFrame::Implicit { .. })) =>
                {
                    Some(super::item_binding())
                }
                _ => None,
            };
            if let Some(name) = subject {
                path.reverse();
                let ty = super::proof::projected_type(&self.lookup(name)?.ty, &path)?.clone();
                return Some((name, path, ty));
            }
            match arena.arena.expr(expr).kind {
                ArenaExprKind::Field { base, name } if path.len() < 128 => {
                    path.push(name);
                    expr = base;
                }
                _ => return None,
            }
        }
    }

    pub(super) fn infer_condition_proof_arena(
        &self,
        arena: &ArenaProgram,
        condition: ExprId,
    ) -> ConditionNarrowings {
        match arena.arena.expr(condition).kind {
            ArenaExprKind::Ident(name) => self
                .lookup(name)
                .and_then(|binding| binding.boolean_proof.as_ref())
                .map(|proof| proof.as_ref().clone())
                .unwrap_or_default(),
            ArenaExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => {
                let inner = self.infer_condition_narrowings_arena(arena, expr);
                ConditionNarrowings {
                    when_true: inner.when_false,
                    when_false: inner.when_true,
                }
            }
            ArenaExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => self
                .infer_condition_narrowings_arena(arena, left)
                .and(self.infer_condition_narrowings_arena(arena, right)),
            ArenaExprKind::Binary {
                op: BinaryOp::Or,
                left,
                right,
            } => self
                .infer_condition_narrowings_arena(arena, left)
                .or(self.infer_condition_narrowings_arena(arena, right)),
            ArenaExprKind::Binary {
                op: BinaryOp::Eq | BinaryOp::Ne,
                left,
                right,
            } => self.infer_null_comparison_narrowings_arena(arena, condition, left, right),
            ArenaExprKind::Binary {
                op: BinaryOp::In | BinaryOp::NotIn,
                left,
                right,
            } => {
                let narrowing = self.infer_record_membership_narrowing_arena(arena, left, right);
                if matches!(
                    arena.arena.expr(condition).kind,
                    ArenaExprKind::Binary {
                        op: BinaryOp::NotIn,
                        ..
                    }
                ) {
                    ConditionNarrowings {
                        when_true: narrowing.when_false,
                        when_false: narrowing.when_true,
                    }
                } else {
                    narrowing
                }
            }
            ArenaExprKind::PatternTest { value, arms }
            | ArenaExprKind::PatternCondition { value, arms } => {
                let Some((name, path, subject_ty)) = self.proof_subject_arena(arena, value) else {
                    return ConditionNarrowings::default();
                };
                let Some(binding) = self.lookup(name) else {
                    return ConditionNarrowings::default();
                };
                let pattern = arena.arena.match_expr_arms(arms)[0].pattern;
                // An optional binding succeeds exactly when the subject is
                // not null, so its branch may also use the subject itself.
                if let Type::Optional(present) = &subject_ty
                    && self
                        .optional_binding_spans
                        .contains(&arena.arena.expr(condition).span)
                {
                    return ConditionNarrowings {
                        when_true: vec![binding.proof.fact(name, path, (**present).clone())],
                        when_false: Vec::new(),
                    };
                }
                // A type test splits a union's members: the ones it accepts
                // remain when it passes, the others when it fails.
                if let Type::Union(members) = &subject_ty {
                    let Some(tested) = self.pattern_test_member_types(arena, pattern) else {
                        return ConditionNarrowings::default();
                    };
                    let (accepted, rest): (Vec<_>, Vec<_>) = members.iter().cloned().partition(
                        |member| tested.iter().any(|tested| member.matches_expected(tested)),
                    );
                    let fact = |mut side: Vec<Type>| {
                        let ty = match side.len() {
                            0 => return Vec::new(),
                            1 => side.pop().expect("one member"),
                            _ => Type::Union(side),
                        };
                        vec![binding.proof.fact(name, path.clone(), ty)]
                    };
                    return ConditionNarrowings {
                        when_true: fact(accepted),
                        when_false: fact(rest),
                    };
                }
                let ty = self.pattern_test_narrowed_type(arena, pattern);
                // A facet filters a nominal error without changing its family or
                // variant. Keep that precision when no intersection type is available.
                let ty = ty.map(|ty| {
                    if matches!(ty, Type::ErrorFacet(_))
                        && matches!(
                            subject_ty,
                            Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError
                        )
                    {
                        subject_ty.clone()
                    } else {
                        ty
                    }
                });
                ConditionNarrowings {
                    when_true: ty
                        .into_iter()
                        .map(|ty| binding.proof.fact(name, path.clone(), ty))
                        .collect(),
                    when_false: Vec::new(),
                }
            }
            _ => ConditionNarrowings::default(),
        }
    }

    fn infer_null_comparison_narrowings_arena(
        &self,
        arena: &ArenaProgram,
        condition: ExprId,
        left: ExprId,
        right: ExprId,
    ) -> ConditionNarrowings {
        let Some((name, path, inner)) =
            self.null_compared_optional_binding_arena(arena, left, right)
        else {
            return ConditionNarrowings::default();
        };
        let narrowing = self.lookup(name).unwrap().proof.fact(name, path, inner);
        if matches!(
            arena.arena.expr(condition).kind,
            ArenaExprKind::Binary {
                op: BinaryOp::Ne,
                ..
            }
        ) {
            ConditionNarrowings {
                when_true: vec![narrowing],
                when_false: Vec::new(),
            }
        } else {
            ConditionNarrowings {
                when_true: Vec::new(),
                when_false: vec![narrowing],
            }
        }
    }

    fn null_compared_optional_binding_arena(
        &self,
        arena: &ArenaProgram,
        left: ExprId,
        right: ExprId,
    ) -> Option<(Name, Vec<Name>, Type)> {
        let subject = match (arena.arena.expr(left).kind, arena.arena.expr(right).kind) {
            (_, ArenaExprKind::Null) => left,
            (ArenaExprKind::Null, _) => right,
            _ => return None,
        };
        let (name, path, ty) = self.proof_subject_arena(arena, subject)?;
        let Type::Optional(inner) = ty else {
            return None;
        };
        Some((name, path, *inner))
    }

    fn infer_record_membership_narrowing_arena(
        &self,
        arena: &ArenaProgram,
        field_expr: ExprId,
        record_expr: ExprId,
    ) -> ConditionNarrowings {
        let Some((record_name, path, subject_ty)) = self.proof_subject_arena(arena, record_expr)
        else {
            return ConditionNarrowings::default();
        };
        let ArenaExprKind::Str(field_name_id) = arena.arena.expr(field_expr).kind else {
            return ConditionNarrowings::default();
        };
        let Some(binding) = self.lookup(record_name) else {
            return ConditionNarrowings::default();
        };
        let Type::Record(mut fields) = subject_ty else {
            return ConditionNarrowings::default();
        };
        let field_name = arena.arena.string_literal(field_name_id);
        fields.entry(Name::intern(field_name)).or_insert(Type::Any);
        ConditionNarrowings {
            when_true: vec![binding.proof.fact(record_name, path, Type::Record(fields))],
            when_false: Vec::new(),
        }
    }

    fn check_if_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        branches: ArenaRange,
        else_block: Option<BlockId>,
    ) {
        let initial_scopes = self.scopes.clone();
        let branch_list = arena.arena.if_branches(branches);
        let original = self
            .scopes
            .iter()
            .flat_map(|scope| scope.iter())
            .map(|(name, binding)| (*name, binding.clone()))
            .collect::<FxHashMap<_, _>>();
        let mut reaching = Vec::new();
        let mut previous_failure = Vec::new();
        for branch in branch_list {
            self.push_scope();
            self.apply_narrowings(&previous_failure);
            let facts = self.check_condition_arena(
                arena,
                source,
                branch.condition,
                DiagnosticCode::CheckIfCondition,
            );
            let failure_scopes = self.scopes.clone();
            self.apply_narrowings(&facts.when_true);
            self.bind_pattern_condition_arena(arena, source, branch.condition);
            self.check_block_arena(arena, source, branch.block);
            if !self
                .definitely_exiting_block_spans
                .contains(&arena.arena.span(arena.arena.block(branch.block).span))
                && let Some(bindings) = self.block_exit_bindings.get(&branch.block)
            {
                reaching.push(bindings.clone());
            }
            previous_failure.extend(facts.when_false);
            self.scopes = failure_scopes;
            self.pop_scope();
        }
        self.push_scope();
        self.apply_narrowings(&previous_failure);
        if let Some(block) = else_block {
            self.check_block_arena(arena, source, block);
            self.check_required_block_exit(arena, block);
            if !self
                .definitely_exiting_block_spans
                .contains(&arena.arena.span(arena.arena.block(block).span))
                && let Some(bindings) = self.block_exit_bindings.get(&block)
            {
                reaching.push(bindings.clone());
            }
        } else {
            reaching.push(
                self.scopes
                    .iter()
                    .flat_map(|scope| scope.iter())
                    .map(|(name, binding)| (*name, binding.clone()))
                    .collect(),
            );
        }
        self.pop_scope();
        self.scopes = initial_scopes;
        for (name, initial) in &original {
            for bindings in &reaching {
                if let Some(binding) = bindings
                    .get(name)
                    .filter(|binding| binding.proof.same_binding(&initial.proof))
                {
                    for path in binding.proof.mutation_paths_since(&initial.proof) {
                        for scope in &mut self.scopes {
                            for binding in scope.values_mut().filter(|binding| {
                                binding.proof.same_binding(&initial.proof) && binding.mutable
                            }) {
                                binding.proof.mutate(&path);
                                if let Some(original) = &binding.unrefined_ty {
                                    super::proof::restore_projection(
                                        &mut binding.ty,
                                        original,
                                        &path,
                                    );
                                }
                                if path.is_empty() {
                                    binding.unrefined_ty = None;
                                }
                            }
                        }
                    }
                }
            }
        }
        if reaching.is_empty() {
            return;
        }
        let mut facts = Vec::new();
        for (name, initial) in original {
            let Some(current) = self.lookup(name) else {
                continue;
            };
            if !initial.proof.same_binding(&current.proof) {
                continue;
            }
            let types = reaching
                .iter()
                .map(|bindings| {
                    bindings
                        .get(&name)
                        .filter(|binding| binding.proof.same_binding(&initial.proof))
                        .map(|binding| binding.ty.clone())
                        .unwrap_or_else(|| {
                            initial.unrefined_ty.as_ref().unwrap_or(&initial.ty).clone()
                        })
                })
                .collect::<Vec<_>>();
            let ty = super::proof::intersection_type(
                initial.unrefined_ty.as_ref().unwrap_or(&initial.ty),
                &types,
            );
            if ty != current.ty {
                facts.push(current.proof.fact(name, Vec::new(), ty));
            }
        }
        self.apply_narrowings(&facts);
    }

    /// Reports a block that an expansion requires to leave the enclosing
    /// continuation (the failure block of `guard ... else`) when some path
    /// through it falls through. Call it once the block has been checked.
    fn check_required_block_exit(&mut self, arena: &ArenaProgram, block: BlockId) {
        if arena.arena.block_must_exit(block) && !self.block_definitely_exits_arena(arena, block) {
            self.error(
                arena.arena.span(arena.arena.block(block).span),
                "guard failure branch must leave the enclosing continuation on every reachable path",
                DiagnosticCode::CheckGuardFallthrough,
            );
        }
    }

    fn check_while_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        condition: ExprId,
        block: BlockId,
    ) {
        let narrowings = self.check_condition_arena(
            arena,
            source,
            condition,
            DiagnosticCode::CheckWhileCondition,
        );
        self.push_scope();
        self.apply_narrowings(&narrowings.when_true);
        self.bind_pattern_condition_arena(arena, source, condition);
        self.loop_depth += 1;
        self.check_block_arena(arena, source, block);
        self.loop_depth -= 1;
        self.pop_scope();
    }

    fn check_for_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: BindingTargetId,
        iter: ExprId,
        block: BlockId,
        span: Span,
    ) {
        let iter_ty = self.check_expr_arena(arena, source, iter, None);
        if matches!(&iter_ty, Type::Result(ok, _) if matches!(ok.as_ref(), Type::Map(_, _) | Type::Str | Type::Bytes))
        {
            self.note_run_propagated(arena, iter);
            self.check_propagation(&iter_ty, arena.arena.expr(iter).span);
        }
        let item_ty = iter_ty
            .iteration_item_type()
            .unwrap_or_else(|| match iter_ty {
                Type::Any => Type::Any,
                Type::Unknown => Type::Unknown,
                ref union @ Type::Union(_) => {
                    self.reject_unnarrowed_union(union, "iteration", arena.arena.expr(iter).span);
                    Type::Unknown
                }
                _ => {
                    self.error(
                        arena.arena.expr(iter).span,
                        "`for` iterates over List, Stream, Map, Str, or Bytes values",
                        DiagnosticCode::CheckForIterator,
                    );
                    Type::Unknown
                }
            });
        self.push_scope();
        self.define_binding_target_arena(arena, target, &item_ty, false, span);
        self.loop_depth += 1;
        self.check_block_arena(arena, source, block);
        self.loop_depth -= 1;
        self.pop_scope();
    }

    fn check_match_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ExprId,
        arms: ArenaRange,
    ) {
        let value_ty = self.check_expr_arena(arena, source, value, None);
        let arm_list = arena.arena.match_arms(arms);
        for arm in arm_list {
            self.push_scope();
            self.check_pattern_arena(arena, source, arm.pattern, &value_ty);
            if let Some(value) = Self::single_error_handler_value_arena(arena, arm.block) {
                self.warn_flattened_error_handler_arena(arena, value, arm.pattern, &value_ty);
            }
            if let Some(guard) = arm.guard {
                let guard_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(guard),
                    Some(&Type::Bool),
                    None,
                );
                let guard_span = arena.arena.expr(guard).span;
                self.expect_type(&Type::Bool, &guard_ty, guard_span);
            }
            self.check_block_arena(arena, source, arm.block);
            self.pop_scope();
        }
        let value_span = arena.arena.expr(value).span;
        self.check_list_match_coverage_arena(
            arena,
            &value_ty,
            arm_list
                .iter()
                .map(|arm| (arm.pattern, arena.arena.span(arm.span), arm.guard.is_some())),
            value_span,
        );
        self.check_tag_exhaustiveness_arena(
            arena,
            &value_ty,
            arm_list
                .iter()
                .filter(|arm| arm.guard.is_none())
                .map(|arm| (arm.pattern, arena.arena.span(arm.span)))
                .collect(),
            value_span,
        );
    }

    fn check_with_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        bindings: ArenaRange,
        body: BlockId,
        else_block: BlockId,
        _span: Span,
    ) {
        self.push_scope();
        let mut error_ty = None;
        for binding in arena.arena.with_bindings(bindings) {
            let previous_errors = self.with_initializer_errors.replace(Vec::new());
            let ty = self.check_expr_arena(arena, source, binding.initializer, None);
            let mut errors = self.with_initializer_errors.take().unwrap_or_default();
            self.with_initializer_errors = previous_errors;
            if let Type::Result(_, error) = &ty {
                errors.push((**error).clone());
            }
            for error in errors {
                error_ty = Some(match error_ty {
                    None => error,
                    Some(previous) if previous == error => previous,
                    Some(_) => Type::Error,
                });
            }
            let value_ty = match ty {
                Type::Result(ok, _) => *ok,
                other => other,
            };
            let binding_span = arena.arena.span(binding.span);
            if self.current_scope().contains_key(&binding.name) {
                self.error(
                    binding_span,
                    "duplicate name in scope",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            if binding.name.as_str() != "_" {
                self.define(binding.name, Binding::new(value_ty, false), binding_span);
            }
        }
        self.check_block_arena(arena, source, body);
        self.pop_scope();
        let error_ty = error_ty.unwrap_or(Type::Error);
        self.handler_input_types.insert(
            arena.arena.span(arena.arena.block(else_block).span),
            error_ty.clone(),
        );
        self.check_error_handler_block_arena(arena, source, else_block, &error_ty);
    }

    fn check_guard_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
        else_block: BlockId,
        span: Span,
    ) {
        let expected = ty.map(|id| {
            Type::Result(
                Box::new(self.type_from_arena(arena, id)),
                Box::new(Type::Error),
            )
        });
        let schema = ty
            .and_then(|id| {
                self.record_constructors
                    .annotation_expectation(&arena.arena, id, self.current_namespace)
                    .ok()
            })
            .map(|schema| crate::sema::constants::SchemaExpectation {
                instances: Vec::new(),
                children: std::collections::BTreeMap::from([(
                    crate::sema::constants::SchemaComponent::Success,
                    schema,
                )]),
            });
        let init_ty = self.check_expr_with_schema_arena(
            arena,
            source,
            initializer,
            expected.as_ref(),
            schema,
        );
        // The outermost type selects the form, so `Result[T?]` stays a Result
        // binding and `Result[T]?` binds the Result itself. The fact is
        // rewritten on every check of this statement, because inference may
        // check a body more than once with a sharper subject type.
        self.optional_binding_spans.remove(&span);
        if let Type::Optional(present) = init_ty {
            self.optional_binding_spans.insert(span);
            self.check_optional_guard_arena(
                arena,
                source,
                target,
                ty,
                initializer,
                else_block,
                *present,
                span,
            );
            return;
        }
        let (ok_ty, error_ty) = match init_ty {
            Type::Result(ok, error) => (*ok, *error),
            Type::Unknown => (Type::Unknown, Type::Unknown),
            other => {
                self.error(
                    span,
                    "`guard let` binding must produce a Result or an optional value",
                    DiagnosticCode::CheckGuardBinding,
                );
                (other, Type::Error)
            }
        };
        if record_target_requires_schema_check(arena, target, &ok_ty) {
            self.error(
                span,
                "record destructuring of Any requires an explicit schema check",
                DiagnosticCode::CheckDestructureType,
            );
        }
        let bind_ty = if let Some(ty_id) = ty {
            let ann = self.type_from_arena(arena, ty_id);
            self.expect_type(&ann, &ok_ty, span);
            ann
        } else {
            ok_ty
        };
        self.check_error_handler_block_arena(arena, source, else_block, &error_ty);
        // A failed Result binds nothing, so the statements after the guard
        // cannot run: the block leaves, exactly as an optional guard's does.
        let else_span = arena.arena.span(arena.arena.block(else_block).span);
        if !self.definitely_exiting_block_spans.contains(&else_span) {
            self.error(
                else_span,
                "guard failure branch must leave the enclosing continuation on every reachable path",
                DiagnosticCode::CheckGuardFallthrough,
            );
        }
        self.define_binding_target_arena(arena, target, &bind_ty, false, span);
    }

    /// `guard let target = subject else { ... }` over an optional subject:
    /// `null` runs the block, any other value is bound as `present`.
    ///
    /// A null value carries no error, so the block is an ordinary guard
    /// failure block: it takes no parameter and must leave the continuation.
    /// A subject that is a stable binding or field path is narrowed for the
    /// following statements exactly as `guard subject != null` narrows it, so
    /// code that kept using the subject after a null test still checks.
    #[allow(clippy::too_many_arguments)]
    fn check_optional_guard_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
        else_block: BlockId,
        present: Type,
        span: Span,
    ) {
        if record_target_requires_schema_check(arena, target, &present) {
            self.error(
                span,
                "record destructuring of Any requires an explicit schema check",
                DiagnosticCode::CheckDestructureType,
            );
        }
        let bind_ty = if let Some(ty_id) = ty {
            let ann = self.type_from_arena(arena, ty_id);
            self.expect_type(&ann, &present, span);
            ann
        } else {
            present.clone()
        };
        let else_span = arena.arena.span(arena.arena.block(else_block).span);
        let success_scopes = self.scopes.clone();
        if let Some(param) = arena
            .arena
            .block_params(arena.arena.block(else_block).params)
            .first()
        {
            self.error(
                arena.arena.span(param.span),
                "an optional `guard let` has no error to bind: a null subject carries none, so its `else` block takes no parameter",
                DiagnosticCode::CheckBlockParams,
            );
            // Check the body with the name defined so its uses do not add
            // unknown-name errors to the one above.
            self.check_error_handler_block_arena(arena, source, else_block, &Type::Unknown);
        } else {
            self.push_scope();
            self.check_block_arena(arena, source, else_block);
            self.pop_scope();
            if !self.definitely_exiting_block_spans.contains(&else_span) {
                self.error(
                    else_span,
                    "guard failure branch must leave the enclosing continuation on every reachable path",
                    DiagnosticCode::CheckGuardFallthrough,
                );
            }
        }
        self.scopes = success_scopes;
        if let ArenaExprOrRun::Expr(subject) = initializer
            && let Some((name, path, Type::Optional(_))) = self.proof_subject_arena(arena, subject)
            && let Some(binding) = self.lookup(name)
        {
            let fact = binding.proof.fact(name, path, present);
            self.apply_narrowings(&[fact]);
        }
        self.define_binding_target_arena(arena, target, &bind_ty, false, span);
    }

    fn check_error_handler_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block: BlockId,
        error_ty: &Type,
    ) {
        let params = arena.arena.block_params(arena.arena.block(block).params);
        self.push_scope();
        if params.len() > 1 {
            self.error(
                arena.arena.span(params[1].span),
                "an error handler accepts at most one parameter",
                DiagnosticCode::CheckHandlerBlockParams,
            );
        }
        for param in params {
            if param.name.as_str() == "_" {
                continue;
            }
            let span = arena.arena.span(param.span);
            if self.current_scope().contains_key(&param.name) {
                self.error(
                    span,
                    "duplicate name in scope",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            self.define(param.name, Binding::new(error_ty.clone(), false), span);
        }
        self.check_statement_block_contents_arena(arena, source, block);
        self.pop_scope();
    }

    pub(super) fn check_value_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
        expected: &Type,
    ) {
        let block = arena.arena.block(block_id);
        if let Some(param) = arena.arena.block_params(block.params).first() {
            let param_span = arena.arena.span(param.span);
            self.error(
                param_span,
                "this block does not receive parameters",
                DiagnosticCode::CheckBlockParams,
            );
        }
        self.check_value_block_contents_arena(arena, source, block_id, expected);
    }

    fn check_value_block_contents_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
        expected: &Type,
    ) {
        let enclosing_bound = self.enter_block_effect_bound(arena, block_id);
        self.check_value_block_statements_arena(arena, source, block_id, expected);
        self.leave_block_effect_bound(enclosing_bound);
    }

    fn check_value_block_statements_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
        expected: &Type,
    ) {
        let block = arena.arena.block(block_id);
        self.push_scope();
        self.block_depth += 1;
        let stmt_ids: Vec<StmtId> = arena
            .arena
            .stmt_ids(block.statements)
            .map(|id| arena.arena.core_stmt_id(id))
            .collect();
        let block_span = arena.arena.span(block.span);
        if let Some((&tail, non_tail)) = stmt_ids.split_last() {
            let tail_producing = match arena.arena.stmt(tail).kind {
                ArenaStmtKind::Expr(_)
                | ArenaStmtKind::Command(_)
                | ArenaStmtKind::TailBareIdent(_)
                | ArenaStmtKind::Match { .. }
                | ArenaStmtKind::Return(_)
                | ArenaStmtKind::Exit(_)
                | ArenaStmtKind::Break { .. }
                | ArenaStmtKind::Continue => true,
                ArenaStmtKind::If { .. } => if_stmt_may_produce_value(arena, tail),
                _ => false,
            };
            let checked_stmts: &[StmtId] = if tail_producing { non_tail } else { &stmt_ids };
            for &stmt_id in checked_stmts {
                self.check_non_tail_stmt_arena(arena, source, stmt_id);
            }
            if tail_producing {
                let actual = self.check_tail_stmt_arena(arena, source, tail, Some(expected));
                if !tail_type_matches_expected(expected, &actual) {
                    let tail_span = arena.arena.stmt(tail).span;
                    self.expect_type(expected, &actual, tail_span);
                }
            } else if expected != &Type::Unit
                && !expected.is_result_unit()
                && !block_always_returns_arena(arena, block_id)
            {
                self.error(
                    block_span,
                    "function can fall through without returning its declared type",
                    DiagnosticCode::CheckMissingReturn,
                );
            }
        } else if expected != &Type::Unit && !expected.is_result_unit() {
            self.error(
                block_span,
                "function can fall through without returning its declared type",
                DiagnosticCode::CheckMissingReturn,
            );
        }
        self.block_depth -= 1;
        self.pop_scope();
    }

    // Deferred bodies read mutable captures after the declaration's current proofs can expire.
    pub(super) fn push_deferred_capture_scope(&mut self) {
        let visible = self
            .scopes
            .iter()
            .flat_map(|scope| scope.iter())
            .map(|(name, binding)| (*name, binding.clone()))
            .collect::<FxHashMap<_, _>>();
        self.push_scope();
        for (name, mut binding) in visible {
            if binding.mutable {
                binding.proof.mutate(&[]);
                if let Some(original) = binding.unrefined_ty.take() {
                    binding.ty = original;
                }
                binding.pure_local_mutation = false;
                self.current_scope_mut().insert(name, binding);
            }
        }
    }

    pub(super) fn check_function_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        def: &ArenaFunctionDef,
        pure: bool,
    ) {
        let body_span = arena.arena.span(arena.arena.block(def.body).span);
        if def.test_declaration {
            if self.current_exported {
                self.error(
                    body_span,
                    "test declarations cannot be exported",
                    DiagnosticCode::CheckTestExport,
                );
            }
            if self.block_depth != 0 || self.current_return.is_some() {
                self.error(
                    body_span,
                    "test declarations must be top-level",
                    DiagnosticCode::CheckTestNested,
                );
            }
        }
        if pure
            && def.return_ty_defaulted
            && self.inferred_returns.is_none()
            && self.function_return_types.contains_key(&body_span)
        {
            return;
        }
        let saved_capture_scopes = self.scopes.clone();
        self.collect_function_local_constraints(arena, source, def, pure);
        let previous_errors = self.with_initializer_errors.take();
        let previous_defer = std::mem::replace(&mut self.in_defer_block, false);
        let previous_boundary_depth = std::mem::replace(&mut self.retry_attempt_depth, 0);
        let previous_boundary_errors = std::mem::take(&mut self.error_boundary_errors);
        let previous_context_scopes = std::mem::take(&mut self.context_scope_depths);
        let previous_return = self.current_return.clone();
        let previous_return_schema = self.return_schema.clone();
        let previous_expected_schema = self.expected_schema.clone();
        let previous_pure = self.in_pure;
        let previous_effects = self.current_effects.clone();
        let previous_effect_owner = self.effect_owner;
        self.effect_owner = (!pure).then(|| self.effect_declaration_id(arena, def.body));
        // A proc return probe infers only the top-level declaration it targets.
        let inferring = def.return_ty_defaulted
            && self.inferred_returns.is_some()
            && (pure || previous_return.is_none());
        let outer_inference = if inferring {
            None
        } else {
            self.inferred_returns.take()
        };
        let inferred_proc_return = (!pure && def.return_ty_defaulted && !inferring)
            .then(|| self.function_return_types.get(&body_span).cloned())
            .flatten();
        let return_ty = if inferring {
            Type::Unknown
        } else {
            inferred_proc_return
                .clone()
                .unwrap_or_else(|| self.type_from_arena(arena, def.return_ty))
        };
        if !inferring {
            self.function_return_types
                .insert(body_span, return_ty.clone());
        }
        self.return_schema = (!inferring && inferred_proc_return.is_none())
            .then(|| {
                self.record_constructors
                    .annotation_expectation(&arena.arena, def.return_ty, self.current_namespace)
                    .ok()
            })
            .flatten();
        self.expected_schema = self.return_schema.clone();
        self.current_return = Some(return_ty.clone());
        self.in_pure = pure;
        self.current_effects = if pure {
            None
        } else {
            self.effective_function_effects(arena, def)
        };
        self.push_deferred_capture_scope();
        let mut saw_default = false;
        let mut param_types = Vec::new();
        let mut names = FxHashSet::default();
        let params = arena.arena.params(def.params);
        for (index, param) in params.iter().enumerate() {
            let param_span = arena.arena.span(param.span);
            if !names.insert(param.name) {
                self.error(
                    param_span,
                    "duplicate name in scope",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            if param.rest && index + 1 != params.len() {
                self.error(
                    param_span,
                    "rest parameters must be last",
                    DiagnosticCode::CheckRestPosition,
                );
            }
            let param_ty = self.infer_checked_parameter(arena, source, param);
            if param.rest && !matches!(param_ty, Type::List(_)) {
                self.error(
                    arena.arena.type_expr_span(param.ty),
                    "rest parameters require a List type",
                    DiagnosticCode::CheckRestType,
                );
            }
            if param.default.is_some() {
                saw_default = true;
            } else if saw_default && !param.rest {
                self.error(
                    param_span,
                    "required parameters cannot follow defaulted parameters",
                    DiagnosticCode::CheckDefaultParam,
                );
            }
            if let Some(default) = param.default {
                let actual = self.check_expr_arena(arena, source, default, Some(&param_ty));
                let default_span = arena.arena.expr(default).span;
                self.expect_type(&param_ty, &actual, default_span);
                if param.ty_defaulted && param_ty.annotation_source().is_some() {
                    self.annotation_facts.push(AnnotationFact {
                        kind: AnnotationFactKind::DefaultedParam {
                            span: param_span,
                            default: default_span,
                        },
                        ty: param_ty.clone(),
                    });
                }
            }
            let schema = (!param.ty_defaulted)
                .then(|| {
                    self.record_constructors
                        .annotation_expectation(&arena.arena, param.ty, self.current_namespace)
                        .ok()
                })
                .flatten();
            param_types.push((param.name, param_span, param_ty, schema));
        }
        self.publish_parameter_types(arena, def);
        for (name, span, ty, schema) in param_types {
            let mut binding = Binding::new(ty, false);
            binding.schema_expectation = schema;
            self.define(name, binding, span);
        }
        let body_tail = arena
            .arena
            .stmt_ids(arena.arena.block(def.body).statements)
            .last()
            .map(|tail| arena.arena.core_stmt_id(tail));
        if inferring {
            let enclosing_tail = std::mem::replace(&mut self.result_unit_function_tail, body_tail);
            let tail = self.check_tail_block_arena(arena, source, def.body, None);
            self.result_unit_function_tail = enclosing_tail;
            if tail != Type::Unknown {
                self.inferred_returns
                    .as_mut()
                    .unwrap()
                    .push((tail, body_span));
            }
        } else {
            let enclosing_tail = std::mem::replace(
                &mut self.result_unit_function_tail,
                body_tail.filter(|_| return_ty.is_result_unit()),
            );
            if def.test_declaration {
                if !cfg!(feature = "native-tests") {
                    self.error(body_span, "test declarations require native-test support; use an xsht build with the native-tests feature", DiagnosticCode::CheckTestFeatureDisabled);
                }
                self.check_value_block_contents_arena(arena, source, def.body, &return_ty);
            } else {
                self.check_value_block_arena(arena, source, def.body, &return_ty);
            }
            self.result_unit_function_tail = enclosing_tail;
        }
        if !pure
            && self.current_exported
            && def.return_ty_defaulted
            && return_ty == Type::Result(Box::new(Type::Unit), Box::new(Type::Error))
        {
            let body_span = arena.arena.span(arena.arena.block(def.body).span);
            self.annotation_facts.push(AnnotationFact {
                kind: AnnotationFactKind::ExportedProcReturn { body: body_span },
                ty: return_ty.clone(),
            });
        }
        self.pop_scope();
        if !inferring {
            self.inferred_returns = outer_inference;
        }
        self.scopes = saved_capture_scopes;
        self.current_return = previous_return;
        self.return_schema = previous_return_schema;
        self.expected_schema = previous_expected_schema;
        self.in_pure = previous_pure;
        self.current_effects = previous_effects;
        self.effect_owner = previous_effect_owner;
        self.in_defer_block = previous_defer;
        self.with_initializer_errors = previous_errors;
        self.retry_attempt_depth = previous_boundary_depth;
        self.error_boundary_errors = previous_boundary_errors;
        self.context_scope_depths = previous_context_scopes;
    }

    pub(super) fn check_stream_function_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        def: &ArenaFunctionDef,
    ) {
        let saved_capture_scopes = self.scopes.clone();
        self.collect_stream_local_constraints(arena, source, def);
        let previous_errors = self.with_initializer_errors.take();
        let previous_defer = std::mem::replace(&mut self.in_defer_block, false);
        let previous_boundary_depth = std::mem::replace(&mut self.retry_attempt_depth, 0);
        let previous_boundary_errors = std::mem::take(&mut self.error_boundary_errors);
        let previous_context_scopes = std::mem::take(&mut self.context_scope_depths);
        let previous_return = self.current_return.clone();
        let previous_yield = self.current_yield.clone();
        let previous_pure = self.in_pure;
        let previous_effects = self.current_effects.clone();
        let previous_effect_owner = self.effect_owner;
        self.effect_owner = Some(self.effect_declaration_id(arena, def.body));
        let return_ty = self.type_from_arena(arena, def.return_ty);
        let item_ty = match return_ty {
            Type::Stream(item) => *item,
            Type::Unknown | Type::Invalid => Type::Unknown,
            _ => {
                self.error(
                    arena.arena.type_expr_span(def.return_ty),
                    "stream producer must return Stream[T]",
                    DiagnosticCode::CheckStreamReturn,
                );
                Type::Unknown
            }
        };
        self.current_return = Some(Type::Unit);
        self.current_yield = Some(item_ty);
        self.in_pure = false;
        self.current_effects = def
            .effects
            .map(|effects| arena.arena.effects(effects).collect());
        self.push_deferred_capture_scope();
        let mut saw_default = false;
        let mut param_types = Vec::new();
        let mut names = FxHashSet::default();
        let params = arena.arena.params(def.params);
        for (index, param) in params.iter().enumerate() {
            let param_span = arena.arena.span(param.span);
            if !names.insert(param.name) {
                self.error(
                    param_span,
                    "duplicate name in scope",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            if param.rest && index + 1 != params.len() {
                self.error(
                    param_span,
                    "rest parameters must be last",
                    DiagnosticCode::CheckRestPosition,
                );
            }
            let param_ty = self.infer_checked_parameter(arena, source, param);
            if param.rest && !matches!(param_ty, Type::List(_)) {
                self.error(
                    arena.arena.type_expr_span(param.ty),
                    "rest parameters require a List type",
                    DiagnosticCode::CheckRestType,
                );
            }
            if param.default.is_some() {
                saw_default = true;
            } else if saw_default && !param.rest {
                self.error(
                    param_span,
                    "required parameters cannot follow defaulted parameters",
                    DiagnosticCode::CheckDefaultParam,
                );
            }
            if let Some(default) = param.default {
                let actual = self.check_expr_arena(arena, source, default, Some(&param_ty));
                let default_span = arena.arena.expr(default).span;
                self.expect_type(&param_ty, &actual, default_span);
            }
            let schema = (!param.ty_defaulted)
                .then(|| {
                    self.record_constructors
                        .annotation_expectation(&arena.arena, param.ty, self.current_namespace)
                        .ok()
                })
                .flatten();
            param_types.push((param.name, param_span, param_ty, schema));
        }
        self.publish_parameter_types(arena, def);
        for (name, span, ty, schema) in param_types {
            let mut binding = Binding::new(ty, false);
            binding.schema_expectation = schema;
            self.define(name, binding, span);
        }
        self.check_value_block_arena(arena, source, def.body, &Type::Unit);
        self.pop_scope();
        self.scopes = saved_capture_scopes;
        self.current_return = previous_return;
        self.current_yield = previous_yield;
        self.in_pure = previous_pure;
        self.current_effects = previous_effects;
        self.effect_owner = previous_effect_owner;
        self.in_defer_block = previous_defer;
        self.with_initializer_errors = previous_errors;
        self.retry_attempt_depth = previous_boundary_depth;
        self.error_boundary_errors = previous_boundary_errors;
        self.context_scope_depths = previous_context_scopes;
    }

    pub(super) fn check_signal_hook_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        hook: &ArenaSignalHook,
        span: Span,
    ) {
        if self.options.interactive_commands.is_some() {
            self.error(
                span,
                "signal hooks are not supported in interactive input",
                DiagnosticCode::CheckSignalHook,
            );
        }
        if self.current_exported {
            self.error(
                span,
                "signal hooks are not exported",
                DiagnosticCode::CheckSignalHook,
            );
        }
        if self.module_depth > 0 {
            self.error(
                span,
                "signal hooks are entry-script-only in v1",
                DiagnosticCode::CheckSignalHookModule,
            );
        } else if self.block_depth > 0 || self.current_return.is_some() {
            self.error(
                span,
                "signal hooks are allowed only at the entry script top level",
                DiagnosticCode::CheckSignalHook,
            );
        }

        match normalize_hook_signal(&hook.signal.as_str(), span) {
            Ok(info) => {
                if self.module_depth == 0
                    && let Some(previous) = self
                        .root_signal_hooks
                        .insert(Name::intern(&info.name), span)
                {
                    self.diagnostics.push(
                        crate::diagnostic::Diagnostic::error("duplicate signal hook")
                            .with_code(DiagnosticCode::CheckDuplicateSignalHook)
                            .with_label(crate::diagnostic::Label::primary(
                                span,
                                format!("duplicate hook for `{}`", info.name),
                            ))
                            .with_label(crate::diagnostic::Label::secondary(
                                previous,
                                "first hook declared here",
                            )),
                    );
                }
            }
            Err(rejection) => self.error(
                span,
                &signal_rejection_message(&hook.signal.as_str(), rejection),
                DiagnosticCode::CheckSignalHook,
            ),
        }

        if hook.options.pre_cancel.as_deref().is_some_and(|duration| {
            crate::runtime::value::DurationValue::from_literal(duration).is_none()
        }) {
            self.error(
                span,
                "`--pre-cancel` expects a duration literal",
                DiagnosticCode::CheckSignalHook,
            );
        }
        let body = arena.arena.block(hook.body);
        if let Some(param) = arena.arena.block_params(body.params).first() {
            let param_span = arena.arena.span(param.span);
            self.error(
                param_span,
                "signal hook blocks do not accept parameters",
                DiagnosticCode::CheckSignalHook,
            );
        }

        let saved_capture_scopes = self.scopes.clone();
        self.push_deferred_capture_scope();
        let previous_return = self.current_return.clone();
        let previous_pure = self.in_pure;
        let previous_effects = self.current_effects.clone();
        let previous_effect_owner = self.effect_owner.take();
        let previous_in_signal_hook = self.in_signal_hook;
        self.current_return = Some(Type::Result(Box::new(Type::Unit), Box::new(Type::Error)));
        self.in_pure = false;
        self.current_effects = Some(arena.arena.effects(hook.effects).collect());
        self.in_signal_hook = true;
        let ty = self.check_tail_block_arena(arena, source, hook.body, None);
        self.pop_scope();
        self.scopes = saved_capture_scopes;
        self.current_return = previous_return;
        self.in_pure = previous_pure;
        self.current_effects = previous_effects;
        self.effect_owner = previous_effect_owner;
        self.in_signal_hook = previous_in_signal_hook;

        match ty {
            Type::Unit | Type::Status | Type::Unknown | Type::Invalid => {}
            Type::Result(ok, _) if *ok == Type::Unit => {}
            _ => {
                let body_span = arena.arena.span(body.span);
                self.error(
                    body_span,
                    "signal hook body must produce Unit, Status, or Result[Unit]",
                    DiagnosticCode::CheckSignalHook,
                );
            }
        }
    }

    pub(super) fn check_binding_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ArenaExprOrRun,
        mutable: bool,
        span: Span,
    ) {
        let expected = ty.map(|ty_id| self.type_from_arena(arena, ty_id));
        let schema = ty.and_then(|ty| {
            self.record_constructors
                .annotation_expectation(&arena.arena, ty, self.current_namespace)
                .ok()
        });
        let local_expected = if ty.is_none() {
            self.local_binding_expectation(span)
        } else {
            None
        };
        let actual = self.check_expr_with_schema_arena(
            arena,
            source,
            initializer,
            expected.as_ref().or(local_expected.as_ref()),
            schema.clone(),
        );
        let callable_alias = if !mutable && ty.is_none() {
            match initializer {
                ArenaExprOrRun::Expr(expression) => {
                    self.resolve_callable_alias_target(arena, expression)
                }
                _ => None,
            }
        } else {
            None
        };
        if ty.is_none() {
            self.record_inert_local_discard(arena, target, initializer);
        }
        if let Some(expected) = &expected
            && !contextual_empty_map_initializer_arena(arena, initializer, expected, &actual)
        {
            let init_span = expr_or_run_span_arena(arena, initializer);
            self.expect_type(expected, &actual, init_span);
        }
        if record_target_requires_schema_check(arena, target, &actual) {
            self.error(
                span,
                "record destructuring of Any requires an explicit schema check",
                DiagnosticCode::CheckDestructureType,
            );
        }
        let final_ty = if ty.is_none() {
            self.infer_local_binding(arena, target, initializer, mutable, span, actual)
        } else {
            expected.unwrap_or(actual)
        };
        if ty.is_none()
            && callable_alias.is_none()
            && should_record_binding_annotation_arena(
                arena,
                target,
                &final_ty,
                self.current_exported,
            )
        {
            let init_span = expr_or_run_span_arena(arena, initializer);
            self.annotation_facts.push(AnnotationFact {
                kind: AnnotationFactKind::Binding {
                    span,
                    initializer: init_span,
                    exported: self.current_exported,
                },
                ty: final_ty.clone(),
            });
        }
        let boolean_proof = if !mutable && final_ty == Type::Bool {
            if let ArenaExprOrRun::Expr(expr) = initializer {
                if let ArenaExprKind::Ident(name) = arena.arena.expr(expr).kind {
                    self.lookup(name)
                        .and_then(|binding| binding.boolean_proof.clone())
                } else {
                    Some(std::sync::Arc::new(
                        self.infer_condition_narrowings_arena(arena, expr),
                    ))
                }
            } else {
                None
            }
        } else {
            None
        };
        let schema = schema.or_else(|| match initializer {
            ArenaExprOrRun::Expr(expression) => self.schema_expectation_for_expr(arena, expression),
            ArenaExprOrRun::Run(_) => None,
        });
        self.define_binding_target_arena(arena, target, &final_ty, mutable, span);
        self.record_checked_local_binding(arena, target, span, &final_ty);
        if let Some(alias) = callable_alias
            && let ArenaBindingTargetKind::Name(name) = arena.arena.binding_target(target).kind
        {
            if self.current_exported
                && (alias.signature.definition.is_none()
                    || !alias.signature.explicit_return
                    || (!alias.pure
                        && (alias.signature.inferred_effects || alias.signature.effects.is_none())))
            {
                self.error(span, "an exported callable alias requires an explicit return and effect contract on its target", DiagnosticCode::CheckCallableAliasExport);
            }
            self.attach_callable_alias(name, alias, expr_or_run_span_arena(arena, initializer));
        }
        if let crate::syntax::arena::ArenaBindingTargetKind::Name(name) =
            arena.arena.binding_target(target).kind
            && let Some(binding) = self.current_scope_mut().get_mut(&name)
        {
            binding.boolean_proof = boolean_proof;
        }
        self.set_binding_schema_arena(arena, target, schema);
    }

    fn set_binding_schema_arena(
        &mut self,
        arena: &ArenaProgram,
        target: BindingTargetId,
        schema: Option<super::super::constants::SchemaExpectation>,
    ) {
        match &arena.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => {
                if let Some(binding) = self.current_scope_mut().get_mut(name) {
                    binding.schema_expectation = schema;
                }
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                for field in arena.arena.destructure_fields(*fields) {
                    let child = schema
                        .as_ref()
                        .and_then(|context| {
                            context
                                .value_context()
                                .children
                                .get(&super::super::constants::SchemaComponent::Field(field.name))
                        })
                        .cloned();
                    self.set_binding_schema_arena(arena, field.target, child);
                }
            }
        }
    }

    /// `x = 1` without `let` or `var` is the shell and Python spelling of a
    /// declaration. A plain name gets a `let` fix (never auto-applied: the
    /// name may instead be a typo of an existing one) and is then declared from
    /// its value, so its later uses do not repeat the same mistake as
    /// unresolved names.
    fn report_undeclared_assignment(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: AssignTargetId,
        op: AssignOp,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        let Some(name) = assign_target_root_name_arena(arena, target) else {
            return;
        };
        let mut diagnostic = Diagnostic::error(format!(
            "assignment to undefined name `{name}`; declare it with `let` or `var`"
        ))
        .with_code(DiagnosticCode::CheckUndefinedName)
        .with_label(Label::primary(span, "assignment to undefined name"));
        if op == AssignOp::Set
            && matches!(
                arena.arena.assign_target(target).kind,
                ArenaAssignTargetKind::Name(_)
            )
        {
            diagnostic = diagnostic.with_fix_hint(
                FixHint::replacement(
                    Span::at(span.source_id, span.start()),
                    "declare it with `let`",
                    "let ",
                )
                .dangerous(),
            );
            self.diagnostics.push(diagnostic);
            let ty = self.check_expr_or_run_arena(arena, source, value, None);
            self.define(name, Binding::new(ty, true), span);
            return;
        }
        self.diagnostics.push(diagnostic);
    }

    fn check_assignment_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: AssignTargetId,
        op: AssignOp,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        let Some(name) = assign_target_root_name_arena(arena, target) else {
            self.check_env_variable_assignment_arena(arena, source, op, value, span);
            return;
        };
        let Some(binding) = self.lookup(name).cloned() else {
            self.report_undeclared_assignment(arena, source, target, op, value, span);
            return;
        };
        if self.in_pure && !binding.pure_local_mutation {
            self.error(
                span,
                "pure functions can assign only to local `var` bindings declared inside the same pure function",
                DiagnosticCode::CheckPureAssignment,
            );
        }
        if !binding.mutable {
            self.error(
                span,
                &format!("cannot assign to `{name}`: it is not a `var`; declare with `var` to allow reassignment"),
                DiagnosticCode::CheckAssignLet,
            );
        }
        let target_ty = self.assignment_target_type_arena(
            arena,
            source,
            target,
            binding.unrefined_ty.as_ref().unwrap_or(&binding.ty),
            span,
        );
        if op == AssignOp::Set {
            let actual = self.check_expr_or_run_arena(arena, source, value, Some(&target_ty));
            let value_span = expr_or_run_span_arena(arena, value);
            if !actual.can_escape_context_scope()
                && self.context_scope_depths.last().is_some_and(|depth| {
                    self.scopes
                        .iter()
                        .rposition(|scope| scope.contains_key(&name))
                        .is_some_and(|owner| owner < *depth)
                })
            {
                self.error(
                    value_span,
                    "a live producer or host handle cannot escape through an outer assignment",
                    DiagnosticCode::CheckContextScopeEscape,
                );
            }
            self.expect_type(&target_ty, &actual, value_span);
            self.invalidate_binding_projection_arena(arena, target, name);
            return;
        }
        // The operand is read as the base type: what is appended to a
        // validated list need not pass the validation itself.
        let rhs =
            self.check_expr_or_run_arena(arena, source, value, Some(target_ty.unvalidated()));
        let value_span = expr_or_run_span_arena(arena, value);
        let result = self.check_compound_assignment_op(op, &target_ty, &rhs, span, value_span);
        self.expect_type(&target_ty, &result, span);
        self.invalidate_binding_projection_arena(arena, target, name);
    }

    /// `e"NAME" = value` sets the variable in the evaluator environment, so
    /// it needs the `env` effect, and its value converts like an `env (...)`
    /// overlay value. `null` is not an unset.
    fn check_env_variable_assignment_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        op: AssignOp,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        self.require_effect(crate::syntax::node::Effect::Env, span, "environment assignment");
        if self.in_pure {
            self.error(
                span,
                "environment assignment is not allowed in pure functions",
                DiagnosticCode::CheckPureEffect,
            );
        }
        if op != AssignOp::Set {
            self.error(
                span,
                "an environment variable is set only with `=`",
                DiagnosticCode::CheckAssignTarget,
            );
        }
        let actual = self.check_expr_or_run_arena(arena, source, value, None);
        let value_span = expr_or_run_span_arena(arena, value);
        if self.reject_dynamic_word(&actual, value_span) {
        } else if !super::command::can_be_env_value(&actual)
            && !matches!(actual, Type::Unknown | Type::Invalid)
        {
            self.error(
                value_span,
                &format!("environment value of type `{actual}` cannot convert to one value"),
                DiagnosticCode::CheckEnvValue,
            );
        }
    }

    fn invalidate_binding_projection_arena(
        &mut self,
        arena: &ArenaProgram,
        mut target: AssignTargetId,
        name: Name,
    ) {
        let mut path = Vec::new();
        loop {
            match arena.arena.assign_target(target).kind {
                ArenaAssignTargetKind::Field { base, name } if path.len() < 128 => {
                    path.push(name);
                    target = base;
                }
                ArenaAssignTargetKind::Name(_) => {
                    path.reverse();
                    break;
                }
                _ => {
                    path.clear();
                    break;
                }
            }
        }
        let Some(identity) = self.lookup(name).map(|binding| binding.proof.clone()) else {
            return;
        };
        for scope in &mut self.scopes {
            for binding in scope
                .values_mut()
                .filter(|binding| binding.proof.same_binding(&identity))
            {
                if !binding.mutable {
                    continue;
                }
                binding.proof.mutate(&path);
                if let Some(original) = &binding.unrefined_ty {
                    super::proof::restore_projection(&mut binding.ty, original, &path);
                }
                if path.is_empty() {
                    binding.unrefined_ty = None;
                }
            }
        }
    }

    pub(super) fn invalidate_mutable_narrowings(&mut self) {
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                if !binding.mutable {
                    continue;
                }
                binding.proof.mutate(&[]);
                if let Some(original) = binding.unrefined_ty.take() {
                    binding.ty = original;
                }
            }
        }
    }

    fn assignment_target_type_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        target: AssignTargetId,
        root_ty: &Type,
        span: Span,
    ) -> Type {
        match &arena.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(_) | ArenaAssignTargetKind::Env(_) => root_ty.clone(),
            ArenaAssignTargetKind::Field { base, name } => {
                let base_ty =
                    self.assignment_target_type_arena(arena, source, *base, root_ty, span);
                match base_ty {
                    Type::Record(fields) => fields.get(name).cloned().unwrap_or_else(|| {
                        self.error(
                            span,
                            &format!("unknown record field `{name}`"),
                            DiagnosticCode::CheckUnknownField,
                        );
                        Type::Unknown
                    }),
                    Type::Unknown => Type::Unknown,
                    _ => {
                        self.error(
                            span,
                            "field assignment requires a record value",
                            DiagnosticCode::CheckAssignTarget,
                        );
                        Type::Unknown
                    }
                }
            }
            ArenaAssignTargetKind::Index { base, index } => {
                // Replacing an element keeps the list's length, so the
                // target is read as its base and keeps its own type.
                let base_ty = self
                    .assignment_target_type_arena(arena, source, *base, root_ty, span)
                    .into_unvalidated();
                let literal_key = match &base_ty {
                    Type::Map(key_ty, _) => self.path_literal_expectation(arena, *index, key_ty),
                    _ => None,
                };
                let index_ty = self.check_expr_arena(arena, source, *index, literal_key.as_ref());
                match base_ty {
                    Type::Map(key_ty, item_ty) => {
                        let index_span = arena.arena.expr(*index).span;
                        self.expect_type(&key_ty, &index_ty, index_span);
                        item_ty.as_ref().clone()
                    }
                    Type::List(item_ty) => {
                        self.expect_type(&Type::Int, &index_ty, arena.arena.expr(*index).span);
                        item_ty.as_ref().clone()
                    }
                    Type::Unknown => Type::Unknown,
                    _ => {
                        self.error(
                            span,
                            "indexed assignment requires List or Map values",
                            DiagnosticCode::CheckAssignTarget,
                        );
                        Type::Unknown
                    }
                }
            }
        }
    }

    fn check_return_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: Option<ArenaExprOrRun>,
        span: Span,
    ) {
        // Scripts select an exit status with a final top-level `Int` or
        // `abort`; top-level code has no callable to return from.
        if self.current_return.is_none() {
            self.error(
                span,
                "`return` is valid only inside a callable body",
                DiagnosticCode::CheckReturnOutsideCallable,
            );
        }
        if self.in_defer_block {
            self.error(
                span,
                "`return` cannot leave a deferred cleanup block",
                DiagnosticCode::CheckDeferControlFlow,
            );
        }
        if self.in_signal_hook {
            self.error(
                span,
                "`return` is not allowed in signal hooks",
                DiagnosticCode::CheckSignalHook,
            );
        }
        if self.current_yield.is_some() && value.is_some() {
            let value_span = value.map_or(span, |v| expr_or_run_span_arena(arena, v));
            self.error(
                value_span,
                "stream producer return cannot include a value",
                DiagnosticCode::CheckStreamReturn,
            );
        }
        if self.current_return.is_none() {
            // There is no return type to hold the value to, so the value is
            // checked on its own and the misplaced `return` is the one report.
            if let Some(value) = value {
                self.check_expr_with_schema_arena(arena, source, value, None, None);
            }
            return;
        }
        let expected = self.current_return.clone().unwrap_or(Type::Unit);
        if value.is_none() && expected.is_result_unit() {
            return;
        }
        let context = match value {
            Some(ArenaExprOrRun::Expr(expr)) => {
                tail_expr_context_arena(arena, expr, Some(&expected))
            }
            _ => None,
        };
        let actual = value
            .map(|value| {
                let schema = self.return_schema.as_ref().map(|schema| {
                    if matches!(expected, Type::Result(_, _))
                        && !matches!(context, Some(Type::Result(_, _)))
                    {
                        schema
                            .children
                            .get(&crate::sema::constants::SchemaComponent::Success)
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        schema.clone()
                    }
                });
                self.check_expr_with_schema_arena(arena, source, value, context.as_ref(), schema)
            })
            .unwrap_or(Type::Unit);
        let actual = self.resolve_local_tail_type(actual, Some(&expected), span);
        if !self.context_scope_depths.is_empty() && !actual.can_escape_context_scope() {
            self.error(
                span,
                "a live producer or host handle cannot escape through a lexical return",
                DiagnosticCode::CheckContextScopeEscape,
            );
        }
        if self.inference_reachable
            && let Some(returns) = &mut self.inferred_returns
        {
            returns.push((actual.clone(), span));
        }
        if !tail_type_matches_expected(&expected, &actual) {
            let value_span = value.map_or(span, |v| expr_or_run_span_arena(arena, v));
            self.expect_type(&expected, &actual, value_span);
        }
    }

    fn check_yield_delegation_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ExprId,
        span: Span,
    ) {
        if self.in_defer_block {
            self.error(
                span,
                "`yield` is not allowed in a deferred cleanup block",
                DiagnosticCode::CheckDeferControlFlow,
            );
        }
        self.reject_yield_in_retry(span);
        let expected = self.current_yield.clone();
        if expected.is_none() {
            self.error(
                span,
                "`yield` is valid only in stream producers",
                DiagnosticCode::CheckYield,
            );
        }
        // Fresh list syntax receives an item context for empty and nested
        // literals. Callable sources retain their declared List or Stream kind.
        let collection = match arena.arena.expr(value).kind {
            ArenaExprKind::List(_) | ArenaExprKind::ListComp { .. } => {
                expected.as_ref().map(|ty| Type::List(Box::new(ty.clone())))
            }
            _ => None,
        };
        let actual = self.check_expr_arena(arena, source, value, collection.as_ref());
        let value_span = arena.arena.expr(value).span;
        match actual.into_unvalidated() {
            Type::List(item) | Type::Stream(item) => {
                if !self.context_scope_depths.is_empty() && !item.can_escape_context_scope() {
                    self.error(
                        value_span,
                        "a delegated live producer or host handle cannot escape a context",
                        DiagnosticCode::CheckContextScopeEscape,
                    );
                }
                if let Some(expected) = expected {
                    self.expect_type(&expected, &item, value_span);
                }
            }
            Type::Unknown => {}
            _ => self.error(
                value_span,
                "yield delegation requires a List or Stream; handle Results explicitly",
                DiagnosticCode::CheckYieldDelegation,
            ),
        }
    }

    /// A retry attempt runs outside its producer's frame, so a `yield` there
    /// used to check and then fail at runtime.
    fn reject_yield_in_retry(&mut self, span: Span) {
        if self.retry_block_depth > 0 {
            self.error(
                span,
                "`yield` is not allowed inside a retry attempt",
                DiagnosticCode::CheckYield,
            );
        }
        // A producer suspended inside the block would keep its deadline open
        // while its consumer runs.
        if self.within_block_depth > 0 {
            self.error(
                span,
                "`yield` is not allowed inside a `within` block",
                DiagnosticCode::CheckYield,
            );
        }
    }

    fn check_yield_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        if self.in_defer_block {
            self.error(
                span,
                "`yield` is not allowed in a deferred cleanup block",
                DiagnosticCode::CheckDeferControlFlow,
            );
        }
        self.reject_yield_in_retry(span);
        let expected = match self.current_yield.clone() {
            Some(ty) => ty,
            None => {
                self.error(
                    span,
                    "`yield` is valid only in stream producers",
                    DiagnosticCode::CheckYield,
                );
                self.check_expr_or_run_arena(arena, source, value, None);
                return;
            }
        };
        let actual = self.check_expr_or_run_arena(arena, source, value, Some(&expected));
        let value_span = expr_or_run_span_arena(arena, value);
        if matches!(actual, Type::Stream(_)) {
            self.error(
                value_span,
                "`yield` does not accept a stream; use `yield @stream`",
                DiagnosticCode::CheckYieldStream,
            );
            return;
        }
        if !self.context_scope_depths.is_empty() && !actual.can_escape_context_scope() {
            self.error(
                value_span,
                "a live producer or host handle cannot escape through yield",
                DiagnosticCode::CheckContextScopeEscape,
            );
        }
        self.expect_type(&expected, &actual, value_span);
    }

    fn check_defer_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        if self.in_pure {
            self.error(
                span,
                "`defer` is not allowed in pure functions",
                DiagnosticCode::CheckPureDefer,
            );
        }
        if let ArenaExprOrRun::Expr(expr) = value
            && let ArenaExprKind::ValueBlock(block) = arena.arena.expr(expr).kind
        {
            let saved_capture_scopes = self.scopes.clone();
            let previous_errors = self.with_initializer_errors.take();
            let previous_defer = std::mem::replace(&mut self.in_defer_block, true);
            let previous_loop = std::mem::replace(&mut self.loop_depth, 0);
            self.push_scope();
            // Cleanup reads mutable captures later, after current branch refinements may expire.
            let mut captures = FxHashMap::default();
            for scope in &self.scopes {
                for (&name, binding) in scope {
                    let mut binding = binding.clone();
                    if binding.mutable {
                        binding.proof.mutate(&[]);
                        if let Some(original) = binding.unrefined_ty.take() {
                            binding.ty = original;
                        }
                    }
                    captures.insert(name, binding);
                }
            }
            self.current_scope_mut().extend(captures);
            let body = arena.arena.block(block);
            if let Some(param) = arena.arena.block_params(body.params).first() {
                self.error(
                    arena.arena.span(param.span),
                    "deferred cleanup blocks have no parameters",
                    DiagnosticCode::CheckBlockParams,
                );
            }
            self.push_scope();
            self.block_depth += 1;
            for statement in arena.arena.stmt_ids(body.statements) {
                let statement = arena.arena.core_stmt_id(statement);
                self.check_non_tail_stmt_arena(arena, source, statement);
                if let ArenaStmtKind::TailBareIdent(name) = arena.arena.stmt(statement).kind {
                    let ty = self
                        .lookup(name)
                        .map(|binding| binding.ty.clone())
                        .unwrap_or(Type::Unknown);
                    if !expr_ty_auto_propagates(&ty)
                        && !ty.matches_expected(&Type::Unit)
                        && ty != Type::Bool
                    {
                        self.error(arena.arena.stmt(statement).span, "cleanup statement must produce Unit; use `let _ = ...` to discard a value", DiagnosticCode::CheckDeferType);
                    }
                }
            }
            self.block_depth -= 1;
            self.pop_scope();
            self.pop_scope();
            self.scopes = saved_capture_scopes;
            self.in_defer_block = previous_defer;
            self.with_initializer_errors = previous_errors;
            self.loop_depth = previous_loop;
            self.expr_types
                .insert(arena.arena.expr(expr).span, Type::Unit);
            return;
        }
        let ty = self.check_expr_or_run_arena(arena, source, value, None);
        self.record_statement_error(&ty, span);
        match ty {
            Type::Unit | Type::Status | Type::Unknown => {}
            Type::Result(ok, _) if *ok == Type::Unit => {}
            _ => {
                let value_span = expr_or_run_span_arena(arena, value);
                self.error(
                    value_span,
                    "deferred cleanup must produce Unit, Status, or Result[Unit]",
                    DiagnosticCode::CheckDeferType,
                );
            }
        }
    }

    pub(super) fn check_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
    ) {
        let block = arena.arena.block(block_id);
        if let Some(param) = arena.arena.block_params(block.params).first() {
            let param_span = arena.arena.span(param.span);
            self.error(
                param_span,
                "this block does not receive parameters",
                DiagnosticCode::CheckBlockParams,
            );
        }
        self.push_scope();
        self.check_statement_block_contents_arena(arena, source, block_id);
        self.pop_scope();
    }

    pub(super) fn check_statement_block_contents_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
    ) {
        let enclosing_bound = self.enter_block_effect_bound(arena, block_id);
        self.check_statement_block_statements_arena(arena, source, block_id);
        self.leave_block_effect_bound(enclosing_bound);
    }

    fn check_statement_block_statements_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
    ) {
        let block = arena.arena.block(block_id);
        self.block_depth += 1;
        let previous_reachable = self.inference_reachable;
        for stmt_id in arena.arena.stmt_ids(block.statements) {
            self.check_stmt_arena(arena, source, stmt_id);
            if self.inferred_returns.is_some() && self.return_inference_stmt_returns(arena, stmt_id)
            {
                self.inference_reachable = false;
            }
        }
        self.inference_reachable = previous_reachable;
        let bindings = self
            .scopes
            .iter()
            .flat_map(|scope| scope.iter())
            .map(|(name, binding)| (*name, binding.clone()))
            .collect();
        self.block_exit_bindings.insert(block_id, bindings);
        if self.block_definitely_exits_arena(arena, block_id) {
            self.definitely_exiting_block_spans
                .insert(arena.arena.span(block.span));
        }
        self.block_depth -= 1;
    }

    // Only checked exits count. A call that can fail still has a success continuation.
    fn block_definitely_exits_arena(&self, arena: &ArenaProgram, block: BlockId) -> bool {
        arena
            .arena
            .stmt_ids(arena.arena.block(block).statements)
            .any(|id| self.stmt_definitely_exits_arena(arena, id))
    }

    fn stmt_definitely_exits_arena(&self, arena: &ArenaProgram, statement: StmtId) -> bool {
        match arena.arena.stmt(statement).kind {
            ArenaStmtKind::Sugar { expansion, .. } => {
                self.stmt_definitely_exits_arena(arena, expansion)
            }
            ArenaStmtKind::Return(_) => self.current_return.is_some(),
            ArenaStmtKind::Exit(_) => true,
            ArenaStmtKind::Break { .. } | ArenaStmtKind::Continue => self.loop_depth > 0,
            ArenaStmtKind::Expr(expr) => self.expr_definitely_exits_arena(arena, expr),
            ArenaStmtKind::Let {
                initializer: ArenaExprOrRun::Expr(expr),
                ..
            }
            | ArenaStmtKind::Var {
                initializer: ArenaExprOrRun::Expr(expr),
                ..
            }
            | ArenaStmtKind::Assign {
                value: ArenaExprOrRun::Expr(expr),
                ..
            } => self.expr_definitely_exits_arena(arena, expr),
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                for branch in arena.arena.if_branches(branches) {
                    if self.expr_definitely_exits_arena(arena, branch.condition) {
                        return true;
                    }
                    match arena.arena.expr(branch.condition).kind {
                        ArenaExprKind::Bool(false) => continue,
                        ArenaExprKind::Bool(true) => {
                            return self.block_definitely_exits_arena(arena, branch.block);
                        }
                        _ if !self.block_definitely_exits_arena(arena, branch.block) => {
                            return false;
                        }
                        _ => {}
                    }
                }
                else_block.is_some_and(|block| self.block_definitely_exits_arena(arena, block))
            }
            ArenaStmtKind::Match { value, arms } => {
                let arms = arena.arena.match_arms(arms);
                let ty = self
                    .expr_types
                    .get(&arena.arena.expr(value).span)
                    .cloned()
                    .unwrap_or(Type::Unknown);
                match_is_exhaustive_arena(arena, &ty, arms, &self.exhaustiveness_facts())
                    && arms
                        .iter()
                        .all(|arm| self.block_definitely_exits_arena(arena, arm.block))
            }
            ArenaStmtKind::With {
                body, else_block, ..
            } => {
                self.block_definitely_exits_arena(arena, body)
                    && self.block_definitely_exits_arena(arena, else_block)
            }
            ArenaStmtKind::Loop { block } => !block_has_exit_point_arena(arena, block),
            ArenaStmtKind::While { condition, block }
                if matches!(arena.arena.expr(condition).kind, ArenaExprKind::Bool(true)) =>
            {
                !block_has_exit_point_arena(arena, block)
            }
            _ => false,
        }
    }

    fn expr_definitely_exits_arena(&self, arena: &ArenaProgram, expr: ExprId) -> bool {
        match arena.arena.expr(expr).kind {
            ArenaExprKind::ValueBlock(block) => self.block_definitely_exits_arena(arena, block),
            _ => false,
        }
    }

    pub(super) fn check_tail_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
        expected: Option<&Type>,
    ) -> Type {
        let block = arena.arena.block(block_id);
        if let Some(param) = arena.arena.block_params(block.params).first() {
            let param_span = arena.arena.span(param.span);
            self.error(
                param_span,
                "this block does not receive parameters",
                DiagnosticCode::CheckBlockParams,
            );
        }
        self.check_tail_block_contents_arena(arena, source, block_id, expected)
    }

    pub(super) fn check_tail_block_contents_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
        expected: Option<&Type>,
    ) -> Type {
        let enclosing_bound = self.enter_block_effect_bound(arena, block_id);
        let ty = self.check_tail_block_statements_arena(arena, source, block_id, expected);
        self.leave_block_effect_bound(enclosing_bound);
        ty
    }

    fn check_tail_block_statements_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: BlockId,
        expected: Option<&Type>,
    ) -> Type {
        let block = arena.arena.block(block_id);
        self.block_depth += 1;
        let stmt_ids: Vec<StmtId> = arena
            .arena
            .stmt_ids(block.statements)
            .map(|id| arena.arena.core_stmt_id(id))
            .collect();
        let previous_reachable = self.inference_reachable;
        let result = if let Some((&tail, non_tail)) = stmt_ids.split_last() {
            let valueless_if = matches!(arena.arena.stmt(tail).kind, ArenaStmtKind::If { .. })
                && !if_stmt_may_produce_value(arena, tail);
            let tail_producing = !valueless_if
                && matches!(
                    arena.arena.stmt(tail).kind,
                    ArenaStmtKind::Expr(_)
                        | ArenaStmtKind::Command(_)
                        | ArenaStmtKind::TailBareIdent(_)
                        | ArenaStmtKind::Match { .. }
                        | ArenaStmtKind::If { .. }
                        | ArenaStmtKind::Return(_)
                        | ArenaStmtKind::Exit(_)
                        | ArenaStmtKind::Break { .. }
                        | ArenaStmtKind::Continue
                );
            for &stmt_id in non_tail {
                let previous_tail = std::mem::replace(&mut self.context_scope_tail_value, false);
                self.check_non_tail_stmt_arena(arena, source, stmt_id);
                self.context_scope_tail_value = previous_tail;
                if self.inferred_returns.is_some()
                    && self.return_inference_stmt_returns(arena, stmt_id)
                {
                    self.inference_reachable = false;
                }
            }
            if tail_producing {
                let ty = self.check_tail_stmt_arena(arena, source, tail, expected);
                if let Some(expected) = expected {
                    let tail_span = arena.arena.stmt(tail).span;
                    if !tail_type_matches_expected(expected, &ty) {
                        self.expect_type(expected, &ty, tail_span);
                    }
                }
                ty
            } else if valueless_if {
                self.check_valueless_tail_if_arena(arena, source, tail)
            } else {
                self.check_stmt_arena(arena, source, tail);
                Type::Unit
            }
        } else {
            Type::Unit
        };
        let reachable = self.inference_reachable;
        self.inference_reachable = previous_reachable;
        self.block_depth -= 1;
        let always_returns = if self.inferred_returns.is_some() {
            self.return_inference_block_returns(arena, block_id)
        } else {
            block_always_returns_arena(arena, block_id)
        };
        if !reachable || always_returns {
            Type::Unknown
        } else {
            result
        }
    }

    /// A value branch whose block ends without a value (a trailing `let`, or
    /// nothing) completes with Unit. The block checker reports a mismatch only
    /// for a tail value, so such a branch used to satisfy any expected type
    /// and the function then failed preparation.
    fn check_unit_branch_completion(
        &mut self,
        arena: &ArenaProgram,
        block: BlockId,
        expected: Option<Type>,
        actual: &Type,
    ) {
        if *actual == Type::Unit
            && let Some(expected) = expected
            && !tail_type_matches_expected(&expected, actual)
        {
            self.expect_type(
                &expected,
                actual,
                arena.arena.span(arena.arena.block(block).span),
            );
        }
    }

    fn return_inference_block_returns(&self, arena: &ArenaProgram, block: BlockId) -> bool {
        arena
            .arena
            .stmt_ids(arena.arena.block(block).statements)
            .any(|id| self.return_inference_stmt_returns(arena, id))
    }

    fn return_inference_stmt_returns(&self, arena: &ArenaProgram, id: StmtId) -> bool {
        match arena.arena.stmt(id).kind {
            ArenaStmtKind::Sugar { expansion, .. } => {
                self.return_inference_stmt_returns(arena, expansion)
            }
            ArenaStmtKind::Return(_) => true,
            ArenaStmtKind::With {
                body, else_block, ..
            } => {
                self.return_inference_block_returns(arena, body)
                    && self.return_inference_block_returns(arena, else_block)
            }
            ArenaStmtKind::Expr(expr) => match arena.arena.expr(expr).kind {
                ArenaExprKind::ErrorContext { block, .. } => {
                    self.return_inference_block_returns(arena, block)
                }
                _ => false,
            },
            ArenaStmtKind::If {
                branches,
                else_block: Some(other),
            } => {
                arena
                    .arena
                    .if_branches(branches)
                    .iter()
                    .all(|branch| self.return_inference_block_returns(arena, branch.block))
                    && self.return_inference_block_returns(arena, other)
            }
            ArenaStmtKind::Match { value, arms } => {
                let Some(ty) = self.expr_types.get(&arena.arena.expr(value).span) else {
                    return false;
                };
                let arms = arena.arena.match_arms(arms);
                patterns_are_exhaustive_arena(
                    arena,
                    ty,
                    arms.iter()
                        .filter(|arm| arm.guard.is_none())
                        .map(|arm| arm.pattern),
                    &self.exhaustiveness_facts(),
                ) && arms
                    .iter()
                    .all(|arm| self.return_inference_block_returns(arena, arm.block))
            }
            _ => false,
        }
    }

    fn check_non_tail_stmt_arena(&mut self, arena: &ArenaProgram, source: &str, id: StmtId) {
        let id = arena.arena.core_stmt_id(id);
        let stmt = arena.arena.stmt(id);
        self.statement_positions
            .insert(stmt.span, super::StatementPosition::Statement);
        if let ArenaStmtKind::Expr(expr_id) = stmt.kind {
            self.statement_root = Some(arena.arena.expr(expr_id).span);
            self.statement_expression_spans
                .insert(arena.arena.expr(expr_id).span);
            self.propagating_statements.insert(stmt.span);
            let ty = if let ArenaExprKind::ValueBlock(block) = arena.arena.expr(expr_id).kind {
                self.check_block_arena(arena, source, block);
                self.expr_types
                    .insert(arena.arena.expr(expr_id).span, Type::Unit);
                Type::Unit
            } else {
                self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(expr_id),
                    None,
                    None,
                )
            };
            self.record_inert_expression_discard(arena, ArenaExprOrRun::Expr(expr_id));
            if self.reject_bool_statement(source, &ty, stmt.span) {
                return;
            }
            self.record_statement_error(&ty, stmt.span);
            self.reject_discarded_value(
                &ty,
                stmt.span,
                arena.arena.expr(expr_id).span,
                true,
                copy_update_mistake(arena, expr_id, &ty),
            );
            return;
        }
        self.check_stmt_arena(arena, source, id);
    }

    pub(super) fn check_tail_stmt_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: StmtId,
        expected: Option<&Type>,
    ) -> Type {
        let id = arena.arena.core_stmt_id(id);
        let stmt = arena.arena.stmt(id);
        self.statement_positions
            .insert(stmt.span, super::StatementPosition::Value);
        // A `Unit` body leaves a `Result[Unit]` tail nothing to become, and
        // the direct tail of a `Result[Unit]` function propagates as that
        // function's result. Against any other result type, or none, the tail
        // is the body's value, as is a tail that builds or captures a
        // `Result`. The question is asked of the operand under a `?`, which
        // is what remains when the `?` is redundant.
        let propagating = match stmt.kind {
            ArenaStmtKind::Expr(expr) => {
                let operand = match arena.arena.expr(expr).kind {
                    ArenaExprKind::Try(operand) => operand,
                    _ => expr,
                };
                expected == Some(&Type::Unit)
                    || (expected.is_some_and(Type::is_result_unit)
                        && self.result_unit_function_tail == Some(id)
                        && !tail_expr_uses_result_context_arena(arena, operand))
            }
            _ => false,
        };
        if propagating {
            self.propagating_statements.insert(stmt.span);
        } else {
            self.propagating_statements.remove(&stmt.span);
        }
        if let ArenaStmtKind::Expr(expr_id) = stmt.kind {
            self.statement_root = Some(arena.arena.expr(expr_id).span);
        }
        if expected.is_some_and(|ty| ty == &Type::Unit || ty.is_result_unit())
            && !(expected.is_some_and(Type::is_result_unit)
                && tail_stmt_uses_result_context_arena(arena, id))
        {
            if let ArenaStmtKind::Expr(expr_id) = stmt.kind {
                let previous_tail = std::mem::replace(&mut self.context_scope_tail_value, false);
                // A statement's Unit result is produced by consuming its value;
                // it must not constrain a Bool-producing call before Bool statement
                // classification. Blocks and inferred schemas still need their
                // declared success context while checking their contents.
                let context = statement_tail_needs_value_context_arena(arena, expr_id)
                    .then(|| tail_expr_context_arena(arena, expr_id, expected))
                    .flatten();
                let schema = self.expected_schema.as_ref().map(|schema| {
                    if expected.is_some_and(Type::is_result)
                        && !context.as_ref().is_some_and(Type::is_result)
                    {
                        schema
                            .children
                            .get(&crate::sema::constants::SchemaComponent::Success)
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        schema.clone()
                    }
                });
                let actual = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(expr_id),
                    context.as_ref(),
                    schema,
                );
                self.context_scope_tail_value = previous_tail;
                self.record_inert_expression_discard(arena, ArenaExprOrRun::Expr(expr_id));
                if actual.is_result() {
                    if expected.is_some_and(Type::is_result_unit) {
                        // The type stays the function's result, so its error
                        // is still checked against the declared one and no
                        // `error` effect is asked of a proc that only hands
                        // its callee's result back.
                        if expr_ty_auto_propagates(&actual)
                            && self.propagating_statements.contains(&stmt.span)
                        {
                            self.statement_positions
                                .insert(stmt.span, super::StatementPosition::Statement);
                        }
                        return actual;
                    }
                    if expr_ty_auto_propagates(&actual) {
                        self.statement_positions
                            .insert(stmt.span, super::StatementPosition::Statement);
                        return Type::Unit;
                    }
                    self.reject_discarded_value(
                        &actual,
                        stmt.span,
                        arena.arena.expr(expr_id).span,
                        true,
                        copy_update_mistake(arena, expr_id, &actual),
                    );
                }
                self.statement_expression_spans
                    .insert(arena.arena.expr(expr_id).span);
                if !self.reject_bool_statement(source, &actual, stmt.span) {
                    self.record_statement_error(&actual, stmt.span);
                    if !actual.is_result()
                        && !self.is_inert_expression_discard(arena.arena.expr(expr_id).span)
                    {
                        self.expect_type(&Type::Unit, &actual, arena.arena.expr(expr_id).span);
                    }
                }
                self.statement_positions
                    .insert(stmt.span, super::StatementPosition::Statement);
                return Type::Unit;
            }
            self.statement_positions
                .insert(stmt.span, super::StatementPosition::Statement);
            self.check_stmt_arena(arena, source, id);
            return Type::Unit;
        }
        match stmt.kind {
            ArenaStmtKind::Assert { .. } => {
                self.statement_positions
                    .insert(stmt.span, super::StatementPosition::Statement);
                self.check_stmt_arena(arena, source, id);
                Type::Unit
            }
            ArenaStmtKind::Expr(expr_id) => {
                if expected.is_some_and(|ty| *ty == Type::Unit || ty.is_result_unit()) {
                    self.statement_expression_spans
                        .insert(arena.arena.expr(expr_id).span);
                }
                let ctx = tail_expr_context_arena(arena, expr_id, expected);
                let previous = std::mem::replace(&mut self.context_scope_tail_value, true);
                let schema = self.expected_schema.as_ref().map(|schema| {
                    if matches!(expected, Some(Type::Result(_, _)))
                        && !matches!(ctx, Some(Type::Result(_, _)))
                    {
                        schema
                            .children
                            .get(&crate::sema::constants::SchemaComponent::Success)
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        schema.clone()
                    }
                });
                let ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(expr_id),
                    ctx.as_ref(),
                    schema,
                );
                self.context_scope_tail_value = previous;
                let ty = self.resolve_local_tail_type(ty, expected, stmt.span);
                // An inferred return takes this tail's `Result[Unit]` as the
                // function's result, which makes it the propagating tail of a
                // `Result[Unit]` body. A constructed `Ok()` or `Err(..)` is
                // the value itself.
                if expected.is_none()
                    && expr_ty_auto_propagates(&ty)
                    && self.result_unit_function_tail == Some(id)
                    && !tail_stmt_uses_result_context_arena(arena, id)
                {
                    self.statement_positions
                        .insert(stmt.span, super::StatementPosition::Statement);
                    self.propagating_statements.insert(stmt.span);
                }
                ty
            }
            ArenaStmtKind::TailBareIdent(name) => {
                let ty = match self.tail_typed_callable(name, stmt.span, expected) {
                    Some(ty) => ty,
                    None => self.check_tail_bare_ident_arena(arena, source, name, stmt.span),
                };
                let ty = self.resolve_local_tail_type(ty, expected, stmt.span);
                if expected.is_some_and(|ty| *ty == Type::Unit || ty.is_result_unit())
                    && self.reject_bool_statement(source, &ty, stmt.span)
                {
                    Type::Unit
                } else {
                    ty
                }
            }
            ArenaStmtKind::Command(command_id) => {
                let command_stmt = arena.arena.command_stmt(command_id);
                if self.in_pure {
                    self.error(
                        stmt.span,
                        "commands are not allowed in pure functions",
                        DiagnosticCode::CheckPureCommand,
                    );
                }
                let ty = self.check_command_arena(arena, source, &command_stmt.command, stmt.span);
                // A consumed capture tail keeps its output. Unit tails have
                // already taken the statement path, which discards that output.
                if let crate::syntax::arena::ArenaCommand::Run(run) = command_stmt.command
                    && super::command::run_capture_result_type_arena(arena, run).is_some()
                {
                    return ty;
                }
                if command_stmt_asserts_success_arena(arena, &command_stmt.command) {
                    self.record_statement_error(
                        &Type::Result(Box::new(Type::Unit), Box::new(Type::ProcessError)),
                        stmt.span,
                    );
                    return Type::Unit;
                }
                if command_stmt.propagate || command_ty_auto_propagates(&ty) {
                    self.check_propagation(&ty, stmt.span)
                } else {
                    ty
                }
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                if !if_stmt_may_produce_value(arena, id) {
                    return self.check_valueless_tail_if_arena(arena, source, id);
                }
                let infer_branches = self.inferred_returns.is_some() && expected.is_none();
                let mut inferred = None;
                // What every earlier condition proved by failing holds in
                // each later condition and branch, and in the final `else`.
                let mut previous_failure = Vec::new();
                for branch in arena.arena.if_branches(branches) {
                    self.push_scope();
                    self.apply_narrowings(&previous_failure);
                    let narrowings = self.check_condition_arena(
                        arena,
                        source,
                        branch.condition,
                        DiagnosticCode::CheckIfCondition,
                    );
                    self.push_scope();
                    self.apply_narrowings(&narrowings.when_true);
                    self.bind_pattern_condition_arena(arena, source, branch.condition);
                    let actual = self.check_tail_block_arena(
                        arena,
                        source,
                        branch.block,
                        if infer_branches {
                            None
                        } else {
                            expected.or(inferred.as_ref())
                        },
                    );
                    self.pop_scope();
                    if !infer_branches {
                        self.check_unit_branch_completion(
                            arena,
                            branch.block,
                            expected.or(inferred.as_ref()).cloned(),
                            &actual,
                        );
                    }
                    if actual != Type::Unknown {
                        inferred = Some(if infer_branches {
                            inferred.map_or(actual.clone(), |previous| {
                                self.unify_inferred_returns(previous, actual, stmt.span)
                            })
                        } else {
                            inferred.unwrap_or(actual)
                        });
                    }
                    previous_failure.extend(narrowings.when_false);
                    self.pop_scope();
                }
                if let Some(block) = else_block {
                    self.push_scope();
                    self.apply_narrowings(&previous_failure);
                    let actual = self.check_tail_block_arena(
                        arena,
                        source,
                        block,
                        if infer_branches {
                            None
                        } else {
                            expected.or(inferred.as_ref())
                        },
                    );
                    self.check_required_block_exit(arena, block);
                    self.pop_scope();
                    if !infer_branches {
                        self.check_unit_branch_completion(
                            arena,
                            block,
                            expected.or(inferred.as_ref()).cloned(),
                            &actual,
                        );
                    }
                    if actual != Type::Unknown {
                        inferred = Some(if infer_branches {
                            inferred.map_or(actual.clone(), |previous| {
                                self.unify_inferred_returns(previous, actual, stmt.span)
                            })
                        } else {
                            inferred.unwrap_or(actual)
                        });
                    }
                } else {
                    self.error(
                        stmt.span,
                        "value-producing if requires an else branch",
                        DiagnosticCode::CheckIfValueElse,
                    );
                }
                inferred.unwrap_or(Type::Unknown)
            }
            ArenaStmtKind::Match { value, arms } => {
                self.check_tail_match_arena(arena, source, value, arms, expected)
            }
            ArenaStmtKind::Return(_)
            | ArenaStmtKind::Break { .. }
            | ArenaStmtKind::Continue
            | ArenaStmtKind::Exit(_) => {
                self.check_stmt_arena(arena, source, id);
                Type::Unknown
            }
            _ => {
                self.check_stmt_arena(arena, source, id);
                Type::Unit
            }
        }
    }

    /// Checks a tail `if` none of whose branches ends in a value. It runs as
    /// a statement; the block it ends has no value when every path through
    /// the `if` leaves, and completes with Unit otherwise.
    fn check_valueless_tail_if_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: StmtId,
    ) -> Type {
        self.statement_positions.insert(
            arena.arena.stmt(id).span,
            super::StatementPosition::Statement,
        );
        self.check_stmt_arena(arena, source, id);
        if self.stmt_definitely_exits_arena(arena, id) {
            Type::Unknown
        } else {
            Type::Unit
        }
    }

    fn check_tail_match_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ExprId,
        arms: ArenaRange,
        expected: Option<&Type>,
    ) -> Type {
        let value_ty = self.check_expr_arena(arena, source, value, None);
        let arm_list = arena.arena.match_arms(arms);
        let all_arms_return = match_is_exhaustive_arena(
            arena,
            &value_ty,
            arm_list,
            &self.exhaustiveness_facts(),
        ) && arm_list
            .iter()
            .all(|arm| block_always_returns_arena(arena, arm.block));
        let infer_branches = self.inferred_returns.is_some() && expected.is_none();
        let mut inferred: Option<Type> = None;
        for arm in arm_list {
            self.push_scope();
            self.check_pattern_arena(arena, source, arm.pattern, &value_ty);
            if let Some(value) = Self::single_error_handler_value_arena(arena, arm.block) {
                self.warn_flattened_error_handler_arena(arena, value, arm.pattern, &value_ty);
            }
            if let Some(guard) = arm.guard {
                let guard_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(guard),
                    Some(&Type::Bool),
                    None,
                );
                let guard_span = arena.arena.expr(guard).span;
                self.expect_type(&Type::Bool, &guard_ty, guard_span);
            }
            let arm_ty = self.check_tail_block_arena(
                arena,
                source,
                arm.block,
                if infer_branches {
                    None
                } else {
                    expected.or(inferred.as_ref())
                },
            );
            if !infer_branches {
                self.check_unit_branch_completion(
                    arena,
                    arm.block,
                    expected.or(inferred.as_ref()).cloned(),
                    &arm_ty,
                );
            }
            if arm_ty != Type::Unknown {
                inferred = Some(if infer_branches {
                    inferred.map_or(arm_ty.clone(), |previous| {
                        self.unify_inferred_returns(previous, arm_ty, arena.arena.span(arm.span))
                    })
                } else {
                    inferred.unwrap_or(arm_ty)
                });
            }
            self.pop_scope();
        }
        let value_span = arena.arena.expr(value).span;
        self.check_list_match_coverage_arena(
            arena,
            &value_ty,
            arm_list
                .iter()
                .map(|arm| (arm.pattern, arena.arena.span(arm.span), arm.guard.is_some())),
            value_span,
        );
        if !match_is_exhaustive_arena(
            arena,
            &value_ty,
            arm_list,
            &self.exhaustiveness_facts(),
        ) && !self.match_scrutinee_definitely_exits_arena(arena, value)
        {
            let unguarded = arm_list
                .iter()
                .filter(|arm| arm.guard.is_none())
                .map(|arm| (arm.pattern, arena.arena.span(arm.span)))
                .collect::<Vec<_>>();
            self.report_value_match_not_exhaustive(arena, &value_ty, &unguarded, value_span);
        }
        if all_arms_return {
            Type::Unknown
        } else {
            inferred.unwrap_or(Type::Unknown)
        }
    }

    /// Whether a match scrutinee can only exit, never produce a value.
    ///
    /// Flow analysis types an always-exiting block as `Unknown`; without this,
    /// a value match on such a scrutinee would be reported as non-exhaustive
    /// even though no value ever reaches it.
    pub(super) fn match_scrutinee_definitely_exits_arena(
        &self,
        arena: &ArenaProgram,
        value: ExprId,
    ) -> bool {
        if self.expr_definitely_exits_arena(arena, value) {
            return true;
        }
        match &arena.arena.expr(value).kind {
            ArenaExprKind::ErrorContext { block, .. }
            | ArenaExprKind::ContextScope { block, .. }
            | ArenaExprKind::TempDirScope { block, .. } => {
                self.block_definitely_exits_arena(arena, *block)
            }
            _ => false,
        }
    }
}

/// The binding an assignment writes through, or `None` for an environment
/// variable target.
fn assign_target_root_name_arena(arena: &ArenaProgram, target: AssignTargetId) -> Option<Name> {
    match &arena.arena.assign_target(target).kind {
        ArenaAssignTargetKind::Name(name) => Some(*name),
        ArenaAssignTargetKind::Env(_) => None,
        ArenaAssignTargetKind::Field { base, .. } | ArenaAssignTargetKind::Index { base, .. } => {
            assign_target_root_name_arena(arena, *base)
        }
    }
}

#[allow(dead_code)]
fn contextual_empty_map_initializer_arena(
    arena: &ArenaProgram,
    initializer: ArenaExprOrRun,
    expected: &Type,
    actual: &Type,
) -> bool {
    if !matches!(
        (expected, actual),
        (Type::Map(_, _), Type::Map(_, item)) if matches!(item.as_ref(), Type::Any)
    ) {
        return false;
    }
    let ArenaExprOrRun::Expr(expr_id) = initializer else {
        return false;
    };
    let ArenaExprKind::Call { callee, args } = arena.arena.expr(expr_id).kind else {
        return false;
    };
    if !args.is_empty() {
        return false;
    }
    let ArenaExprKind::Field { base, name } = arena.arena.expr(callee).kind else {
        return false;
    };
    matches!(arena.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "map")
        && name == "empty"
}

#[allow(dead_code)]
fn should_record_binding_annotation_arena(
    arena: &ArenaProgram,
    target: BindingTargetId,
    ty: &Type,
    exported: bool,
) -> bool {
    let simple_name = match &arena.arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(name) => Some(*name),
        ArenaBindingTargetKind::Record { .. } => None,
    };
    let Some(name) = simple_name else {
        return false;
    };
    if name == "_" || matches!(ty, Type::Unit) || ty.annotation_source().is_none() {
        return false;
    }
    exported || annotation_type_is_nontrivial(ty)
}

// Explicit Result tails consume the complete annotation through branch and
// block boundaries. Their Unit success payload still leaves the Result as data.
pub(super) fn tail_expr_uses_result_context_arena(arena: &ArenaProgram, expr: ExprId) -> bool {
    match arena.arena.expr(expr).kind {
        ArenaExprKind::Capture(_)
        | ArenaExprKind::ContextScope { .. }
        | ArenaExprKind::TempDirScope { .. } => true,
        ArenaExprKind::Call { callee, .. } => matches!(arena.arena.expr(callee).kind,
            ArenaExprKind::Ident(name) if name == "Ok" || name == "Err"),
        ArenaExprKind::ValueBlock(block) | ArenaExprKind::ErrorContext { block, .. } => arena
            .arena
            .stmt_ids(arena.arena.block(block).statements)
            .last()
            .is_some_and(|tail| tail_stmt_uses_result_context_arena(arena, tail)),
        ArenaExprKind::Match { arms, .. } => arena
            .arena
            .match_expr_arms(arms)
            .iter()
            .any(|arm| tail_expr_uses_result_context_arena(arena, arm.value)),
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            tail_expr_uses_result_context_arena(arena, else_value)
                || arena
                    .arena
                    .if_expr_branches(branches)
                    .iter()
                    .any(|branch| tail_expr_uses_result_context_arena(arena, branch.value))
        }
        _ => false,
    }
}

fn tail_stmt_uses_result_context_arena(arena: &ArenaProgram, stmt: StmtId) -> bool {
    let block_uses_result = |block| {
        arena
            .arena
            .stmt_ids(arena.arena.block(block).statements)
            .last()
            .is_some_and(|tail| tail_stmt_uses_result_context_arena(arena, tail))
    };
    match arena.arena.stmt(stmt).kind {
        ArenaStmtKind::Sugar { expansion, .. } => {
            tail_stmt_uses_result_context_arena(arena, expansion)
        }
        ArenaStmtKind::Expr(expr) => tail_expr_uses_result_context_arena(arena, expr),
        ArenaStmtKind::Match { arms, .. } => arena
            .arena
            .match_arms(arms)
            .iter()
            .any(|arm| block_uses_result(arm.block)),
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            else_block.is_some_and(block_uses_result)
                || arena
                    .arena
                    .if_branches(branches)
                    .iter()
                    .any(|branch| block_uses_result(branch.block))
        }
        _ => false,
    }
}

fn tail_expr_context_arena(
    arena: &ArenaProgram,
    expr_id: ExprId,
    expected: Option<&Type>,
) -> Option<Type> {
    let expected = expected?;
    let explicit_result = tail_expr_uses_result_context_arena(arena, expr_id);
    Some(if explicit_result {
        expected.clone()
    } else {
        expected.result_ok().unwrap_or(expected).clone()
    })
}

/// `assert ` is inserted before the statement. An unbraced match arm ends at a
/// comma, which `assert` would read as its message separator, so such an arm
/// becomes a braced block.
fn bool_statement_assert_fix(source: &str, statement: Span) -> Option<FixHint> {
    let text = source.get(statement.range())?.trim_end();
    let Some(content) = text.strip_suffix(',') else {
        // Statement-start grouping is not needed after `assert`.
        if let Some(inner) = whole_paren_group(text) {
            return Some(FixHint::replacement(
                Span::new(
                    statement.source_id,
                    statement.start(),
                    statement.start() + text.len(),
                ),
                "insert `assert`",
                format!("assert {inner}"),
            ));
        }
        return Some(FixHint::replacement(
            Span::at(statement.source_id, statement.start()),
            "insert `assert`",
            "assert ",
        ));
    };
    let content = content.trim_end();
    if content.contains('#') {
        return None;
    }
    Some(FixHint::replacement(
        Span::new(
            statement.source_id,
            statement.start(),
            statement.start() + content.len(),
        ),
        "insert `assert` in a braced match arm",
        format!("{{ assert {content} }}"),
    ))
}

/// The contents of `text` when one pair of parentheses encloses all of it.
fn whole_paren_group(text: &str) -> Option<&str> {
    let tokens = crate::syntax::lexer::lex_spellings(text);
    if tokens.first()?.0 != crate::syntax::token::TokenTag::LParen
        || tokens.last()?.0 != crate::syntax::token::TokenTag::RParen
    {
        return None;
    }
    let mut depth = 0usize;
    for (index, (tag, _)) in tokens.iter().enumerate() {
        match tag {
            crate::syntax::token::TokenTag::LParen => depth += 1,
            crate::syntax::token::TokenTag::RParen => {
                depth -= 1;
                if depth == 0 && index + 1 != tokens.len() {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some(text[1..text.len() - 1].trim())
}

fn statement_tail_needs_value_context_arena(arena: &ArenaProgram, expr: ExprId) -> bool {
    match arena.arena.expr(expr).kind {
        ArenaExprKind::ValueBlock(_)
        | ArenaExprKind::ErrorContext { .. }
        | ArenaExprKind::Require { schema: None, .. } => true,
        ArenaExprKind::Try(inner) => statement_tail_needs_value_context_arena(arena, inner),
        _ => false,
    }
}

fn record_target_requires_schema_check(
    arena: &ArenaProgram,
    target: BindingTargetId,
    ty: &Type,
) -> bool {
    match arena.arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(_) => false,
        ArenaBindingTargetKind::Record { fields, .. } => match ty {
            Type::Any => true,
            Type::Record(schema) => arena.arena.destructure_fields(fields).iter().any(|field| {
                schema
                    .get(&field.name)
                    .is_some_and(|ty| record_target_requires_schema_check(arena, field.target, ty))
            }),
            _ => false,
        },
    }
}

pub(super) struct CopyUpdateMistake {
    message: String,
    repair: String,
}

/// Collections are values, so `.push`, `.set`, and `.remove` return an updated
/// copy and leave the receiver unchanged. A discarded call of one of them is
/// almost always a mutation written in another language's style.
fn copy_update_mistake(
    arena: &ArenaProgram,
    expr_id: ExprId,
    ty: &Type,
) -> Option<CopyUpdateMistake> {
    let ArenaExprKind::Call { callee, .. } = arena.arena.expr(expr_id).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.arena.expr(callee).kind else {
        return None;
    };
    let name = name.as_str();
    let name: &str = name.as_ref();
    let ArenaExprKind::Ident(receiver) = arena.arena.expr(base).kind else {
        return None;
    };
    let collection = match ty {
        Type::List(_) => "list",
        Type::Map(_, _) => "map",
        _ => return None,
    };
    let repair = match (collection, name) {
        ("list", "push") => format!("append in place with `{receiver} += [value]` on a `var`"),
        ("list", "extend") => format!("append in place with `{receiver} += other` on a `var`"),
        ("map", "push" | "set" | "remove") => {
            format!("assign the result back with `{receiver} = {receiver}.{name}(...)` on a `var`")
        }
        _ => return None,
    };
    Some(CopyUpdateMistake {
        message: format!(
            "`.{name}` returns a new {collection} and leaves `{receiver}` unchanged; this statement discards it"
        ),
        repair,
    })
}
