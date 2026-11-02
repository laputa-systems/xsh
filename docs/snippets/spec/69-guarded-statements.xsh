proc stage(src: Path, tmp: Path, verbose: Bool) -> Result[Int] {
  var copied = 0
  # begin example
  print f"copying {src}" when verbose
  tmp.remove() when tmp.exists()
  copied = 1 unless src == tmp
  # end example
  Ok(copied)
}

print ${stage(p"/tmp/guarded-src", p"/tmp/guarded-tmp", true)?}
