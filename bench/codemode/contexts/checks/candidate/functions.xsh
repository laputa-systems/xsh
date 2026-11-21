pure twice(x: Int) -> Int {
  x * 2
}

proc listing(dir: Path) {
  fs.children(dir)? |> count()
}

tempdir dir {
  p"x".write("1") unless false
  print f"{twice(4)} {listing(dir) is Ok(_)}"
}
print f"{args.len()} {args.join("+")}"
