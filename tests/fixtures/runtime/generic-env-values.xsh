type EnvScalar = Union[Str, Int, Bool, Path, Duration]
var env_cleanup_calls = 0
proc direct_env_value(value) [env, error] -> Result[Str] {
  env ({XSH_DIRECT_ENV_VALUE: value}) { env.get("XSH_DIRECT_ENV_VALUE")? }?
}
proc direct_env_paths(values) [env, error] -> Result[Str] {
  env ({XSH_DIRECT_ENV_VALUE: values}) { env.get("XSH_DIRECT_ENV_VALUE")? }?
}
proc direct_env_map(value) [env, error] -> Result[Str] {
  env ({["XSH_DIRECT_ENV_VALUE"]: value}) { env.get("XSH_DIRECT_ENV_VALUE")? }?
}
proc direct_env_pair(left, right) [env, error] -> Result[Str] {
  env ({XSH_DIRECT_ENV_VALUE: left, XSH_DIRECT_ENV_OTHER: right}) {
    env.get("XSH_DIRECT_ENV_VALUE")? + ":" + env.get("XSH_DIRECT_ENV_OTHER")?
  }?
}
proc observe_env(value) [env, error] -> Result[List[Str]] {
  let before = env.get_or("XSH_DIRECT_ENV_VALUE", "outer")?
  var deferred = ""
  let inside = env ({XSH_DIRECT_ENV_VALUE: value}) {
    defer { deferred = env.get("XSH_DIRECT_ENV_VALUE")? }
    env.get("XSH_DIRECT_ENV_VALUE")?
  }?
  [before, inside, deferred, env.get_or("XSH_DIRECT_ENV_VALUE", "outer")?]
}
proc fail_env(value) [env, error] -> Result[Unit] {
  env ({XSH_DIRECT_ENV_VALUE: value}) {
    defer { assert env.get("XSH_DIRECT_ENV_VALUE")? == "failure"; env_cleanup_calls += 1 }
    assert false
  }?
}
stream env_items() [env, error] -> Stream[Str] { yield env.get("XSH_DIRECT_ENV_VALUE")? }
proc collect_env(value) [env, error] -> Result[List[Str]] {
  env ({XSH_DIRECT_ENV_VALUE: value}) { env_items() |> collect() }?
}
proc verify_generic_env_values() [env, error] -> Result[Unit] {
  let before = env.get_or("XSH_DIRECT_ENV_VALUE", "outer")?
  assert direct_env_value("kept")? == "kept"
  assert direct_env_value(-7)? == "-7"
  assert direct_env_value(1KiB)? == "1024"
  assert direct_env_value(true)? == "true"
  assert direct_env_value(p"/one")? == "/one"
  assert direct_env_value(1s)? == "1s"
  let paths: List[Path] = [p"/one", p"/two"]
  let empty: List[Path] = []
  assert direct_env_paths(paths)? == "/one:/two"
  assert direct_env_paths(empty)? == ""
  assert direct_env_map("mapped")? == "mapped"
  let invalid: List[Path] = [p"/one:two"]
  let invalid_result = try { direct_env_paths(invalid)? }
  assert invalid_result is Err(_)
  assert env.get_or("XSH_DIRECT_ENV_VALUE", "outer")? == before
  assert direct_env_pair("left", "right")? == "left:right"
  assert observe_env("inside")? == [before, "inside", "inside", before]
  let failure = try { fail_env("failure")? }
  assert failure is Err(_)
  assert env_cleanup_calls == 1
  assert env.get_or("XSH_DIRECT_ENV_VALUE", "outer")? == before
  assert collect_env("stream")? == ["stream"]
  assert env.get_or("XSH_DIRECT_ENV_VALUE", "outer")? == before
}
