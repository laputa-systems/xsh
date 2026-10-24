##! Independent right guarded handlers.

## Renders a present value or the right null marker.
export pure render(value) -> Str {
  if value != null { f"right:{value}" } else { "right:null" }
}

## Forwards a value to the right guarded renderer.
export pure relay(value) -> Str { render(value) }

## Preserves the selected path domain of a present value.
export pure parent(value) { if value != null { value.parent() } else { null } }

## Forwards the selected path result.
export pure relay_parent(value) { parent(value) }
