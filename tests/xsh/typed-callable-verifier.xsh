type Scale = pure(value: Int) -> Int
type RootWriter = proc(root: FsRoot) [fs, error] -> Result[Unit]
type RootBundle = {root: FsRoot, write: RootWriter}

pure twice(value: Int) -> Int {
  value * 2
}

pure thrice(value: Int) -> Int {
  value * 3
}

proc write_marker(root: FsRoot) [fs, error] -> Result[Unit] {
  root.write(p"marker", "kept")
}

pure callback_map() -> Map[Str, Scale] {
  let empty: Map[Str, Scale] = {}
  empty.set("twice", twice).set("thrice", thrice)
}

pure lookup_callback(callbacks: Map[Str, Scale], name: Str) -> Result[Scale] {
  Ok(callbacks.get(name) ?? twice)
}

proc make_root_bundle() [fs, error] -> Result[RootBundle] {
  let root = fs.tempdir()?
  Ok({root: root, write: write_marker})
}

test test_callable_lookup_and_try_preserve_callable_type {
  let callbacks = callback_map()
  let selected = callbacks.get("thrice") ?? twice
  assert selected(4) == 12

  let maybe: Scale? = selected
  let recovered = maybe ?? twice
  assert recovered(5) == 15

  let fetched = lookup_callback(callbacks, "missing")?
  assert fetched(6) == 12
}

test test_record_callable_and_resource_survive_returned_frame {
  let bundle = make_root_bundle()?
  defer bundle.root.close()

  bundle.write(bundle.root)?
  assert bundle.root.read_text(p"marker")? == "kept"
}

