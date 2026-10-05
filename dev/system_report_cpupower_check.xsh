##! Optional cpupower corroboration for CPU frequency and idle metadata.
error CpupowerCheckError = Invalid(message: Str)

pure cpupower_failure(message: Str) -> CpupowerCheckError {
  CpupowerCheckError.Invalid(message:)
}

## Holds only the stable fields exported by the selected cpupower commands.
export type CpupowerReference = {
  driver: Str,
  hardware_min_khz: Int,
  hardware_max_khz: Int,
  idle_driver: Str,
  idle_governor: Str,
  idle_state_names: List[Str],
}

## Retains the explicit frequency-info driver and numeric hardware bounds.
export type CpupowerFrequency = {driver: Str, minimum_khz: Int, maximum_khz: Int}

## Retains the global idle identity and advertised state names.
export type CpupowerIdle = {driver: Str, governor: Str, names: List[Str]}

type CandidatePolicy = {
  name: Str,
  related_cpus: List[Int],
  driver: Str?,
  hardware_min_khz: Int?,
  hardware_max_khz: Int?,
}

type CandidateIdleState = {cpu_id: Int, state_index: Int, name: Str}

type CandidateCpu = {
  frequency_policies: List[CandidatePolicy],
  global_idle_driver: Str?,
  global_idle_governor: Str?,
  idle_states: List[CandidateIdleState],
}

type CandidateReport = {cpu: CandidateCpu}

## Separates unsupported cpupower output from a genuine candidate mismatch.
export type CpupowerComparison = {matched_fields: Int, mismatches: List[Str], partial: List[Str]}

## Records the utility identity, timing, and each output digest for a live comparison.
export type CpupowerRun = {
  version: Str,
  executable: Str,
  comparison: CpupowerComparison,
  reference_started_unix_ms: Int,
  candidate_started_unix_ms: Int,
  driver_sha256_hex: Str,
  limits_sha256_hex: Str,
  idle_sha256_hex: Str,
}

pure one_prefixed_line(output: Str, prefix: Str) -> Result[Str] {
  var values = []
  for line in output.lines() {
    let trimmed = line.trim()
    if trimmed.starts_with(prefix) {
      values += [trimmed.byte_slice(prefix.byte_len()).trim()]
    }
  }

  if values.len() != 1 or values[0] == "" {
    return Err(cpupower_failure(f"cpupower output lacks one {prefix} line"))
  }

  values[0]
}

pure decimal_khz(value: Str) -> Result[Int] {
  if value == "" or value.starts_with("+") or value.starts_with("-") {
    return Err(cpupower_failure("cpupower hardware limit is not an unsigned decimal number"))
  }

  for digit in value {
    if digit not in [
      "0",
      "1",
      "2",
      "3",
      "4",
      "5",
      "6",
      "7",
      "8",
      "9",
    ] {
      return Err(cpupower_failure("cpupower hardware limit is not decimal"))
    }
  }

  let number = value.parse_int()?
  if number > 9007199254740991 {
    return Err(cpupower_failure("cpupower hardware limit exceeds exact JSON range"))
  }

  number
}

## Parses the explicit `frequency-info --driver` and `--hwlimits` forms.
export pure parse_cpupower_frequency(driver_output: Str, limits_output: Str) -> Result[CpupowerFrequency] {
  let driver = one_prefixed_line(driver_output, "driver:")?
  var limits = []
  for line in limits_output.lines() {
    let words = line.trim().split(" ") |> where . != ""
    if words.len() == 2 and (words[0].parse_int() ?? -1) >= 0 and (words[1].parse_int() ?? -1) >= 0 {
      limits += [words]
    }
  }

  if limits.len() != 1 {
    return Err(cpupower_failure("cpupower hardware limits are absent or ambiguous"))
  }

  let minimum = decimal_khz(limits[0][0])?
  let maximum = decimal_khz(limits[0][1])?
  if minimum > maximum {
    return Err(cpupower_failure("cpupower hardware limits are reversed"))
  }

  {driver: driver, minimum_khz: minimum, maximum_khz: maximum}
}

## Parses the driver, governor, and advertised names without using usage counters.
export pure parse_cpupower_idle(output: Str) -> Result[CpupowerIdle] {
  let driver = one_prefixed_line(output, "CPUidle driver:")?
  let governor = one_prefixed_line(output, "CPUidle governor:")?
  let names = one_prefixed_line(output, "Available idle states:")?.split(" ") |> where . != ""
  let count = one_prefixed_line(output, "Number of idle states:")? |> decimal_khz(_)?
  if names.len() != count or count > 256 {
    return Err(cpupower_failure("cpupower idle names do not match the reported count"))
  }

  {driver: driver, governor: governor, names: names}
}

## Finds CPU 0 by policy membership because kernel policy directory indexes can be sparse.
export pure compare_cpupower(candidate_json: Str, reference: CpupowerReference) -> Result[CpupowerComparison] {
  let report = json.decode(candidate_json)?.require(CandidateReport)?
  var mismatches = []
  var partial = []
  var matched = 0
  let policies = report.cpu.frequency_policies |> where 0 in .related_cpus
  if policies.len() != 1 {
    partial += ["cpu0_policy"]
  } else {
    let policy = policies[0]
    if policy.driver == null {
      partial += ["driver"]
    } else if policy.driver != reference.driver {
      mismatches += ["driver"]
    } else {
      matched += 1
    }

    if policy.hardware_min_khz == null {
      partial += ["hardware_min_khz"]
    } else if policy.hardware_min_khz != reference.hardware_min_khz {
      mismatches += ["hardware_min_khz"]
    } else {
      matched += 1
    }

    if policy.hardware_max_khz == null {
      partial += ["hardware_max_khz"]
    } else if policy.hardware_max_khz != reference.hardware_max_khz {
      mismatches += ["hardware_max_khz"]
    } else {
      matched += 1
    }
  }

  if report.cpu.global_idle_driver == null {
    partial += ["idle_driver"]
  } else if report.cpu.global_idle_driver != reference.idle_driver {
    mismatches += ["idle_driver"]
  } else {
    matched += 1
  }

  if report.cpu.global_idle_governor == null {
    partial += ["idle_governor"]
  } else if report.cpu.global_idle_governor != reference.idle_governor {
    mismatches += ["idle_governor"]
  } else {
    matched += 1
  }

  let states = report.cpu.idle_states
    |> where .cpu_id == 0
    |> sort-by .state_index
  let names = states |> map .name
  var indexes_match = true
  for index in range(states.len()) {
    if states[index].state_index != index {
      indexes_match = false
    }
  }

  if states.len() == 0 {
    partial += ["idle_state_names"]
  } else if ! indexes_match or names != reference.idle_state_names {
    mismatches += ["idle_state_names"]
  } else {
    matched += 1
  }

  {matched_fields: matched, mismatches: mismatches, partial: partial}
}

proc cpupower_output(root: FsRoot, executable: Str, name: Str, argv: List[Str]) [fs, process, error] -> Result[Str] {
  let scratch_path = root.host_path()?
  let status = process.run(
    process.command_argv(
      executable,
      argv,
      cwd: /,
      env: {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"},
      stdout: fp"{scratch_path}/{name}",
      stderr: fp"{scratch_path}/{name}-error",
    ),
  )?
  if ! status.exited_with(0) {
    return Err(cpupower_failure(f"cpupower {name} command failed"))
  }

  let raw = root.read_result(fp"{name}", max_bytes: 65536)?
  if raw.state != "observed" or raw.truncated or raw.data == null {
    return Err(cpupower_failure(f"cpupower {name} output is incomplete"))
  }

  raw.data.utf8()?
}

## Runs only explicit utility subcommands around one product collection.
export proc compare_live_cpupower(
  xsh_bin: Str,
  script: Str,
  executable: Str,
) [fs, process, time, error] -> Result[CpupowerRun] {
  if ! xsh_bin.starts_with("/") or ! script.starts_with("/") or ! executable.starts_with("/") {
    return Err(cpupower_failure("cpupower comparison requires absolute paths"))
  }

  let scratch = fs.tempdir()?
  defer scratch.close()?
  let version_output = cpupower_output(scratch, executable, "version", [executable, "--version"])?
  let version = ((version_output.lines() |> collect()).get(0) ?? "").trim()
  if ! version.starts_with("cpupower ") {
    return Err(cpupower_failure("cpupower version is unsupported"))
  }

  let started = time.now()
  let driver_output = cpupower_output(
    scratch,
    executable,
    "driver",
    [executable, "-c", "0", "frequency-info", "--driver"],
  )?
  let limits_output = cpupower_output(
    scratch,
    executable,
    "limits",
    [executable, "-c", "0", "frequency-info", "--hwlimits"],
  )?
  let idle_output = cpupower_output(scratch, executable, "idle", [executable, "-c", "0", "idle-info"])?
  let frequency = parse_cpupower_frequency(driver_output, limits_output)?
  let idle = parse_cpupower_idle(idle_output)?
  let reference = CpupowerReference(
    driver: frequency.driver,
    hardware_min_khz: frequency.minimum_khz,
    hardware_max_khz: frequency.maximum_khz,
    idle_driver: idle.driver,
    idle_governor: idle.governor,
    idle_state_names: idle.names,
  )
  let candidate_started = time.now()
  let scratch_path = scratch.host_path()?
  let candidate_status = process.run(
    process.command_argv(
      xsh_bin,
      [xsh_bin, script, "--", "--section", "cpu", "--json"],
      cwd: /,
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"{scratch_path}/candidate",
      stderr: fp"{scratch_path}/candidate-error",
    ),
  )?
  if ! candidate_status.exited_with(0) {
    return Err(cpupower_failure("candidate CPU collection failed"))
  }

  let candidate_raw = scratch.read_result(p"candidate", max_bytes: 8388608)?
  if candidate_raw.state != "observed" or candidate_raw.truncated or candidate_raw.data == null {
    return Err(cpupower_failure("candidate CPU output is incomplete"))
  }

  let candidate = candidate_raw.data.utf8()?
  if json.get(json.decode(candidate)?, ["source_mode"])?.require(Str)? != "live_linux" {
    return Err(cpupower_failure("candidate is not a live Linux report"))
  }

  {
    version: version,
    executable: executable,
    comparison: compare_cpupower(candidate, reference)?,
    reference_started_unix_ms: started,
    candidate_started_unix_ms: candidate_started,
    driver_sha256_hex: hash.sha256(fp"{scratch_path}/driver")?.hex(),
    limits_sha256_hex: hash.sha256(fp"{scratch_path}/limits")?.hex(),
    idle_sha256_hex: hash.sha256(fp"{scratch_path}/idle")?.hex(),
  }
}
