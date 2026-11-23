##! Profile module.
use shared
## A computed value.
export let value = shared.calculate(40)
## Return the value.
export pure answer() -> Int { value }
