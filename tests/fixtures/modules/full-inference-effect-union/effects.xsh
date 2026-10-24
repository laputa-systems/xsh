##! Independent higher-order effect boundaries.

## Runs both callbacks while preserving both effect requirements.
export proc pair(first, second, value) {
  let left = first(value)
  let right = second(value)
  {left, right}
}
