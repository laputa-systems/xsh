##! Closed optional callback invocation.

## The callback's labels, result, and effect contract.
export type Observer = proc(value: Int) [io, error] -> Result[Int, Error]

## Invoke only a present callback and preserve its failure.
export proc invoke(action: Observer?, value: Int) [io, error] -> Result[Int?, Error] {
  if action != null { action(value)? } else { null }
}
