match 1 {
  1 => {
    let value = ctx "let initializer" {
      {
        print "let nested block"
      }
      1
    }
  }
  else => {}
}
match 2 {
  2 => {
    var value = ctx "var initializer" {
      {
        print "var nested block"
      }
      2
    }
  }
  else => {}
}
var assigned = 0
match 3 {
  3 => {
    assigned = ctx "assignment" {
      {
        print "assignment nested block"
      }
      3
    }
  }
  else => {}
}
assert assigned == 3
match 4 {
  4 => {
    {
      print "direct arm block"
    }
  }
  else => {}
}
