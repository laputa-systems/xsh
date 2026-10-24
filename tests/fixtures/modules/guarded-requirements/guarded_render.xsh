##! Render values only when they are present.

## Renders a present string and returns an empty string for null.
export pure typed_render(value: Str?) -> Str {
  if value == null { "" } else { f"{value}" }
}

## Renders a present value and returns an empty string for null.
export pure render(value) -> Str {
  if value == null { "" } else { f"{value}" }
}
