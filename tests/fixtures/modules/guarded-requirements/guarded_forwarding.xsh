##! Forward presence-dependent requirements through generic calls.

use guarded_render
use guarded_preload

## Renders a present value and preserves null absence.
export pure render(value) -> Str {
  guarded_render.render(value)
}

## Renders through two generic call boundaries.
export pure repeated_render(value) -> Str {
  render(value)
}

## Observes a present value inside a temporary environment overlay.
export proc preload(value) [env, error] -> Result[Str, Error] {
  guarded_preload.preload(value)
}
