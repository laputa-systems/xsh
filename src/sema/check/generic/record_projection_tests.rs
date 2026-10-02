use super::super::{Checker, ExpressionIdentity};
use crate::sema::inference::{ScopedRoot, TypeNode};
use crate::source::SourceId;
use crate::syntax::arena::ArenaExprKind;
use crate::syntax::parser::Parser;

#[test]
fn top_level_fixed_record_projections_publish_the_original_receiver_and_selected_roots() {
    let source = "type Box[T] = {value: T, items: List[T] = []}\ntype Count = Box[Int]\npure retain(value: Box[Int]) -> Box[Int] { value }\nlet value = retain(Count(value: 7))\nlet items = value.items\nlet selected = value.value\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    checked.solved.validate().unwrap();
    let mut supplied = 0;
    for (origin, projection) in &checked.solved.projections {
        if !matches!(projection.field.as_str().as_str(), "items" | "value") { continue; }
        let ArenaExprKind::Field { base, name } = parsed.arena.arena.expr(origin.expression).kind else { panic!("the selected graph field retains an authored field expression"); };
        assert_eq!(name, projection.field);
        assert_eq!(checked.solved.expression_owners.get(origin), None);
        let receiver = ExpressionIdentity { expression: base, ..*origin };
        assert_eq!(checked.solved.expressions.get(&receiver), Some(&projection.receiver), "the projection preserves its original receiver root");
        assert_eq!(checked.solved.expressions.get(origin), Some(&projection.result), "the expression preserves the exact require_field result root");
        checked.solved.graph.validate_scoped(ScopedRoot { ty: projection.receiver, scope: None }).unwrap();
        checked.solved.graph.validate_scoped(ScopedRoot { ty: projection.result, scope: None }).unwrap();
        let TypeNode::Record(row) = checked.solved.graph.node(checked.solved.graph.resolved(projection.receiver).unwrap()).unwrap() else { panic!("the checked receiver remains a record row"); };
        let field = checked.solved.graph.row_data(*row).unwrap().fields.iter().find(|field| field.label == projection.field).unwrap();
        assert_eq!(checked.solved.graph.resolved(field.ty).unwrap(), checked.solved.graph.resolved(projection.result).unwrap());
        supplied += 1;
    }
    assert_eq!(supplied, 2, "both original top-level fields publish graph projections");
}
