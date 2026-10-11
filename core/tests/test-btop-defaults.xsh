use core.lib.btop_defaults as defaults

type LedgerDefault = {key: Str, value: Any}
type Ledger = {defaults: List[LedgerDefault]}

test test_btop_default_constants_match_the_ledger_extraction {
  let root = fs.cwd()?
  let ledger = json.decode(fp"{root}/dev/btop-progress.json".read_text()?)?.require(Ledger)?
  let actual = defaults.records()

  assert actual.len() == ledger.defaults.len()
  for index in range(actual.len()) {
    let entry = actual[index]
    var expected_matches = 0
    for expected in ledger.defaults {
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
