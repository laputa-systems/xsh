use core.lib.btop_defaults as defaults

type UpstreamDefault = {key: Str, value: Any, upstream_ref: Str}
type UpstreamDefaults = {defaults: List[UpstreamDefault]}

test test_btop_default_constants_match_upstream {
  let root = fs.cwd()?
  let upstream = json.decode(fp"{root}/core/tests/data/btop/defaults.json".read_text()?)?.require(UpstreamDefaults)?
  let actual = defaults.records()

  assert actual.len() == upstream.defaults.len()
  for index in range(actual.len()) {
    let entry = actual[index]
    var expected_matches = 0
    for expected in upstream.defaults {
      if expected.key == entry.key {
        assert expected.value == entry.value
        expected_matches += 1
      }
    }
    assert expected_matches == 1
    for prior in range(index) {
      assert actual[prior].key != entry.key
    }
  }
}
