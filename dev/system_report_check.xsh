##! Fixed-denominator coverage manifest validation and report generation.
use context

error SystemReportCheckError = Invalid(message: Str)

pure check_failure(message: Str) -> SystemReportCheckError {
  return SystemReportCheckError.Invalid(message: message)
}

# The manifest keeps every comparison case independent of candidate output.
type CoverageAssertion = {
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

type CoverageManifest = {
  schema_version: Int,
  producer: Str,
  assertions: List[CoverageAssertion],
  fixture_scenarios: List[Str],
}

type CheckOptions = {
  manifest: Str,
  no_subprocess: Bool,
  xsh_bin: Str,
  script: Str,
}

## Counts required assertions by their declared domain without consulting a report.
export pure summary(assertions: List[CoverageAssertion]) -> Str {
  let groups = assertions |> group-by .domain |> sort-by .key
  var lines = ""
  var mandatory = 0
  var supplemental = 0

  for domain_group in groups {
    var group_mandatory = 0
    var group_supplemental = 0

    for assertion in domain_group.items {
      if assertion.tier == "mandatory" {
        group_mandatory += 1
        mandatory += 1
      } else {
        group_supplemental += 1
        supplemental += 1
      }
    }

    lines = f"${lines}  ${domain_group.key}: ${group_mandatory} mandatory, ${group_supplemental} supplemental\n"
  }

  return f"${lines}total: ${mandatory} mandatory, ${supplemental} supplemental"
}

## Rejects a manifest whose expected denominator can be accidentally reduced.
export pure validate(manifest: CoverageManifest) -> Result[Unit] {
  if manifest.schema_version != 1 {
    return Err(check_failure("unsupported coverage manifest schema"))
  }

  if manifest.producer != "system-report" or manifest.assertions.len() == 0 {
    return Err(check_failure("coverage manifest has no producer or assertions"))
  }

  var seen: List[Str] = []
  var has_mandatory = false
  for assertion in manifest.assertions {
    if assertion.id.trim() == "" or assertion.domain.trim() == "" or assertion.field.trim() == "" {
      return Err(check_failure("coverage assertion has an empty identity or field"))
    }
    if assertion.tier != "mandatory" and assertion.tier != "supplemental" {
      return Err(check_failure(f"unsupported tier for '${assertion.id}'"))
    }
    if assertion.tier == "mandatory" {
      has_mandatory = true
    }
    if assertion.source_abi.trim() == "" or assertion.reference_adapter.trim() == "" or assertion.reference_argv.len() == 0 {
      return Err(check_failure(f"assertion '${assertion.id}' has no source ABI or reference command"))
    }
    if assertion.eligibility.trim() == "" or assertion.equality_rule.trim() == "" or assertion.fixture_scenarios.len() == 0 {
      return Err(check_failure(f"assertion '${assertion.id}' is missing eligibility, comparison, or fixture policy"))
    }
    if assertion.id in seen {
      return Err(check_failure(f"duplicate assertion id '${assertion.id}'"))
    }
    seen = seen.push(assertion.id)
  }

  if ! has_mandatory {
    return Err(check_failure("coverage manifest declares no mandatory assertions"))
  }

  var seen_scenarios: List[Str] = []
  for scenario in manifest.fixture_scenarios {
    if scenario.trim() == "" {
      return Err(check_failure("coverage manifest has an empty fixture scenario"))
    }
    if scenario in seen_scenarios {
      return Err(check_failure(f"duplicate fixture scenario '${scenario}'"))
    }
    seen_scenarios = seen_scenarios.push(scenario)
  }

  for assertion in manifest.assertions {
    for scenario in assertion.fixture_scenarios {
      if scenario not in manifest.fixture_scenarios {
        return Err(check_failure(f"assertion '${assertion.id}' references undeclared fixture scenario '${scenario}'"))
      }
    }
  }

  return Ok()
}

## Flags child-process creation and secondary execution in a process trace.
export pure process_trace_violations(trace: Str) -> List[Str] {
  var violations: List[Str] = []
  var initial_execs = 0

  for line in trace.lines() {
    if line.contains("execve(") or line.contains("execveat(") {
      initial_execs += 1
      if initial_execs > 1 {
        violations = violations.push("secondary exec syscall")
      }
    }

    if line.contains("fork(") {
      violations = violations.push("fork syscall")
    }
    if line.contains("vfork(") {
      violations = violations.push("vfork syscall")
    }
    if line.contains("clone(") or line.contains("clone3(") {
      if ! line.contains("CLONE_THREAD") {
        violations = violations.push("process clone syscall")
      }
    }
  }

  if initial_execs == 0 {
    violations = violations.push("initial XSH exec was not traced")
  }

  return violations
}

# Traces one production command path with an empty command search path.
proc audit_no_subprocess_case(
  xsh_bin: Str,
  script: Str,
  applet_args: List[Str],
  expected_success: Bool,
  label: Str,
  scratch: FsRoot,
) [fs, process, error] -> Result[Unit] {
  let trace_name = fp"trace-${label}"
  let stdout_name = fp"stdout-${label}"
  let stderr_name = fp"stderr-${label}"
  fs.root_write(scratch, trace_name, "")?
  fs.root_write(scratch, stdout_name, "")?
  fs.root_write(scratch, stderr_name, "")?
  let scratch_path = fs.root_path(scratch)?
  let trace_path = fp"${scratch_path}/${trace_name}"
  let stdout_path = fp"${scratch_path}/${stdout_name}"
  let stderr_path = fp"${scratch_path}/${stderr_name}"
  let strace = "/usr/bin/strace"
  var argv = [
    strace,
    "-f",
    "-qq",
    "-e",
    "trace=process",
    "-o",
    trace_path.display(),
    "--",
  ]
  argv = argv.extend([xsh_bin, script, "--"])
  argv = argv.extend(applet_args)
  let command = process.command_argv(
    strace,
    argv,
    cwd: p"/",
    env: {
      HOME: "/nonexistent",
      PATH: "/nonexistent",
      LANG: "C",
      LC_ALL: "C",
      TERM: "dumb",
      XSH_LINUX_REAL: "1",
    },
    stdout: stdout_path,
    stderr: stderr_path,
  )
  let status = process.run(command)?
  let trace = fs.root_read_text(scratch, trace_name)?
  let output = fs.root_read_text(scratch, stdout_name)?
  let candidate_error = fs.root_read_text(scratch, stderr_name)?
  let violations = process_trace_violations(trace)

  if status.exited_with(0) != expected_success {
    return Err(check_failure(f"${label} command had unexpected exit status: ${candidate_error.trim()}"))
  }
  if violations.len() > 0 {
    return Err(check_failure(violations.join(", ")))
  }
  if expected_success and output.trim() == "" {
    return Err(check_failure(f"${label} emitted no output"))
  }
  if expected_success and applet_args.contains("--json") {
    let _ = json.decode(output)?
  }
  if !expected_success and output.trim() != "" {
    return Err(check_failure(f"${label} wrote stdout before failing"))
  }

  return Ok()
}

# Executes live, help-like, invalid-argument, and malformed-replay paths under strace.
proc audit_no_subprocess(xsh_bin: Str, script: Str) [fs, process, error] -> Result[Unit] {
  if ! xsh_bin.starts_with("/") or ! script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }

  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  fs.root_write(scratch, p"malformed.json", "{invalid")?
  let scratch_path = fs.root_path(scratch)?
  let malformed_path = fp"${scratch_path}/malformed.json"

  audit_no_subprocess_case(xsh_bin, script, ["--json"], true, "live-json", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--version"], true, "version", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--section", "invalid"], false, "invalid-section", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--from", malformed_path.display()], false, "malformed-replay", scratch)?
  return Ok()
}

## Validates and summarizes the checked-in comparison denominator.
export proc validate_and_run(ctx: context.Context, args: List[Str]) [fs, process, error, io] -> Result[Unit] {
  let parsed = cli.parse(args, {
    manifest: {
      form: "--manifest FILE",
      default: "dev/system-report-coverage.json",
    },
    no_subprocess: {
      form: "--no-subprocess",
      default: false,
    },
    xsh_bin: {
      form: "--xsh-bin FILE",
      default: "",
    },
    script: {
      form: "--script FILE",
      default: "",
    },
  })?
  let options: CheckOptions = {
    manifest: parsed.manifest,
    no_subprocess: parsed.no_subprocess,
    xsh_bin: parsed.xsh_bin,
    script: parsed.script,
  }
  let manifest_path = context.repo_path(ctx.root, options.manifest)
  let raw = json.read(manifest_path)?
  let manifest = raw.require(CoverageManifest)?
  validate(manifest)?
  print f"system-report coverage manifest v${manifest.schema_version}: ${manifest.assertions.len()} fixed assertions, ${manifest.fixture_scenarios.len()} fixture scenarios"
  print summary(manifest.assertions)

  if options.no_subprocess {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--no-subprocess requires --xsh-bin and --script"))
    }

    audit_no_subprocess(options.xsh_bin, options.script)?
    print "no-subprocess process traces passed for live JSON, version, invalid-section, and malformed-replay paths"
  } else {
    print "No candidate or reference cases were run by manifest validation."
  }

  return Ok()
}
