##! Exports a generic identity function and a marker for import ordering.

## Returns its input without changing its inferred type.
export pure public_identity(value) {
  value
}

## Projects the required name field.
export pure public_name(value) {
  value.name
}

## Provides a stable value alongside the generic export.
export let marker = 17
