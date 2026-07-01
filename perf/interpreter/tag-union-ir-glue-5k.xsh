type Level = Info | Warn | Error | Debug

type Shape = Circle(Int) | Rect(Int, Int) | Point

pure classify(index: Int) -> Level {
  return match index % 4 { 0 => Info, 1 => Warn, 2 => Error, _ => Debug }
}

pure level_score(level: Level) -> Int {
  if level == Info {
    return 1
  }

  if level == Warn {
    return 2
  }

  if level == Error {
    return 3
  }

  4
}

pure area(shape: Shape) -> Int {
  match shape {
    Circle(r) => r * r * 3
    Rect(w, h) => w * h
    Point => 0
  }
}

var i = 0
var total = 0

while i < 5000 {
  let shape = if i % 3 == 0 { Circle(i % 11) } else { if i % 3 == 1 { Rect(i % 7 + 1, i % 5 + 2) } else { Point } }
  total += area(shape) + level_score(classify(i))
  i += 1
}

print (total % 256)
