use xsh::frontend::{check::Checker,syntax::parser::Parser,source::SourceId,query::SolvedQuery};
fn main() {
 for path in std::env::args().skip(1) {
  let text=std::fs::read_to_string(&path).unwrap();
  let parsed=Parser::parse_source_arena_only(SourceId::new(0),&text);
  println!("SOURCE={path}");
  if !parsed.diagnostics.is_empty(){println!("PARSE={:?}",parsed.diagnostics);continue;}
  let _symbols=parsed.arena.symbol_owner().enter();
  let checked=Checker::check_arena(&parsed.arena,&text);
  println!("DIAGNOSTICS={:?}",checked.diagnostics);
  println!("VALIDATE={:?}",checked.solved.validate());
  let query=SolvedQuery::new(&checked.solved,parsed.arena.symbol_owner());
  for (id,_) in &checked.solved.declarations {
   println!("DECLARATION={} {:?}",parsed.arena.arena.function_def(id.declaration).name,query.declaration(*id));
  }
  for (id,fact) in &checked.solved.bindings {
   println!("BINDING={:?} scheme={} {:?}",parsed.arena.arena.binding_target(id.target).kind,fact.scheme.is_some(),query.binding(*id));
  }
  for (id,call) in &checked.solved.calls {
   println!("CALL={:?} declaration={:?} {:?}",parsed.arena.arena.expr(id.expression).kind,call.declaration,query.expression(*id));
  }
 }
}
