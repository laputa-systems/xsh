use system_report_check as report_checks

type ReportCoverageAssertion = {
  id: Str,
  domain: Str,
  field: Str,
  relation: Str,
  tier: Str,
  source_abi: Str,
  reference_adapter: Str,
  reference_commands: List[List[Str]],
  eligibility: Str,
  equality_rule: Str,
  fixture_scenarios: List[Str],
}

type ReportFixtureCase = {scenario: Str, tests: List[Str]}
type KernelParameterReferenceFixture = {name: Str, source: Str, state: Str, value: Str?, raw_bytes_base64: Str?}

type ReportCoverageManifest = {
  schema_version: Int,
  producer: Str,
  assertions: List[ReportCoverageAssertion],
  fixture_cases: List[ReportFixtureCase],
  macos_fixture_cases: List[ReportFixtureCase],
  fixture_scenarios: List[Str],
}

pure assertion(id: Str, tier: Str) -> ReportCoverageAssertion {
  return {
    id: id,
    domain: "cpu",
    field: "frequency_policy.related_cpus",
    relation: "membership",
    tier: tier,
    source_abi: "/sys/devices/system/cpu/cpufreq/policy*/related_cpus",
    reference_adapter: "lscpu-json",
    reference_commands: [["lscpu", "--json"]],
    eligibility: "CPUFreq policy exists and is readable",
    equality_rule: "exact CPU ID set",
    fixture_scenarios: ["offline_related_cpu"],
  }
}

pure manifest(assertions: List[ReportCoverageAssertion]) -> ReportCoverageManifest {
  return {
    schema_version: 4,
    producer: "system-report",
    assertions: assertions,
    fixture_cases: [],
    macos_fixture_cases: [],
    fixture_scenarios: ["offline_related_cpu"],
  }
}

proc test_system_report_ip_link_reference_scores_stable_identity_and_state() [error] {
  let ip_output = """[{"ifindex":2,"ifname":"eth0","flags":["BROADCAST","MULTICAST","UP","LOWER_UP"],"mtu":1500,"operstate":"UP","link_type":"ether"},{"ifindex":1,"ifname":"lo","flags":["LOOPBACK","UP"],"mtu":65536,"operstate":"UNKNOWN","link_type":"loopback"}]"""
  let reference = report_checks.parse_ip_link_json(ip_output)?
  test.eq(reference.len(), 2)?
  test.eq(reference[0].ifindex, 2)?
  test.eq(reference[0].name, "eth0")?
  test.eq(reference[0].admin_up, true)?
  test.eq(reference[0].operstate, "up")?
  test.ok(report_checks.ip_link_reference_stable(reference, [reference[1], reference[0]]))?
  test.ok(!report_checks.ip_link_reference_stable(reference, [{...reference[0], mtu: 1400}, reference[1]]))?
  let reordered_candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":1,"name":{"state":"observed","value":"lo"},"mtu":65536,"admin_up":true,"operational_state":"unknown","kind":null,"master_ifindex":null,"lower_ifindex":null},{"ifindex":2,"name":{"state":"observed","value":"eth0"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":null,"master_ifindex":null,"lower_ifindex":null}]}}"""
  let exact = report_checks.compare_ip_links(reordered_candidate, reference)?
  test.ok(exact.exact)?
  test.eq(exact.matched_count, 2)?
  let partial = report_checks.compare_ip_links(reordered_candidate.replace("\"complete\"", "\"partial\""), reference)?
  test.ok(!partial.candidate_field_missing)?
  test.ok(partial.exact)?
  let incomplete = report_checks.compare_ip_links(reordered_candidate.replace("\"enumeration_succeeded\":true", "\"enumeration_succeeded\":false"), reference)?
  test.ok(incomplete.candidate_field_missing)?
  test.ok(!incomplete.exact)?
  let changed_candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":1,"name":{"state":"observed","value":"lo"},"mtu":65536,"admin_up":true,"operational_state":"unknown","kind":null,"master_ifindex":null,"lower_ifindex":null},{"ifindex":3,"name":{"state":"observed","value":"eth0"},"mtu":1400,"admin_up":false,"operational_state":"down","kind":null,"master_ifindex":null,"lower_ifindex":null}]}}"""
  let changed = report_checks.compare_ip_links(changed_candidate, reference)?
  test.ok(!changed.exact)?
  test.eq(changed.missing_ids, [2])?
  test.eq(changed.unexpected_ids, [3])?
  let field_changed_candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"name":{"state":"observed","value":"eth0"},"mtu":1400,"admin_up":false,"operational_state":"down","kind":null,"master_ifindex":null,"lower_ifindex":null},{"ifindex":1,"name":{"state":"observed","value":"lo"},"mtu":65536,"admin_up":true,"operational_state":"unknown","kind":null,"master_ifindex":null,"lower_ifindex":null}]}}"""
  let fields = report_checks.compare_ip_links(field_changed_candidate, reference)?
  test.eq(fields.field_mismatches, ["2.mtu", "2.admin_up", "2.operational_state"])?
  test.ok(!fields.exact)?
  let unknown_state = """[{"ifindex":7,"ifname":"vlan7","flags":["UP"],"mtu":1500,"operstate_index":77}]"""
  test.eq(report_checks.parse_ip_link_json(unknown_state)?[0].operstate, "operstate_77")?
  let malformed = """[{"ifindex":2,"ifname":"eth0","flags":["UP"],"mtu":1500},{"ifindex":2,"ifname":"eth1","flags":[],"mtu":1500}]"""
  test.error_kind(report_checks.parse_ip_link_json(malformed), "SystemReportCheckError.Invalid")?
  let duplicate_name = """[{"ifindex":2,"ifname":"eth0","flags":[],"mtu":1500},{"ifindex":3,"ifname":"eth0","flags":[],"mtu":1500}]"""
  test.error_kind(report_checks.parse_ip_link_json(duplicate_name), "SystemReportCheckError.Invalid")?

  let typed_reference = report_checks.parse_ip_link_json("""[{"ifindex":5,"ifname":"eth0.42","flags":["UP"],"mtu":1500,"operstate":"UP","linkinfo":{"info_kind":"vlan"}}]""")?
  test.eq(typed_reference[0].kind, "vlan")?
  test.ok(!report_checks.ip_link_reference_stable(typed_reference, [{...typed_reference[0], kind: "bridge"}]))?
  test.error_kind(report_checks.parse_ip_link_json("""[{"ifindex":5,"ifname":"eth0.42","flags":["UP"],"mtu":1500,"linkinfo":{"info_kind":""}}]"""), "SystemReportCheckError.Invalid")?
  let typed_candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":5,"name":{"state":"observed","value":"eth0.42"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":"vlan","master_ifindex":null,"lower_ifindex":null}]}}"""
  test.ok(report_checks.compare_ip_links(typed_candidate, typed_reference)?.exact)?
  let wrong_kind = report_checks.compare_ip_links(typed_candidate.replace("\"kind\":\"vlan\"", "\"kind\":\"bridge\""), typed_reference)?
  test.ok(wrong_kind.field_mismatches == ["5.kind"])?
  test.ok(!wrong_kind.exact)?
}

proc test_system_report_ip_link_reference_checks_master_relationship() [error] {
  let reference = report_checks.parse_ip_link_json("""[{"ifindex":6,"ifname":"br0","flags":["UP"],"mtu":1500,"operstate":"UP","linkinfo":{"info_kind":"bridge"}},{"ifindex":5,"ifname":"eth0","flags":["UP"],"mtu":1500,"operstate":"UP","master":"br0"}]""")?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":5,"name":{"state":"observed","value":"eth0"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":null,"master_ifindex":6,"lower_ifindex":null},{"ifindex":6,"name":{"state":"observed","value":"br0"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":"bridge","master_ifindex":null,"lower_ifindex":null}]}}"""
  test.ok(report_checks.compare_ip_links(candidate, reference)?.exact)?
  let wrong_parent = report_checks.compare_ip_links(candidate.replace("\"master_ifindex\":6", "\"master_ifindex\":7"), reference)?
  test.ok(wrong_parent.field_mismatches == ["5.master_ifindex"])?
  test.ok(!wrong_parent.exact)?
  let unresolved_master = report_checks.parse_ip_link_json("""[{"ifindex":5,"ifname":"eth0","flags":["UP"],"mtu":1500,"operstate":"UP","master":"missing"}]""")?
  test.error_kind(report_checks.compare_ip_links(candidate, unresolved_master), "SystemReportCheckError.Invalid")?
}

proc test_system_report_ip_link_reference_checks_lower_link_relationship() [error] {
  let reference = report_checks.parse_ip_link_json("""[{"ifindex":2,"ifname":"eth0","flags":["UP"],"mtu":1500,"operstate":"UP"},{"ifindex":5,"ifname":"eth0.42","flags":["UP"],"mtu":1500,"operstate":"UP","link":"eth0","linkinfo":{"info_kind":"vlan"}}]""")?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":5,"name":{"state":"observed","value":"eth0.42"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":"vlan","master_ifindex":null,"lower_ifindex":2},{"ifindex":2,"name":{"state":"observed","value":"eth0"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":null,"master_ifindex":null,"lower_ifindex":null}]}}"""
  test.ok(report_checks.compare_ip_links(candidate, reference)?.exact)?
  let wrong_lower = report_checks.compare_ip_links(candidate.replace("\"lower_ifindex\":2", "\"lower_ifindex\":7"), reference)?
  test.ok(wrong_lower.field_mismatches == ["5.lower_ifindex"])?
  let numeric_reference = report_checks.parse_ip_link_json("""[{"ifindex":5,"ifname":"veth0","flags":["UP"],"mtu":1500,"operstate":"UP","link_index":27,"link_netnsid":1}]""")?
  let numeric_candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":5,"name":{"state":"observed","value":"veth0"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":null,"master_ifindex":null,"lower_ifindex":27}]}}"""
  test.ok(report_checks.compare_ip_links(numeric_candidate, numeric_reference)?.exact)?
  let ambiguous_link = """[{"ifindex":5,"ifname":"veth0","flags":["UP"],"mtu":1500,"link":"eth0","link_index":27}]"""
  test.error_kind(report_checks.parse_ip_link_json(ambiguous_link), "SystemReportCheckError.Invalid")?
  let unresolved_lower = report_checks.parse_ip_link_json("""[{"ifindex":5,"ifname":"eth0.42","flags":["UP"],"mtu":1500,"link":"missing"}]""")?
  test.error_kind(report_checks.compare_ip_links(candidate, unresolved_lower), "SystemReportCheckError.Invalid")?
}

proc test_system_report_ip_address_reference_preserves_ipv6_identity_and_link_membership() [error] {
  let ip_output = """[{"ifindex":2,"ifname":"eth0","addr_info":[{"family":"inet","local":"192.0.2.10","prefixlen":24,"broadcast":"192.0.2.255","scope":"global","valid_life_time":300,"preferred_life_time":120},{"family":"inet6","local":"2001:db8::10","prefixlen":64,"scope":"global","valid_life_time":240,"preferred_life_time":100}]},{"ifindex":3,"ifname":"eth0.42","addr_info":[{"family":"inet6","local":"2001:db8:42::5","prefixlen":64,"scope":"link"}]}]"""
  let reference = report_checks.parse_ip_address_json(ip_output)?
  test.eq(reference.len(), 3)?
  test.eq(reference[1].ifindex, 2)?
  test.eq(reference[1].family, "ipv6")?
  test.eq(reference[1].address, "2001:db8::10")?
  test.eq(reference[2].ifindex, 3)?
  let later = report_checks.parse_ip_address_json(ip_output.replace("\"valid_life_time\":300", "\"valid_life_time\":299"))?
  test.ok(report_checks.ip_address_reference_stable(reference, later))?
  let moved = report_checks.parse_ip_address_json(ip_output.replace("2001:db8:42::5", "2001:db8:42::6"))?
  test.ok(!report_checks.ip_address_reference_stable(reference, moved))?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":3,"addresses":[{"family":"ipv6","address":{"state":"observed","value":"2001:db8:42::5"},"prefix_length":64,"broadcast":{"state":"absent","value":null},"scope":"link"}]},{"ifindex":2,"addresses":[{"family":"ipv6","address":{"state":"observed","value":"2001:db8::10"},"prefix_length":64,"broadcast":{"state":"absent","value":null},"scope":"global"},{"family":"ipv4","address":{"state":"observed","value":"192.0.2.10"},"prefix_length":24,"broadcast":{"state":"observed","value":"192.0.2.255"},"scope":"global"}]}]}}"""
  let exact = report_checks.compare_ip_addresses(candidate, reference)?
  test.ok(exact.exact_static)?
  test.eq(exact.matched_count, 3)?
  test.ok(report_checks.compare_ip_addresses(candidate.replace("\"complete\"", "\"partial\""), reference)?.exact_static)?
  test.ok(report_checks.compare_ip_addresses(candidate.replace("\"enumeration_succeeded\":true", "\"enumeration_succeeded\":false"), reference)?.candidate_field_missing)?
  let missing = candidate.replace("2001:db8:42::5", "2001:db8:42::6")
  let changed = report_checks.compare_ip_addresses(missing, reference)?
  test.ok(!changed.exact_static)?
  test.eq(changed.missing_keys.len(), 1)?
  test.eq(changed.unexpected_keys.len(), 1)?
  let wrong_scope = report_checks.compare_ip_addresses(candidate.replace("\"scope\":\"link\"", "\"scope\":\"host\""), reference)?
  test.eq(wrong_scope.field_mismatches.len(), 1)?
  test.ok(!wrong_scope.exact_static)?
  let malformed = """[{"ifindex":2,"ifname":"eth0","addr_info":[{"family":"inet6","local":"2001:db8::10","prefixlen":129,"scope":"global"}]}]"""
  test.error_kind(report_checks.parse_ip_address_json(malformed), "SystemReportCheckError.Invalid")?
  let unsupported = """[{"ifindex":2,"ifname":"eth0","addr_info":[{"family":"mpls","local":"100","prefixlen":20,"scope":"global"}]}]"""
  test.error_kind(report_checks.parse_ip_address_json(unsupported), "SystemReportCheckError.Invalid")?
}

proc test_system_report_ip_rule_reference_keeps_family_and_static_selectors() [error] {
  let ipv4 = report_checks.parse_ip_rule_json("""[{"priority":0,"src":"all","table":"local"},{"priority":100,"src":"192.0.2.0","srclen":24,"dst":"all","fwmark":"0x7","fwmask":"0xff","iif":"eth0","table":"main"}]""", "ipv4")?
  let ipv6 = report_checks.parse_ip_rule_json("""[{"priority":101,"src":"2001:db8::","srclen":64,"table":"1000"}]""", "ipv6")?
  let reference = ipv4.extend(ipv6)
  test.eq(reference.len(), 3)?
  test.eq(reference[0].table, 255)?
  test.eq(reference[1].source_prefix_length, 24)?
  test.eq(reference[2].family, "ipv6")?
  test.eq(report_checks.parse_ip_rule_json("""[{"priority":2,"src":"all","nop":null}]""", "ipv4")?[0].action, "nop")?
  test.eq(report_checks.parse_ip_rule_json("""[{"priority":3,"src":"all","fwmark":"0x7","fwmask":"0xffffffff"}]""", "ipv4")?[0].fwmask, null)?
  test.error_kind(report_checks.parse_ip_rule_json("""[{"priority":4,"src":"all","fwmark":7}]""", "ipv4"), "schema")?
  test.ok(report_checks.ip_rule_reference_stable(reference, [reference[2], reference[0], reference[1]])?)?
  test.ok(!report_checks.ip_rule_reference_stable(reference, [reference[0], reference[1]])?)?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"name":{"state":"observed","value":"eth0"}}],"rules":[{"family":"ipv6","priority":101,"source":{"state":"observed","value":"2001:db8::"},"source_prefix_length":64,"destination":{"state":"absent","value":null},"destination_prefix_length":0,"fwmark":null,"fwmask":null,"table":1000,"action":"to_table","input_ifindex":null,"output_ifindex":null},{"family":"ipv4","priority":100,"source":{"state":"observed","value":"192.0.2.0"},"source_prefix_length":24,"destination":{"state":"absent","value":null},"destination_prefix_length":0,"fwmark":7,"fwmask":255,"table":254,"action":"to_table","input_ifindex":2,"output_ifindex":null},{"family":"ipv4","priority":0,"source":{"state":"absent","value":null},"source_prefix_length":0,"destination":{"state":"absent","value":null},"destination_prefix_length":0,"fwmark":null,"fwmask":null,"table":255,"action":"to_table","input_ifindex":null,"output_ifindex":null}]}}"""
  let exact = report_checks.compare_ip_rules(candidate, reference)?
  test.ok(exact.exact_static)?
  test.eq(exact.matched_count, 3)?
  test.ok(report_checks.compare_ip_rules(candidate.replace("\"priority\":0", "\"priority\":null"), reference)?.exact_static)?
  let wrong_mark = report_checks.compare_ip_rules(candidate.replace("\"fwmark\":7", "\"fwmark\":8"), reference)?
  test.ok(!wrong_mark.exact_static)?
  test.eq(wrong_mark.missing_keys.len(), 1)?
  test.eq(wrong_mark.unexpected_keys.len(), 1)?
  let partial = report_checks.compare_ip_rules(candidate.replace("\"complete\"", "\"partial\""), reference)?
  test.ok(partial.exact_static)?
  test.ok(report_checks.compare_ip_rules(candidate.replace("\"enumeration_succeeded\":true", "\"enumeration_succeeded\":false"), reference)?.candidate_field_missing)?
  test.error_kind(report_checks.parse_ip_rule_json("""[{"priority":1,"src":"all","table":"main"},{"priority":1,"src":"all","table":"main"}]""", "ipv4"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_ip_rule_json("[]", "unspec"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_ip_route_reference_keeps_family_table_and_link_identity() [error] {
  let ipv4 = report_checks.parse_ip_route_json("""[{"dst":"default","gateway":"192.0.2.1","dev":"eth0","metric":100},{"type":"local","dst":"192.0.2.10","dev":"eth0","table":"local","scope":"host","protocol":"kernel"}]""", "ipv4")?
  let ipv6 = report_checks.parse_ip_route_json("""[{"dst":"2001:db8:42::/64","dev":"eth0.42","table":"1000","protocol":"static","metric":20}]""", "ipv6")?
  let reference = ipv4.extend(ipv6)
  test.eq(reference[0].destination, "0.0.0.0")?
  test.eq(reference[0].prefix_length, 0)?
  test.eq(reference[0].table, 254)?
  test.eq(reference[1].prefix_length, 32)?
  test.eq(reference[2].family, "ipv6")?
  test.eq(report_checks.parse_ip_route_json("""[{"dst":"2001:db8::/64","protocol":"ra"}]""", "ipv6")?[0].protocol, "router_advertisement")?
  test.ok(report_checks.ip_route_reference_stable(reference, [reference[2], reference[0], reference[1]])?)?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"name":{"state":"observed","value":"eth0"}},{"ifindex":3,"name":{"state":"observed","value":"eth0.42"}}],"routes":[{"family":"ipv6","destination":{"state":"observed","value":"2001:db8:42::"},"source":{"state":"absent","value":null},"source_prefix_length":0,"preferred_source":{"state":"absent","value":null},"prefix_length":64,"gateway":{"state":"absent","value":null},"table":1000,"metric":20,"route_type":"unicast","scope":"global","protocol":"static","flags":0,"nexthops":[],"output_ifindex":3},{"family":"ipv4","destination":{"state":"observed","value":"192.0.2.10"},"source":{"state":"absent","value":null},"source_prefix_length":0,"preferred_source":{"state":"absent","value":null},"prefix_length":32,"gateway":{"state":"absent","value":null},"table":255,"metric":null,"route_type":"local","scope":"host","protocol":"kernel","flags":0,"nexthops":[],"output_ifindex":2},{"family":"ipv4","destination":{"state":"observed","value":"0.0.0.0"},"source":{"state":"absent","value":null},"source_prefix_length":0,"preferred_source":{"state":"absent","value":null},"prefix_length":0,"gateway":{"state":"observed","value":"192.0.2.1"},"table":254,"metric":100,"route_type":"unicast","scope":"global","protocol":"boot","flags":0,"nexthops":[],"output_ifindex":2}]}}"""
  let exact = report_checks.compare_ip_routes(candidate, reference)?
  test.ok(exact.exact_static)?
  test.eq(exact.matched_count, 3)?
  test.ok(report_checks.compare_ip_routes(candidate.replace("\"complete\"", "\"partial\""), reference)?.exact_static)?
  test.ok(report_checks.compare_ip_routes(candidate.replace("\"enumeration_succeeded\":true", "\"enumeration_succeeded\":false"), reference)?.candidate_field_missing)?
  let changed = report_checks.compare_ip_routes(candidate.replace("\"metric\":100", "\"metric\":101"), reference)?
  test.ok(!changed.exact_static)?
  test.eq(changed.missing_keys.len(), 1)?
  test.eq(changed.unexpected_keys.len(), 1)?
  test.error_kind(report_checks.parse_ip_route_json("""[{"dst":"2001:db8::/129"}]""", "ipv6"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_ip_route_json("""[{"dst":"default"},{"dst":"default"}]""", "ipv4"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_ip_route_json("[]", "unspec"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_ip_route_reference_scores_source_and_multipath_hops() [error] {
  let output = """[{"dst":"2001:db8::/64","from":"2001:db8:1::/64","prefsrc":"2001:db8::10","table":"1000","flags":["notify"],"nexthops":[{"gateway":"2001:db8::1","dev":"eth0","weight":2,"flags":["onlink"]},{"gateway":"2001:db8::2","dev":"eth1","weight":1,"flags":[]}]}]"""
  let reference = report_checks.parse_ip_route_json(output, "ipv6")?
  test.eq(reference[0].source, "2001:db8:1::")?
  test.eq(reference[0].source_prefix_length, 64)?
  test.eq(reference[0].preferred_source, "2001:db8::10")?
  test.eq(reference[0].flags, 256)?
  test.eq(reference[0].nexthops.len(), 2)?
  test.eq(reference[0].nexthops[0].weight, 2)?
  test.eq(reference[0].nexthops[0].flags, 4)?
  let zero_source = report_checks.parse_ip_route_json("""[{"dst":"2001:db8::/64","src":"0/64"}]""", "ipv6")?
  test.eq(zero_source[0].source, null)?
  test.eq(zero_source[0].source_prefix_length, 64)?
  let reordered = report_checks.parse_ip_route_json(output.replace("\"nexthops\":[{\"gateway\":\"2001:db8::1\",\"dev\":\"eth0\",\"weight\":2,\"flags\":[\"onlink\"]},{\"gateway\":\"2001:db8::2\",\"dev\":\"eth1\",\"weight\":1,\"flags\":[]}]", "\"nexthops\":[{\"gateway\":\"2001:db8::2\",\"dev\":\"eth1\",\"weight\":1,\"flags\":[]},{\"gateway\":\"2001:db8::1\",\"dev\":\"eth0\",\"weight\":2,\"flags\":[\"onlink\"]}]"), "ipv6")?
  test.ok(report_checks.ip_route_reference_stable(reference, reordered)?)?
  let changed_flags = report_checks.parse_ip_route_json(output.replace("\"onlink\"", "\"offload\""), "ipv6")?
  test.ok(!report_checks.ip_route_reference_stable(reference, changed_flags)?)?
  let changed_route_flags = report_checks.parse_ip_route_json(output.replace("\"notify\"", "\"rt_offload\""), "ipv6")?
  test.ok(!report_checks.ip_route_reference_stable(reference, changed_route_flags)?)?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"name":{"state":"observed","value":"eth0"}},{"ifindex":3,"name":{"state":"observed","value":"eth1"}}],"routes":[{"family":"ipv6","destination":{"state":"observed","value":"2001:db8::"},"prefix_length":64,"source":{"state":"observed","value":"2001:db8:1::"},"source_prefix_length":64,"preferred_source":{"state":"observed","value":"2001:db8::10"},"gateway":{"state":"absent","value":null},"table":1000,"metric":null,"route_type":"unicast","scope":"global","protocol":"boot","flags":256,"output_ifindex":null,"nexthops":[{"ifindex":3,"hops":0,"flags":0,"gateway":{"state":"observed","value":"2001:db8::2"}},{"ifindex":2,"hops":1,"flags":4,"gateway":{"state":"observed","value":"2001:db8::1"}}]}]}}"""
  test.ok(report_checks.compare_ip_routes(candidate, reference)?.exact_static)?
  let wrong_source = report_checks.compare_ip_routes(candidate.replace("2001:db8:1::", "2001:db8:2::"), reference)?
  test.ok(!wrong_source.exact_static)?
  let wrong_source_prefix = report_checks.compare_ip_routes(candidate.replace("\"source_prefix_length\":64", "\"source_prefix_length\":63"), reference)?
  test.ok(!wrong_source_prefix.exact_static)?
  let wrong_preferred_source = report_checks.compare_ip_routes(candidate.replace("2001:db8::10", "2001:db8::11"), reference)?
  test.ok(!wrong_preferred_source.exact_static)?
  let wrong_gateway = report_checks.compare_ip_routes(candidate.replace("2001:db8::1", "2001:db8::3"), reference)?
  test.ok(!wrong_gateway.exact_static)?
  let wrong_weight = report_checks.compare_ip_routes(candidate.replace("\"hops\":1", "\"hops\":2"), reference)?
  test.ok(!wrong_weight.exact_static)?
  test.ok(!report_checks.compare_ip_routes(candidate.replace("\"flags\":256", "\"flags\":0"), reference)?.exact_static)?
  test.ok(report_checks.compare_ip_routes(candidate.replace("\"flags\":256", "\"flags\":512"), reference)?.candidate_field_missing)?
  let missing_hop = report_checks.compare_ip_routes(candidate.replace("},{\"ifindex\":2,\"hops\":1,\"flags\":4,\"gateway\":{\"state\":\"observed\",\"value\":\"2001:db8::1\"}}", "}"), reference)?
  test.ok(!missing_hop.exact_static)?
  test.ok(!report_checks.compare_ip_routes(candidate.replace("\"flags\":4", "\"flags\":8"), reference)?.exact_static)?
  let unknown_candidate_flag = report_checks.compare_ip_routes(candidate.replace("\"flags\":4", "\"flags\":128"), reference)?
  test.ok(unknown_candidate_flag.candidate_field_missing)?
  test.error_kind(report_checks.parse_ip_route_json(output.replace("\"weight\":2", "\"weight\":0"), "ipv6"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_ip_route_json(output.replace("\"onlink\"", "\"future_flag\""), "ipv6"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_ip_route_json(output.replace("\"onlink\"", "\"onlink\",\"onlink\""), "ipv6"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_ip_route_json(output.replace("\"notify\"", "\"future_flag\""), "ipv6"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_cpu_set_capture_validates_saved_reference() [fs, time, error] {
  let source = fs.tempdir()?
  defer fs.close_root(source)?
  let bundle = fs.tempdir()?
  defer fs.close_root(bundle)?
  fs.root_mkdir(source, p"sys/devices/system/cpu", parents: true)?
  fs.root_write(source, p"sys/devices/system/cpu/possible", "0-2\n")?
  fs.root_write(source, p"sys/devices/system/cpu/present", "0,2\n")?
  fs.root_write(source, p"sys/devices/system/cpu/online", "0\n")?
  fs.root_write(source, p"sys/devices/system/cpu/offline", "2\n")?
  report_checks.capture_cpu_set_bundle(source, bundle, "synthetic_fixture")?
  test.eq(report_checks.validate_cpu_set_bundle(bundle)?.present, [0, 2])?
  fs.root_write(bundle, p"sys/devices/system/cpu/present", "0,2 ")?
  test.error_kind(report_checks.validate_cpu_set_bundle(bundle), "SystemReportCheckError.Invalid")?
  fs.root_write(bundle, p"sys/devices/system/cpu/present", "0,2\n")?
  test.error_kind(report_checks.capture_cpu_set_bundle(source, bundle, "synthetic_fixture"), "SystemReportCheckError.Invalid")?
  fs.root_write(source, p"sys/devices/system/cpu/present", "1")?
  test.eq(report_checks.validate_cpu_set_bundle(bundle)?.present, [0, 2])?
  fs.root_write(bundle, p"sys/devices/system/cpu/present", "0,1\n")?
  test.error_kind(report_checks.validate_cpu_set_bundle(bundle), "SystemReportCheckError.Invalid")?
}

proc test_system_report_cpu_set_capture_replays_raw_sources() [fs, time, error] {
  let source = fs.tempdir()?
  defer fs.close_root(source)?
  let bundle = fs.tempdir()?
  defer fs.close_root(bundle)?
  fs.root_mkdir(source, p"sys/devices/system/cpu", parents: true)?
  fs.root_write(source, p"sys/devices/system/cpu/possible", "0-4\n")?
  fs.root_write(source, p"sys/devices/system/cpu/present", "2,4\n")?
  fs.root_write(source, p"sys/devices/system/cpu/online", "2\n")?
  fs.root_write(source, p"sys/devices/system/cpu/offline", "4\n")?
  report_checks.capture_cpu_set_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_cpu_set_bundle(bundle)?
  test.ok(replay.exact)?
  test.eq(replay.present.reference_count, 2)?
  test.eq(replay.present.candidate_count, 2)?
  let raw = fs.root_read_result(bundle, p"sys/devices/system/cpu/present")?
  test.eq(raw.data, b"2,4\n")?
  fs.root_write(source, p"sys/devices/system/cpu/present", "0")?
  test.ok(report_checks.replay_cpu_set_bundle(bundle)?.exact)?
  fs.root_write(bundle, p"sys/devices/system/cpu/present", "0,4\n")?
  test.error_kind(report_checks.replay_cpu_set_bundle(bundle), "SystemReportCheckError.Invalid")?
}

proc test_system_report_cpu_set_capture_records_missing_source() [fs, time, error] {
  let source = fs.tempdir()?
  defer fs.close_root(source)?
  let bundle = fs.tempdir()?
  defer fs.close_root(bundle)?
  fs.root_mkdir(source, p"sys/devices/system/cpu", parents: true)?
  fs.root_write(source, p"sys/devices/system/cpu/possible", "0")?
  report_checks.capture_cpu_set_bundle(source, bundle, "synthetic_fixture")?
  test.contains(fs.root_read_text(bundle, p"capture.json")?, "\"state\": \"absent\"")?
  test.contains(fs.root_read_text(bundle, p"capture.json")?, "\"reference\": null")?
  test.error_kind(report_checks.replay_cpu_set_bundle(bundle), "SystemReportCheckError.Invalid")?
}

proc test_system_report_memory_capture_validates_raw_sources_and_oracles() [fs, time, error] {
  let source = fs.tempdir()?
  defer fs.close_root(source)?
  let bundle = fs.tempdir()?
  defer fs.close_root(bundle)?
  fs.root_mkdir(source, p"proc", parents: true)?
  fs.root_mkdir(source, p"sys/kernel/mm/transparent_hugepage", parents: true)?
  fs.root_write(source, p"proc/meminfo", "MemTotal: 16 kB\nMemFree: 4 kB\n")?
  fs.root_write(source, p"sys/kernel/mm/transparent_hugepage/enabled", "always [madvise] never\n")?
  report_checks.capture_memory_bundle(source, bundle, "synthetic_fixture")?
  let reference = report_checks.validate_memory_bundle(bundle)?
  test.eq(reference.meminfo.len(), 2)?
  test.eq(reference.thp.len(), 1)?
  test.eq(reference.thp[0].name, "enabled")?
  let metadata = fs.root_read_text(bundle, p"capture.json")?
  test.contains(metadata, "\"stable\": true")?
  fs.root_write(bundle, p"capture.json", metadata.replace("\"stable\": true", "\"stable\": false"))?
  test.eq(report_checks.validate_memory_bundle(bundle)?.meminfo.len(), 2)?
  fs.root_write(source, p"proc/meminfo", "MemTotal: 64 kB\n")?
  test.eq(report_checks.validate_memory_bundle(bundle)?.meminfo[0].value, reference.meminfo[0].value)?
  fs.root_write(bundle, p"proc/meminfo", "MemTotal: 16 kB\nMemFree: 5 kB\n")?
  test.error_kind(report_checks.validate_memory_bundle(bundle), "SystemReportCheckError.Invalid")?
  fs.root_write(bundle, p"proc/meminfo", "MemTotal: 16 kB\nMemFree: 4 kB\n")?
  test.eq(report_checks.validate_memory_bundle(bundle)?.thp.len(), 1)?
  fs.root_write(bundle, p"sys/kernel/mm/transparent_hugepage/defrag", "[always] never\n")?
  test.error_kind(report_checks.validate_memory_bundle(bundle), "SystemReportCheckError.Invalid")?
}

proc test_system_report_memory_capture_replays_raw_sources() [fs, time, error] {
  let source = fs.tempdir()?
  defer fs.close_root(source)?
  let bundle = fs.tempdir()?
  defer fs.close_root(bundle)?
  fs.root_mkdir(source, p"proc", parents: true)?
  fs.root_mkdir(source, p"sys/kernel/mm/transparent_hugepage", parents: true)?
  fs.root_write(source, p"proc/meminfo", "MemTotal: 16 kB\nMemFree: 4 kB\n")?
  fs.root_write(source, p"sys/kernel/mm/transparent_hugepage/enabled", "always [madvise] never\n")?
  report_checks.capture_memory_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_memory_bundle(bundle)?
  test.ok(replay.meminfo.exact_scored)?
  test.ok(replay.thp.exact)?
}

proc test_system_report_proc_stat_reference_preserves_identity_and_rejects_unsafe_fields() [error] {
  let stat = "123 (worker) pool) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2\n"
  let parsed = report_checks.parse_proc_stat_identity_reference(stat)?
  test.eq(parsed.pid, 123)?
  test.eq(parsed.command, "worker) pool")?
  test.eq(parsed.parent_pid, 1)?
  test.eq(parsed.state, "S")?
  test.eq(parsed.start_ticks, 100)?
  test.eq(report_checks.parse_proc_stat_thread_reference(stat)?.thread_count, 2)?
  test.error_kind(report_checks.parse_proc_stat_identity_reference(stat.replace("123 (", "0x7b (")), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_stat_identity_reference(stat.replace("100 8192 2", "9007199254740992 8192 2")), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_stat_thread_reference(stat.replace("2 0 100", "9007199254740992 0 100")), "SystemReportCheckError.Invalid")?
  let valid_identity = report_checks.parse_proc_stat_identity_reference(stat.replace("100 8192 2", "100 8192 9007199254740992"))?
  test.eq(valid_identity.pid, 123)?
  test.eq(valid_identity.start_ticks, 100)?
  test.eq(valid_identity.command, "worker) pool")?
  let valid_threads = report_checks.parse_proc_stat_thread_reference(stat.replace("100 8192 2", "100 9007199254740992 9007199254740992"))?
  test.eq(valid_threads.thread_count, 2)?
  test.eq(valid_threads.start_ticks, 100)?
  test.error_kind(report_checks.parse_proc_stat_identity_reference(stat.replace("100 8192 2", "9007199254740992 8192 2")), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_stat_identity_reference("123 (worker) S 1 1 1\n"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_proc_status_uid_reference_requires_one_numeric_row() [error] {
  test.eq(report_checks.parse_proc_status_uid_reference("Name:\tworker\nUid:\t1000\t1001\t1001\t1001\n")?, 1000)?
  test.error_kind(report_checks.parse_proc_status_uid_reference("Name:\tworker\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_status_uid_reference("Uid:\t1000\t1001\t1001\t1001\nUid:\t2000\t2000\t2000\t2000\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_status_uid_reference("Uid:\t0x3e8\t1001\t1001\t1001\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_status_uid_reference("Uid:\t9007199254740992\t1001\t1001\t1001\n"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_proc_statm_reference_uses_reported_page_size_and_exact_bytes() [error] {
  let parsed = report_checks.parse_proc_statm_reference("2 1 0 0 0 0 0\n", 65536)?
  test.eq(parsed.virtual_bytes, 131072)?
  test.eq(parsed.resident_bytes, 65536)?
  test.error_kind(report_checks.parse_proc_statm_reference("2 1 0 0 0 0 0", 0), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_statm_reference("137438953472 1 0 0 0 0 0", 65536), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_statm_reference("2 9007199254740992 0 0 0 0 0", 4096), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_statm_reference("2 0x1 0 0 0 0 0", 4096), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_statm_reference("2 1", 4096), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_statm_reference("2 1 malformed 0 0 0 0", 4096), "SystemReportCheckError.Invalid")?
  test.eq(report_checks.parse_proc_statm_reference("2 1 9007199254740992 0 0 0 0", 4096)?.resident_bytes, 4096)?
}

proc test_system_report_proc_cgroup_reference_requires_one_absolute_v2_path() [error] {
  test.eq(report_checks.parse_proc_cgroup_reference("0::/tenant/worker\n2:cpu:/legacy\n")?, "/tenant/worker")?
  test.eq(report_checks.parse_proc_cgroup_reference("2:cpu:/legacy\n")?, null)?
  test.error_kind(report_checks.parse_proc_cgroup_reference("0::relative\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_cgroup_reference("0::/first\n0::/second\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_cgroup_reference("0:/missing-controller:/path\n"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_meminfo_reference_preserves_units_and_rejects_ambiguous_rows() [error] {
  let source = "MemTotal: 16 kB\nMemFree:\t4\tkB\nHugePages_Total: 2\nVendorCounter: 12 widgets\n"
  let counters = report_checks.parse_meminfo_reference(source)?
  test.eq(counters.len(), 4)?
  test.ok(counters |> any .name == "MemTotal" and .value == 16384 and .unit == "bytes")?
  test.ok(counters |> any .name == "MemFree" and .value == 4096 and .unit == "bytes")?
  test.ok(counters |> any .name == "HugePages_Total" and .value == 2 and .unit == "count")?
  test.ok(counters |> any .name == "VendorCounter" and .value == 12 and .unit == "widgets")?
  test.error_kind(report_checks.parse_meminfo_reference("MemTotal: 16 MB\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_meminfo_reference("MemTotal: 16 kB\nMemTotal: 32 kB\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_meminfo_reference("MemTotal: 8796093022208 kB\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_meminfo_reference("MemTotal: 16 kB\nBrokenRow\n"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_meminfo_comparison_scores_stable_fields_and_host_projection() [error] {
  let before = report_checks.parse_meminfo_reference("MemTotal: 16 kB\nMemFree: 4 kB\nVendorCounter: 12 widgets\n")?
  let after = report_checks.parse_meminfo_reference("MemTotal: 16 kB\nMemFree: 5 kB\nVendorCounter: 12 widgets\n")?
  let candidate = """{"memory":{"host":{"total_bytes":16384,"free_bytes":4608,"counters":[{"name":"MemTotal","value":16384,"unit":"bytes"},{"name":"MemFree","value":4608,"unit":"bytes"},{"name":"VendorCounter","value":12,"unit":"widgets"}]}}}"""
  let compared = report_checks.compare_meminfo(candidate, before, after)?
  test.eq(compared.reference_count, 3)?
  test.eq(compared.stable_count, 2)?
  test.eq(compared.changed_count, 1)?
  test.eq(compared.missing_names, [])?
  test.eq(compared.mismatched_names, [])?
  test.ok(compared.exact_scored)?

  let wrong_scalar = candidate.replace("\"total_bytes\":16384", "\"total_bytes\":32768")
  let projection = report_checks.compare_meminfo(wrong_scalar, before, after)?
  test.eq(projection.scalar_mismatches, ["MemTotal"])?
  test.ok(!projection.exact_scored)?
  let missing = candidate.replace(",{\"name\":\"VendorCounter\",\"value\":12,\"unit\":\"widgets\"}", "")
  test.eq(report_checks.compare_meminfo(missing, before, after)?.missing_names, ["VendorCounter"])?
  let duplicate = report_checks.parse_meminfo_reference("MemTotal: 16 kB\n")?
  test.error_kind(report_checks.compare_meminfo(candidate, duplicate.extend(duplicate), duplicate), "SystemReportCheckError.Invalid")?
}

proc test_system_report_thp_reference_requires_one_selected_policy_and_stable_fields() [error] {
  test.eq(report_checks.parse_thp_reference("always [future_policy] never\n")?, "always [future_policy] never")?
  for bad in ["", "always never\n", "[always] [never]\n", "[always] always\n", "[always]\n[never]\n"] {
    test.error_kind(report_checks.parse_thp_reference(bad), "SystemReportCheckError.Invalid")?
  }
  let before = [
    {name: "enabled", value: "always [future_policy] never"},
    {name: "defrag", value: "always defer [madvise] never"},
  ]
  let candidate = """{"memory":{"transparent_huge_pages":["defrag=always defer [madvise] never","enabled=always [future_policy] never"]}}"""
  let matched = report_checks.compare_thp(candidate, before, before)?
  test.ok(matched.exact)?
  test.eq(matched.matched_count, 2)?
  let missing = candidate.replace("\"defrag=always defer [madvise] never\",", "")
  test.eq(report_checks.compare_thp(missing, before, before)?.missing_names, ["defrag"])?
  let wrong = candidate.replace("[future_policy]", "[always]")
  test.eq(report_checks.compare_thp(wrong, before, before)?.mismatched_names, ["enabled"])?
  let duplicate = candidate.replace("\"enabled=always [future_policy] never\"", "\"enabled=always [future_policy] never\",\"enabled=always [future_policy] never\"")
  test.error_kind(report_checks.compare_thp(duplicate, before, before), "SystemReportCheckError.Invalid")?
  let changed = [{name: "enabled", value: "[always] never"}, {name: "defrag", value: "always defer [madvise] never"}]
  test.error_kind(report_checks.compare_thp(candidate, before, changed), "SystemReportCheckError.Invalid")?
}

proc test_system_report_vulnerability_reference_requires_complete_stable_named_values() [error] {
  let before = [
    {name: "spectre_v1", description: "Mitigation: custom policy"},
    {name: "mmio_stale_data", description: "Not affected"},
  ]
  let candidate = """{"cpu":{"vulnerabilities":[{"name":"mmio_stale_data","description":{"state":"observed","value":"Not affected","raw_bytes_base64":null}},{"name":"spectre_v1","description":{"state":"observed","value":"Mitigation: custom policy","raw_bytes_base64":null}}]}}"""
  let compared = report_checks.compare_vulnerabilities(candidate, before, before)?
  test.ok(compared.exact)?
  test.eq(compared.matched_count, 2)?
  let missing = """{"cpu":{"vulnerabilities":[{"name":"mmio_stale_data","description":{"state":"observed","value":"Not affected","raw_bytes_base64":null}}]}}"""
  test.eq(report_checks.compare_vulnerabilities(missing, before, before)?.missing_names, ["spectre_v1"])?
  let changed_value = candidate.replace("Mitigation: custom policy", "Vulnerable: custom policy")
  test.eq(report_checks.compare_vulnerabilities(changed_value, before, before)?.mismatched_names, ["spectre_v1"])?
  let unavailable = candidate.replace("\"state\":\"observed\",\"value\":\"Mitigation: custom policy\"", "\"state\":\"absent\",\"value\":null")
  test.eq(report_checks.compare_vulnerabilities(unavailable, before, before)?.mismatched_names, ["spectre_v1"])?
  let duplicate = candidate.replace("}]}}", "},{\"name\":\"spectre_v1\",\"description\":{\"state\":\"observed\",\"value\":\"Mitigation: custom policy\",\"raw_bytes_base64\":null}}]}}")
  test.error_kind(report_checks.compare_vulnerabilities(duplicate, before, before), "SystemReportCheckError.Invalid")?
  let changed_reference = [{name: "spectre_v1", description: "Vulnerable"}, {name: "mmio_stale_data", description: "Not affected"}]
  test.error_kind(report_checks.compare_vulnerabilities(candidate, before, changed_reference), "SystemReportCheckError.Invalid")?
}

proc test_system_report_process_identity_snapshot_keeps_complete_stable_sources() [fs, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"proc/123", parents: true)?
  fs.root_mkdir(root, p"proc/124", parents: true)?
  fs.root_write(root, p"proc/123/stat", "123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2\n")?
  fs.root_write(root, p"proc/123/status", "Name:\tworker\nUid:\t1000\t1000\t1000\t1000\n")?
  fs.root_write(root, p"proc/124/stat", "124 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 101 8192 2\n")?
  let captured = report_checks.read_process_identity_snapshot(root)?
  test.eq(captured.processes.len(), 1)?
  test.eq(captured.processes[0].pid, 123)?
  test.eq(captured.processes[0].uid, 1000)?
  test.eq(captured.skipped_count, 1)?
}

proc test_system_report_process_resource_snapshot_requires_complete_per_pid_sources() [fs, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"proc/123", parents: true)?
  fs.root_mkdir(root, p"proc/124", parents: true)?
  fs.root_write(root, p"proc/123/stat", "123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2\n")?
  fs.root_write(root, p"proc/123/statm", "2 1 0 0 0 0 0\n")?
  fs.root_write(root, p"proc/123/cgroup", "0::/tenant\n")?
  fs.root_write(root, p"proc/124/stat", "124 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 101 8192 2\n")?
  fs.root_write(root, p"proc/124/statm", "2 1\n")?
  fs.root_write(root, p"proc/124/cgroup", "0::/tenant\n")?
  let captured = report_checks.read_process_resource_snapshot(root, 65536)?
  test.eq(captured.processes.len(), 1)?
  test.eq(captured.processes[0].pid, 123)?
  test.eq(captured.processes[0].resident_bytes, 65536)?
  test.eq(captured.processes[0].virtual_bytes, 131072)?
  test.eq(captured.processes[0].cgroup, "/tenant")?
  test.eq(captured.skipped_count, 1)?
}

proc test_system_report_process_identity_reference_excludes_pid_reuse_and_scores_stable_values() [error] {
  let stable = {pid: 123, start_ticks: 100, parent_pid: 1, uid: 1000, command: "worker", state: "S"}
  let reused_before = {pid: 124, start_ticks: 200, parent_pid: 1, uid: 1000, command: "old", state: "S"}
  let reused_after = {...reused_before, start_ticks: 201, command: "new"}
  let candidate = """{"processes":{"processes":[{"pid":123,"parent_pid":1,"uid":1000,"command":{"state":"observed","value":"worker"},"state":"R","start_ticks":100},{"pid":124,"parent_pid":1,"uid":1000,"command":{"state":"observed","value":"new"},"state":"S","start_ticks":201}]}}"""
  let compared = report_checks.compare_process_identity(candidate, [stable, reused_before], [stable, reused_after])?
  test.eq(compared.stable_count, 1)?
  test.eq(compared.unstable_count, 1)?
  test.eq(compared.matched_count, 1)?
  test.eq(compared.missing_pids, [])?
  test.eq(compared.mismatched_pids, [])?
  test.eq(compared.state_unscored_count, 1)?
  test.ok(compared.exact_static)?

  let wrong_uid = candidate.replace("\"uid\":1000", "\"uid\":1001")
  let changed = report_checks.compare_process_identity(wrong_uid, [stable], [stable])?
  test.eq(changed.mismatched_pids, [123])?
  test.ok(!changed.exact_static)?
  let wrong_start = candidate.replace("\"start_ticks\":100", "\"start_ticks\":101")
  let missing = report_checks.compare_process_identity(wrong_start, [stable], [stable])?
  test.eq(missing.missing_pids, [123])?
  test.ok(!missing.exact_static)?
  test.error_kind(report_checks.compare_process_identity(candidate, [stable, stable], [stable]), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.compare_process_identity(candidate, [{...stable, parent_pid: -1}], [stable]), "SystemReportCheckError.Invalid")?
  test.ok(!report_checks.compare_process_identity(candidate, [], [])?.exact_static)?
}

proc test_system_report_process_reference_excludes_exits_and_new_arrivals_from_static_scoring() [error] {
  let stable = {pid: 123, start_ticks: 100, parent_pid: 1, uid: 1000, command: "worker", state: "S"}
  let exited = {...stable, pid: 124, start_ticks: 200, command: "short-lived"}
  let arrived = {...stable, pid: 125, start_ticks: 300, command: "new"}
  let candidate = """{"processes":{"processes":[{"pid":123,"parent_pid":1,"uid":1000,"command":{"state":"observed","value":"worker"},"state":"R","start_ticks":100},{"pid":124,"parent_pid":1,"uid":1000,"command":{"state":"observed","value":"short-lived"},"state":"S","start_ticks":200},{"pid":125,"parent_pid":1,"uid":1000,"command":{"state":"observed","value":"new"},"state":"S","start_ticks":300}]}}"""
  let compared = report_checks.compare_process_identity(candidate, [stable, exited], [stable, arrived])?
  test.eq(compared.stable_count, 1)?
  test.eq(compared.unstable_count, 1)?
  test.eq(compared.candidate_count, 3)?
  test.eq(compared.matched_count, 1)?
  test.eq(compared.missing_pids, [])?
  test.eq(compared.mismatched_pids, [])?
  test.ok(compared.exact_static)?

  let only_exited = report_checks.compare_process_identity(candidate, [exited], [arrived])?
  test.eq(only_exited.stable_count, 0)?
  test.eq(only_exited.unstable_count, 1)?
  test.ok(!only_exited.exact_static)?

  let stable_resources = {pid: 123, start_ticks: 100, thread_count: 2, resident_bytes: 65536, virtual_bytes: 131072, cgroup: "/tenant"}
  let exited_resources = {...stable_resources, pid: 124, start_ticks: 200}
  let arrived_resources = {...stable_resources, pid: 125, start_ticks: 300}
  let resource_candidate = """{"processes":{"processes":[{"pid":123,"start_ticks":100,"thread_count":2,"resident_bytes":65536,"virtual_bytes":131072,"cgroup":{"state":"observed","value":"/tenant"}},{"pid":124,"start_ticks":200,"thread_count":2,"resident_bytes":65536,"virtual_bytes":131072,"cgroup":{"state":"observed","value":"/tenant"}},{"pid":125,"start_ticks":300,"thread_count":2,"resident_bytes":65536,"virtual_bytes":131072,"cgroup":{"state":"observed","value":"/tenant"}}]}}"""
  let resources = report_checks.compare_process_resources(resource_candidate, [stable_resources, exited_resources], [stable_resources, arrived_resources])?
  test.eq(resources.stable_count, 1)?
  test.eq(resources.unstable_count, 1)?
  test.eq(resources.candidate_count, 3)?
  test.eq(resources.scored_fields, 4)?
  test.eq(resources.missing_pids, [])?
  test.eq(resources.mismatched_fields, [])?
  test.ok(resources.exact_scored)?
}

proc test_system_report_traced_live_and_replay_paths_keep_host_effect_contract(ctx: TestContext) [fs, process, env, error] {
  if system.uname()?.sysname != "Linux" {
    test.skip("the production syscall audit requires Linux strace")
    return
  }
  let script = fp"${ctx.core_dir.parent()}/core/system-report.xsh"
  report_checks.audit_no_subprocess(ctx.xsh_bin.display(), script.display())?
}

proc test_system_report_process_resource_reference_scores_only_stable_fields() [error] {
  let stable = {pid: 123, start_ticks: 100, thread_count: 2, resident_bytes: 65536, virtual_bytes: 131072, cgroup: "/tenant"}
  let reused = {...stable, pid: 124, start_ticks: 200}
  let reused_after = {...reused, start_ticks: 201}
  let candidate = """{"processes":{"processes":[{"pid":123,"start_ticks":100,"thread_count":2,"resident_bytes":65536,"virtual_bytes":131072,"cgroup":{"state":"observed","value":"/tenant"}},{"pid":124,"start_ticks":201,"thread_count":2,"resident_bytes":65536,"virtual_bytes":131072,"cgroup":{"state":"observed","value":"/tenant"}}]}}"""
  let exact = report_checks.compare_process_resources(candidate, [stable, reused], [stable, reused_after])?
  test.eq(exact.stable_count, 1)?
  test.eq(exact.unstable_count, 1)?
  test.eq(exact.scored_fields, 4)?
  test.eq(exact.unscored_fields, [])?
  test.eq(exact.mismatched_fields, [])?
  test.ok(exact.exact_scored)?

  let moved = {...stable, resident_bytes: 131072}
  let changed = report_checks.compare_process_resources(candidate, [stable], [moved])?
  test.eq(changed.scored_fields, 3)?
  test.eq(changed.unscored_fields, ["123.resident_bytes"])?
  test.ok(changed.exact_scored)?

  let wrong = candidate.replace("\"virtual_bytes\":131072", "\"virtual_bytes\":262144")
  let mismatch = report_checks.compare_process_resources(wrong, [stable], [stable])?
  test.eq(mismatch.mismatched_fields, ["123.virtual_bytes"])?
  test.ok(!mismatch.exact_scored)?
  let reused_candidate = candidate.replace("\"start_ticks\":100", "\"start_ticks\":101")
  let missing = report_checks.compare_process_resources(reused_candidate, [stable], [stable])?
  test.eq(missing.missing_pids, [123])?
  test.error_kind(report_checks.compare_process_resources(candidate, [stable, stable], [stable]), "SystemReportCheckError.Invalid")?
  test.ok(!report_checks.compare_process_resources(candidate, [], [])?.exact_scored)?
}

proc test_system_report_coverage_manifest_contract() [error] {
  let required = assertion("cpu.policy.related-cpus", "mandatory")
  let supplemental = assertion("cpu.policy.governor", "supplemental")
  let valid = manifest([required, supplemental])

  report_checks.validate(valid)?
  test.error_kind(report_checks.validate({...valid, schema_version: 3}), "SystemReportCheckError.Invalid")?
  let rooted_process = {...required, id: "process.identity", domain: "process", reference_adapter: "procfs-rooted-v1", reference_commands: []}
  report_checks.validate(manifest([rooted_process]))?
  test.error_kind(report_checks.validate(manifest([{...required, reference_commands: []}])), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate(manifest([{...required, reference_adapter: "procfs-rooted-v1", reference_commands: []}])), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate(manifest([{...required, reference_commands: [[]]}])), "SystemReportCheckError.Invalid")?
  let route_commands = [["ip", "-json", "-family", "inet", "route", "show", "table", "all"], ["ip", "-json", "-family", "inet6", "route", "show", "table", "all"]]
  let rule_commands = [["ip", "-json", "-family", "inet", "rule", "show"], ["ip", "-json", "-family", "inet6", "rule", "show"]]
  let route = {...required, id: "network.routes", domain: "network", reference_adapter: "iproute2", reference_commands: route_commands}
  let rule = {...required, id: "network.rules", domain: "network", reference_adapter: "iproute2", reference_commands: rule_commands}
  report_checks.validate(manifest([route, rule]))?
  let pci_command = ["lspci", "-D", "-vmm", "-n", "-k"]
  let pci_identity = {...required, id: "pci.identity", domain: "pci", reference_adapter: "lspci", reference_commands: [pci_command]}
  let pci_binding = {...pci_identity, id: "pci.binding"}
  report_checks.validate(manifest([pci_identity, pci_binding]))?
  test.error_kind(report_checks.validate(manifest([{...pci_identity, reference_adapter: "pci-sysfs"}])), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate(manifest([{...pci_identity, reference_commands: [["lspci", "-D", "-vmm", "-n"]]}])), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate(manifest([{...pci_binding, reference_commands: [["lspci", "-D", "-vmm", "-n", "-k", "-t"]]}])), "SystemReportCheckError.Invalid")?
  let trace_command = ["strace", "-f", "-qq", "-s", "4096", "-e", "trace=process,network,file,init_module,finit_module,delete_module,swapon,swapoff,write,writev,pwrite64,pwritev,pwritev2", "-o", "<trace file>", "--", "<xsh binary>", "<system-report script>", "--", "--json"]
  let trace_assertion = {...required, id: "safety.no-child", domain: "safety", reference_adapter: "strace", reference_commands: [trace_command]}
  report_checks.validate(manifest([trace_assertion]))?
  test.error_kind(report_checks.validate(manifest([{...trace_assertion, reference_commands: [["strace", "-f", "-qq", "system-report", "--json"]]}])), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate(manifest([{...trace_assertion, reference_adapter: "shell-trace"}])), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate(manifest([{...route, reference_commands: [route_commands[0]]}])), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate(manifest([{...rule, reference_commands: [rule_commands[1]]}])), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate(manifest([{...route, reference_commands: [route_commands[0], route_commands[0]]}])), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate({...valid, schema_version: 1}), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate({...valid, schema_version: 2}), "SystemReportCheckError.Invalid")?
  let summary = report_checks.summary(valid.assertions)
  test.contains(summary, "cpu: 1 mandatory, 1 supplemental")?
  test.contains(summary, "total: 1 mandatory, 1 supplemental")?

  let duplicate = manifest([required, required])
  test.error_kind(report_checks.validate(duplicate), "SystemReportCheckError.Invalid")?

  let no_required_cases = manifest([])
  test.error_kind(report_checks.validate(no_required_cases), "SystemReportCheckError.Invalid")?

  let undeclared_scenario = manifest([{...required, fixture_scenarios: ["not-in-manifest"]}])
  test.error_kind(report_checks.validate(undeclared_scenario), "SystemReportCheckError.Invalid")?

  let mapped = {...valid, fixture_cases: [{scenario: "offline_related_cpu", tests: ["tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset"]}]}
  report_checks.validate(mapped)?
  let macos_mapped = {...valid, macos_fixture_cases: [{scenario: "offline_related_cpu", tests: ["tests/xsh/system-report.xsh::test_system_report_command_replays_saved_json_offline"]}]}
  report_checks.validate(macos_mapped)?
  test.error_kind(report_checks.validate({...macos_mapped, fixture_cases: mapped.fixture_cases}), "SystemReportCheckError.Invalid")?
  report_checks.validate({...valid, fixture_cases: [{scenario: "offline_related_cpu", tests: ["dev/tests/test-system-report-check.xsh::test_system_report_cpu_set_comparison_scores_all_four_kernel_sets"]}]})?
  report_checks.validate({...valid, fixture_cases: [{scenario: "offline_related_cpu", tests: ["src/modules/linux/real/netlink.rs::dump_accumulator_reads_multipart_messages_across_datagrams"]}]})?
  report_checks.validate({...valid, fixture_cases: [{scenario: "offline_related_cpu", tests: ["tests/linux_priv.rs::system_report_sysctl_denial_as_unprivileged_reader_creates_no_child"]}]})?
  test.error_kind(report_checks.validate({...valid, fixture_cases: [{scenario: "offline_related_cpu", tests: ["dev/tests/unrelated.xsh::test_system_report_cpu_set_comparison_scores_all_four_kernel_sets"]}]}), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate({...mapped, fixture_cases: mapped.fixture_cases.extend(mapped.fixture_cases)}), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate({...valid, fixture_cases: [{scenario: "not-in-manifest", tests: mapped.fixture_cases[0].tests}]}), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate({...valid, fixture_cases: [{scenario: "offline_related_cpu", tests: []}]}), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate({...valid, fixture_cases: [{scenario: "offline_related_cpu", tests: ["tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset", "tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset"]}]}), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate({...valid, fixture_cases: [{scenario: "offline_related_cpu", tests: ["tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset::extra"]}]}), "SystemReportCheckError.Invalid")?

  test.ok(report_checks.fixture_single_test_passed("running 1 tests\ntest result: ok. 1 passed; 0 failed; 0 skipped\n"))?
  test.ok(!report_checks.fixture_single_test_passed("running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 skipped\n"))?
  test.ok(!report_checks.fixture_single_test_passed("running 2 tests\ntest result: ok. 2 passed; 0 failed; 0 skipped\n"))?
  test.ok(!report_checks.fixture_single_test_passed("running 1 tests\ntest result: ok. 0 passed; 0 failed; 1 skipped\n"))?
  let rust_fixture = "src/modules/linux/real/netlink.rs::dump_accumulator_reads_multipart_messages_across_datagrams"
  test.eq(report_checks.rust_fixture_argv("/opt/cargo", rust_fixture)?, [
    "/opt/cargo", "test", "--offline", "-p", "xsh", "--lib", "--target", "aarch64-unknown-linux-musl",
    "modules::linux::real::netlink::tests::dump_accumulator_reads_multipart_messages_across_datagrams", "--", "--exact", "--test-threads=1",
  ])?
  test.eq(report_checks.rust_fixture_argv("/opt/cargo", "tests/linux_priv.rs::system_report_sysctl_denial_as_unprivileged_reader_creates_no_child")?, [
    "/opt/cargo", "test", "--offline", "-p", "xsh", "--test", "linux_priv", "--features", "linux-priv-tests",
    "--target", "aarch64-unknown-linux-musl", "system_report_sysctl_denial_as_unprivileged_reader_creates_no_child",
    "--", "--exact", "--test-threads=1",
  ])?
  test.error_kind(report_checks.rust_fixture_argv("/opt/cargo", "src/modules/linux/real/netlink.rs::missing::extra"), "SystemReportCheckError.Invalid")?
  test.ok(report_checks.rust_fixture_single_test_passed("running 1 test\ntest modules::linux::real::netlink::tests::example ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out; finished in 0.00s\n"))?
  test.ok(!report_checks.rust_fixture_single_test_passed("running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out\n"))?
  test.ok(!report_checks.rust_fixture_single_test_passed("running 1 test\ntest result: ok. 0 passed; 1 failed; 0 ignored; 0 measured; 30 filtered out\n"))?

  let thread_trace = """execve("/target/xsh", ["xsh"], 0x0) = 0
clone3({flags=CLONE_VM|CLONE_FS|CLONE_FILES|CLONE_SIGHAND|CLONE_THREAD}, 88) = 42
"""
  test.eq(report_checks.process_trace_violations(thread_trace), [])?

  let child_trace = """execve("/target/xsh", ["xsh"], 0x0) = 0
clone3({flags=CLONE_VM|CLONE_VFORK}, 88) = 42
execve("/bin/sh", ["sh"], 0x0) = 0
"""
  let violations = report_checks.process_trace_violations(child_trace)
  test.ok("process clone syscall" in violations)?
  test.ok("secondary exec syscall" in violations)?
  test.eq(report_checks.process_trace_violations(""), ["initial XSH exec was not traced"])?

  let read_only_trace = """42 openat2(3, "proc/cpuinfo", {flags=O_RDONLY|O_CLOEXEC}, 24) = 4
42 socket(AF_NETLINK, SOCK_RAW|SOCK_CLOEXEC, NETLINK_ROUTE) = 4
42 sendto(4, [{nlmsg_type=RTM_GETLINK}], 32, 0, {sa_family=AF_NETLINK, nl_pid=0}, 12) = 32
"""
  test.ok(report_checks.host_effect_trace_violations(read_only_trace).len() == 0)?
  test.eq(report_checks.host_effect_trace_violations("42 socket(AF_INET, SOCK_DGRAM|SOCK_CLOEXEC, IPPROTO_UDP) = -1 EPERM"), ["external network socket"])?
  test.eq(report_checks.host_effect_trace_violations("42 socket(AF_UNIX, SOCK_STREAM|SOCK_CLOEXEC, 0) = 4"), ["unexpected socket family"])?
  test.eq(report_checks.host_effect_trace_violations("42 socket(AF_NETLINK, SOCK_RAW|SOCK_CLOEXEC, NETLINK_GENERIC) = 4"), ["unexpected netlink protocol"])?
  test.eq(report_checks.host_effect_trace_violations("42 socket(AF_NETLINK, SOCK_STREAM|SOCK_CLOEXEC, NETLINK_ROUTE) = -1 EPROTONOSUPPORT"), ["unexpected netlink socket type"])?
  test.eq(report_checks.host_effect_trace_violations("42 socket(AF_NETLINK, SOCK_RAW|SOCK_CLOEXEC, NETLINK_ROUTE) = 4"), [])?
  test.eq(report_checks.host_effect_trace_violations("42 socketpair(AF_UNIX, SOCK_STREAM, 0, [4, 5]) = 0"), ["unexpected socket pair"])?
  test.eq(report_checks.host_effect_trace_violations("42 listen(4, 1) = -1 EACCES"), ["unexpected network listener"])?
  test.eq(report_checks.host_effect_trace_violations("42 accept4(4, NULL, NULL, SOCK_CLOEXEC) = -1 EAGAIN"), ["unexpected network accept"])?
  let mixed_queries = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}, {nlmsg_type=RTM_GETADDR}], 64, 0, {sa_family=AF_NETLINK, nl_pid=0}, 12) = 64"
  test.eq(report_checks.host_effect_trace_violations(mixed_queries), [])?
  let unknown_mixed_request = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}, {nlmsg_type=RTM_UNKNOWN}], 64, 0, {sa_family=AF_NETLINK, nl_pid=0}, 12) = 64"
  test.eq(report_checks.host_effect_trace_violations(unknown_mixed_request), ["non-query netlink request"])?
  let numeric_mixed_request = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}, {nlmsg_type=0x12}], 64, 0, {sa_family=AF_NETLINK, nl_pid=0}, 12) = 64"
  test.eq(report_checks.host_effect_trace_violations(numeric_mixed_request), ["non-query netlink request"])?
  let abbreviated_request = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}, ...], 64, 0, {sa_family=AF_NETLINK, nl_pid=0}, 12) = 64"
  test.eq(report_checks.host_effect_trace_violations(abbreviated_request), ["non-query netlink request"])?

  let violating_trace = """42 openat2(3, "sys/kernel/test", {flags=O_WRONLY|O_CLOEXEC}, 24) = -1 EACCES
42 unlinkat(3, "file", 0) = -1 EPERM
42 connect(4, {sa_family=AF_INET, sin_port=htons(53)}, 16) = -1 ENETUNREACH
42 sendto(4, [{nlmsg_type=RTM_GETLINK}, {nlmsg_type=RTM_SETLINK}], 64, 0, {sa_family=AF_NETLINK, nl_pid=0}, 12) = -1 EPERM
42 sendmsg(4, {msg_name={nl_family=AF_NETLINK}, msg_iov=[{iov_base={nlmsg_type=RTM_NEWROUTE}}]}, 0) = -1 EPERM
"""
  let host_violations = report_checks.host_effect_trace_violations(violating_trace)
  test.ok("writable file open" in host_violations)?
  test.ok("system mutation syscall unlinkat" in host_violations)?
  test.ok("external network syscall" in host_violations)?
  test.ok("non-query netlink request" in host_violations)?
  let sendmsg_trace = "42 sendmsg(4, {msg_name={nl_family=AF_NETLINK}, msg_iov=[{iov_base={nlmsg_type=RTM_NEWROUTE}}]}, 0) = -1 EPERM"
  test.eq(report_checks.host_effect_trace_violations(sendmsg_trace), ["unexpected network send"])?
  let packet_send_trace = "42 sendto(4, \"probe\", 5, 0, {sa_family=AF_PACKET, sll_protocol=htons(0x0800)}, 20) = 5"
  test.ok("external network syscall" in report_checks.host_effect_trace_violations(packet_send_trace))?
  let local_helper_trace = "42 sendto(4, \"query\", 5, 0, {sa_family=AF_UNIX, sun_path=\"/var/run/nscd/socket\"}, 110) = 5"
  test.eq(report_checks.host_effect_trace_violations(local_helper_trace), ["unexpected network send"])?
  let attempted_local_helper = "42 connect(4, {sa_family=AF_UNIX, sun_path=\"/var/run/nscd/socket\"}, 110) = -1 ENOENT"
  test.eq(report_checks.host_effect_trace_violations(attempted_local_helper), ["local socket connection"])?
  let inherited_socket_trace = "42 sendmsg(7, {msg_iov=[{iov_base=\"probe\", iov_len=5}]}, 0) = 5"
  test.eq(report_checks.host_effect_trace_violations(inherited_socket_trace), ["unexpected network send"])?
  let spoofed_query_trace = "42 sendmsg(7, {msg_name={sa_family=AF_UNIX, sun_path=\"/tmp/helper\"}, msg_iov=[{iov_base={nlmsg_type=RTM_GETLINK}}]}, 0) = 32"
  test.eq(report_checks.host_effect_trace_violations(spoofed_query_trace), ["unexpected network send"])?
  let targetless_query_trace = "42 sendto(7, [{nlmsg_type=RTM_GETLINK}], 32, 0, NULL, 0) = 32"
  test.eq(report_checks.host_effect_trace_violations(targetless_query_trace), ["unexpected network send"])?
  let forged_sendmsg_target = "42 sendmsg(7, {msg_name={sa_family=AF_UNIX, sun_path=\"/tmp/helper\"}, msg_iov=[{iov_base={sa_family=AF_NETLINK, nlmsg_type=RTM_GETLINK}}]}, 0) = 32"
  test.eq(report_checks.host_effect_trace_violations(forged_sendmsg_target), ["unexpected network send"])?
  let forged_sendto_target = "42 sendto(7, [{sa_family=AF_NETLINK, nlmsg_type=RTM_GETLINK}], 32, 0, {sa_family=AF_UNIX, sun_path=\"/tmp/helper\"}, 110) = 32"
  test.eq(report_checks.host_effect_trace_violations(forged_sendto_target), ["unexpected network send"])?
  let targetless_sendmsg_query = "42 sendmsg(7, {msg_name=NULL, msg_iov=[{iov_base={nlmsg_type=RTM_GETLINK}}]}, 0) = 32"
  test.eq(report_checks.host_effect_trace_violations(targetless_sendmsg_query), ["unexpected network send"])?
  let quoted_query_marker = "42 sendto(4, \"nlmsg_type=RTM_GETLINK,\", 23, 0, {sa_family=AF_NETLINK, nl_pid=0}, 12) = 23"
  test.eq(report_checks.host_effect_trace_violations(quoted_query_marker), ["non-query netlink request"])?
  let abbreviated_sendto = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}], 32, 0, {sa_family=AF_NETLINK, nl_pid=0}, ..."
  test.eq(report_checks.host_effect_trace_violations(abbreviated_sendto), ["unexpected network send"])?
  let local_bind = "42 bind(7, {sa_family=AF_UNIX, sun_path=\"/tmp/helper\"}, 110) = 0"
  test.eq(report_checks.host_effect_trace_violations(local_bind), ["unexpected network bind"])?
  let forged_bind_target = "42 bind(7, {sa_family=AF_UNIX, sun_path=\"/tmp/sa_family=AF_NETLINK\"}, 110) = 0"
  test.eq(report_checks.host_effect_trace_violations(forged_bind_target), ["unexpected network bind"])?
  let route_bind = "42 bind(7, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=0}, 12) = 0"
  test.eq(report_checks.host_effect_trace_violations(route_bind), [])?
  let netlink_connect = "42 connect(7, {sa_family=AF_NETLINK, nl_pid=0}, 12) = 0"
  test.eq(report_checks.host_effect_trace_violations(netlink_connect), ["unexpected network connection"])?
  let output_writes = "42 write(1, \"report\", 6) = 6\n42 writev(2, [{iov_base=\"error\", iov_len=5}], 1) = 5"
  test.eq(report_checks.host_effect_trace_violations(output_writes), [])?
  let inherited_write = "42 write(3, \"unexpected\", 10) = 10"
  test.eq(report_checks.host_effect_trace_violations(inherited_write), ["write to non-output descriptor"])?
  let positioned_write = "42 pwrite64(3, \"unexpected\", 10, 0) = 10"
  test.eq(report_checks.host_effect_trace_violations(positioned_write), ["positioned write syscall"])?

  let replay_trace = """42 openat2(3, "stdout-live-json", {flags=O_RDONLY}, 24) = 4
42 openat2(3, "proc/meminfo", {flags=O_RDONLY}, 24) = 5
"""
  test.eq(report_checks.replay_host_read_violations(replay_trace), ["saved-report replay read a live source"])?
  let absolute_replay_trace = """42 openat(AT_FDCWD, "/proc/meminfo", O_RDONLY) = 5
42 openat2(3, "/sys/devices/system/cpu/online", {flags=O_RDONLY}, 24) = 6
"""
  test.eq(report_checks.replay_host_read_violations(absolute_replay_trace).len(), 2)?
  let replay_metadata_trace = """42 readlinkat(3, "sys/class/net/eth0", "../../devices/pci0000:00", 4096) = 25
42 open("/etc/os-release", O_RDONLY) = -1 EACCES
42 statx(3, "proc/meminfo", AT_STATX_SYNC_AS_STAT, STATX_ALL, 0x7ffc) = 0
"""
  test.eq(report_checks.replay_host_read_violations(replay_metadata_trace).len(), 3)?
  let saved_link_target = "42 readlinkat(3, \"saved-report\", \"/sys/devices/virtual\", 4096) = 20"
  test.eq(report_checks.replay_host_read_violations(saved_link_target), [])?
  test.eq(report_checks.replay_host_read_violations("42 openat(3, \"proc\", O_RDONLY|O_DIRECTORY) = 5"), ["saved-report replay read a live source"])?

  let permitted_process_trace = """42 openat2(3, "proc/123/stat", {flags=O_RDONLY}, 24) = 4
42 openat2(3, "proc/123/status", {flags=O_RDONLY}, 24) = 4
42 openat2(3, "proc/self/mountinfo", {flags=O_RDONLY}, 24) = 4
42 openat2(3, "proc/cmdline", {flags=O_RDONLY}, 24) = 4
"""
  test.eq(report_checks.forbidden_process_read_violations(permitted_process_trace), [])?
  let forbidden_process_trace = """42 openat2(3, "proc/123/environ", {flags=O_RDONLY}, 24) = -1 EACCES
42 openat(AT_FDCWD, "/proc/self/cmdline", O_RDONLY) = 4
42 readlinkat(3, "proc/123/fd/1", "/private/path", 4096) = 13
42 openat2(3, "proc/123/task/123/mem", {flags=O_RDONLY}, 24) = -1 EACCES
"""
  test.eq(report_checks.forbidden_process_read_violations(forbidden_process_trace).len(), 4)?
  let permitted_process_json = """{"processes":{"processes":[{"pid":123,"command":{"state":"observed","value":"worker"}}]}}"""
  test.eq(report_checks.forbidden_process_field_violations(permitted_process_json)?, [])?
  let forbidden_process_json = """{"processes":{"processes":[{"pid":123,"environment":"secret","cmdline":"private","open_paths":["/private"]}]}}"""
  test.eq(report_checks.forbidden_process_field_violations(forbidden_process_json)?.len(), 3)?
}

proc test_system_report_trace_ignores_syscall_names_inside_file_paths() [error] {
  let trace = """42 execve("/target/xsh", ["xsh"], 0x0) = 0
42 openat(AT_FDCWD, "/tmp/ execve( fork( clone( socket( O_WRONLY", O_RDONLY) = 3
"""
  test.eq(report_checks.process_trace_violations(trace).len(), 0)?
  test.eq(report_checks.host_effect_trace_violations(trace).len(), 0)?
}

proc test_system_report_fixture_definition_check_ignores_comments_and_partial_names() [error] {
  let source = "# proc test_system_report_comment_only() {}\nproc test_system_report_existing() [error] {}\nproc test_system_report_existing_more() [error] {}\n"
  test.ok(report_checks.fixture_test_definition_exists(source, "test_system_report_existing"))?
  test.ok(!report_checks.fixture_test_definition_exists(source, "test_system_report_comment_only"))?
  test.ok(!report_checks.fixture_test_definition_exists(source, "test_system_report_missing"))?
  test.ok(!report_checks.fixture_test_definition_exists(source, "test_system_report_existing_m"))?
  let rust_source = "// #[test]\nfn comment_only() {}\n/*\n#[test]\nfn block_comment_only() {}\n*/\n#[test]\nfn observed() {}\nfn unmarked() {}\n#[test]\nfn observed_more() {}\n"
  test.ok(report_checks.rust_fixture_test_definition_exists(rust_source, "observed"))?
  test.ok(!report_checks.rust_fixture_test_definition_exists(rust_source, "comment_only"))?
  test.ok(!report_checks.rust_fixture_test_definition_exists(rust_source, "block_comment_only"))?
  test.ok(!report_checks.rust_fixture_test_definition_exists(rust_source, "unmarked"))?
  test.ok(!report_checks.rust_fixture_test_definition_exists(rust_source, "observed_m"))?
}

proc test_system_report_forbidden_process_read_trace_normalizes_source_paths() [error] {
  let forbidden = """42 openat(AT_FDCWD, "/proc/./123/environ", O_RDONLY) = -1 EACCES
42 openat2(3, "proc//self/cmdline", {flags=O_RDONLY}, 24) = 4
42 readlinkat(3, "proc/self/task/321/../321/fd/4", "/private/path", 4096) = 13
"""
  test.eq(report_checks.forbidden_process_read_violations(forbidden).len(), 3)?
  let permitted = """42 openat(AT_FDCWD, "/proc/./123/stat", O_RDONLY) = 4
42 readlinkat(3, "/tmp/safe", "proc/123/environ", 4096) = 19
"""
  test.eq(report_checks.forbidden_process_read_violations(permitted), [])?
}

proc test_system_report_replay_trace_rejects_normalized_live_source_paths() [error] {
  let forbidden = """42 openat(AT_FDCWD, "/tmp/../proc/meminfo", O_RDONLY) = 4
42 newfstatat(AT_FDCWD, "cache/../../sys/devices/system/cpu/online", 0x7fff, 0) = 0
42 readlinkat(3, "tmp/../etc/os-release", "/private/link", 4096) = 13
"""
  test.eq(report_checks.replay_host_read_violations(forbidden).len(), 3)?
  let permitted = """42 openat(AT_FDCWD, "/tmp/etc/os-release", O_RDONLY) = 4
42 readlinkat(3, "/tmp/saved-report", "/proc/meminfo", 4096) = 13
"""
  test.eq(report_checks.replay_host_read_violations(permitted), [])?
}

proc test_system_report_lscpu_reference_parser_keeps_sparse_online_ids() [error] {
  let reference = """{"cpus":[{"cpu":0,"online":true,"node":0},{"cpu":1,"online":false,"node":0},{"cpu":65,"online":true,"node":1}]}"""
  test.eq(report_checks.parse_lscpu_online_cpu_ids(reference)?, [0, 65])?
  let repeated = """{"cpus":[{"cpu":0,"online":true},{"cpu":0,"online":false}]}"""
  test.error_kind(report_checks.parse_lscpu_online_cpu_ids(repeated), "SystemReportCheckError.Invalid")?
  let unknown = """{"cpus":[{"cpu":0,"online":"yes"}]}"""
  match report_checks.parse_lscpu_online_cpu_ids(unknown) {
    Ok(_) => test.fail("lscpu parser accepted a non-boolean online field")?
    Err(_) => {}
  }
}

proc test_system_report_lscpu_reference_parser_rejects_empty_cpu_set() [error] {
  test.error_kind(report_checks.parse_lscpu_online_cpu_ids("""{"cpus":[]}"""), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_lscpu_online_cpu_ids("""{"cpus":[{"cpu":0,"online":false}]}"""), "SystemReportCheckError.Invalid")?
}

proc test_system_report_sysfs_reference_cpu_list_parser_handles_sparse_ids() [error] {
  test.eq(report_checks.parse_reference_cpu_list("0-2,65,129-130", false)?, [0, 1, 2, 65, 129, 130])?
  test.eq(report_checks.parse_reference_cpu_list("", true)?, [])?
  test.error_kind(report_checks.parse_reference_cpu_list("", false), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_cpu_list("2-1", false), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_cpu_list("0,0", false), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_cpu_list("0-65536", false), "SystemReportCheckError.Invalid")?
}

proc test_system_report_cpu_online_comparison_counts_missing_and_unexpected_ids() [error] {
  let reference = """{"cpus":[{"cpu":0,"online":true},{"cpu":1,"online":false},{"cpu":65,"online":true}]}"""
  let candidate = """{"cpu":{"online":[0,2]}}"""
  let compared = report_checks.compare_cpu_online_ids(candidate, reference)?
  test.eq(compared.reference_count, 2)?
  test.eq(compared.candidate_count, 2)?
  test.eq(compared.matched_count, 1)?
  test.eq(compared.missing_ids, [65])?
  test.eq(compared.unexpected_ids, [2])?
  test.ok(!compared.exact)?
  let matching = report_checks.compare_cpu_online_ids("""{"cpu":{"online":[65,0]}}""", reference)?
  test.ok(matching.exact)?
  test.error_kind(report_checks.compare_cpu_online_ids("""{"cpu":{"online":[0,0]}}""", reference), "SystemReportCheckError.Invalid")?
}

proc test_system_report_cpu_set_comparison_scores_all_four_kernel_sets() [error] {
  let reference = {
    possible: [0, 1, 2],
    present: [0, 2],
    online: [0, 2],
    offline: [],
  }
  let candidate = """{"cpu":{"possible":[0,1,2],"present":[0,2],"online":[0],"offline":[2]}}"""
  let compared = report_checks.compare_cpu_sets(candidate, reference)?
  test.ok(compared.possible.exact)?
  test.ok(compared.present.exact)?
  test.eq(compared.online.missing_ids, [2])?
  test.eq(compared.offline.unexpected_ids, [2])?
  test.ok(!compared.exact)?
  let matching = """{"cpu":{"possible":[2,1,0],"present":[2,0],"online":[0,2],"offline":[]}}"""
  test.ok(report_checks.compare_cpu_sets(matching, reference)?.exact)?
  let duplicate = """{"cpu":{"possible":[0,0],"present":[],"online":[],"offline":[]}}"""
  test.error_kind(report_checks.compare_cpu_sets(duplicate, reference), "SystemReportCheckError.Invalid")?
}

proc test_system_report_swapon_raw_parser_and_comparison_keep_swap_identity() [error] {
  let output = "NAME TYPE SIZE USED PRIO\n/dev/zram0 partition 4096 1024 42\n/swapfile file 8192 0 -1\n"
  let reference = report_checks.parse_swapon_raw(output)?
  test.eq(reference.len(), 2)?
  test.eq(reference[0].name, "/dev/zram0")?
  test.eq(reference[0].size_bytes, 4096)?
  test.eq(reference[1].priority, -1)?
  let reordered = report_checks.parse_swapon_raw("NAME TYPE SIZE USED PRIO\n/swapfile file 8192 0 -1\n/dev/zram0 partition 4096 1024 42\n")?
  test.ok(report_checks.swap_reference_stable(reference, reordered))?
  let changed = report_checks.parse_swapon_raw("NAME TYPE SIZE USED PRIO\n/dev/zram0 partition 4096 2048 42\n/swapfile file 8192 0 -1\n")?
  test.ok(!report_checks.swap_reference_stable(reference, changed))?
  let candidate = """{"memory":{"swaps":[{"name":{"state":"observed","value":"/swapfile"},"kind":"file","size_bytes":8192,"used_bytes":0,"priority":-1},{"name":{"state":"observed","value":"/dev/zram0"},"kind":"partition","size_bytes":4096,"used_bytes":1024,"priority":42}]}}"""
  let exact = report_checks.compare_swap_devices(candidate, reference)?
  test.ok(exact.exact)?
  test.eq(exact.matched_count, 2)?
  let missing = report_checks.compare_swap_devices("""{"memory":{"swaps":[]}}""", reference)?
  test.eq(missing.missing_names, ["/dev/zram0", "/swapfile"])?
  test.ok(!missing.exact)?
  let wrong_used = """{"memory":{"swaps":[{"name":{"state":"observed","value":"/dev/zram0"},"kind":"partition","size_bytes":4096,"used_bytes":2048,"priority":42},{"name":{"state":"observed","value":"/swapfile"},"kind":"file","size_bytes":8192,"used_bytes":0,"priority":-1}]}}"""
  let mismatch = report_checks.compare_swap_devices(wrong_used, reference)?
  test.eq(mismatch.field_mismatches, ["/dev/zram0"])?
  test.eq(mismatch.used_mismatches, 1)?
  test.eq(mismatch.kind_mismatches + mismatch.size_mismatches + mismatch.priority_mismatches, 0)?
  test.ok(!mismatch.exact)?
  let absent_field = report_checks.compare_swap_devices("""{"memory":{}}""", reference)?
  test.ok(absent_field.candidate_field_missing)?
  test.ok(!absent_field.exact)?
  test.error_kind(report_checks.compare_swap_devices("""{"memory":{}}""", reference.extend([reference[0]])), "SystemReportCheckError.Invalid")?
  let redacted = """{"memory":{"swaps":[{"name":{"state":"redacted","value":null},"kind":"partition","size_bytes":4096,"used_bytes":1024,"priority":42}]}}"""
  let redacted_result = report_checks.compare_swap_devices(redacted, reference)?
  test.ok(redacted_result.candidate_field_missing)?
  test.eq(redacted_result.matched_count, 0)?
  let duplicate = """{"memory":{"swaps":[{"name":{"state":"observed","value":"/dev/zram0"},"kind":"partition","size_bytes":4096,"used_bytes":1024,"priority":42},{"name":{"state":"observed","value":"/dev/zram0"},"kind":"partition","size_bytes":4096,"used_bytes":1024,"priority":42}]}}"""
  test.error_kind(report_checks.compare_swap_devices(duplicate, reference), "SystemReportCheckError.Invalid")?
}

proc test_system_report_swapon_raw_parser_rejects_ambiguous_or_unsafe_rows() [error] {
  for output in [
    "",
    "/dev/zram0 partition 4096 0 42\n",
    "NAME TYPE SIZE USED PRIO\n/swap file file 4096 0 42\n",
    "NAME TYPE SIZE USED PRIO\n/dev/zram0 partition 0x1000 0 42\n",
    "NAME TYPE SIZE USED PRIO\n/dev/zram0 partition 9007199254740992 0 42\n",
    "NAME TYPE SIZE USED PRIO\n/dev/zram0 partition 4096 8192 42\n",
    "NAME TYPE SIZE USED PRIO\n/dev/zram0 partition 4096 0 42\n/dev/zram0 partition 4096 0 42\n",
  ] {
    test.error_kind(report_checks.parse_swapon_raw(output), "SystemReportCheckError.Invalid")?
  }
}

proc test_system_report_lsblk_json_scores_devices_and_layering() [error] {
  let output = """{"blockdevices":[{"name":"sda","kname":"sda","maj:min":"8:0","size":8192,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096,"children":[{"name":"sda1","kname":"sda1","maj:min":"8:1","size":4096,"type":"part","pkname":"sda","ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096,"children":[{"name":"cryptroot","kname":"dm-0","maj:min":"253:0","size":4096,"type":"crypt","pkname":"sda1","ro":false,"rm":false,"rota":false,"log-sec":512,"phy-sec":4096}]}]}]}"""
  let reference = report_checks.parse_lsblk_json(output)?
  test.eq(reference.devices.len(), 3)?
  test.eq(reference.edges.len(), 2)?
  test.eq(reference.devices[2].name, "dm-0")?
  test.eq(reference.edges[0].parent_name, "sda")?
  test.eq(reference.edges[1].child_name, "dm-0")?
  let candidate = """{"storage":{"devices":[{"name":"dm-0","major":253,"minor":0,"kind":"virtual","size_bytes":4096,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":false,"read_only":false,"parent_device_index":null,"holder_indices":[],"slave_indices":[2]},{"name":"sda","major":8,"minor":0,"kind":"disk","size_bytes":8192,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":true,"read_only":false,"parent_device_index":null,"holder_indices":[],"slave_indices":[]},{"name":"sda1","major":8,"minor":1,"kind":"partition","size_bytes":4096,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":true,"read_only":false,"parent_device_index":1,"holder_indices":[0],"slave_indices":[]}]}}"""
  let exact = report_checks.compare_block_devices(candidate, reference)?
  test.ok(exact.exact)?
  test.eq(exact.matched_count, 3)?
  test.eq(exact.matched_edges, 2)?
  let wrong_size = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 2, "size_bytes"], 2048)?)?
  let size_mismatch = report_checks.compare_block_devices(wrong_size, reference)?
  test.eq(size_mismatch.size_mismatches, 1)?
  test.ok(!size_mismatch.exact)?
  let lost_edge = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 0, "slave_indices"], [])?)?
  let relation_mismatch = report_checks.compare_block_devices(lost_edge, reference)?
  test.eq(relation_mismatch.missing_edges, 1)?
  test.ok(!relation_mismatch.exact)?
  let extra_edge = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 0, "slave_indices"], [1, 2])?)?
  let extra_relation = report_checks.compare_block_devices(extra_edge, reference)?
  test.eq(extra_relation.unexpected_edges, 1)?
  test.ok(!extra_relation.exact)?
  let duplicate_relation = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 0, "slave_indices"], [2, 2])?)?
  test.error_kind(report_checks.compare_block_devices(duplicate_relation, reference), "SystemReportCheckError.Invalid")?
  let missing = report_checks.compare_block_devices("""{"storage":{"devices":[]}}""", reference)?
  test.eq(missing.missing_names, ["dm-0", "sda", "sda1"])?
  test.ok(!missing.exact)?
  test.ok(report_checks.block_reference_stable(reference, report_checks.parse_lsblk_json(output)?))?
}

proc test_system_report_lsblk_json_preserves_sparse_partition_identity_and_parent_edges() [error] {
  let output = """{"blockdevices":[{"name":"sda","kname":"sda","maj:min":"8:0","size":524288,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096,"children":[{"name":"sda1","kname":"sda1","maj:min":"8:1","size":65536,"type":"part","pkname":"sda","ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096},{"name":"sda3","kname":"sda3","maj:min":"8:3","size":65536,"type":"part","pkname":"sda","ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096}]}]}"""
  let reference = report_checks.parse_lsblk_json(output)?
  test.eq(reference.devices.len(), 3)?
  test.eq(reference.edges.len(), 2)?
  test.eq((reference.devices |> where .name == "sda2").len(), 0)?
  let candidate = """{"storage":{"devices":[{"name":"sda3","major":8,"minor":3,"kind":"partition","size_bytes":65536,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":true,"read_only":false,"parent_device_index":1,"holder_indices":[],"slave_indices":[]},{"name":"sda","major":8,"minor":0,"kind":"disk","size_bytes":524288,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":true,"read_only":false,"parent_device_index":null,"holder_indices":[],"slave_indices":[]},{"name":"sda1","major":8,"minor":1,"kind":"partition","size_bytes":65536,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":true,"read_only":false,"parent_device_index":1,"holder_indices":[],"slave_indices":[]}]}}"""
  let exact = report_checks.compare_block_devices(candidate, reference)?
  test.ok(exact.exact)?
  test.eq(exact.matched_edges, 2)?
  let wrong_parent = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 0, "parent_device_index"], null)?)?
  test.ok(!report_checks.compare_block_devices(wrong_parent, reference)?.exact)?
}

proc test_system_report_lspci_vmm_numeric_identity_keeps_repeated_ids_distinct() [error] {
  let first = "Slot:\t0000:00:1f.6\nClass:\t0600\nVendor:\t8086\nDevice:\t1234\nSVendor:\t8086\nSDevice:\t0001\nRev:\t01\nProgIf:\t00\nDriver:\tpcieport\nNUMANode:\t0\nIOMMUGroup:\t42\n"
  let second = "Slot:\t0001:02:03.0\nClass:\t0600\nVendor:\t8086\nDevice:\t1234\nProgIf:\t00\n"
  let reference = report_checks.parse_lspci_vmm_numeric(first + "\n" + second + "\n")?
  test.eq(reference.len(), 2)?
  test.eq(reference[0].address, "0000:00:1f.6")?
  test.eq(reference[1].address, "0001:02:03.0")?
  test.eq(reference[0].vendor_id, reference[1].vendor_id)?
  test.eq(reference[0].class_code, 393216)?
  test.ok(report_checks.pci_reference_stable(reference, report_checks.parse_lspci_vmm_numeric(second + "\n" + first + "\n")?))?
  let candidate = """{"pci":{"status":{"state":"complete","enumeration_succeeded":true},"functions":[{"address":"0001:02:03.0","domain":1,"bus":2,"device":3,"function":0,"vendor_id":32902,"device_id":4660,"class_code":393216,"revision":0,"subsystem_vendor_id":null,"subsystem_device_id":null,"driver":null,"numa_node":null,"iommu_group":null},{"address":"0000:00:1f.6","domain":0,"bus":0,"device":31,"function":6,"vendor_id":32902,"device_id":4660,"class_code":393216,"revision":1,"subsystem_vendor_id":32902,"subsystem_device_id":1,"driver":"pcieport","numa_node":0,"iommu_group":"42"}]}}"""
  let exact = report_checks.compare_lspci_identity(candidate, reference)?
  test.ok(exact.exact_static)?
  test.eq(exact.matched_count, 2)?
  let changed_vendor = json.encode(json.set(json.decode(candidate)?, ["pci", "functions", 0, "vendor_id"], 32903)?)?
  let wrong_vendor = report_checks.compare_lspci_identity(changed_vendor, reference)?
  test.ok(!wrong_vendor.exact_static)?
  test.eq(wrong_vendor.field_mismatches, ["0001:02:03.0.vendor_id"])?
  let changed_driver = json.encode(json.set(json.decode(candidate)?, ["pci", "functions", 1, "driver"], "other")?)?
  test.eq(report_checks.compare_lspci_identity(changed_driver, reference)?.field_mismatches, ["0000:00:1f.6.driver"])?
  let missing_vendor = json.encode(json.set(json.decode(candidate)?, ["pci", "functions", 0, "vendor_id"], null)?)?
  test.ok(report_checks.compare_lspci_identity(missing_vendor, reference)?.candidate_field_missing)?
  test.ok(!report_checks.pci_reference_stable(reference, report_checks.parse_lspci_vmm_numeric(first.replace("Driver:\tpcieport", "Driver:\tother") + "\n" + second + "\n")?))?
  let no_prog_if = report_checks.parse_lspci_vmm_numeric(first + "\n" + second.replace("ProgIf:\t00\n", "") + "\n")?
  let candidate_unknown_prog_if = json.encode(json.set(json.decode(candidate)?, ["pci", "functions", 0, "class_code"], 393217)?)?
  test.ok(report_checks.compare_lspci_identity(candidate_unknown_prog_if, no_prog_if)?.exact_static)?
  test.error_kind(report_checks.parse_lspci_vmm_numeric(first + "\n" + first + "\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_lspci_vmm_numeric(first.replace("Class:\t0600", "Class:\t06zz") + "\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_lspci_vmm_numeric(first.replace("NUMANode:\t0", "NUMANode:\t0x1") + "\n"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_lsblk_json_rejects_incomplete_or_unsafe_rows() [error] {
  for output in [
    """{"blockdevices":[{"name":"sda"}]}""",
    """{"blockdevices":[{"name":"sda","kname":"sda","maj:min":"8:0","size":9007199254740992,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096}]}""",
    """{"blockdevices":[{"name":"sda","kname":"sda","maj:min":"8:0","size":4096,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096},{"name":"duplicate","kname":"sda","maj:min":"8:1","size":4096,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096}]}""",
    """{"blockdevices":[{"name":"sda","kname":"sda","maj:min":"8:0","size":4096,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096},{"name":"sdb","kname":"sdb","maj:min":"08:0","size":4096,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096}]}""",
  ] {
    test.error_kind(report_checks.parse_lsblk_json(output), "SystemReportCheckError.Invalid")?
  }
}

proc test_system_report_lsblk_json_preserves_shared_tree_edges() [error] {
  let first = """{"name":"sda","kname":"sda","maj:min":"8:0","size":8192,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096,"children":[{"name":"cryptroot","kname":"dm-0","maj:min":"253:0","size":4096,"type":"crypt","pkname":"sda","ro":false,"rm":false,"rota":false,"log-sec":512,"phy-sec":4096}]}"""
  let second = """{"name":"sdb","kname":"sdb","maj:min":"8:16","size":8192,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096,"children":[{"name":"cryptroot","kname":"dm-0","maj:min":"253:0","size":4096,"type":"crypt","pkname":"sdb","ro":false,"rm":false,"rota":false,"log-sec":512,"phy-sec":4096}]}"""
  let before = report_checks.parse_lsblk_json("{\"blockdevices\":[" + first + "," + second + "]}")?
  let after = report_checks.parse_lsblk_json("{\"blockdevices\":[" + second + "," + first + "]}")?
  test.eq(before.devices.len(), 3)?
  test.eq(before.edges.len(), 2)?
  test.ok(report_checks.block_reference_stable(before, after))?
  let changed = report_checks.parse_lsblk_json("{\"blockdevices\":[" + first + "]}")?
  test.ok(!report_checks.block_reference_stable(before, changed))?
}

proc test_system_report_lsblk_queue_json_scores_supported_fields() [error] {
  let output = """{"blockdevices":[{"kname":"sda","sched":"mq-deadline","ra":128,"disc-gran":4096,"disc-max":1048576,"model":"Fixture Disk   ","rev":"1.0"},{"kname":"loop0","sched":null,"ra":null,"disc-gran":null,"disc-max":null,"model":null,"rev":null}]}"""
  let reference = report_checks.parse_lsblk_queue_json(output)?
  test.eq(reference.len(), 2)?
  test.eq(reference[0].model, "Fixture Disk")?
  test.eq(reference[0].revision_hint, "1.0")?
  let candidate = """{"storage":{"devices":[{"name":"loop0","active_scheduler":null,"read_ahead_kb":null,"discard_granularity_bytes":null,"discard_max_bytes":null,"model":{"state":"absent","value":null}},{"name":"sda","active_scheduler":"mq-deadline","read_ahead_kb":128,"discard_granularity_bytes":4096,"discard_max_bytes":1048576,"model":{"state":"observed","value":"Fixture Disk"}}]}}"""
  let exact = report_checks.compare_block_queue_fields(candidate, reference)?
  test.ok(exact.exact)?
  test.eq(exact.matched_count, 2)?
  let wrong = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 1, "read_ahead_kb"], 256)?)?
  let compared = report_checks.compare_block_queue_fields(wrong, reference)?
  test.eq(compared.read_ahead_mismatches, 1)?
  test.ok(!compared.exact)?
}

proc test_system_report_lsblk_queue_json_rejects_duplicate_and_unsafe_rows() [error] {
  let row = """{"kname":"sda","sched":"none","ra":128,"disc-gran":4096,"disc-max":1048576,"model":null,"rev":null}"""
  test.error_kind(report_checks.parse_lsblk_queue_json("{\"blockdevices\":[" + row + "," + row + "]}"), "SystemReportCheckError.Invalid")?
  let unsafe = """{"kname":"sda","sched":"none","ra":9007199254740992,"disc-gran":4096,"disc-max":1048576,"model":null,"rev":null}"""
  test.error_kind(report_checks.parse_lsblk_queue_json("{\"blockdevices\":[" + unsafe + "]}"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_block_queue_raw_sources_bracket_counters_and_firmware() [fs, error] {
  let output = """{"blockdevices":[{"kname":"sda","sched":"none","ra":128,"disc-gran":4096,"disc-max":1048576,"model":"Fixture Disk","rev":null}]}"""
  let queue = report_checks.parse_lsblk_queue_json(output)?
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"sys/class/block/sda/device", parents: true)?
  fs.root_write(root, p"sys/class/block/sda/device/firmware_rev", "firmware-7\n")?
  fs.root_write(root, p"sys/class/block/sda/stat", "10 0 8 1 2 0 16 2 0 3 4\n")?
  let before = report_checks.read_block_queue_sources(root, queue)?
  test.eq(before[0].firmware, "firmware-7")?
  test.eq(before[0].counters.len(), 11)?
  fs.root_write(root, p"sys/class/block/sda/stat", "12 0 8 1 2 0 16 2 0 3 4\n")?
  let after = report_checks.read_block_queue_sources(root, queue)?
  let candidate = """{"storage":{"devices":[{"name":"sda","firmware":{"state":"observed","value":"firmware-7"},"io_counters":[{"name":"read_ios","value":11,"unit":"requests"},{"name":"read_merges","value":0,"unit":"requests"},{"name":"read_sectors","value":8,"unit":"sectors"},{"name":"read_ms","value":1,"unit":"milliseconds"},{"name":"write_ios","value":2,"unit":"requests"},{"name":"write_merges","value":0,"unit":"requests"},{"name":"write_sectors","value":16,"unit":"sectors"},{"name":"write_ms","value":2,"unit":"milliseconds"},{"name":"in_flight","value":0,"unit":"requests"},{"name":"io_ms","value":3,"unit":"milliseconds"},{"name":"weighted_io_ms","value":4,"unit":"milliseconds"}]}]}}"""
  let compared = report_checks.compare_block_queue_sources(candidate, before, after)?
  test.ok(compared.exact)?
  test.eq(compared.counter_mismatches, 0)?
  let outside = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 0, "io_counters", 0, "value"], 13)?)?
  test.eq(report_checks.compare_block_queue_sources(outside, before, after)?.counter_mismatches, 1)?
  let firmware_wrong = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 0, "firmware", "value"], "other")?)?
  test.eq(report_checks.compare_block_queue_sources(firmware_wrong, before, after)?.firmware_mismatches, 1)?
  let gauge_changed = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 0, "io_counters", 8, "value"], 1)?)?
  test.ok(report_checks.compare_block_queue_sources(gauge_changed, before, after)?.unstable)?
  fs.root_write(root, p"sys/class/block/sda/stat", "12 0 8 1 2 0 16 2 0 3 4 5 6 7 8 9 10\n")?
  test.eq(report_checks.read_block_queue_sources(root, queue)?[0].counters.len(), 17)?
}

proc test_system_report_block_queue_raw_sources_reject_incomplete_and_unsafe_stats() [fs, error] {
  let queue = report_checks.parse_lsblk_queue_json("""{"blockdevices":[{"kname":"sda","sched":null,"ra":null,"disc-gran":null,"disc-max":null,"model":null,"rev":null}]}""")?
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"sys/class/block/sda", parents: true)?
  fs.root_write(root, p"sys/class/block/sda/stat", "1 0 8 1 2 0 16 2 0 3 4 5 6\n")?
  test.error_kind(report_checks.read_block_queue_sources(root, queue), "SystemReportCheckError.Invalid")?
  fs.root_write(root, p"sys/class/block/sda/stat", "9007199254740992 0 8 1 2 0 16 2 0 3 4\n")?
  test.error_kind(report_checks.read_block_queue_sources(root, queue), "SystemReportCheckError.Invalid")?
  let unsafe_name = report_checks.parse_lsblk_queue_json("""{"blockdevices":[{"kname":"../outside","sched":null,"ra":null,"disc-gran":null,"disc-max":null,"model":null,"rev":null}]}""")?
  test.error_kind(report_checks.read_block_queue_sources(root, unsafe_name), "SystemReportCheckError.Invalid")?
}

proc test_system_report_findmnt_json_scores_repeated_targets_and_redaction() [error] {
  let output = """{"filesystems":[{"id":12,"parent":1,"maj:min":"8:1","fsroot":"/","target":"/mnt/data","fstype":"ext4","source":"/dev/sda1","vfs-options":"rw,relatime","fs-options":"rw,errors=remount-ro,password=private","propagation":"private"},{"id":13,"parent":12,"maj:min":"0:2","fsroot":"/","target":"/mnt/data","fstype":"cifs","source":"//user:private@server/share","vfs-options":"rw,nosuid","fs-options":"rw","propagation":"shared"}]}"""
  let reference = report_checks.parse_findmnt_json(output)?
  test.eq(reference.len(), 2)?
  test.eq(reference[1].parent_id, 12)?
  let nsfs = """{"filesystems":[{"id":320,"parent":28,"maj:min":"0:4","fsroot":"net:[4026532945]","target":"/run/netns/demo","fstype":"nsfs","source":"nsfs","vfs-options":"rw","fs-options":"rw","propagation":"private"}]}"""
  test.eq(report_checks.parse_findmnt_json(nsfs)?[0].root, "net:[4026532945]")?
  let candidate = """{"storage":{"mounts":[{"mount_id":13,"parent_id":12,"major":0,"minor":2,"root":{"state":"observed","value":"/"},"target":{"state":"observed","value":"/mnt/data"},"mount_options":["rw","nosuid"],"optional_fields":["shared:8"],"filesystem":"cifs","source":{"state":"redacted","value":null},"super_options":["rw"]},{"mount_id":12,"parent_id":1,"major":8,"minor":1,"root":{"state":"observed","value":"/"},"target":{"state":"observed","value":"/mnt/data"},"mount_options":["rw","relatime"],"optional_fields":[],"filesystem":"ext4","source":{"state":"observed","value":"/dev/sda1"},"super_options":["rw","errors=remount-ro","redacted"]}]}}"""
  let exact = report_checks.compare_mounts(candidate, reference)?
  test.ok(exact.exact)?
  test.eq(exact.matched_count, 2)?
  let unrelated_optional = json.encode(json.set(json.decode(candidate)?, ["storage", "mounts", 1, "optional_fields"], ["redacted"])?)?
  test.ok(report_checks.compare_mounts(unrelated_optional, reference)?.exact)?
  let changed = json.encode(json.set(json.decode(candidate)?, ["storage", "mounts", 0, "parent_id"], 1)?)?
  let mismatch = report_checks.compare_mounts(changed, reference)?
  test.eq(mismatch.parent_mismatches, 1)?
  test.ok(!mismatch.exact)?
  let unredacted = json.encode(json.set(json.decode(candidate)?, ["storage", "mounts", 0, "source"], {state: "observed", value: "//user:private@server/share"})?)?
  test.eq(report_checks.compare_mounts(unredacted, reference)?.source_mismatches, 1)?
  test.ok(report_checks.mount_reference_stable(reference, report_checks.parse_findmnt_json(output)?))?
}

proc test_system_report_findmnt_json_rejects_ambiguous_rows() [error] {
  let base = """{"id":12,"parent":1,"maj:min":"8:1","fsroot":"/","target":"/mnt/data","fstype":"ext4","source":"/dev/sda1","vfs-options":"rw","fs-options":"rw","propagation":"private"}"""
  test.error_kind(report_checks.parse_findmnt_json("{\"filesystems\":[" + base + "," + base + "]}"), "SystemReportCheckError.Invalid")?
  let malformed = """{"id":12,"parent":1,"maj:min":"8:x","fsroot":"/","target":"/mnt/data","fstype":"ext4","source":"/dev/sda1","vfs-options":"rw","fs-options":"rw","propagation":"private"}"""
  test.error_kind(report_checks.parse_findmnt_json("{\"filesystems\":[" + malformed + "]}"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_findmnt_usage_scores_safe_mounts_and_explicit_skips() [error] {
  let mounts = report_checks.parse_findmnt_json("""{"filesystems":[{"id":1,"parent":0,"maj:min":"8:1","fsroot":"/","target":"/","fstype":"ext4","source":"/dev/sda1","vfs-options":"rw","fs-options":"rw","propagation":"private"},{"id":2,"parent":1,"maj:min":"0:2","fsroot":"/","target":"/auto","fstype":"autofs","source":"autofs","vfs-options":"rw","fs-options":"rw","propagation":"private"},{"id":3,"parent":2,"maj:min":"8:3","fsroot":"/","target":"/auto/local","fstype":"ext4","source":"/dev/sdb1","vfs-options":"rw","fs-options":"rw","propagation":"private"},{"id":4,"parent":1,"maj:min":"0:4","fsroot":"/","target":"/shared","fstype":"tmpfs","source":"tmpfs","vfs-options":"rw","fs-options":"rw","propagation":"private"},{"id":5,"parent":1,"maj:min":"0:5","fsroot":"/","target":"/shared","fstype":"tmpfs","source":"tmpfs","vfs-options":"rw","fs-options":"rw","propagation":"private"}]}""")?
  test.eq(report_checks.mount_usage_eligible_ids(mounts), [1])?
  let before = [report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":1,"size":1000,"used":300,"avail":650}]}""", 1)?]
  let after = [report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":1,"size":1000,"used":320,"avail":630}]}""", 1)?]
  let candidate = """{"storage":{"mounts":[{"mount_id":1,"usage_state":"observed","usage_total_bytes":1000,"usage_used_bytes":310,"usage_available_bytes":640},{"mount_id":2,"usage_state":"not_requested","usage_total_bytes":null,"usage_used_bytes":null,"usage_available_bytes":null},{"mount_id":3,"usage_state":"not_requested","usage_total_bytes":null,"usage_used_bytes":null,"usage_available_bytes":null},{"mount_id":4,"usage_state":"not_requested","usage_total_bytes":null,"usage_used_bytes":null,"usage_available_bytes":null},{"mount_id":5,"usage_state":"not_requested","usage_total_bytes":null,"usage_used_bytes":null,"usage_available_bytes":null}]}}"""
  test.ok(report_checks.compare_mount_usage(candidate, mounts, before, after)?.exact)?
  let unsafe_value = json.encode(json.set(json.decode(candidate)?, ["storage", "mounts", 3, "usage_state"], "observed")?)?
  test.ok(!report_checks.compare_mount_usage(unsafe_value, mounts, before, after)?.exact)?
  let wrong_usage = json.encode(json.set(json.decode(candidate)?, ["storage", "mounts", 0, "usage_used_bytes"], 321)?)?
  test.ok(!report_checks.compare_mount_usage(wrong_usage, mounts, before, after)?.exact)?
}

proc test_system_report_findmnt_usage_rejects_ambiguous_or_unsafe_rows() [error] {
  test.error_kind(report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":1,"size":100,"used":20,"avail":80},{"id":1,"size":100,"used":20,"avail":80}]}""", 1), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":1,"size":100,"used":101,"avail":0}]}""", 1), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":2,"size":100,"used":20,"avail":80}]}""", 1), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":1,"size":null,"used":20,"avail":null}]}""", 1), "SystemReportCheckError.Invalid")?
}

proc test_system_report_findmnt_usage_keeps_unavailable_capacity_explicit() [error] {
  let mounts = report_checks.parse_findmnt_json("""{"filesystems":[{"id":8,"parent":0,"maj:min":"0:8","fsroot":"/","target":"/opaque","fstype":"overlay","source":"overlay","vfs-options":"rw","fs-options":"rw","propagation":"private"}]}""")?
  let unavailable = report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":8,"size":null,"used":null,"avail":null}]}""", 8)?
  let candidate = """{"storage":{"mounts":[{"mount_id":8,"usage_state":"disappeared","usage_total_bytes":null,"usage_used_bytes":null,"usage_available_bytes":null}]}}"""
  test.ok(report_checks.compare_mount_usage(candidate, mounts, [unavailable], [unavailable])?.exact)?
  let invented = json.encode(json.set(json.decode(candidate)?, ["storage", "mounts", 0, "usage_state"], "observed")?)?
  test.ok(!report_checks.compare_mount_usage(invented, mounts, [unavailable], [unavailable])?.exact)?
  let observed = report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":8,"size":100,"used":20,"avail":80}]}""", 8)?
  test.ok(report_checks.compare_mount_usage(candidate, mounts, [unavailable], [observed])?.unstable)?
}

proc test_system_report_lsmod_reference_scores_module_values_and_state() [error] {
  let formatted = "Module                  Size  Used by\nalpha                  4096  2 beta,gamma\nbeta                   8192  0\n"
  let raw = "alpha 4096 2 beta,gamma, Live 0x0\nbeta 8192 0 - Loading 0x0\n"
  let reference = report_checks.parse_lsmod_reference(formatted, raw)?
  test.eq(reference.len(), 2)?
  test.eq(reference[0].state, "Live")?
  let candidate = """{"kernel":{"modules":[{"name":"beta","size_bytes":8192,"users":0,"state":"Loading"},{"name":"alpha","size_bytes":4096,"users":2,"state":"Live"}]}}"""
  let exact = report_checks.compare_kernel_modules(candidate, reference)?
  test.ok(exact.exact)?
  test.eq(exact.matched_count, 2)?
  let changed = json.encode(json.set(json.decode(candidate)?, ["kernel", "modules", 0, "state"], "Live")?)?
  let mismatch = report_checks.compare_kernel_modules(changed, reference)?
  test.eq(mismatch.state_mismatches, 1)?
  test.ok(!mismatch.exact)?
  let missing = report_checks.compare_kernel_modules("""{"kernel":{"modules":[]}}""", reference)?
  test.eq(missing.missing_names, ["alpha", "beta"])?
  test.ok(!missing.exact)?
  test.ok(report_checks.kernel_module_reference_stable(reference, report_checks.parse_lsmod_reference(formatted, raw)?))?
}

proc test_system_report_lsmod_reference_rejects_conflicting_or_unsafe_rows() [error] {
  let header = "Module Size Used by\n"
  test.error_kind(report_checks.parse_lsmod_reference(header + "alpha 4096 0\n", "alpha 4096 1 - Live 0x0\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_lsmod_reference(header + "alpha 9007199254740992 0\n", "alpha 9007199254740992 0 - Live 0x0\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_lsmod_reference(header + "alpha 4096 0\nalpha 4096 0\n", "alpha 4096 0 - Live 0x0\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_lsmod_reference(header + "alpha 4096 0\n", "beta 4096 0 - Live 0x0\n"), "SystemReportCheckError.Invalid")?
}

proc test_system_report_kernel_command_line_reference_preserves_bytes_and_redaction() [error] {
  let raw = b"  root=UUID=private  quiet  \n"
  let sensitive = json.encode({kernel: {command_line: {state: "observed", value: "  root=UUID=private  quiet  \n", raw_bytes_base64: null}}})?
  let redacted = json.encode({kernel: {command_line: {state: "redacted", value: null, raw_bytes_base64: null}}})?
  test.ok(report_checks.compare_kernel_command_line(sensitive, redacted, raw)?.exact)?
  let trimmed = json.encode({kernel: {command_line: {state: "observed", value: "root=UUID=private  quiet", raw_bytes_base64: null}}})?
  test.ok(!report_checks.compare_kernel_command_line(trimmed, redacted, raw)?.exact)?
  test.ok(!report_checks.compare_kernel_command_line(sensitive, sensitive, raw)?.exact)?

  let malformed = b"\xff private\n"
  let encoded = malformed.base64()
  let raw_sensitive = json.encode({kernel: {command_line: {state: "malformed", value: null, raw_bytes_base64: encoded}}})?
  test.ok(report_checks.compare_kernel_command_line(raw_sensitive, redacted, malformed)?.exact)?
  let leaked = json.encode({kernel: {command_line: {state: "redacted", value: null, raw_bytes_base64: encoded}}})?
  test.ok(!report_checks.compare_kernel_command_line(raw_sensitive, leaked, malformed)?.exact)?
}

proc test_system_report_kernel_parameter_reference_scores_values_and_absence() [error] {
  let reference: List[KernelParameterReferenceFixture] = [
    {name: "kernel.pid_max", source: "sysctl", state: "observed", value: "4194304", raw_bytes_base64: null},
    {name: "intel_pstate.no_turbo", source: "module", state: "absent", value: null, raw_bytes_base64: null},
  ]
  let candidate = json.encode({kernel: {
    sysctls: [{name: "kernel.pid_max", value: {state: "observed", value: "4194304", raw_bytes_base64: null}}],
    parameters: [{name: "intel_pstate.no_turbo", value: {state: "absent", value: null, raw_bytes_base64: null}}],
  }})?
  test.ok(report_checks.compare_kernel_parameters(candidate, reference)?.exact)?
  let wrong = json.encode(json.set(json.decode(candidate)?, ["kernel", "sysctls", 0, "value", "value"], "4194303")?)?
  test.ok(!report_checks.compare_kernel_parameters(wrong, reference)?.exact)?
  let invented = json.encode(json.set(json.decode(candidate)?, ["kernel", "parameters", 0, "value", "state"], "observed")?)?
  test.ok(!report_checks.compare_kernel_parameters(invented, reference)?.exact)?
}

proc test_system_report_kernel_parameter_reference_rejects_duplicate_or_unexpected_names() [error] {
  let reference = [{name: "kernel.pid_max", source: "sysctl", state: "observed", value: "4194304", raw_bytes_base64: null}]
  test.error_kind(report_checks.compare_kernel_parameters("{}", reference.extend(reference)), "SystemReportCheckError.Invalid")?
  let invalid = [{name: "kernel.pid_max", source: "sysctl", state: "absent", value: "4194304", raw_bytes_base64: null}]
  test.error_kind(report_checks.compare_kernel_parameters("{}", invalid), "SystemReportCheckError.Invalid")?
  let repeated = json.encode({kernel: {sysctls: [
    {name: "kernel.pid_max", value: {state: "observed", value: "4194304", raw_bytes_base64: null}},
    {name: "kernel.pid_max", value: {state: "observed", value: "4194304", raw_bytes_base64: null}},
  ], parameters: []}})?
  test.error_kind(report_checks.compare_kernel_parameters(repeated, reference), "SystemReportCheckError.Invalid")?
  let extra = json.encode({kernel: {sysctls: [
    {name: "kernel.pid_max", value: {state: "observed", value: "4194304", raw_bytes_base64: null}},
    {name: "kernel.hostname", value: {state: "observed", value: "private", raw_bytes_base64: null}},
  ], parameters: []}})?
  test.ok(!report_checks.compare_kernel_parameters(extra, reference)?.exact)?
}

proc test_system_report_identity_comparison_scores_release_and_architecture_separately() [error] {
  let matching = """{"identity":{"kernel_release":"6.1-test","architecture":"aarch64"}}"""
  let exact = report_checks.compare_identity(matching, "6.1-test", "aarch64")?
  test.ok(exact.release_exact)?
  test.ok(exact.architecture_exact)?
  test.ok(exact.exact)?

  let incomplete = """{"identity":{"kernel_release":null,"architecture":"x86_64"}}"""
  let compared = report_checks.compare_identity(incomplete, "6.1-test", "aarch64")?
  test.ok(compared.release_missing)?
  test.ok(!compared.release_exact)?
  test.ok(!compared.architecture_missing)?
  test.ok(!compared.architecture_exact)?
  test.ok(!compared.exact)?
}

proc test_system_report_os_release_reference_decodes_data_and_requires_candidate_agreement() [error] {
  let reference = report_checks.parse_reference_os_release("ID=example\nVERSION_ID=\"2026\\\"release\"\n")?
  test.eq(reference.id, "example")?
  test.eq(reference.version_id, "2026\"release")?
  let matching = """{"identity":{"os_release":{"id":"example","version_id":"2026\\"release"}}}"""
  test.ok(report_checks.compare_os_release(matching, reference)?.exact)?
  let missing = """{"identity":{"os_release":null}}"""
  let compared = report_checks.compare_os_release(missing, reference)?
  test.ok(compared.id_missing)?
  test.ok(compared.version_id_missing)?
  test.ok(!compared.exact)?
  let absent_version = report_checks.parse_reference_os_release("ID=example\n")?
  test.ok(report_checks.compare_os_release("""{"identity":{"os_release":{"id":"example","version_id":null}}}""", absent_version)?.exact)?
  let wrong_version = report_checks.compare_os_release("""{"identity":{"os_release":{"id":"example","version_id":"other"}}}""", reference)?
  test.ok(wrong_version.id_exact)?
  test.ok(!wrong_version.version_id_exact)?
  test.ok(!wrong_version.exact)?
  test.error_kind(report_checks.parse_reference_os_release("VERSION_ID=1\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=\"unterminated\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=Unquoted Name\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=path/segment\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=pipe|value\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=escaped\\ space\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID= leading\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=trailing \n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=\"quoted\" \n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=\"Not A Distro\"\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=Ubuntu\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=example\nVERSION_ID=\"unescaped $value\"\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release(" ID =example\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_os_release("ID=example\nnot-an-assignment\n"), "SystemReportCheckError.Invalid")?
  test.eq(report_checks.parse_reference_os_release("ID=example\nVERSION_ID=\"v\\$token\"\n")?.version_id, "v$token")?
  test.eq(report_checks.parse_reference_os_release("ID=example\nVERSION_ID=v1.2-release_3\n")?.version_id, "v1.2-release_3")?
  let repeated = report_checks.parse_reference_os_release("# local source\nID=first\nID=second\nVERSION_ID=2\n")?
  test.eq(repeated.id, "second")?
  test.eq(repeated.version_id, "2")?
}

proc test_system_report_device_tree_reference_requires_exact_terminated_bytes_and_order() [error] {
  test.eq(report_checks.parse_reference_od_bytes(" 41 52 4d 00\n", 4)?, b"ARM\0")?
  test.error_kind(report_checks.parse_reference_od_bytes(" 41 5g 00\n", 4), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_od_bytes(" 41 52 4d 00 00\n", 4), "SystemReportCheckError.Invalid")?
  let reference = report_checks.parse_reference_device_tree(b"ARM Board\0", b"vendor,board\0arm,v8\0")?
  test.eq(reference.model, "ARM Board")?
  test.eq(reference.compatible, ["vendor,board", "arm,v8"])?
  test.error_kind(report_checks.parse_reference_device_tree(b"ARM Board", b"vendor,board\0"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_device_tree(b"ARM\0Board\0", b"vendor,board\0"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_device_tree(b"ARM Board\0", b"vendor,board\0\0"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_device_tree(bytes.from_ints([255, 0])?, b"vendor,board\0"), "SystemReportCheckError.Invalid")?
  let matching = """{"identity":{"firmware":{"source":"device-tree","device_tree_model":{"state":"observed","value":"ARM Board"},"device_tree_compatible":[{"state":"observed","value":"vendor,board"},{"state":"observed","value":"arm,v8"}]}} ,"issues":[]}"""
  test.ok(report_checks.compare_device_tree(matching, reference)?.exact)?
  let reordered = """{"identity":{"firmware":{"source":"device-tree","device_tree_model":{"state":"observed","value":"ARM Board"},"device_tree_compatible":[{"state":"observed","value":"arm,v8"},{"state":"observed","value":"vendor,board"}]}} ,"issues":[]}"""
  test.ok(!report_checks.compare_device_tree(reordered, reference)?.exact)?
  let missing = """{"identity":{"firmware":{"source":"dmi","device_tree_model":{"state":"absent","value":null},"device_tree_compatible":[]}} ,"issues":[]}"""
  test.ok(!report_checks.compare_device_tree(missing, reference)?.exact)?
  let compatible_only = report_checks.parse_reference_device_tree(null, b"vendor,board\0")?
  let compatible_candidate = """{"identity":{"firmware":{"source":"device-tree","device_tree_model":{"state":"absent","value":null},"device_tree_compatible":[{"state":"observed","value":"vendor,board"}]}} ,"issues":[]}"""
  test.ok(report_checks.compare_device_tree(compatible_candidate, compatible_only)?.exact)?
  let model_only = report_checks.parse_reference_device_tree(b"ARM\0", null)?
  let malformed_optional = """{"identity":{"firmware":{"source":"device-tree","device_tree_model":{"state":"observed","value":"ARM"},"device_tree_compatible":[]}} ,"issues":[{"field":"firmware.device_tree_compatible"}]}"""
  test.ok(!report_checks.compare_device_tree(malformed_optional, model_only)?.exact)?
}

proc test_system_report_device_tree_od_reference_reads_bounded_raw_source() [fs, process, time, error] {
  let source = fs.tempdir()?
  defer fs.close_root(source)?
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  fs.root_write(source, p"model", b"ARM\0")?
  let source_root = fs.root_path(source)?
  let source_path = fp"${source_root}/model"
  let observed = report_checks.read_device_tree_raw_reference(scratch, source_path, 4, "model-before")?
  test.eq(observed.state, "observed")?
  test.eq(observed.data, b"ARM\0")?
  test.ok(observed.ended >= observed.started)?
  fs.root_write(source, p"model", b"ARM\0X")?
  test.error_kind(report_checks.read_device_tree_raw_reference(scratch, source_path, 4, "model-oversize"), "SystemReportCheckError.Invalid")?
  fs.root_remove(source, p"model")?
  let absent = report_checks.read_device_tree_raw_reference(scratch, source_path, 4, "model-absent")?
  test.eq(absent.state, "absent")?
  test.eq(absent.data, null)?
}

proc test_system_report_live_reference_rejects_synthetic_candidate_mode() [error] {
  report_checks.require_live_linux_report("""{"source_mode":"live_linux"}""")?
  test.error_kind(report_checks.require_live_linux_report("""{"source_mode":"synthetic_fixture"}"""), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.require_live_linux_report("""{"source_mode":null}"""), "SystemReportCheckError.Invalid")?
}

proc test_system_report_uptime_reference_requires_a_bracketed_integer_second() [error] {
  test.eq(report_checks.parse_reference_uptime_seconds("123.456 987.654\n")?, 123)?
  test.error_kind(report_checks.parse_reference_uptime_seconds("12x.5 4.0"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_uptime_seconds("123.456 invalid"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_uptime_seconds(""), "SystemReportCheckError.Invalid")?
  let candidate = """{"identity":{"uptime_seconds":124}}"""
  let bracketed = report_checks.compare_uptime(candidate, 123, 125)?
  test.ok(bracketed.bracketed)?
  test.ok(!bracketed.candidate_missing)?
  let missing = report_checks.compare_uptime("""{"identity":{"uptime_seconds":null}}""", 123, 125)?
  test.ok(missing.candidate_missing)?
  test.ok(!missing.bracketed)?
  let outside = report_checks.compare_uptime("""{"identity":{"uptime_seconds":126}}""", 123, 125)?
  test.ok(!outside.bracketed)?
  test.error_kind(report_checks.compare_uptime(candidate, 125, 123), "SystemReportCheckError.Invalid")?
}

proc test_system_report_namespace_reference_requires_observed_exact_targets() [error] {
  let reference = [
    {field: "mount_namespace", target: "mnt:[10]"},
    {field: "time_namespace", target: "time:[20]"},
  ]
  let matching = """{"scope":{"mount_namespace":{"state":"observed","value":"mnt:[10]"},"time_namespace":{"state":"observed","value":"time:[20]"}}}"""
  let exact = report_checks.compare_namespace_scope(matching, reference)?
  test.ok(exact.exact)?
  test.eq(exact.matched_count, 2)?

  let incomplete = """{"scope":{"mount_namespace":{"state":"redacted","value":null},"time_namespace":{"state":"observed","value":"time:[21]"}}}"""
  let compared = report_checks.compare_namespace_scope(incomplete, reference)?
  test.eq(compared.missing_fields, ["mount_namespace"])?
  test.eq(compared.mismatched_fields, ["time_namespace"])?
  test.ok(!compared.exact)?
  let duplicate = [reference[0], reference[0]]
  test.error_kind(report_checks.compare_namespace_scope(matching, duplicate), "SystemReportCheckError.Invalid")?
}
