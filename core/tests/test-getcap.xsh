use core.lib.capability

test test_file_capability_revisions_round_trip_and_reject_invalid { |ctx|
  for revision in [1, 2, 3] {
    let fixture = capability.parse("cap_chown,cap_net_raw=ep cap_setuid=ei", rootid: if revision == 3 { 1000 } else { null })?
    let record: capability.FileCaps = {revision: capability.revision(revision)?, effective: fixture.effective, permitted: fixture.permitted, inheritable: fixture.inheritable, rootid: fixture.rootid}
    let payload = capability.encode(record)?
    assert payload.len() == (if revision == 1 { 12 } else if revision == 2 { 20 } else { 24 })
    assert capability.decode(payload)? == record
    assert capability.format(record) == "cap_setuid=ei cap_chown,cap_net_raw+ep"
  }
  assert capability.decode(b"\0") is Err(_)
  assert capability.decode(b"\x02\0\0\x02\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0") is Err(_)
  assert capability.parse("cap_unknown=ep") is Err(_)
  assert capability.parse("cap_chown=e") is Err(_)
  assert capability.parse("cap_chown=pe cap_net_raw=p") is Err(_)
}

test test_file_capability_preserves_all_64_payload_bits { |ctx|
  let payload = b"\x01\0\0\x02\0\0\0\0\0\0\0\0\0\0\0\x80\0\0\0\0"
  let record = capability.decode(payload)?
  assert record.permitted.high == 2147483648
  assert capability.encode(record)? == payload
}

test test_file_capability_format_preserves_combined_noneffective_sets { |ctx|
  let record = capability.parse("cap_chown=ip")?
  assert capability.format(record) == "cap_chown=ip"
  assert capability.parse(capability.format(record))? == record
  assert capability.format(capability.parse("")?) == "="
}
