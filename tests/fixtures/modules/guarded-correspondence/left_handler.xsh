##! Independent left guarded handlers.

## Renders a present value or the left null marker.
export pure render(value) -> Str {
  if value == null { "left:null" } else { f"left:{value}" }
}

## Forwards a value to the left guarded renderer.
export pure relay(value) -> Str { render(value) }

## Preserves the selected path domain of a present value.
export pure parent(value) { if value == null { null } else { value.parent() } }

## Forwards the selected path result.
export pure relay_parent(value) { parent(value) }
