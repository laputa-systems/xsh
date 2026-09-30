proc compile(src: Path) -> Result[Unit] {
  print $src
}

compile(p"main.c")?
