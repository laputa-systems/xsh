##! Exports the same generic identity API with declarations in a different order.

## Provides a stable value alongside the generic export.
export let marker = 17

## Projects the required name field.
export pure public_name(record) {
  record.name
}

## Returns its input without changing its inferred type.
export pure public_identity(item) {
  item
}
