pure parent_transform(location) { location.parent() }
pure normalize_transform(location) { location.normalize() }
pure relay_parent(location) { parent_transform(location) }
pure path_text(location: Path) -> Str { location.display() }
pure consumed_parent(location) {
  let first = parent_transform(location)
  path_text(first)
}
pure ordinary_parent(location: Path) -> Path { location.parent() }
pure ordinary_relative_parent(location: RelPath) -> RelPath { location.parent() }
pure ordinary_normalize(location: Path) -> Path { location.normalize() }
pure ordinary_relative_normalize(location: RelPath) -> RelPath { location.normalize() }

proc path_sink(location: Path) [] { let _ = location }
proc prepare_parent_chain(location) {
  let first = location.parent()
  let second = first.parent()
  let third = second.parent()
  path_sink(first)
  path_sink(second)
  path_sink(third)
}

proc observed_receiver() [io] -> Result[Path] {
  print "receiver"
  p"/a/b"
}

pure written_parent(location) -> Path { location.parent() }
pure written_normal(location) -> Path { location.normalize() }
proc written_proc_parent(location) -> Result[Path] { location.parent() }
proc written_proc_normal(location) -> Result[Path] { location.normalize() }
pure relative_parent(location) -> RelPath { location.parent() }

test generic_path_transforms_preserve_canonical_results {
  let absolute: Path = p"/a/./b"
  let relative: RelPath = p"a/./b"
  let parent_absolute: Path = parent_transform(absolute)
  let parent_relative: RelPath = parent_transform(relative)
  let normal_absolute: Path = normalize_transform(absolute)
  let normal_relative: RelPath = normalize_transform(relative)
  assert parent_absolute == ordinary_parent(absolute)
  assert parent_relative == ordinary_relative_parent(relative)
  assert normal_absolute == ordinary_normalize(absolute)
  assert normal_relative == ordinary_relative_normalize(relative)
  let relayed: RelPath = relay_parent(relative)
  assert relayed == parent_relative
  assert consumed_parent(absolute) == ordinary_parent(absolute).display()
  assert consumed_parent(relative) == ordinary_relative_parent(relative).display()
  assert prepare_parent_chain(absolute) == Ok()
  assert prepare_parent_chain(relative) == Ok()
  let observed: Path = parent_transform(observed_receiver()?)
  assert observed == p"/a"
  assert written_parent(absolute) == parent_absolute
  assert written_parent(relative) == parent_relative
  assert written_normal(absolute) == normal_absolute
  assert written_normal(relative) == normal_relative
  assert written_proc_parent(absolute) == Ok(parent_absolute)
  assert written_proc_parent(relative) == Ok(parent_relative)
  assert written_proc_normal(absolute) == Ok(normal_absolute)
  assert written_proc_normal(relative) == Ok(normal_relative)
  assert relative_parent(relative) == parent_relative
}
