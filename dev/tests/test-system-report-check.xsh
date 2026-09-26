use system_report_check as report_checks

type ReportCoverageAssertion = {
  id: Str,
  domain: Str,
  field: Str,
  relation: Str,
  tier: Str,
  source_abi: Str,
  reference_adapter: Str,
  reference_argv: List[Str],
  eligibility: Str,
  equality_rule: Str,
  fixture_scenarios: List[Str],
}

type ReportCoverageManifest = {
  schema_version: Int,
  producer: Str,
  assertions: List[ReportCoverageAssertion],
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
    reference_argv: ["lscpu", "--json"],
    eligibility: "CPUFreq policy exists and is readable",
    equality_rule: "exact CPU ID set",
    fixture_scenarios: ["offline_related_cpu"],
  }
}

pure manifest(assertions: List[ReportCoverageAssertion]) -> ReportCoverageManifest {
  return {
    schema_version: 1,
    producer: "system-report",
    assertions: assertions,
    fixture_scenarios: ["offline_related_cpu"],
  }
}

proc test_system_report_coverage_manifest_contract() [error] {
  let required = assertion("cpu.policy.related-cpus", "mandatory")
  let supplemental = assertion("cpu.policy.governor", "supplemental")
  let valid = manifest([required, supplemental])

  report_checks.validate(valid)?
  let summary = report_checks.summary(valid.assertions)
  test.contains(summary, "cpu: 1 mandatory, 1 supplemental")?
  test.contains(summary, "total: 1 mandatory, 1 supplemental")?

  let duplicate = manifest([required, required])
  test.error_kind(report_checks.validate(duplicate), "system-report-manifest")?

  let no_required_cases = manifest([])
  test.error_kind(report_checks.validate(no_required_cases), "system-report-manifest")?

  let undeclared_scenario = manifest([{...required, fixture_scenarios: ["not-in-manifest"]}])
  test.error_kind(report_checks.validate(undeclared_scenario), "system-report-manifest")?

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
}
