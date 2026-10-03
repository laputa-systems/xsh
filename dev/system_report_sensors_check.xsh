##! Independent parser for the classic lm-sensors JSON reference format.
use context

error SensorsCheckError = Invalid(message: Str)

pure sensors_check_failure(message: Str) -> SensorsCheckError {
  SensorsCheckError.Invalid(message:)
}

## Keeps the bus-qualified chip key and raw subfeature name distinct from display labels.
export type SensorsJsonReading = {chip_key: Str, chip: Str, subfeature: Str, value: Float}

type CandidateSensor = {chip_entry_name: Str?, chip: Str, channel: Str, value: Int?}

type CandidateSensorSection = {channels: List[CandidateSensor]}

## Reports only uniquely mapped raw input readings with equal sampled values as agreements.
export type SensorsJsonComparison = {reference_count: Int, compared: Int, mismatches: List[Str], partial: List[Str]}

## Retains command identity, version, timing, and output fingerprints for the live reference.
export type SensorsJsonRun = {
  comparison: SensorsJsonComparison,
  version: Str,
  executable: Str,
  before_started_unix_ms: Int,
  candidate_started_unix_ms: Int,
  after_ended_unix_ms: Int,
  before_sha256_hex: Str,
  after_sha256_hex: Str,
}

pure sensors_json_number(value: Any) -> Result[Float] {
  match value {
    number is Float => Ok(number)
    number is Int => Ok(number.float())
    _ => Err(sensors_check_failure("sensors JSON subfeature is not numeric"))
  }
}

## Reads bounded `sensors -j -c /dev/null` output without treating feature labels as identities.
export pure parse_sensors_json(output: Str) -> Result[List[SensorsJsonReading]] {
  if output.count_chars() > 8388608 {
    return Err(sensors_check_failure("sensors JSON output exceeds its bound"))
  }

  let document = json.decode(output)?.require(Record)?
  var readings: List[SensorsJsonReading] = []
  for chip_key in document.keys() |> sort-by . {
    let chip = chip_key.split("-")[0]
    return Err(sensors_check_failure("sensors JSON chip has no name")) when chip == ""

    let features = json.get(document, [chip_key])?.require(Record)?
    for label in features.keys() |> sort-by . {
      continue when label == "Adapter"
      let subfeatures = json.get(features, [label])?.require(Record)?
      for name in subfeatures.keys() |> sort-by . {
        if readings.len() >= 65536 {
          return Err(sensors_check_failure("sensors JSON has too many subfeatures"))
        }

        let value = sensors_json_number(json.get(subfeatures, [name])?)?
        readings = readings.push({chip_key: chip_key, chip: chip, subfeature: name, value: value})
      }
    }
  }

  readings
}

pure sensors_json_scale(channel: Str) -> Int? {
  for spec in [
    {
      prefix: "temp",
      scale: 1000,
    },
    {
      prefix: "in",
      scale: 1000,
    },
    {
      prefix: "curr",
      scale: 1000,
    },
    {
      prefix: "power",
      scale: 1000000,
    },
    {
      prefix: "energy",
      scale: 1000000,
    },
    {
      prefix: "fan",
      scale: 1,
    },
  ] {
    continue unless channel.starts_with(spec.prefix)
    let suffix = channel.split("") |> drop(spec.prefix.count_chars()).join("")
    continue when suffix == ""
    var digits = true
    for digit in suffix {
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
        digits = false
      }
    }

    return spec.scale when digits
  }

  null
}

## Corroborates raw input values only when the utility and report identify one chip channel.
export pure compare_sensors_json(
  candidate_json: Str,
  before: List[SensorsJsonReading],
  after: List[SensorsJsonReading],
) -> Result[SensorsJsonComparison] {
  let document = json.decode(candidate_json)?
  let candidate = json.get(document, ["sensors"])?.require(CandidateSensorSection)?
  var mismatches: List[Str] = []
  var partial: List[Str] = []
  var reference_count = 0
  var compared = 0
  var before_chip_counts: Map[Int] = {}
  var after_counts: Map[Int] = {}
  var after_values: Map[Float] = {}
  var candidate_counts: Map[Int] = {}
  var candidate_indices: Map[Int] = {}
  for reading in before {
    let key = f"${reading.chip}:${reading.subfeature}"
    before_chip_counts = before_chip_counts.set(key, (before_chip_counts.get(key) ?? 0) + 1)
  }

  for reading in after {
    let key = f"${reading.chip_key}:${reading.subfeature}"
    after_counts = after_counts.set(key, (after_counts.get(key) ?? 0) + 1)
    after_values = after_values.set(key, reading.value)
  }

  for index in range(candidate.channels.len()) {
    let item = candidate.channels[index]
    let key = f"${item.chip}:${item.channel}"
    candidate_counts = candidate_counts.set(key, (candidate_counts.get(key) ?? 0) + 1)
    candidate_indices = candidate_indices.set(key, index)
  }

  for reading in before {
    continue unless reading.subfeature.ends_with("_input")
    let channel = reading.subfeature.split("") |> take(reading.subfeature.count_chars() - 6).join("")
    let scale = sensors_json_scale(channel)
    continue when scale == null
    reference_count += 1
    let key = f"${reading.chip_key}:${reading.subfeature}"
    let before_chip_matches = before_chip_counts.get(f"${reading.chip}:${reading.subfeature}") ?? 0
    let after_matches = after_counts.get(key) ?? 0
    let following = after_values.get(key) ?? reading.value
    let candidate_key = f"${reading.chip}:${channel}"
    let candidate_matches = candidate_counts.get(candidate_key) ?? 0
    var raw: Int? = null
    if candidate_matches == 1 {
      raw = candidate.channels[candidate_indices.get(candidate_key)?].value
    }

    if before_chip_matches != 1 or after_matches != 1 or following != reading.value or candidate_matches != 1 {
      partial += [key]
      continue
    }

    if raw == null {
      mismatches += [key]
      continue
    }

    let value = raw
    if value < -9007199254740991 or value > 9007199254740991 {
      partial += [key]
      continue
    }

    if (reading.value - value.float() / scale.float()).abs() > 0.000001 {
      partial += [key]
    } else {
      compared += 1
    }
  }

  Ok({
    reference_count: reference_count,
    compared: compared,
    mismatches: mismatches |> sort-by .,
    partial: partial |> sort-by .,
  })
}

## Runs the optional utility with configuration disabled and brackets one live sensor report.
export proc compare_live_sensors_json(
  xsh_bin: Str,
  script: Str,
  executable: Str,
) [fs, process, time, error] -> Result[SensorsJsonRun] {
  if ! xsh_bin.starts_with("/") or ! script.starts_with("/") or ! executable.starts_with("/") {
    return Err(sensors_check_failure("sensor comparison requires absolute executable and script paths"))
  }

  let scratch = fs.tempdir()?
  defer scratch.close()?
  for name in ["version", "version-error", "before", "before-error", "candidate", "after", "after-error"] {
    scratch.write(fp"${name}", "")?
  }

  let scratch_path = scratch.host_path()?
  let version_status = process.run(
    process.command_argv(
      executable,
      [executable, "-v"],
      cwd: /,
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"${scratch_path}/version",
      stderr: fp"${scratch_path}/version-error",
    ),
  )?
  if ! version_status.exited_with(0) {
    return Err(sensors_check_failure("sensors version probe failed"))
  }

  let version_source = scratch.read_result(p"version", max_bytes: 4096)?
  if version_source.state != "observed" or version_source.truncated or version_source.data == null {
    return Err(sensors_check_failure("sensors version probe output is incomplete"))
  }

  let version = version_source.data.utf8()?.trim()
  if version == "" {
    return Err(sensors_check_failure("sensors version probe returned no version"))
  }

  let argv = [executable, "-j", "-c", "/dev/null"]
  let started = time.now()
  let before_status = process.run(
    process.command_argv(
      executable,
      argv,
      cwd: /,
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"${scratch_path}/before",
      stderr: fp"${scratch_path}/before-error",
    ),
  )?
  let candidate_started = time.now()
  let candidate_status = process.run(
    process.command_argv(
      xsh_bin,
      [xsh_bin, script, "--", "--section", "sensors", "--sensitive", "--json"],
      cwd: /,
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"${scratch_path}/candidate",
    ),
  )?
  let after_status = process.run(
    process.command_argv(
      executable,
      argv,
      cwd: /,
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"${scratch_path}/after",
      stderr: fp"${scratch_path}/after-error",
    ),
  )?
  let ended = time.now()
  if ! before_status.exited_with(0) or ! after_status.exited_with(0) {
    return Err(sensors_check_failure("sensors JSON reference command failed"))
  }

  if ! candidate_status.exited_with(0) {
    return Err(sensors_check_failure("candidate sensor collection failed"))
  }

  let before_source = scratch.read_result(p"before", max_bytes: 8388608)?
  let after_source = scratch.read_result(p"after", max_bytes: 8388608)?
  let candidate_source = scratch.read_result(p"candidate", max_bytes: 8388608)?
  if before_source.truncated or after_source.truncated or candidate_source.truncated or before_source.data == null or after_source.data == null or candidate_source.data == null {
    return Err(sensors_check_failure("sensor comparison output exceeds its bound"))
  }

  let before_bytes = before_source.data
  let after_bytes = after_source.data
  let candidate_text = candidate_source.data.utf8()?
  let candidate_mode = json.get(json.decode(candidate_text)?, ["source_mode"])?.require(Str)?
  if candidate_mode != "live_linux" {
    return Err(sensors_check_failure("candidate is not a live Linux report"))
  }

  let comparison = compare_sensors_json(
    candidate_text,
    parse_sensors_json(before_bytes.utf8()?)?,
    parse_sensors_json(after_bytes.utf8()?)?,
  )?
  Ok({
    comparison: comparison,
    version: version,
    executable: executable,
    before_started_unix_ms: started,
    candidate_started_unix_ms: candidate_started,
    after_ended_unix_ms: ended,
    before_sha256_hex: hash.sha256(before_bytes).hex(),
    after_sha256_hex: hash.sha256(after_bytes).hex(),
  })
}
