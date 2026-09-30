type Leaf = { value: Int }
type Node = { rows: List[Leaf] }

proc descend(depth: Int) [error] -> Result[Node] {
  if depth == 0 { return Ok({rows: [{value: 7}]}) }
  test.eq(descend(depth - 1)?.rows[0].value, 7)?
  Ok({rows: [{value: 7}]})
}

print descend(120)?.rows[0].value
