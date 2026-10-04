pure object_name(src: Path) -> Path { src.with_ext("o") }

proc compile(src: Path, out: Path) [process, error] {
  run cc -c $src -o $out
}

stream lines_of(paths: List[Path]) [fs, error] -> Stream[Str] {
  for file in paths {
    yield @file.lines()?
  }
}
