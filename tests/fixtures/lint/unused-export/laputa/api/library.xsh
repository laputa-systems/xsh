##! Published library.

## A value consumed outside this project.
export const published_value = 5

## A helper consumed outside this project.
export pure published_helper() -> Int {
  return published_value
}

## An unreferenced value the published interface still promises.
export const dead_api_value = 1

## An unreferenced helper the published interface still promises.
export pure dead_api_helper() -> Int {
  return 2
}
