error FetchError = Busy | Gone(url: Str)

proc parse(text: Str) -> Result[Int] {
  let n = text as Int
  n * 2
}

proc first_ok(texts: List[Str]) -> Result[Int] {
  for text in texts {
    guard let n = parse(text) else {
      continue
    }

    return n
  }

  Err(FetchError.Gone(url: "nowhere"))
}

proc writes(dir: Path) -> Result[Str] {
  fp"{dir}/missing/x".write("x")
  "unreachable"
}

let doubled = parse("21")?
let fallback = parse("x") ?? -1
let captured = try { parse("y")? }
print f"{doubled} {fallback} {captured is Err(_)} {first_ok(["a", "4"])?}"
match first_ok(["a"]) {
  Ok(_) => print "found"
  Err(FetchError.Gone {url}) => print f"gone: {url}"
  Err(error) => print f"other: {error.message}"
}
let busy = FetchError.Busy("try later")
print (busy.message)
tempdir dir {
  print f"statement failure: {writes(dir) is Err(_)}"
}
let maybe: Str? = if doubled > 40 { "big" } else { null }
if maybe != null {
  print (maybe.byte_len())
}
let ok = false
exit 4 unless ok
