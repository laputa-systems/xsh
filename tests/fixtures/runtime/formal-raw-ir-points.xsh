type Point = {x: Int, y: Int}

pure taxicab(point: Point) -> Int {
  let horizontal = if point.x < 0 { 0 - point.x } else { point.x }
  horizontal + point.y
}

print f"{taxicab({x: -3, y: 4})}"
