type Scale = pure(value: Int) -> Int
type RootWriter = proc(root: FsRoot) [fs, error] -> Result[Unit]
type RootBundle = {root: FsRoot, write: RootWriter}

pure twice(value: Int) -> Int {
  value * 2
}

pure thrice(value: Int) -> Int {
  value * 3
}

proc write_marker(root: FsRoot) [fs, error] {
  return root.write(p"marker", "kept")
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

  let _ = bundle.write(bundle.root)?
  assert bundle.root.read_text(p"marker")? == "kept"
}


pure count_ints(first: Int, ...items: List[Int]) -> Int {
  first + items.len()
}

pure join_texts(separator: Str, ...items: List[Str]) -> Str {
  items.join(separator)
}

pure keep_rest(f, first, ...items) {
  let _ = f(first, @items)
  f
}

test test_one_generic_callback_returned_for_two_instantiations_keeps_each_signature {
  let counter = keep_rest(count_ints, 1, 2, 3)
  let joiner = keep_rest(join_texts, "-", "x", "y")
  assert counter(10, 20, 30) == 12
  assert joiner("+", "p", "q") == "p+q"
}

test test_record_spread_and_rest_splice_into_returned_callable {
  let integer = keep_rest(count_ints, 1, 2, 3)
  let tail = [4, 5]
  assert integer(...{first: 7}, @tail) == 9
}

pure keep_plain(f, value) {
  let _ = f(value)
  f
}

test test_record_spread_into_returned_callable_without_a_rest_parameter {
  let doubled = keep_plain(twice, 1)
  assert doubled(...{value: 7}) == 14
}
