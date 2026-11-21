use lib

let inside = lib.fail_here()
print f"built in lib, tested in main: {inside is Err(is lib.Transient)}"
let outside: Result[Int, lib.LibError] = Err(lib.LibError.Down("outside"))
print f"built in main, tested in main: {outside is Err(is lib.Transient)}"
match outside {
  Err(error) => print f"built in main, tested in lib: {lib.has_facet(error)}"
  Ok(_) => print "ok"
}
match inside {
  Err(error) => print f"built in lib, tested in lib: {lib.has_facet(error)}"
  Ok(_) => print "ok"
}
