use core.lib.dhcp_machine as machine
use core.lib.dhcp_net as network
use core.lib.dhcp_store as store

const MAC = b"\x02\x00\x00\x00\x00\x01"
const BOOT = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0"

pure sample_lease() -> machine.Lease {
  machine.Lease(
    address: 3232235876,
    prefix: 24,
    broadcast: 3232236031,
    routers: [3232235777],
    dns: [3232235777, 167772161],
    domain: "example.org",
    server: 3232235777,
    lease_secs: 600,
    acquired_at: 5000,
    renew_at: 305000,
    rebind_at: 530000,
    expire_at: 605000,
    timers: machine.Server,
  )
}

pure sample_owned() -> network.Owned {
  {
    ifindex: 3,
    addresses: [{address: 3232235876, prefix: 24, broadcast: 3232236031}],
    routes: [{destination: 0, prefix: 0, gateway: 3232235777, table: 254, tos: 0, priority: 100, protocol: 16}],
  }
}

pure sample_record() -> store.LeaseRecord {
  store.LeaseRecord(
    name: "eth0",
    ifindex: 3,
    mac: MAC,
    client_id: bytes.concat([b"\x01", MAC]),
    boot_id: BOOT,
    boottime_ms: 5000,
    lease: sample_lease(),
    owned: sample_owned(),
    reconcile: true,
  )
}

pure current(index: Int, boot: Str, mac: Bytes) -> store.Current {
  {name: "eth0", ifindex: index, mac: mac, boot_id: boot}
}

test test_a_record_round_trips_exactly {
  let record = sample_record()
  assert store.decode(store.encode(record)?)? == record
  let idle = {...record, lease: null, owned: network.empty_owned(3), reconcile: false}
  assert store.decode(store.encode(idle)?)? == idle
  let infinite = {...record, lease: {...sample_lease(), renew_at: null, rebind_at: null, expire_at: null, lease_secs: 4294967295, timers: machine.Infinite}}
  assert store.decode(store.encode(infinite)?)? == infinite
}

test test_the_document_is_versioned_and_names_addresses_as_text {
  let text = store.encode(sample_record())?
  assert "\"version\":1" in text and "\"address\":\"192.168.1.100\"" in text and "\"mac\":\"020000000001\"" in text
}

pure filler(count: Int) -> Str {
  ["x" for _ in range(count)].join("")
}

pure edited(text: Str, from: Str, to: Str) -> Str {
  text.replace(from, with: to)
}

test test_malformed_documents_are_typed_rejections_never_partial_records {
  let good = store.encode(sample_record())?
  let cases = [
    ["version", edited(good, "\"version\":1", "\"version\":2")],
    ["version", edited(good, "\"version\":1", "\"version\":\"1\"")],
    ["schema", edited(good, "\"boot_id\":", "\"boot\":")],
    ["unknown", edited(good, "\"version\":1", "\"version\":1,\"extra\":true")],
    ["unknown", edited(good, "\"reconcile\":true", "\"reconcile\":true,\"owned_by\":\"someone\"")],
    ["schema", edited(good, "\"prefix\":24,\"rebind_at\"", "\"prefix\":\"24\",\"rebind_at\"")],
    ["value", edited(good, "\"address\":\"192.168.1.100\",\"broadcast\"", "\"address\":\"192.168.1.1000\",\"broadcast\"")],
    ["value", edited(good, "\"server\":\"192.168.1.1\"", "\"server\":\"\"")],
    ["value", edited(good, "\"prefix\":24,\"rebind_at\"", "\"prefix\":33,\"rebind_at\"")],
    ["value", edited(good, "\"mac\":\"020000000001\"", "\"mac\":\"0200\"")],
    ["value", edited(good, "\"mac\":\"020000000001\"", "\"mac\":\"02000000000g\"")],
    ["value", edited(good, "\"name\":\"eth0\"", "\"name\":\"../x\"")],
    ["value", edited(good, "\"timers\":\"server\"", "\"timers\":\"fast\"")],
    ["value", edited(good, "\"ifindex\":3,\"lease\"", "\"ifindex\":4,\"lease\"")],
    ["json", "not json"],
    ["json", ""],
    ["schema", "[1]"],
    ["schema", "{\"version\":1}"],
  ]
  for case in cases {
    match store.decode(case[1]) {
      Ok(_) => assert false, f"{case[0]} was accepted: {case[1]}"
      Err(_) => {}
    }
  }
  if let Err(store.StoreReject.Version {found}) = store.decode(edited(good, "\"version\":1", "\"version\":2")) {
    assert found == 2
  } else {
    assert false, "an unknown version is its own rejection"
  }
  if let Err(store.StoreReject.Unknown) = store.decode(edited(good, "\"version\":1", "\"version\":1,\"extra\":true")) {} else {
    assert false, "an unknown field is its own rejection"
  }
}

test test_oversized_documents_and_lists_are_refused {
  let big = "{\"version\":1,\"pad\":\"" + filler(70000) + "\"}"
  if let Err(store.StoreReject.TooLarge {limit}) = store.decode(big) {
    assert limit == store.MAX_RECORD_BYTES
  } else {
    assert false, "an oversize document is refused before parsing"
  }
  let routers = [3232235777, 3232235778, 3232235779, 3232235780, 3232235781, 3232235782, 3232235783, 3232235784, 3232235785]
  let many = {...sample_record(), lease: {...sample_lease(), routers: routers}}
  assert store.decode(store.encode(many)?) is Err(_)
}

test test_revalidation_trusts_only_what_still_matches_this_boot_and_this_link {
  let record = sample_record()
  let same = store.revalidate(record, current(3, BOOT, MAC))
  assert same.same_boot and same.same_link and same.owned == sample_owned() and same.lease == sample_lease() and same.reconcile and same.note == ""

  let rebooted = store.revalidate(record, current(3, "another-boot", MAC))
  assert ! rebooted.same_boot and rebooted.owned == network.empty_owned(3) and rebooted.lease == sample_lease() and rebooted.note == "different_boot"

  let recreated = store.revalidate(record, current(8, BOOT, MAC))
  assert recreated.same_boot and ! recreated.same_link and recreated.owned == network.empty_owned(8) and recreated.note == "interface_recreated"

  let other_nic = store.revalidate(record, current(3, BOOT, b"\x02\x00\x00\x00\x00\x09"))
  assert other_nic.lease == null and other_nic.owned == network.empty_owned(3) and other_nic.note == "different_interface"

  let other_name = store.revalidate(record, {name: "eth1", ifindex: 3, mac: MAC, boot_id: BOOT})
  assert other_name.lease == null and other_name.note == "different_interface"

  let unknown_boot = store.revalidate({...record, boot_id: ""}, current(3, "", MAC))
  assert ! unknown_boot.same_boot, "an empty boot identity never matches"

  let none = store.revalidate(null, current(3, BOOT, MAC))
  assert none.lease == null and none.owned == network.empty_owned(3) and ! none.same_boot
}

test test_a_remembered_lease_counts_as_configured_only_when_its_owned_address_is_present {
  let revived = store.revalidate(sample_record(), current(3, BOOT, MAC))
  let present = {ifindex: 3, name: "eth0", mac: MAC, up: true, carrier: true, addresses: [{address: 3232235876, prefix: 24, broadcast: 3232236031}], routes: []}
  assert store.configured(revived, present)
  assert ! store.configured(revived, {...present, addresses: []})
  assert ! store.configured(revived, {...present, addresses: [{address: 3232235876, prefix: 16, broadcast: 0}]})
  let rebooted = store.revalidate(sample_record(), current(3, "another-boot", MAC))
  assert ! store.configured(rebooted, present)
}

test test_records_are_saved_privately_and_loaded_back { |ctx|
  let dir = test.temp_dir(ctx, name: "lease-store")?
  let root = fs.open_root(dir)?
  assert store.load(root, "eth0")? == null
  store.save(root, sample_record())
  assert store.load(root, "eth0")? == sample_record()
  assert root.stat(p"eth0.lease")?.mode.bit_and(0o7777) == 0o600
  store.save(root, {...sample_record(), reconcile: false})
  assert store.load(root, "eth0")? == {...sample_record(), reconcile: false}
  for entry in fs.children(dir)? {
    assert ".tmp." not in entry.name
  }
}

test test_loading_refuses_corrupt_oversized_and_misnamed_records { |ctx|
  let dir = test.temp_dir(ctx, name: "lease-corrupt")?
  let root = fs.open_root(dir)?
  fp"{dir}/eth0.lease".write("{\"version\":1")
  assert store.load(root, "eth0") is Err(_)
  fp"{dir}/eth0.lease".write(filler(store.MAX_RECORD_BYTES + 1))
  assert store.load(root, "eth0") is Err(_)
  assert store.load(root, "../etc") is Err(_)
  assert store.load(root, "") is Err(_)
  assert store.load(root, filler(16)) is Err(_)
  assert ! store.interface_name_ok("a b") and ! store.interface_name_ok("..") and store.interface_name_ok("eth0.100")
}

test test_saving_never_follows_or_replaces_a_symlink { |ctx|
  let dir = test.temp_dir(ctx, name: "lease-symlink")?
  let root = fs.open_root(dir)?
  fp"{dir}/elsewhere".write("keep")
  fp"{dir}/eth0.lease".symlink(to: p"elsewhere")
  assert store.save(root, sample_record()) is Err(_)
  assert fp"{dir}/elsewhere".read_text()? == "keep"
  assert fp"{dir}/eth0.lease".readlink()? == p"elsewhere"
}
