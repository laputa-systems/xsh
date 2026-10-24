##! Compute values from present inputs.

## Adds one to a present value and preserves absence.
export pure add(value) {
  if value != null { value + 1 } else { null }
}

## Selects the registered parent result for a present path.
export pure parent(value) {
  if value == null { null } else { value.parent() }
}

## Preserves a present record's typed name.
export pure name(value) {
  if value != null { value.name } else { null }
}

## Preserves a conditional result through an immutable binding.
export pure bound(value) {
  let result = if value != null { value + 1 } else { null }
  result
}

## Forwards arithmetic output selection.
export pure relay_add(value) { add(value) }

## Forwards registered path output selection.
export pure relay_parent(value) { parent(value) }

## Forwards a typed record field output.
export pure relay_name(value) { name(value) }

## Forwards an expression-bound conditional result.
export pure relay_bound(value) { bound(value) }

## Forwards a conditional output through another generic boundary.
export pure repeated_add(value) { relay_add(value) }
