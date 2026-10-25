#!/bin/xsh

enum Action { DeviceAdd, DeviceRemove, DeviceOther }
enum NodeKind { BlockDevice, CharacterDevice }
enum RulePattern { PatternName(Str), PatternEnvValue(Str, Str), PatternMajorMinor(Int, Int, Int) }
enum AliasRule { AliasNone, AliasRename(Str), AliasRenameAndLink(Str), AliasSuppress }
enum CommandTrigger { CommandOnAdd, CommandOnRemove, CommandAny }

type Paths = {dev_root: Path, sysfs_root: Path, conf: Path, firmware_root: Path, seq_file: Path, plain_files: Bool}
type Options = {scan: Bool, daemon: Bool, foreground: Bool}
type Field = {field: Str, rest: Str}
type EnvPrefix = {entry: EnvMatch, rest: Str}
type Owner = {uid: Int, gid: Int}
type DeviceNumbers = {major: Int?, minor: Int?}
type RegexCapture = {start: Int, end: Int}
type EnvMatch = {name: Str, pattern: Str}
type CommandRule = {trigger: CommandTrigger, command: Str}
type Rule = {keep_matching: Bool, env_matches: List[EnvMatch], pattern: RulePattern, uid: Int, gid: Int, mode: Int, alias: AliasRule, command: CommandRule?}
type Event = {action: Action, device_name: Path, sysfs_path: Path, kind: NodeKind?, major: Int?, minor: Int?, env: Map[Str, Bytes]}
type Captures = {matched: Bool, values: List[RegexCapture?]}
type DeviceSpec = {kind: NodeKind, major: Int, minor: Int, mode: Int, uid: Int, gid: Int}
type ResolvedNode = {name: Bytes, create_link: Bool, suppressed: Bool}

const MAX_DEVICE_ID = 4294967295
const UEVENT_BUFFER_SIZE = 65536
const DAEMON_WORKER_ARG = "--mdev-daemon-worker"

pure failure(message: Str) -> Error {
  error.failure(message)
}

pure path_join(root: Path, child: Bytes) -> Result[Path] {
  let prefix = root.bytes()
  if prefix.is_empty() or child.starts_with(b"/") { return Path.parse_bytes(child) }
  Path.parse_bytes(bytes.concat([prefix, if prefix.ends_with(b"/") { b"" } else { b"/" }, child]))
}

pure trim_left(text: Str) -> Str {
  let leading = rx"^\s*".captures(text).get(0) ?? ""
  text.byte_slice(leading.byte_len())
}

pure trim_leading_slashes(value: Bytes) -> Bytes {
  var start = 0
  while start < value.len() and value.byte_at(start) == 47 { start += 1 }
  value[start..]
}

pure decimal_digits(text: Str) -> Bool {
  let digits = if text.starts_with("+") { text.byte_slice(1) } else { text }
  rx"^[0-9]+$".matches(digits)
}

proc environment_value(entries: List[EnvRawEntry], name: Str) -> Bytes? {
  let key = bytes.from_text(name)
  for entry in entries { return entry.value when entry.name == key }
  null
}

proc configured_path(entries: List[EnvRawEntry], name: Str, fallback: Bytes) [error] -> Result[Path] {
  let value = environment_value(entries, f"XSH_MDEV_{name}") ?? environment_value(entries, f"SEED_MDEV_{name}") ?? fallback
  Path.parse_bytes(value)
}

proc paths() [env, error] -> Result[Paths] {
  let entries = env.entries()?
  let dev_root = configured_path(entries, "DEV_ROOT", b"/dev")?
  let sysfs_root = configured_path(entries, "SYSFS", b"/sys")?
  let conf = configured_path(entries, "CONF", b"/etc/mdev.conf")?
  let firmware_root = configured_path(entries, "FIRMWARE_ROOT", b"/lib/firmware")?
  let seq_file = if let value = environment_value(entries, "XSH_MDEV_SEQ") {
    Path.parse_bytes(value)?
  } else if let value = environment_value(entries, "SEED_MDEV_SEQ") {
    Path.parse_bytes(value)?
  } else {
    path_join(dev_root, b"mdev.seq")?
  }
  Ok({
    dev_root: dev_root,
    sysfs_root: sysfs_root,
    conf: conf,
    firmware_root: firmware_root,
    seq_file: seq_file,
    plain_files: environment_value(entries, "XSH_MDEV_TEST_PLAIN_FILES") != null or environment_value(entries, "SEED_MDEV_TEST_PLAIN_FILES") != null,
  })
}

proc parse_options(argv: List[Str]) -> Result[Options] {
  var scan = false
  var daemon = false
  var foreground = false
  for arg in argv {
    if arg == "--scan" { scan = true; continue }
    if arg == "--daemon" { daemon = true; continue }
    if arg == "--foreground" { foreground = true; continue }
    if arg == "--" { return Err(failure("extra operand")) }
    if arg.starts_with("--") { return Err(failure(f"unrecognized option '{arg}'")) }
    if arg.starts_with("-") and arg.byte_len() > 1 {
      for flag in arg.byte_slice(1).split("") {
        if flag == "s" { scan = true }
        if flag == "d" { daemon = true }
        if flag == "f" { foreground = true }
        if flag not in ["s", "d", "f"] { return Err(failure(f"invalid option -- '{flag}'")) }
      }
      continue
    }
    return Err(failure("extra operand"))
  }
  if foreground and ! daemon { return Err(failure("foreground mode requires daemon mode")) }
  Ok({scan: scan, daemon: daemon, foreground: foreground})
}

pure next_field(text: Str) -> Field? {
  let trimmed = trim_left(text)
  return null when trimmed.is_empty()
  var end = 0
  for character in trimmed {
    if rx"^\s$".matches(character) { break }
    end += character.byte_len()
  }
  {field: trimmed.byte_slice(0, length: end), rest: trimmed.byte_slice(end)}
}

pure config_error(line_number: Int, message: Str) -> Error {
  failure(f"mdev.conf:{line_number}: {message}")
}

proc regex_is_valid(pattern: Str) [error] -> Bool {
  match regex.captures_host_bytes(pattern, b"", extended: true) {
    Ok(_) => true
    Err(_) => false
  }
}

pure valid_env_name(name: Str) -> Bool {
  return false when name.is_empty()
  for character in name {
    return false when character != "_" and ! rx"^[A-Za-z0-9]$".matches(character)
  }
  true
}

proc parse_env_prefix(text: Str) [error] -> EnvPrefix? {
  let equals_at = text.find("=")
  return null when equals_at == null
  let equals = equals_at ?? 0
  let name = text.byte_slice(0, length: equals)
  return null when ! valid_env_name(name)
  let remainder = text.byte_slice(equals + 1)
  let separator_at = remainder.find(";")
  return null when separator_at == null
  let separator = separator_at ?? 0
  let pattern = remainder.byte_slice(0, length: separator)
  return null when ! regex_is_valid(pattern)
  {entry: {name: name, pattern: pattern}, rest: remainder.byte_slice(separator + 1)}
}

proc parse_major_minor(text: Str, line_number: Int) -> Result[RulePattern] {
  let comma_at = text.find(",")
  return Err(config_error(line_number, f"invalid major/minor match '@{text}'")) when comma_at == null
  let comma = comma_at ?? 0
  let major_text = text.byte_slice(0, length: comma)
  let minor_text = text.byte_slice(comma + 1)
  let major_value = parse_device_id(major_text)
  return Err(config_error(line_number, f"invalid major in '@{text}'")) when major_value == null
  let major = major_value ?? 0
  let range_separator = minor_text.find("-")
  if range_separator != null {
    let separator = range_separator ?? 0
    let start_value = parse_device_id(minor_text.byte_slice(0, length: separator))
    return Err(config_error(line_number, f"invalid minor range in '@{text}'")) when start_value == null
    let start = start_value ?? 0
    let end_value = parse_device_id(minor_text.byte_slice(separator + 1))
    return Err(config_error(line_number, f"invalid minor range in '@{text}'")) when end_value == null
    let end = end_value ?? 0
    return Ok(.PatternMajorMinor(major, start, end))
  }
  let minor_value = parse_device_id(minor_text)
  return Err(config_error(line_number, f"invalid minor in '@{text}'")) when minor_value == null
  let minor = minor_value ?? 0
  Ok(.PatternMajorMinor(major, minor, minor))
}

pure parse_device_id(text: Str) -> Int? {
  return null when ! decimal_digits(text)
  let digits = if text.starts_with("+") { text.byte_slice(1) } else { text }
  let value = digits.parse_uint()
  return null when value is Err(_)
  let number = value ?? 0
  return null when number > MAX_DEVICE_ID
  number
}

proc parse_rule_pattern(text: Str, line_number: Int) [error] -> Result[RulePattern] {
  if text.starts_with("@") { return parse_major_minor(text.byte_slice(1), line_number) }
  if text.starts_with("$") {
    let pattern_text = text.byte_slice(1)
    let equals_at = pattern_text.find("=")
    return Err(config_error(line_number, f"invalid env match '{text}'")) when equals_at == null
    let equals = equals_at ?? 0
    let name = pattern_text.byte_slice(0, length: equals)
    return Err(config_error(line_number, "missing env name")) when name.is_empty()
    let pattern = pattern_text.byte_slice(equals + 1)
    if ! regex_is_valid(pattern) { return Err(config_error(line_number, f"invalid regex '{pattern}'")) }
    return Ok(.PatternEnvValue(name, pattern))
  }
  if ! regex_is_valid(text) { return Err(config_error(line_number, f"invalid regex '{text}'")) }
  Ok(.PatternName(text))
}

proc resolve_user(text: Str) [fs, error] -> Result[Int] {
  if let id = parse_device_id(text) { return Ok(id) }
  match user.lookup(text) {
    Ok(account) => Ok(account.uid)
    Err(_) => Err(failure(f"unknown user '{text}'"))
  }
}

proc resolve_group(text: Str) [fs, error] -> Result[Int] {
  if let id = parse_device_id(text) { return Ok(id) }
  match group.lookup(text) {
    Ok(account) => Ok(account.gid)
    Err(_) => Err(failure(f"unknown group '{text}'"))
  }
}

proc parse_owner(spec: Str, line_number: Int) [fs, error] -> Result[Owner] {
  let separator_at = spec.find(":")
  return Err(config_error(line_number, "uid:gid field is required")) when separator_at == null
  let separator = separator_at ?? 0
  let uid_result = resolve_user(spec.byte_slice(0, length: separator))
  if let Err(problem) = uid_result { return Err(config_error(line_number, problem.message)) }
  let uid = uid_result?
  let gid_result = resolve_group(spec.byte_slice(separator + 1))
  if let Err(problem) = gid_result { return Err(config_error(line_number, problem.message)) }
  let gid = gid_result?
  Ok({uid: uid, gid: gid})
}

pure parse_mode(text: Str, line_number: Int) -> Result[Int] {
  let digits = if text.starts_with("+") { text.byte_slice(1) } else { text }
  return Err(config_error(line_number, f"invalid mode '{text}'")) when digits.is_empty()
  var mode = 0
  for character in digits {
    let digit = "01234567".find(character)
    if digit == null or mode > (4294967295 - (digit ?? 0)) / 8 {
      return Err(config_error(line_number, f"invalid mode '{text}'"))
    }
    mode = mode * 8 + (digit ?? 0)
  }
  Ok(mode)
}

proc parse_rule_line(line: Str, line_number: Int) [fs, error] -> Result[Rule] {
  guard let first = next_field(line) else { return Err(config_error(line_number, "missing pattern")) }
  guard let second = next_field(first.rest) else { return Err(config_error(line_number, "missing uid:gid")) }
  guard let third = next_field(second.rest) else { return Err(config_error(line_number, "missing mode")) }
  let keep_matching = first.field.starts_with("-")
  var pattern_text = if keep_matching { first.field.byte_slice(1) } else { first.field }
  var env_matches: List[EnvMatch] = []
  loop {
    if let prefix = parse_env_prefix(pattern_text) {
      env_matches += [prefix.entry]
      pattern_text = prefix.rest
    } else { break }
  }
  return Err(config_error(line_number, "missing match pattern")) when pattern_text.is_empty()
  let pattern = parse_rule_pattern(pattern_text, line_number)?
  let owner = parse_owner(second.field, line_number)?
  let mode = parse_mode(third.field, line_number)?
  var rest = trim_left(third.rest)
  var alias: AliasRule = .AliasNone
  if rest.starts_with("!") {
    alias = .AliasSuppress
    rest = trim_left(rest.byte_slice(1))
  } else if rest.starts_with(">") or rest.starts_with("=") {
    let marker = rest.byte_slice(0, length: 1)
    guard let alias_field = next_field(rest) else { return Err(config_error(line_number, "missing alias target")) }
    alias = if marker == ">" { .AliasRenameAndLink(alias_field.field.byte_slice(1)) } else { .AliasRename(alias_field.field.byte_slice(1)) }
    rest = trim_left(alias_field.rest)
  }
  var command: CommandRule? = null
  if ! rest.is_empty() {
    let trigger: CommandTrigger = if rest.starts_with("@") { .CommandOnAdd } else if rest.starts_with("$") { .CommandOnRemove } else if rest.starts_with("*") { .CommandAny } else { return Err(config_error(line_number, "invalid command action")) }
    let text = trim_left(rest.byte_slice(1))
    return Err(config_error(line_number, "empty command")) when text.is_empty()
    command = {trigger: trigger, command: text}
  }
  Ok({keep_matching: keep_matching, env_matches: env_matches, pattern: pattern, uid: owner.uid, gid: owner.gid, mode: mode, alias: alias, command: command})
}

proc read_rules(rules_path: Path) [fs, error] -> Result[List[Rule]] {
  let text_result = rules_path.read_text()
  if let Err(problem) = text_result {
    if problem.errno == 2 { return Ok([]) }
    return Err(error.failure(f"reading {rules_path.display()}: {problem.message}"))
  }
  let text = text_result?
  var rules: List[Rule] = []
  var line_number = 0
  for raw_line in text.lines() {
    line_number += 1
    let line = trim_left(raw_line)
    continue when line.is_empty() or line.starts_with("#")
    rules += [parse_rule_line(line, line_number)?]
  }
  Ok(rules)
}

proc read_uevent_env(uevent_path: Path, missing_ok: Bool) [fs, error] -> Result[Map[Str, Bytes]] {
  let text_result = uevent_path.read_text()
  if let Err(problem) = text_result {
    if missing_ok and problem.errno == 2 { return Ok({}) }
    return Err(error.failure(f"reading {uevent_path.display()}: {problem.message}"))
  }
  let text = text_result?
  var values: Map[Str, Bytes] = {}
  for line in text.lines() {
    if let separator = line.find("=") {
      let name = line.byte_slice(0, length: separator)
      values = values.set(name, bytes.from_text(line.byte_slice(separator + 1)))
    }
  }
  Ok(values)
}
proc read_subsystem(sysfs_path: Path) [fs, error] -> Result[Str?] {
  let subsystem_path = path_join(sysfs_path, b"subsystem")?
  let link_result = subsystem_path.readlink()
  if let Err(problem) = link_result {
    if problem.errno == 2 { return Ok(null) }
    return Err(error.failure(f"reading {subsystem_path.display()}: {problem.message}"))
  }
  let link = link_result?
  Ok(link.name())
}

proc event_from_environment(paths: Paths, values: Map[Str, Bytes]) [fs, error] -> Result[Event] {
  var event_env = values
  guard let action_value = event_env.get("ACTION") else { return Err(failure("missing ACTION environment variable")) }
  let action: Action = if action_value == b"add" { .DeviceAdd } else if action_value == b"remove" { .DeviceRemove } else { .DeviceOther }
  guard let devpath = event_env.get("DEVPATH") else { return Err(failure("missing DEVPATH environment variable")) }
  let sysfs_path = path_join(paths.sysfs_root, trim_leading_slashes(devpath))?
  let uevent_path = path_join(sysfs_path, b"uevent")?
  let uevent_env = if let Ok(loaded) = read_uevent_env(uevent_path, true) { loaded } else { {} }
  for name in uevent_env.keys() {
    if event_env.get(name) is Err(_) { event_env = event_env.set(name, uevent_env.get(name) ?? b"") }
  }
  var subsystem: Str? = null
  if let Ok(value) = event_env.get("SUBSYSTEM") {
    if let Ok(name) = value.utf8() { subsystem = name }
  }
  if subsystem == null {
    if let Ok(found) = read_subsystem(sysfs_path) { subsystem = found }
  }
  if let name = subsystem {
    if event_env.get("SUBSYSTEM") is Err(_) { event_env = event_env.set("SUBSYSTEM", bytes.from_text(name)) }
  }
  let device_name = event_env.get("DEVNAME") ?? bytes.from_text(sysfs_path.name())
  if event_env.get("DEVNAME") is Err(_) { event_env = event_env.set("DEVNAME", device_name) }
  let numbers = if action == .DeviceAdd { read_dev_numbers(sysfs_path) } else { {major: null, minor: null} }
  let has_block_component = sysfs_path.components() |> any { |component| component.name() == "block" }
  let kind: NodeKind = if has_block_component or subsystem == "block" { .BlockDevice } else { .CharacterDevice }
  let device_kind: NodeKind? = if numbers.major == null or numbers.minor == null { null } else { kind }
  Ok({action: action, device_name: Path.parse_bytes(device_name)?, sysfs_path: sysfs_path, kind: device_kind, major: numbers.major, minor: numbers.minor, env: event_env})
}

proc read_dev_numbers(sysfs_path: Path) [fs, error] -> DeviceNumbers {
  if let Ok(dev_path) = path_join(sysfs_path, b"dev") {
    if let Ok(text) = dev_path.read_text() {
      let value = text.trim()
      if let separator = value.find(":") {
        return {major: parse_device_id(value.byte_slice(0, length: separator)), minor: parse_device_id(value.byte_slice(separator + 1))}
      }
    }
  }
  {major: null, minor: null}
}

proc event_from_scan_path(paths: Paths, sysfs_path: Path) [fs, error] -> Result[Event] {
  var values = read_uevent_env(path_join(sysfs_path, b"uevent")?, false)?
  values = values.set("ACTION", b"add")
  let cwd = fs.cwd() ?? p"/"
  let full_path = if sysfs_path.starts_with(p"/") { sysfs_path } else { path_join(cwd, sysfs_path.bytes())? }
  let root_path = if paths.sysfs_root.starts_with(p"/") { paths.sysfs_root } else { path_join(cwd, paths.sysfs_root.bytes())? }
  let relative = if let Ok(relative_path) = full_path.strip_prefix(root_path) { relative_path.bytes() } else { sysfs_path.bytes() }
  values = values.set("DEVPATH", bytes.concat([b"/", relative]))
  if let subsystem = read_subsystem(sysfs_path)? { values = values.set("SUBSYSTEM", bytes.from_text(subsystem)) }
  event_from_environment(paths, values)
}

proc regex_captures(pattern: Str, input: Bytes) [error] -> Result[Captures] {
  let values = regex.captures_host_bytes(pattern, input, extended: true)?
  let full = values.get(0) ?? null
  Ok({matched: if let range = full { range.start == 0 and range.end == input.len() } else { false }, values: values})
}

pure full_match(captures: Captures, input_length: Int) -> Bool {
  return false when ! captures.matched
  let capture = captures.values[0] ?? {start: -1, end: -1}
  capture.start == 0 and capture.end == input_length
}

proc rule_matches(rule: Rule, event: Event) [error] -> Result[Captures] {
  for constraint in rule.env_matches {
    let value = event.env.get(constraint.name)
    if value is Err(_) { return Ok({matched: false, values: []}) }
    let captures = regex_captures(constraint.pattern, value?)?
    if ! full_match(captures, (value?).len()) { return Ok({matched: false, values: []}) }
  }
  match rule.pattern {
    PatternName(pattern) => regex_captures(pattern, event.device_name.bytes())?
    PatternEnvValue(name, pattern) => {
      let value = event.env.get(name)
      if value is Err(_) { return Ok({matched: false, values: []}) }
      regex_captures(pattern, value?)?
    }
    PatternMajorMinor(major, minor_start, minor_end) => {
      let matched = event.major == major and event.minor != null and (event.minor ?? -1) >= minor_start and (event.minor ?? -1) <= minor_end
      Ok({matched: matched, values: []})
    }
  }
}

proc resolve_node(rule: Rule, device_name: Bytes, captures: Captures) [error] -> Result[ResolvedNode] {
  match rule.alias {
    AliasNone => Ok({name: device_name, create_link: false, suppressed: false})
    AliasSuppress => Ok({name: device_name, create_link: false, suppressed: true})
    AliasRename(pattern) => Ok({name: build_alias(pattern, device_name, captures), create_link: false, suppressed: false})
    AliasRenameAndLink(pattern) => Ok({name: build_alias(pattern, device_name, captures), create_link: true, suppressed: false})
  }
}

pure build_alias(pattern: Str, device_name: Bytes, captures: Captures) -> Bytes {
  let pattern_bytes = bytes.from_text(pattern)
  var output: Bytes = b""
  var offset = 0
  while offset < pattern_bytes.len() {
    if pattern_bytes.byte_at(offset) == 37 and offset + 1 < pattern_bytes.len() {
      let digit = pattern_bytes.byte_at(offset + 1)
      let digit_value = digit ?? 0
      if digit_value >= 48 and digit_value <= 57 {
        let index = digit_value - 48
        if let capture = captures.values.get(index) ?? null {
          if capture.start >= 0 and capture.end >= capture.start and capture.end <= device_name.len() {
            output = bytes.concat([output, device_name[capture.start..capture.end]])
          }
        }
        offset += 2
        continue
      }
    }
    output = bytes.concat([output, pattern_bytes[offset..offset + 1]])
    offset += 1
  }
  if output.ends_with(b"/") { output = bytes.concat([output, device_name]) }
  output
}

proc handle_event(paths: Paths, rules: List[Rule], event: Event) [fs, process, env, error, time, io, net] -> Result[Unit] {
  if event.action == .DeviceOther { return Ok() }
  let next_sequence = sequence_next(paths, event.env)?
  let result = handle_event_rules(paths, rules, event)
  if let next = next_sequence {
    let _ = paths.seq_file.write(bytes.from_text(f"{next}\n"))
  }
  result
}

proc sequence_next(paths: Paths, event_env: Map[Str, Bytes]) [fs, time, error] -> Result[Int?] {
  let sequence_result = event_env.get("SEQNUM")
  if sequence_result is Err(_) { return Ok(null) }
  let sequence_bytes = sequence_result?
  let sequence_text = sequence_bytes.utf8()?
  if fs.stat(paths.seq_file) is Err(_) { return Ok(null) }
  if ! decimal_digits(sequence_text) { return Err(failure(f"invalid SEQNUM '{sequence_text}'")) }
  let expected_result = sequence_text.parse_uint()
  if expected_result is Err(_) { return Err(failure(f"invalid SEQNUM '{sequence_text}'")) }
  let expected = expected_result?
  let started = time.monotonic()?
  loop {
    if let Ok(stat) = fs.stat(paths.seq_file) {
      if stat.kind == "file" {
        let text = paths.seq_file.read_text()?
        let current = text.trim()
        if current.is_empty() {
          paths.seq_file.write(bytes.from_text(f"{expected}"))?
          return Ok(expected + 1)
        }
        let seen = current.parse_uint() ?? expected
        if seen >= expected { return Ok(expected + 1) }
      }
    }
    if time.monotonic()? - started >= 2000000000 { return Ok(expected + 1) }
    time.sleep(32ms)
  }
}

proc handle_event_rules(paths: Paths, rules: List[Rule], event: Event) [fs, process, env, error, time, io, net] -> Result[Unit] {
  var matched = false
  for rule in rules {
    let captures = rule_matches(rule, event)?
    if ! captures.matched { continue }
    matched = true
    apply_rule(paths, event, rule, captures)?
    if ! rule.keep_matching { break }
  }
  if ! matched {
    let fallback: Rule = {keep_matching: false, env_matches: [], pattern: .PatternName(".*"), uid: 0, gid: 0, mode: 0o660, alias: .AliasNone, command: null}
    apply_rule(paths, event, fallback, {matched: false, values: []})?
  }
  if event.action == .DeviceAdd { maybe_load_firmware(paths, event)? }
  Ok()
}

proc apply_rule(paths: Paths, event: Event, rule: Rule, captures: Captures) [fs, process, env, error, io, time] -> Result[Unit] {
  let resolved = resolve_node(rule, event.device_name.bytes(), captures)?
  if event.action == .DeviceAdd and ! resolved.suppressed {
    if let kind = event.kind {
      if let major = event.major {
        if let minor = event.minor {
          create_device(paths, resolved.name, {kind: kind, major: major, minor: minor, mode: rule.mode, uid: rule.uid, gid: rule.gid})?
          if resolved.create_link { create_device_link(paths, event.device_name.bytes(), resolved.name)? }
        }
      }
    }
  } else if event.action == .DeviceRemove and ! resolved.suppressed {
    if resolved.create_link { remove_node(paths, event.device_name.bytes())? }
    remove_node(paths, resolved.name)?
  }
  if let command = rule.command {
    let run_command_now = command.trigger == .CommandAny or (command.trigger == .CommandOnAdd and event.action == .DeviceAdd) or (command.trigger == .CommandOnRemove and event.action == .DeviceRemove)
    if run_command_now { run_rule_command(command, event.env, resolved.name)? }
  }
  Ok()
}

proc create_device(paths: Paths, name: Bytes, spec: DeviceSpec) [fs, error] -> Result[Unit] {
  let target = path_join(paths.dev_root, name)?
  ensure_parent(target)?
  if paths.plain_files {
    let marker = if spec.kind == .BlockDevice { b"b" } else { b"c" }
    target.write(bytes.concat([marker, bytes.from_text(f" {spec.major}:{spec.minor}\n")]))?
  } else {
    let kind = if spec.kind == .BlockDevice { "block" } else { "char" }
    if let Err(problem) = fs.mknod(target, kind, spec.mode, major: spec.major, minor: spec.minor) {
      if problem.errno != 17 { return Err(error.failure(f"creating {target.display()}: {problem.message}")) }
    }
  }
  target.chmod(spec.mode)?
  fs.set_owner(target, uid: spec.uid, gid: spec.gid)?
  Ok()
}

proc create_device_link(paths: Paths, device_name: Bytes, node_name: Bytes) [fs, error] -> Result[Unit] {
  let link_path = path_join(paths.dev_root, device_name)?
  ensure_parent(link_path)?
  remove_existing(link_path)?
  link_path.symlink(to: Path.parse_bytes(node_name)?)
}

proc remove_node(paths: Paths, name: Bytes) [fs, error] -> Result[Unit] {
  remove_existing(path_join(paths.dev_root, name)?)
}

proc ensure_parent(target: Path) [fs, error] -> Result[Unit] {
  ensure_directory(target.parent())
}

proc ensure_directory(directory: Path) [fs, error] -> Result[Unit] {
  if let Ok(stat) = fs.stat(directory, follow_symlinks: true) {
    if stat.kind != "dir" { return Err(failure(f"creating {directory.display()}: path is not a directory")) }
    return Ok()
  }
  let parent = directory.parent()
  if parent == directory { return Err(failure(f"creating {directory.display()}: no parent directory")) }
  ensure_directory(parent)?
  let inherited = fs.stat(parent, follow_symlinks: true)?.mode.bit_and(0o2000)
  let root = fs.open_root(parent)?
  defer root.close()
  let components = directory.components()
  let leaf = components[-1].require(RelPath)?
  let mode = 0o755.clear_bits(fs.umask()?).bit_or(inherited)
  match root.mkdir(leaf, mode: mode) {
    Ok(_) => Ok(),
    Err(problem) => {
      return Err(problem) when problem.errno != 17
      match fs.stat(directory, follow_symlinks: true) {
        Ok(stat) => if stat.kind == "dir" { Ok() } else { Err(problem) }
        Err(_) => Err(problem)
      }
    }
  }
}

proc remove_existing(target: Path) [fs, error] -> Result[Unit] {
  match fs.stat(target, follow_symlinks: false) {
    Ok(stat) => {
      if stat.kind == "dir" { return Err(failure(f"refusing to remove directory {target.display()}")) }
      target.unlink()
    }
    Err(problem) => if problem.errno == 2 { Ok() } else { Err(problem) }
  }
}

proc run_rule_command(command: CommandRule, event_env: Map[Str, Bytes], node_name: Bytes) [fs, process, env, error] -> Result[Unit] {
  let environment = event_env.set("MDEV", node_name)
  let words = [p"/bin/sh", p"-c", p"umask 000; exec /bin/sh -c \"$1\" sh", p"mdev", Path.parse_bytes(bytes.from_text(command.command))?]
  let status = process.run(process.command_argv(p"/bin/sh", words, env: environment))?
  if ! status.exited_with(0) { return Err(failure(f"command failed: {command.command}")) }
  Ok()
}

proc write_fd_all(fd: Int, data: Bytes) [process, net, error] -> Result[Unit] {
  var offset = 0
  while offset < data.len() {
    let written = unix.write_fd(fd, data[offset..])?
    if written == 0 { return Err(failure("writing device firmware returned zero bytes")) }
    offset += written
  }
  Ok()
}

proc maybe_load_firmware(paths: Paths, event: Event) [fs, process, env, error, time, net] -> Result[Unit] {
  let firmware_name_result = event.env.get("FIRMWARE")
  if firmware_name_result is Err(_) { return Ok() }
  let firmware_name = firmware_name_result?
  let firmware_path = path_join(paths.firmware_root, firmware_name)?
  let loading_path = path_join(event.sysfs_path, b"loading")?
  let data_path = path_join(event.sysfs_path, b"data")?
  let started = time.monotonic()?
  while fs.stat(loading_path) is Err(_) and time.monotonic()? - started < 30000000000 { time.sleep(1s) }
  let loading_fd = unix.open_fd(loading_path, write: true)?
  defer unix.close_fd(loading_fd)
  let firmware_fd = unix.open_fd(firmware_path)
  var firmware: Bytes? = null
  if let Ok(fd) = firmware_fd {
    defer unix.close_fd(fd)
    firmware = read_firmware(fd)?
  }
  if let data = firmware {
    write_fd_all(loading_fd, b"1")?
    data_path.write(data)?
    write_fd_all(loading_fd, b"0")?
  } else {
    write_fd_all(loading_fd, b"-1")?
  }
  Ok()
}

proc read_firmware(fd: Int) [process, net, error] -> Result[Bytes] {
  var data: Bytes = b""
  loop {
    let chunk = unix.read_fd(fd, 65536)?
    if chunk.is_empty() { break }
    data = bytes.concat([data, chunk])
  }
  Ok(data)
}

proc run_scan(paths: Paths, rules: List[Rule]) [fs, process, env, error, time, io, net] -> Result[Unit] {
  let root = path_join(paths.sysfs_root, b"dev")?
  var pending: List[Path] = [root]
  var visited: Map[Path, Bool] = {}
  while ! pending.is_empty() {
    let last = pending.len() - 1
    let directory = pending[last]
    pending = pending[0..last]
    let identity = directory.resolve() ?? directory
    if visited.get(identity) is Ok(_) { continue }
    visited = visited.set(identity, true)
    let children = fs.children(directory, stat: false, ordered: false)
    if let Err(problem) = children {
      if problem.errno == 2 { continue }
      return Err(error.failure(f"reading {directory.display()}: {problem.message}"))
    }
    for entry in children? {
      let entry_path = entry.path
      let metadata_result = fs.stat(entry_path, follow_symlinks: true)
      if metadata_result is Err(_) { continue }
      if metadata_result?.kind == "dir" {
        pending += [entry_path]
        continue
      }
      continue when entry.name != "dev"
      let event = event_from_scan_path(paths, entry_path.parent())?
      handle_event(paths, rules, event)?
    }
  }
  Ok()
}

proc main(...argv: List[Str]) [fs, process, env, error, io, time, net] -> Int {
  if system.uname()?.sysname != "Linux" {
    eprint "mdev: mdev is only available on Linux"
    return 1
  }
  match run_mdev(argv) {
    Ok(status) => status
    Err(failure) => { eprint f"mdev: {failure.message}"; 1 }
  }
}

proc run_mdev(argv: List[Str]) [fs, process, env, error, io, time, net] -> Result[Int] {
  if argv.len() == 1 and argv[0] == DAEMON_WORKER_ARG {
    mdev_daemon_worker(true)?
    return Ok(0)
  }
  let options = parse_options(argv)?
  let paths = paths()?
  if options.daemon {
    if options.foreground {
      mdev_daemon_worker(false)?
      return Ok(0)
    }
    return start_daemon_worker()
  }
  let rules = read_rules(paths.conf)?
  if options.scan { run_scan(paths, rules)?; return Ok(0) }
  let inherited = inherited_environment()?
  let event = event_from_environment(paths, inherited)?
  handle_event(paths, rules, event)?
  Ok(0)
}

proc start_daemon_worker() [process, error] -> Result[Int] {
  let executable = applet.current_exe()?
  let script = process.script_path()?
  let command = process.command_argv(executable, [executable, script, p"--", p"--mdev-daemon-worker"])
  let status = unix.daemon_start(command, ["HUP", "INT", "TERM"])?
  status?.shell_code() ?? Ok(0)
}

proc mdev_daemon_worker(detached: Bool) [fs, process, env, error, io, time, net] -> Result[Unit] {
  let signals = ["HUP", "INT", "TERM"]
  unix.signal_setup(signals)?
  var ready_fd: Int? = null
  var ready_open = true
  if detached {
    let notification_text = env.get("NOTIFY_FD")?
    let notification_number = notification_text.parse_uint()?
    let descriptor = unix.duplicate_fd(notification_number)?
    ready_fd = descriptor
    defer { if ready_open { let _ = unix.close_fd(descriptor) } }
    unix.close_fd(notification_number)?
  }
  let paths = paths()?
  var rules = read_rules(paths.conf)?
  let socket = open_uevent_socket()?
  defer unix.close_fd(socket)
  run_scan(paths, rules)?
  if let descriptor = ready_fd {
    let _ = process.new_session()?
    let root = p"/"
    cd $root {
      write_fd_all(descriptor, b"R")?
      unix.close_fd(descriptor)?
      ready_open = false
      daemon_loop(socket, paths, rules)?
    }
  } else {
    daemon_loop(socket, paths, rules)?
  }
  Ok()
}

proc open_uevent_socket() [process, net, error] -> Result[Int] {
  let constants = linux.net_constants()
  let socket = linux.netlink_open(constants.NETLINK_KOBJECT_UEVENT, groups: 1)?
  let _ = linux.setsockopt_int(socket, constants.SOL_SOCKET, constants.SO_RCVBUF, 2097152)
  Ok(socket)
}

proc daemon_loop(socket: Int, paths: Paths, rules: List[Rule]) [fs, process, env, error, io, time, net] -> Result[Unit] {
  var current_rules = rules
  loop {
    let pending = unix.take_signals()?
    if "TERM" in pending or "INT" in pending { return Ok() }
    if "HUP" in pending { current_rules = reload_rules(paths, current_rules)? }
    let readable = unix.poll_fd(socket, ["readable"], timeout_ms: 100)?
    let pending_after_poll = unix.take_signals()?
    if "TERM" in pending_after_poll or "INT" in pending_after_poll { return Ok() }
    if "HUP" in pending_after_poll { current_rules = reload_rules(paths, current_rules)? }
    if "readable" in readable { receive_daemon_event(socket, paths, current_rules)? }
  }
}

proc reload_rules(paths: Paths, current: List[Rule]) [fs, io, error] -> Result[List[Rule]] {
  match read_rules(paths.conf) {
    Ok(updated) => Ok(updated)
    Err(problem) => {
      report_daemon_error(f"mdev: reload: {problem.message}")?
      Ok(current)
    }
  }
}

proc report_daemon_error(message: Str) [io, error] -> Result[Unit] {
  eprint $message
  io.flush_stderr()?
  Ok()
}

proc receive_daemon_event(socket: Int, paths: Paths, rules: List[Rule]) [fs, process, env, error, io, time, net] -> Result[Unit] {
  let constants = linux.net_constants()
  let message_result = linux.recvfrom(socket, UEVENT_BUFFER_SIZE, constants.MSG_DONTWAIT.bit_or(constants.MSG_TRUNC))
  if let Err(problem) = message_result {
    if problem.errno == 105 {
      report_daemon_error("mdev: netlink receive buffer overflow; uevents may have been missed")?
      return Ok()
    }
    if problem.errno == 11 { return Ok() }
    return Err(problem)
  }
  let message = message_result?
  if message.flags.bit_and(constants.MSG_TRUNC) != 0 {
    report_daemon_error(f"mdev: truncated uevent message: more than {UEVENT_BUFFER_SIZE} bytes exceeds {UEVENT_BUFFER_SIZE} byte buffer")?
    return Ok()
  }
  let values = parse_uevent_message(message.data)
  if values.get("ACTION") is Err(_) or values.get("DEVPATH") is Err(_) {
    report_daemon_error("mdev: ignoring malformed uevent")?
    return Ok()
  }
  let event = event_from_environment(paths, values)
  if let Err(problem) = event {
    report_daemon_error(f"mdev: event: {problem.message}")?
    return Ok()
  }
  if let Err(problem) = handle_event(paths, rules, event?) {
    report_daemon_error(f"mdev: event: {problem.message}")?
  }
  Ok()
}

pure parse_uevent_message(message: Bytes) -> Map[Str, Bytes] {
  var values: Map[Str, Bytes] = {}
  var offset = 0
  var field_index = 0
  while offset <= message.len() {
    let start = offset
    while offset < message.len() and message.byte_at(offset) != 0 { offset += 1 }
    let field = message[start..offset]
    offset += 1
    if ! field.is_empty() {
      if let separator = find_byte(field, 61) {
        let name = field[0..separator].utf8()
        if let Ok(name_text) = name { values = values.set(name_text, field[separator + 1..]) }
      } else if field_index == 0 {
        if let separator = find_byte(field, 64) {
          if let Ok(action) = field[0..separator].utf8() { values = values.set("ACTION", bytes.from_text(action)) }
          values = values.set("DEVPATH", field[separator + 1..])
        }
      }
      field_index += 1
    }
  }
  values
}

pure find_byte(input: Bytes, needle: Int) -> Int? {
  for index in range(input.len()) {
    if input.byte_at(index) == needle { return index }
  }
  null
}

proc inherited_environment() [env, error] -> Result[Map[Str, Bytes]] {
  var values: Map[Str, Bytes] = {}
  for entry in env.entries()? {
    let name = entry.name.utf8()?
    values = values.set(name, entry.value)
  }
  Ok(values)
}
