##! Typed reference for guarded operations and callbacks.

## A closed callback with observable I/O and failure propagation.
export type Observer = proc(value: Int) [io, error] -> Result[Int, Error]

stream retained(value: Int) [] -> Stream[Int] {
  let rows = [1] |> map { |item| value + item }
  yield @rows
}
stream zero() [] -> Stream[Int] { yield 0 }

## Adds only after the typed value is known to be present.
export pure typed_arithmetic(value: Int?) -> Int? {
  if value != null { value + 1 } else { null }
}

## Selects a parent only after the typed path is known to be present.
export pure typed_parent(value: Path?) -> Path? {
  if value == null { null } else { value.parent() }
}

## Invokes a present typed callback and preserves its failure.
export proc typed_invoke(action: Observer?, value: Int) [io, error] -> Result[Int?, Error] {
  if action != null { action(value)? } else { null }
}

## Retains a typed guarded payload in a suspended producer after guard exit.
export proc typed_capture(value: Int?) [] -> Result[Stream[Int], Error] {
  if value != null { Ok(retained(value)) } else { Ok(zero()) }
}

