type BtopUpstream = {tag: Str, commit: Str}

type BtopAudit = {
  new_core_script: Str,
  launcher_argument_separator: Str,
  stdlib_multi_file_modules: Str,
  pty_test_wiring: Str,
  monotonic_clock: Str,
}

type BtopAssertion = {
  id: Str,
  gate: Str,
  implemented: Bool,
  status: Str,
  tests: List[Str],
  evidence: Str,
}

type BtopBlocker = {
  id: Str,
  kind: Str,
  reason: Str,
  reproducer: Str,
  unblock_command: Str,
}

type BtopContractChange = {kind: Str, name: Str, commit: Str, summary: Str}

type BtopDefault = {key: Str, value: Any, upstream_ref: Str, assertion: Str}

type BtopLedger = {
  schema: Int,
  xsh_start_commit: Str,
  upstream: BtopUpstream,
  gate: Str,
  last_green_commit: Str,
  audit: BtopAudit,
  assertions: List[BtopAssertion],
  blocked: List[BtopBlocker],
  contract_changes: List[BtopContractChange],
  limitations: List[Str],
  measurements: Any,
  defaults: List[BtopDefault],
  next: List[Str],
}

type BtopCoverageRow = {
  item: Str,
  class: Str,
  upstream_ref: Str,
  gate: Str,
  note: Str,
}

type BtopExcludedConfig = {item: Str, upstream_ref: Str, reason: Str}

type BtopCoverage = {
  upstream_version: Str,
  source_scope: Str,
  denominator_method: Str,
  total_rows: Int,
  excluded_internal_config_entry_count: Int,
  excluded_internal_config_entries: List[BtopExcludedConfig],
  rows: List[BtopCoverageRow],
}

pure gate_name(gate: Str) -> Bool {
  gate in ["G0", "G1", "G2", "G3", "G4a"]
}

pure scope_gate(gate: Str) -> Bool {
  gate in ["G1", "G2", "G3", "G4a"]
}

pure valid_status(status: Str) -> Bool {
  status in [
    "not_run",
    "failed",
    "passed",
    "blocked_external",
    "blocked_internal",
    "reference_unavailable",
    "unstable",
    "not_applicable",
  ]
}

test test_btop_ledger_and_coverage_are_current {
  let root = fs.cwd()?
  let ledger = json.decode(fp"{root}/dev/btop-progress.json".read_text()?)?.require(BtopLedger)?
  let coverage = json.decode(fp"{root}/dev/btop-coverage.json".read_text()?)?.require(BtopCoverage)?

  assert ledger.schema == 1
  assert ledger.xsh_start_commit != ""
  assert ledger.upstream.tag == "v1.4.7"
  assert ledger.upstream.commit == "6e39144aaf5a6bc01b9f795010b0914431067183"
  assert gate_name(ledger.gate)
  assert ledger.last_green_commit != ""
  assert ! ledger.next.is_empty()
  for audit_key in [
    "new_core_script",
    "launcher_argument_separator",
    "stdlib_multi_file_modules",
    "pty_test_wiring",
    "monotonic_clock",
  ] {
    assert ledger.audit.get(audit_key)? != ""
  }

  assert coverage.upstream_version == "btop v1.4.7"
  assert coverage.source_scope != ""
  assert coverage.denominator_method != ""
  assert coverage.total_rows == coverage.rows.len()
  assert coverage.excluded_internal_config_entry_count == coverage.excluded_internal_config_entries.len()
  assert coverage.total_rows == 265
  assert coverage.excluded_internal_config_entry_count == 21

  var cli_option_count = 0
  var config_key_count = 0
  var menu_count = 0
  var key_binding_count = 0
  var data_family_count = 0
  var g1_count = 0
  var g2_count = 0
  var g3_count = 0
  var g4a_count = 0
  var deferred_count = 0
  var out_of_scope_count = 0
  for row in coverage.rows {
    if row.class == "cli_option" { cli_option_count += 1 }
    if row.class == "config_key" { config_key_count += 1 }
    if row.class == "menu" { menu_count += 1 }
    if row.class == "key_binding" { key_binding_count += 1 }
    if row.class == "data_family" { data_family_count += 1 }
    if row.gate == "G1" { g1_count += 1 }
    if row.gate == "G2" { g2_count += 1 }
    if row.gate == "G3" { g3_count += 1 }
    if row.gate == "G4a" { g4a_count += 1 }
    if row.gate == "deferred" { deferred_count += 1 }
    if row.gate == "out_of_scope" { out_of_scope_count += 1 }
  }
  assert cli_option_count == 14
  assert config_key_count == 88
  assert menu_count == 14
  assert key_binding_count == 127
  assert data_family_count == 22
  assert g1_count == 13
  assert g2_count == 49
  assert g3_count == 26
  assert g4a_count == 47
  assert deferred_count == 58
  assert out_of_scope_count == 72

  let recorded_coverage_sha = json.get(ledger.measurements, ["coverage_sha256"])?.require(Str)?
  assert recorded_coverage_sha == hash.sha256(fp"{root}/dev/btop-coverage.json")?.hex()
  let g0_coverage_sha = json.get(ledger.measurements, ["coverage_g0_sha256"])?.require(Str)?
  let recorded_coverage_rows = json.get(ledger.measurements, ["coverage_row_count"])?.require(Int)?
  assert recorded_coverage_rows == coverage.total_rows
  if recorded_coverage_sha != g0_coverage_sha {
    let limitation_prefix = "coverage denominator changed from " + g0_coverage_sha + " to " + recorded_coverage_sha + ":"
    var explained = false
    for limitation in ledger.limitations {
      if limitation_prefix in limitation {
        explained = true
      }
    }
    assert explained
  }

  for row_index in range(coverage.rows.len()) {
    let row = coverage.rows[row_index]
    assert row.item != ""
    assert row.upstream_ref != ""
    assert row.note != ""
    assert row.class in ["cli_option", "config_key", "key_binding", "menu", "data_family"]
    assert row.gate in ["G1", "G2", "G3", "G4a", "deferred", "out_of_scope"]
    for prior_index in range(row_index) {
      let prior = coverage.rows[prior_index]
      assert prior.item != row.item or prior.class != row.class
    }
  }

  for excluded_index in range(coverage.excluded_internal_config_entries.len()) {
    let excluded = coverage.excluded_internal_config_entries[excluded_index]
    assert excluded.item != ""
    assert excluded.upstream_ref != ""
    assert excluded.reason != ""
    for row in coverage.rows {
      assert row.item != excluded.item
    }
    for prior_index in range(excluded_index) {
      assert coverage.excluded_internal_config_entries[prior_index].item != excluded.item
    }
  }

  for default_index in range(ledger.defaults.len()) {
    let default = ledger.defaults[default_index]
    assert default.key != ""
    assert default.upstream_ref != ""
    assert default.assertion != ""
    var assertion_matches = 0
    for assertion in ledger.assertions {
      if assertion.id == default.assertion {
        assertion_matches += 1
      }
    }
    assert assertion_matches == 1
    var row_matches = 0
    for row in coverage.rows {
      if row.class == "config_key" and row.item == default.key and scope_gate(row.gate) {
        row_matches += 1
      }
    }
    assert row_matches == 1
    for prior_index in range(default_index) {
      assert ledger.defaults[prior_index].key != default.key
    }
  }

  for row in coverage.rows {
    if row.class == "config_key" and scope_gate(row.gate) {
      var default_matches = 0
      for default in ledger.defaults {
        if default.key == row.item {
          default_matches += 1
        }
      }
      assert default_matches == 1
    }
  }

  for assertion_index in range(ledger.assertions.len()) {
    let assertion = ledger.assertions[assertion_index]
    assert assertion.id != ""
    assert gate_name(assertion.gate)
    assert valid_status(assertion.status)
    for prior_index in range(assertion_index) {
      assert ledger.assertions[prior_index].id != assertion.id
    }
    if assertion.status == "passed" {
      assert ! assertion.tests.is_empty()
      assert assertion.evidence != ""
    }
    if assertion.status != "not_run" {
      assert assertion.evidence != ""
    }
    if assertion.status == "blocked_external" or assertion.status == "blocked_internal" {
      var matching_blockers = 0
      for blocker in ledger.blocked {
        if blocker.id == assertion.id {
          matching_blockers += 1
          if assertion.status == "blocked_external" {
            assert blocker.kind == "external"
          } else {
            assert blocker.kind == "internal"
          }
        }
      }
      assert matching_blockers == 1
    }
    for test_reference in assertion.tests {
      let parts = test_reference.split("::")
      assert parts.len() == 2
      assert parts[0] != ""
      assert parts[1] != ""
      let source = fp"{root}/{parts[0]}".read_text()?
      if parts[0].ends_with(".xsh") {
        assert "test " + parts[1] + " {" in source
      } else if parts[0].ends_with(".rs") {
        assert "#[test]\nfn " + parts[1] + "(" in source
      } else {
        assert false, f"unsupported test reference file {parts[0]}"
      }
    }
  }

  for blocker in ledger.blocked {
    assert blocker.id != ""
    assert blocker.kind in ["external", "internal"]
    assert blocker.reason != ""
    assert blocker.reproducer != ""
    assert blocker.unblock_command != ""
  }
}
