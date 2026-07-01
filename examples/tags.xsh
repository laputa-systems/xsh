type Level =
    Info
  | Warn
  | Error
  | Debug
  | Trace

type Shape =
    Circle(Int)
  | Rect(Int, Int)
  | Point
  | Square(Int)
  | Triangle(Int, Int)

pure level_label(l: Level) -> Str {
  if l == Info {
    return "INFO"
  }

  if l == Warn {
    return "WARN"
  }

  if l == Error {
    return "ERROR"
  }

  if l == Trace {
    return "TRACE"
  }

  "DEBUG"
}

pure area(s: Shape) -> Int {
  match s {
    Circle(r) => r * r * 3
    Rect(w, h) => w * h
    Point => 0
    Square(side) => side * side
    Triangle(base, height) => base * height / 2
  }
}

print level_label(Info) level_label(Error)
print area(Circle(4)) area(Rect(3, 5)) area(Point)
