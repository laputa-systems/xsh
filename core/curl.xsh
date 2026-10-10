#!/bin/xsh
use lib.http_transfer

const VERSION = "8.22.0"

# One command-line option: its long name, its short letter (or ""), whether it
# takes a value, and the key `apply` switches on.
type Spec = {long: Str, short: Str, value: Bool, key: Str}

const OPTIONS: List[Spec] = [
  {long: "silent", short: "s", value: false, key: "silent"},
  {long: "show-error", short: "S", value: false, key: "show_error"},
  {long: "fail", short: "f", value: false, key: "fail"},
  {long: "fail-with-body", short: "", value: false, key: "fail_with_body"},
  {long: "include", short: "i", value: false, key: "include"},
  {long: "show-headers", short: "i", value: false, key: "include"},
  {long: "head", short: "I", value: false, key: "head"},
  {long: "verbose", short: "v", value: false, key: "verbose"},
  {long: "location", short: "L", value: false, key: "location"},
  {long: "location-trusted", short: "", value: false, key: "location"},
  {long: "insecure", short: "k", value: false, key: "insecure"},
  {long: "compressed", short: "", value: false, key: "compressed"},
  {long: "get", short: "G", value: false, key: "get"},
  {long: "globoff", short: "g", value: false, key: "globoff"},
  {long: "remote-name", short: "O", value: false, key: "remote_name"},
  {long: "create-dirs", short: "", value: false, key: "create_dirs"},
  {long: "progress-bar", short: "#", value: false, key: "progress_bar"},
  {long: "no-progress-meter", short: "", value: false, key: "no_progress"},
  {long: "http1.1", short: "", value: false, key: "already_default"},
  {long: "basic", short: "", value: false, key: "already_default"},
  {long: "version", short: "V", value: false, key: "version"},
  {long: "help", short: "h", value: false, key: "help"},
  {long: "output", short: "o", value: true, key: "output"},
  {long: "request", short: "X", value: true, key: "request"},
  {long: "header", short: "H", value: true, key: "header"},
  {long: "data", short: "d", value: true, key: "data"},
  {long: "data-ascii", short: "", value: true, key: "data"},
  {long: "data-raw", short: "", value: true, key: "data_raw"},
  {long: "data-binary", short: "", value: true, key: "data_binary"},
  {long: "data-urlencode", short: "", value: true, key: "data_urlencode"},
  {long: "user", short: "u", value: true, key: "user"},
  {long: "user-agent", short: "A", value: true, key: "user_agent"},
  {long: "referer", short: "e", value: true, key: "referer"},
  {long: "cookie", short: "b", value: true, key: "cookie"},
  {long: "cookie-jar", short: "c", value: true, key: "cookie_jar"},
  {long: "dump-header", short: "D", value: true, key: "dump_header"},
  {long: "write-out", short: "w", value: true, key: "write_out"},
  {long: "connect-timeout", short: "", value: true, key: "connect_timeout"},
  {long: "max-time", short: "m", value: true, key: "max_time"},
  {long: "retry", short: "", value: true, key: "retry"},
  {long: "retry-delay", short: "", value: true, key: "retry_delay"},
  {long: "retry-connrefused", short: "", value: false, key: "retry_connrefused"},
  {long: "retry-all-errors", short: "", value: false, key: "retry_all_errors"},
  {long: "cacert", short: "", value: true, key: "cacert"},
  {long: "max-redirs", short: "", value: true, key: "max_redirs"},
  {long: "upload-file", short: "T", value: true, key: "upload_file"},
  {long: "range", short: "r", value: true, key: "range"},
  {long: "continue-at", short: "C", value: true, key: "continue_at"},
  {long: "max-filesize", short: "", value: true, key: "max_filesize"},
  {long: "stderr", short: "", value: true, key: "stderr"},
  {long: "url", short: "", value: true, key: "url"},
]

# Options real curl has that this applet does !implement. Each one stops
# with curl's "not supported by this build" diagnostic and status 4 rather than
# being accepted and ignored; anything else unknown is curl's "is unknown".
const UNSUPPORTED: List[Str] = [
  "proxy", "x", "form", "F", "config", "K", "parallel", "Z", "http1.0", "0", "http2", "http3",
  "ipv4", "4", "ipv6", "6", "limit-rate", "netrc", "n", "digest", "ntlm", "negotiate", "anyauth",
  "oauth2-bearer", "remote-header-name", "J", "output-dir", "next", "retry-max-time", "cert", "E",
  "key", "interface", "resolve", "connect-to", "unix-socket", "noproxy", "proxy-user", "U", "trace",
  "trace-ascii", "trace-time", "quote", "Q", "append", "a", "tlsv1.2", "tlsv1.3", "ciphers",
  "capath", "path-as-is", "request-target", "time-cond", "z", "json", "speed-limit", "Y",
  "speed-time", "y", "alt-svc", "hsts", "doh-url", "xattr", "remote-time", "R", "remove-on-error",
  "fail-early", "no-clobber", "ssl", "ssl-reqd", "tlsv1", "proto", "proto-redir", "libcurl",
  "no-buffer", "N", "disable", "q", "list-only", "l", "crlf", "ignore-content-length",
  "etag-save", "etag-compare", "variable", "aws-sigv4", "post301", "post302", "post303",
]

type Settings = {
  urls: List[Str],
  outputs: List[Str],
  silent: Bool,
  show_error: Bool,
  fail: Bool,
  fail_with_body: Bool,
  include: Bool,
  head: Bool,
  verbose: Bool,
  location: Bool,
  insecure: Bool,
  compressed: Bool,
  get: Bool,
  globoff: Bool,
  create_dirs: Bool,
  progress_bar: Bool,
  no_progress: Bool,
  method: Str?,
  headers: List[Header],
  data: List[Bytes],
  user: Str?,
  user_agent: Str?,
  referer: Str?,
  cookies: List[Str],
  cookie_jar: Str?,
  dump_header: Str?,
  write_out: Str?,
  connect_timeout: Duration?,
  max_time: Duration?,
  retries: Int,
  retry_delay: Int?,
  retry_connrefused: Bool,
  retry_all_errors: Bool,
  cacert: Path?,
  max_redirs: Int,
  upload_file: Str?,
  range: Str?,
  continue_at: Str?,
  max_filesize: Int?,
  stderr_to_stdout: Bool,
  want_version: Bool,
  want_help: Bool,
}

type Header = http_transfer.Header
type Request = http_transfer.Request

# Stands for the value of a `-H 'Name:'` header, which removes a default.
const REMOVE_MARK = "\0remove"

const HELP = """Usage: curl [options...] <url>
 -d, --data <data>            Post data
 -f, --fail                   Fail fast with no output on HTTP errors
 -I, --head                   Show document info only
 -H, --header <header/@file>  Pass custom header(s) to server
 -h, --help                   Show this help
 -o, --output <file>          Write to file instead of stdout
 -O, --remote-name            Write output to file named as remote file
 -i, --show-headers           Show response headers in output
 -s, --silent                 Silent mode
 -T, --upload-file <file>     Transfer local FILE to destination
 -u, --user <user:password>   Server user and password
 -A, --user-agent <name>      Send User-Agent <name> to server
 -v, --verbose                Make the operation more talkative
 -V, --version                Show version number and quit
"""

proc fail_usage(message: Str, code = 2) [io, error] {
  io.write_stderr(f"curl: {message}\n")
  io.write_stderr("curl: try 'curl --help' or 'curl --manual' for more information\n")
  io.flush_stderr()
  exit code
}

pure initial() -> Settings {
  {
    urls: [], outputs: [], silent: false, show_error: false, fail: false, fail_with_body: false,
    include: false, head: false, verbose: false, location: false, insecure: false,
    compressed: false, get: false, globoff: false, create_dirs: false, progress_bar: false,
    no_progress: false, method: null, headers: [], data: [], user: null, user_agent: null,
    referer: null, cookies: [], cookie_jar: null, dump_header: null, write_out: null,
    connect_timeout: null, max_time: null, retries: 0, retry_delay: null,
    retry_connrefused: false, retry_all_errors: false, cacert: null, max_redirs: 50,
    upload_file: null, range: null, continue_at: null, max_filesize: null,
    stderr_to_stdout: false, want_version: false, want_help: false,
  }
}

# curl takes seconds as a decimal number and rejects anything else.
proc seconds_ms(option: Str, text: Str) [io, error] -> Int {
  let parsed = text.parse_float()
  match parsed {
    Ok(value) => {
      if value < 0.0 { fail_usage(f"option {option}: expected a proper numerical parameter") }
      return (value * 1000.0).round() ?? 0
    }
    Err(_) => fail_usage(f"option {option}: expected a proper numerical parameter")
  }
  0
}

proc whole_number(option: Str, text: Str) [io, error] -> Int {
  let parsed = text.parse_int() ?? -1
  if parsed < 0 and text != "-1" {
    fail_usage(f"option {option}: expected a proper numerical parameter")
  }
  parsed
}

proc parse_header(settings: Settings, line: Str) [io, error] -> Settings {
  if line.starts_with("@") { fail_usage("option --header: reading headers from a file is not supported", 4) }
  let colon = line.find(":")
  if colon == null {
    if line.ends_with(";") {
      let name = line.byte_slice(0, line.byte_len() - 1).trim()
      return {...settings, headers: settings.headers.extend([{name: name, value: ""}])}
    }
    return settings
  }
  let at = colon ?? 0
  let name = line.byte_slice(0, at).trim()
  let value = line.byte_slice(at + 1).trim()
  {...settings, headers: settings.headers.extend([{name: name, value: if value == "" { REMOVE_MARK } else { value }}])}
}

# `-d @file` strips carriage returns and newlines from the file; `@-` reads
# standard input; `--data-binary` keeps the bytes exactly.
proc data_part(option: Str, value: Str, strip: Bool, files: Bool) [io, fs, error] -> Bytes {
  if !files or !value.starts_with("@") { return bytes.from_text(value) }
  let name = value.byte_slice(1)
  let raw = if name == "-" { io.stdin_bytes() } else { fp"{name}".read_bytes() }
  match raw {
    Ok(content) => {
      if !strip { return content }
      var kept: List[Int] = []
      for index in range(content.len()) {
        let byte = content.byte_at(index) ?? 0
        if byte != 10 and byte != 13 { kept += [byte] }
      }
      return bytes.from_ints(kept) ?? b""
    }
    Err(_) => {
      io.write_stderr(f"curl: Failed to open {name}\n")
      io.flush_stderr()
      fail_usage(f"option {option}: error encountered when reading a file", 26)
    }
  }
  b""
}

# --data-urlencode forms: `content`, `=content`, `name=content`, `@file`,
# `name@file`.
proc urlencoded_part(value: Str) [io, fs, error] -> Bytes {
  let equals = value.find("=")
  let at = value.find("@")
  if equals != null and (at == null or (equals ?? 0) < (at ?? 0)) {
    let name = value.byte_slice(0, equals ?? 0)
    let content = bytes.from_text(value.byte_slice((equals ?? 0) + 1))
    let encoded = http_transfer.percent_encode(content, true)
    return bytes.from_text(if name == "" { encoded } else { f"{name}={encoded}" })
  }
  if at != null {
    let name = value.byte_slice(0, at ?? 0)
    let content = data_part("--data-urlencode", value.byte_slice(at ?? 0), false, true)
    let encoded = http_transfer.percent_encode(content, true)
    return bytes.from_text(if name == "" { encoded } else { f"{name}={encoded}" })
  }
  bytes.from_text(http_transfer.percent_encode(bytes.from_text(value), true))
}

proc apply(settings: Settings, spec: Spec, option: Str, value: Str) [io, fs, error] -> Settings {
  match spec.key {
    "silent" => {...settings, silent: true}
    "show_error" => {...settings, show_error: true}
    "fail" => {...settings, fail: true}
    "fail_with_body" => {...settings, fail: true, fail_with_body: true}
    "include" => {...settings, include: true}
    "head" => {...settings, head: true}
    "verbose" => {...settings, verbose: true}
    "location" => {...settings, location: true}
    "insecure" => {...settings, insecure: true}
    "compressed" => {...settings, compressed: true}
    "get" => {...settings, get: true}
    "globoff" => {...settings, globoff: true}
    "create_dirs" => {...settings, create_dirs: true}
    "progress_bar" => {...settings, progress_bar: true}
    "no_progress" => {...settings, no_progress: true}
    "already_default" => settings
    "version" => {...settings, want_version: true}
    "help" => {...settings, want_help: true}
    "remote_name" => {...settings, outputs: settings.outputs.extend(["\0remote"])}
    "output" => {...settings, outputs: settings.outputs.extend([value])}
    "request" => {...settings, method: value}
    "header" => parse_header(settings, value)
    "data" => {...settings, data: settings.data.extend([data_part(option, value, true, true)])}
    "data_raw" => {...settings, data: settings.data.extend([data_part(option, value, false, false)])}
    "data_binary" => {...settings, data: settings.data.extend([data_part(option, value, false, true)])}
    "data_urlencode" => {...settings, data: settings.data.extend([urlencoded_part(value)])}
    "user" => {...settings, user: value}
    "user_agent" => {...settings, user_agent: value}
    "referer" => {...settings, referer: value}
    "cookie" => {...settings, cookies: settings.cookies.extend([value])}
    "cookie_jar" => {...settings, cookie_jar: value}
    "dump_header" => {...settings, dump_header: value}
    "write_out" => {...settings, write_out: value}
    "connect_timeout" => {...settings, connect_timeout: time.millis(seconds_ms(option, value))}
    "max_time" => {...settings, max_time: time.millis(seconds_ms(option, value))}
    "retry" => {...settings, retries: whole_number(option, value)}
    "retry_delay" => {...settings, retry_delay: seconds_ms(option, value)}
    "retry_connrefused" => {...settings, retry_connrefused: true}
    "retry_all_errors" => {...settings, retry_all_errors: true}
    "cacert" => {
      let present = fp"{value}".exists() ?? false
      if !present {
        io.write_stderr(f"curl: The file '{value}' provided to --cacert does not exist\n")
        io.flush_stderr()
        fail_usage("option --cacert: is badly used here")
      }
      {...settings, cacert: fp"{value}"}
    }
    "max_redirs" => {...settings, max_redirs: whole_number(option, value)}
    "upload_file" => {...settings, upload_file: value}
    "range" => {...settings, range: value}
    "continue_at" => {...settings, continue_at: value}
    "max_filesize" => {...settings, max_filesize: whole_number(option, value)}
    "stderr" => {
      if value != "-" { fail_usage("option --stderr: only '-' is supported by this implementation", 4) }
      {...settings, stderr_to_stdout: true}
    }
    "url" => {...settings, urls: settings.urls.extend([value])}
    _ => settings
  }
}

proc find_long(name: Str) [io, error] -> Spec? {
  for spec in OPTIONS {
    if spec.long == name { return spec }
  }
  var matches: List[Spec] = []
  for spec in OPTIONS {
    if spec.long.starts_with(name) and spec.long != "" { matches += [spec] }
  }
  if matches.is_empty() { return null }
  let first = matches[0]
  for spec in matches {
    if spec.key != first.key { fail_usage(f"option --{name}: is ambiguous") }
  }
  first
}

proc find_short(letter: Str) -> Spec? {
  for spec in OPTIONS {
    if spec.short == letter { return spec }
  }
  null
}

proc unsupported(name: Str, display: Str) [io, error] {
  if name in UNSUPPORTED {
    io.write_stderr(f"curl: option {display}: the installed libcurl version doesn't support this\n")
    io.write_stderr("curl: try 'curl --help' or 'curl --manual' for more information\n")
    io.flush_stderr()
    exit 4
  }
}

proc parse(argv: List[Str]) [io, fs, error] -> Settings {
  var settings = initial()
  var index = 0
  var operands_only = false
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if operands_only or word == "-" or !word.starts_with("-") {
      settings = {...settings, urls: settings.urls.extend([word])}
      continue
    }
    if word == "--" {
      operands_only = true
      continue
    }
    if word.starts_with("--") {
      var name = word.byte_slice(2)
      if name.starts_with("no-") and find_long(name) == null and find_long(name.byte_slice(3)) != null {
        name = name.byte_slice(3)
        let negated = find_long(name)
        if negated != null and !(negated ?? initial_spec()).value {
          let spec = negated ?? initial_spec()
          settings = negate(settings, spec)
          continue
        }
      }
      let found = find_long(name)
      if found == null {
        unsupported(name, f"--{name}")
        fail_usage(f"option --{name}: is unknown")
      }
      let spec = found ?? initial_spec()
      var value = ""
      if spec.value {
        if index >= argv.len() { fail_usage(f"option --{name}: requires parameter") }
        value = argv[index]
        index += 1
      }
      settings = apply(settings, spec, f"--{spec.long}", value)
      continue
    }
    var position = 1
    while position < word.byte_len() {
      let letter = word.byte_slice(position, 1)
      let found = find_short(letter)
      if found == null {
        unsupported(letter, f"-{letter}")
        fail_usage(f"option -{letter}: is unknown")
      }
      let spec = found ?? initial_spec()
      if spec.value {
        var value = word.byte_slice(position + 1)
        if value == "" {
          if index >= argv.len() { fail_usage(f"option -{letter}: requires parameter") }
          value = argv[index]
          index += 1
        }
        settings = apply(settings, spec, f"-{letter}", value)
        break
      }
      settings = apply(settings, spec, f"-{letter}", "")
      position += 1
    }
  }
  settings
}

pure initial_spec() -> Spec {
  {long: "", short: "", value: false, key: ""}
}

# `--no-OPTION` turns a switch back off.
pure negate(settings: Settings, spec: Spec) -> Settings {
  match spec.key {
    "silent" => {...settings, silent: false}
    "show_error" => {...settings, show_error: false}
    "fail" => {...settings, fail: false, fail_with_body: false}
    "include" => {...settings, include: false}
    "head" => {...settings, head: false}
    "verbose" => {...settings, verbose: false}
    "location" => {...settings, location: false}
    "insecure" => {...settings, insecure: false}
    "compressed" => {...settings, compressed: false}
    "get" => {...settings, get: false}
    "progress_bar" => {...settings, progress_bar: false}
    "no_progress" => {...settings, no_progress: false}
    "retry_connrefused" => {...settings, retry_connrefused: false}
    "retry_all_errors" => {...settings, retry_all_errors: false}
    _ => settings
  }
}

pure plural(amount: Int, word: Str, many: Str) -> Str {
  if amount == 1 { f"{amount} {word}" } else { f"{amount} {many}" }
}

# curl's compact size column: plain bytes below 100000, then k, M, G.
pure size_column(bytes_count: Int) -> Str {
  let kilo = 1024
  let mega = 1048576
  let giga = 1073741824
  return f"{bytes_count}" when bytes_count < 100000
  return f"{bytes_count / kilo}k" when bytes_count < 10000 * kilo
  return f"{bytes_count / mega}.{bytes_count % mega / (mega / 10)}M" when bytes_count < 100 * mega
  return f"{bytes_count / mega}M" when bytes_count < 10000 * mega
  return f"{bytes_count / giga}.{bytes_count % giga / (giga / 10)}G" when bytes_count < 100 * giga
  f"{bytes_count / giga}G"
}

pure clock_column(elapsed_ms: Int) -> Str {
  let total = elapsed_ms / 1000
  return "" when total <= 0
  let minutes = total / 60
  let secs = total % 60
  let pad = if secs < 10 { "0" } else { "" }
  let minute_pad = if minutes < 10 { "0" } else { "" }
  if minutes >= 60 {
    f"{minutes / 60}:{if minutes % 60 < 10 { "0" } else { "" }}{minutes % 60}:{pad}{secs}"
  } else {
    f"{minute_pad}{minutes}:{pad}{secs}"
  }
}

const BAR = "########################################################################"

const METER_HEADER = "  % Total    % Received % Xferd  Average Speed  Time    Time    Time   Current\n                                 Dload  Upload  Total   Spent   Left   Speed\n"

pure meter_line(percent: Int, received: Int, speed: Int, elapsed_ms: Int) -> Str {
  let rx = size_column(received)
  let dl = size_column(speed)
  let spent = clock_column(elapsed_ms)
  f"{percent:>3} {rx:>6} {percent:>3} {rx:>6}   0      0 {dl:>6}      0         {spent:>7}              0"
}

# Diagnostics and progress go to stderr and a failure to write them is
# ignored; a failed stdout write is curl's write error, status 23.
proc emit(text: Str, to_stdout: Bool) [io, error] {
  if to_stdout {
    if let Err(_) = io.write_stdout(text) { exit 23 }
    if let Err(_) = io.flush_stdout() { exit 23 }
  } else {
    let _ = io.write_stderr(text)
    let _ = io.flush_stderr()
  }
}

# One finished request's observable facts, enough for -w and -D.
type Outcome = {status: Int, reason: Str, headers: List[Header], bytes: Int, url: Str, millis: Int, code: Int}

pure status_line(status: Int, reason: Str) -> Str {
  if reason == "" { f"HTTP/1.1 {status}" } else { f"HTTP/1.1 {status} {reason}" }
}

pure header_block(status: Int, reason: Str, headers: List[Header]) -> Str {
  var out = f"{status_line(status, reason)}\r\n"
  for header in headers {
    out = f"{out}{header.name}: {header.value}\r\n"
  }
  f"{out}\r\n"
}

type Cookie = {domain: Str, subdomains: Bool, path: Str, secure: Bool, expires: Int, name: Str, value: Str}

const MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]

# A cookie jar file in Netscape format; `#HttpOnly_` marks HttpOnly cookies and
# every other `#` line is a comment.
proc load_jar(name: Str) [fs] -> List[Cookie] {
  let text = fp"{name}".read_text() ?? ""
  var jar: List[Cookie] = []
  for line in text.lines() {
    var row = line
    if row.starts_with("#HttpOnly_") { row = row.byte_slice(10) } else if row.starts_with("#") or row.trim() == "" { continue }
    let parts = row.split("\t")
    if parts.len() < 7 { continue }
    jar += [{
      domain: parts[0], subdomains: parts[1] == "TRUE", path: parts[2], secure: parts[3] == "TRUE",
      expires: parts[4].parse_int() ?? 0, name: parts[5], value: parts[6],
    }]
  }
  jar
}

# `Expires=Wed, 21 Oct 2026 07:28:00 GMT` as epoch seconds, or 0 when the date
# does !follow the RFC 1123 shape.
proc parse_expires(text: Str) [time] -> Int {
  let words = text.replace(",", with: "").words()
  if words.len() < 5 { return 0 }
  var month = 0
  for number in range(12) {
    if MONTHS[number] == words[2].lower().byte_slice(0, 3) { month = number + 1 }
  }
  let clock = words[4].split(":")
  if month == 0 or clock.len() != 3 { return 0 }
  let stamp = time.from_calendar(words[3].parse_int() ?? 1970, month, words[1].parse_int() ?? 1, clock[0].parse_int() ?? 0, clock[1].parse_int() ?? 0, clock[2].parse_int() ?? 0, true) ?? 0
  stamp / 1000000000
}

proc cookie_from(header: Str, host: Str, route: Str) [time] -> Cookie? {
  let parts = header.split(";")
  let pair = parts[0].split("=", 1)
  if pair.len() < 2 { return null }
  var domain = host
  var subdomains = false
  var scope = route
  var secure = false
  var expires = 0
  var max_age: Int? = null
  for attribute in parts[1..] {
    let item = attribute.trim()
    let kv = item.split("=", 1)
    let key = kv[0].lower()
    let val = if kv.len() > 1 { kv[1].trim() } else { "" }
    if key == "domain" and val != "" {
      domain = if val.starts_with(".") { val } else { f".{val}" }
      subdomains = true
    } else if key == "path" and val.starts_with("/") {
      scope = val
    } else if key == "secure" {
      secure = true
    } else if key == "expires" {
      expires = parse_expires(val)
    } else if key == "max-age" {
      match val.parse_int() {
        Ok(seconds_valid) => max_age = seconds_valid
        Err(_) => max_age = null
      }
    }
  }
  if let age = max_age { expires = time.now() / 1000 + age }
  {domain: domain, subdomains: subdomains, path: scope, secure: secure, expires: expires, name: pair[0].trim(), value: pair[1].trim()}
}

pure cookie_applies(cookie: Cookie, host: Str, route: Str, https: Bool, now: Int) -> Bool {
  if cookie.secure and !https { return false }
  if cookie.expires != 0 and cookie.expires < now { return false }
  if !route.starts_with(cookie.path) { return false }
  let bare = if cookie.domain.starts_with(".") { cookie.domain.byte_slice(1) } else { cookie.domain }
  host == bare or (cookie.subdomains and host.ends_with(f".{bare}"))
}

pure jar_text(jar: List[Cookie]) -> Str {
  var out = "# Netscape HTTP Cookie File\n# https://curl.se/docs/http-cookies.html\n# This file was generated by libcurl! Edit at your own risk.\n\n"
  for cookie in jar {
    let subdomains = if cookie.subdomains { "TRUE" } else { "FALSE" }
    let secure = if cookie.secure { "TRUE" } else { "FALSE" }
    out = f"{out}{cookie.domain}\t{subdomains}\t{cookie.path}\t{secure}\t{cookie.expires}\t{cookie.name}\t{cookie.value}\n"
  }
  out
}

type Rendered = {stdout: Str, stderr: Str}

# `-w` rendering: `%{name}`, `%header{name}`, `%%` and backslash escapes. A
# variable this implementation cannot know is reported the way curl reports an
# unknown one and prints nothing.
proc render_write_out(format: Str, outcome: Outcome, method: Str, location: Bool) [io, error] -> Rendered {
  var out = ""
  var err = ""
  var to_err = false
  var index = 0
  let total = format.byte_len()
  while index < total {
    let char = format.byte_slice(index, 1)
    var piece = ""
    if char == "\\" and index + 1 < total {
      let next = format.byte_slice(index + 1, 1)
      index += 2
      piece = if next == "n" { "\n" } else if next == "r" { "\r" } else if next == "t" { "\t" } else if next == "\\" { "\\" } else { f"\\{next}" }
    } else if char == "%" and index + 1 < total and format.byte_slice(index + 1, 1) == "%" {
      index += 2
      piece = "%"
    } else if char == "%" and format.byte_slice(index + 1, 7) == "header{" {
      let close = format.find("}", index) ?? -1
      if close < 0 {
        piece = "%"
        index += 1
      } else {
        let name = format.byte_slice(index + 8, close - index - 8)
        piece = http_transfer.header_value(outcome.headers, name) ?? ""
        index = close + 1
      }
    } else if char == "%" and index + 1 < total and format.byte_slice(index + 1, 1) == "{" {
      let close = format.find("}", index) ?? -1
      if close < 0 {
        piece = "%"
        index += 1
      } else {
        let name = format.byte_slice(index + 2, close - index - 2)
        index = close + 1
        if name == "stderr" {
          to_err = true
        } else if name == "stdout" {
          to_err = false
        } else {
          piece = write_out_value(name, outcome, method, location)
        }
      }
    } else {
      piece = char
      index += 1
    }
    if to_err { err = f"{err}{piece}" } else { out = f"{out}{piece}" }
  }
  {stdout: out, stderr: err}
}

proc write_out_value(name: Str, outcome: Outcome, method: Str, location: Bool) [io, error] -> Str {
  let size_header = header_block(outcome.status, outcome.reason, outcome.headers).byte_len()
  match name {
    "http_code" | "response_code" => if outcome.status == 0 { "000" } else { f"{outcome.status}" }
    "size_download" => f"{outcome.bytes}"
    "size_upload" => "0"
    "size_header" => f"{size_header}"
    "url_effective" => outcome.url
    "content_type" => http_transfer.header_value(outcome.headers, "content-type") ?? ""
    "exitcode" => f"{outcome.code}"
    "method" => method
    "scheme" => http_transfer.url_scheme(outcome.url) ?? ""
    "http_version" => "1.1"
    "time_total" => http_transfer.seconds_text(outcome.millis)
    "speed_download" => f"{if outcome.millis < 1 { outcome.bytes * 1000 } else { outcome.bytes * 1000 / outcome.millis }}"
    "num_redirects" => if location { unknown_variable(name) } else { "0" }
    "redirect_url" => if location { unknown_variable(name) } else { "" }
    _ => unknown_variable(name)
  }
}

proc unknown_variable(name: Str) [io, error] -> Str {
  emit(f"curl: unknown --write-out variable: '{name}'\n", false)
  ""
}

const ALLOWED_METHODS: List[Str] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"]

# Statuses curl treats as transient for --retry.
const TRANSIENT_STATUS: List[Int] = [408, 429, 500, 502, 503, 504]

type Attempt = {code: Int, again: Str?}

proc show_error(settings: Settings, code: Int, message: Str) [io, error] {
  if settings.silent and !settings.show_error { return }
  emit(f"curl: ({code}) {message}\n", settings.stderr_to_stdout)
}

proc warn(settings: Settings, message: Str) [io, error] {
  if settings.silent { return }
  emit(f"Warning: {message}\n", settings.stderr_to_stdout)
}

type Failure = {code: Int, message: Str, again: Str?}

# The exit status, message and retry class of a failed transfer.
proc describe_failure(failure: http_transfer.TransferFailure, request: Request, host: Str, millis: Int, follows: Bool) -> Failure {
  match failure {
    http_transfer.TransferFailure.Dns => {code: 6, message: f"Could not resolve host: {host} (Domain name not found)", again: null}
    http_transfer.TransferFailure.Connect => {
      code: 7,
      message: f"Failed to connect to {host}:{http_transfer.url_port(request.url)} after {millis} ms: Could not connect to server",
      again: "connection refused",
    }
    http_transfer.TransferFailure.ConnectTimeout => {code: 28, message: f"Connection timed out after {millis} milliseconds", again: "timeout"}
    http_transfer.TransferFailure.Timeout => {code: 28, message: f"Operation timed out after {millis} milliseconds with 0 bytes received", again: "timeout"}
    http_transfer.TransferFailure.Certificate => {
      code: 60,
      message: "SSL certificate problem: unable to get local issuer certificate\nMore details here: https://curl.se/docs/sslcerts.html\n\ncurl failed to verify the legitimacy of the server and therefore could not\nestablish a secure connection to it. To learn more about this situation and\nhow to fix it, please visit the webpage mentioned above.",
      again: null,
    }
    http_transfer.TransferFailure.Handshake => {code: 35, message: "TLS connect error: handshake failure", again: null}
    http_transfer.TransferFailure.EmptyReply => {code: 52, message: "Empty reply from server", again: "empty reply"}
    http_transfer.TransferFailure.Redirects => {
      code: 47,
      message: if follows { f"Maximum ({request.redirects}) redirects followed" } else { "Received a redirect response, which this build can only follow (use -L)" },
      again: null,
    }
    http_transfer.TransferFailure.Status {code} => {code: 22, message: f"The requested URL returned error: {code}", again: if code in TRANSIENT_STATUS { "HTTP error" } else { null }}
    http_transfer.TransferFailure.Scheme {message} => {code: 4, message: f"{message}", again: null}
    http_transfer.TransferFailure.Write => {code: 23, message: "client returned ERROR on write", again: null}
    http_transfer.TransferFailure.TrustStore => {code: 77, message: "error adding trust anchors from file", again: null}
    http_transfer.TransferFailure.Other {message} => {code: 56, message: f"Failure with receiving network data: {message}", again: null}
  }
}

# The default request headers curl sends, then the user's -H headers, which
# replace a default of the same name; `Name:` with no value removes it.
proc request_headers_for(settings: Settings, url: Str, body: Bytes, resume_from: Int, jar: List[Cookie]) [fs, env, io, error, time] -> List[Header] {
  var headers: List[Header] = [{name: "Host", value: http_transfer.url_authority(url)}]
  if let credentials = settings.user {
    var text = credentials
    if text.find(":") == null {
      emit(f"Enter host password for user '{credentials}':", false)
      let line = io.stdin_line() ?? ""
      text = f"{credentials}:{line.trim()}"
    }
    if text != ":" { headers += [http_transfer.basic_authorization(text)] }
  }
  let agent = settings.user_agent ?? f"curl/{VERSION}"
  if agent != "" { headers += [{name: "User-Agent", value: agent}] }
  headers += [{name: "Accept", value: "*/*"}]
  if let from = settings.referer { headers += [{name: "Referer", value: from}] }
  var pairs: List[Str] = []
  let host = http_transfer.url_host(url)
  let route = http_transfer.url_path(url)
  let https = http_transfer.url_scheme(url) == "https"
  for cookie in settings.cookies {
    if cookie.find("=") != null { pairs += [cookie] }
  }
  let now = time.now() / 1000
  for cookie in jar {
    if cookie_applies(cookie, host, route, https, now) { pairs += [f"{cookie.name}={cookie.value}"] }
  }
  if !pairs.is_empty() { headers += [{name: "Cookie", value: pairs.join("; ")}] }
  if settings.compressed { headers += [{name: "Accept-Encoding", value: "gzip"}] }
  if let span = settings.range { headers += [{name: "Range", value: f"bytes={span}"}] }
  if resume_from > 0 { headers += [{name: "Range", value: f"bytes={resume_from}-"}] }
  for custom in settings.headers {
    headers = http_transfer.without_header(headers, custom.name)
    if custom.value != REMOVE_MARK { headers += [custom] }
  }
  if !body.is_empty() or (!settings.data.is_empty() and !settings.get) {
    if !http_transfer.has_header(headers, "content-length") {
      headers += [{name: "Content-Length", value: f"{body.len()}"}]
    }
    if settings.upload_file == null and !http_transfer.has_header(headers, "content-type") {
      headers += [{name: "Content-Type", value: "application/x-www-form-urlencoded"}]
    }
  }
  headers
}

# The remote name `-O` saves under; curl falls back to "curl_response".
proc remote_name(settings: Settings, url: Str) [io, error] -> Str {
  let name = http_transfer.url_file_name(url)
  if name != "" { return name }
  warn(settings, "No remote filename, uses \"curl_response\"")
  "curl_response"
}

# Build the body, method and URL from the data options and run the transfer
# with retries. Returns curl's exit status for this URL.
proc transfer(settings: Settings, raw_url: Str, output: Str) [net, fs, process, env, io, error, time] -> Int {
  var url = raw_url
  let scheme = http_transfer.url_scheme(url)
  if scheme == null {
    url = f"http://{url}"
  } else if scheme not in ["http", "https"] {
    show_error(settings, 1, f"Protocol \"{scheme ?? ""}\" not supported")
    return 1
  }
  if !settings.globoff and (url.find("[") != null or url.find("{") != null) {
    show_error(settings, 3, "URL using bad/illegal format or missing URL (URL globbing is not supported; use -g)")
    return 3
  }
  var body = b""
  var parts: List[Bytes] = []
  var first = true
  for part in settings.data {
    if !first { parts += [b"&"] }
    parts += [part]
    first = false
  }
  body = bytes.concat(parts)
  var method = settings.method ?? "GET"
  if settings.method == null {
    if settings.head { method = "HEAD" } else if settings.upload_file != null { method = "PUT" } else if !settings.data.is_empty() and !settings.get { method = "POST" }
  }
  if method not in ALLOWED_METHODS {
    show_error(settings, 4, f"HTTP method {method} is not supported by this implementation")
    return 4
  }
  if settings.get and !body.is_empty() {
    let text = body.utf8() ?? ""
    url = if url.find("?") != null { f"{url}&{text}" } else { f"{url}?{text}" }
    body = b""
  }
  if let source = settings.upload_file {
    var target = url
    if target.ends_with("/") { target = f"{target}{fp"{source}".basename()}" }
    url = target
    let content = if source == "-" { io.stdin_bytes() } else { fp"{source}".read_bytes() }
    match content {
      Ok(bytes_in) => body = bytes_in
      Err(_) => {
        show_error(settings, 26, "Failed to open/read local data from file/application")
        return 26
      }
    }
  }

  var dest: Path? = null
  if output == "\0remote" {
    dest = fp"{remote_name(settings, url)}"
  } else if output != "" and output != "-" {
    dest = fp"{output}"
  }
  if let target = dest {
    if settings.create_dirs {
      let parent = target.parent()
      let _ = parent.mkdir(parents: true)
    }
  }

  var jar: List[Cookie] = []
  for cookie in settings.cookies {
    if cookie.find("=") == null { jar = jar.extend(load_jar(cookie)) }
  }

  var resume_from = 0
  if let spec = settings.continue_at {
    if spec == "-" {
      if let target = dest {
        let known = target.metadata()
        match known {
          Ok(info) => resume_from = info.size
          Err(_) => resume_from = 0
        }
      }
    } else {
      resume_from = spec.parse_int() ?? 0
    }
  }

  let headers = request_headers_for(settings, url, body, resume_from, jar)
  let reply_needs_buffer = settings.include or settings.head or resume_from > 0 or settings.compressed or settings.max_filesize != null or settings.dump_header == "-" or settings.cookie_jar != null or settings.write_out != null
  var left = settings.retries
  var wait_ms = settings.retry_delay ?? 1000
  while true {
    let request: Request = {
      method: method,
      url: url,
      headers: headers,
      body: body,
      timeout: settings.max_time,
      connect_timeout: settings.connect_timeout,
      idle_timeout: null,
      redirects: if settings.location { settings.max_redirs } else { 0 },
      verify: !settings.insecure,
      cacert: settings.cacert,
      fail_status: settings.fail and !settings.fail_with_body and dest != null and !reply_needs_buffer,
    }
    let result = attempt(settings, request, dest, resume_from, method, jar, reply_needs_buffer)
    if result.again == null or left == 0 { return result.code }
    let reason = result.again ?? ""
    let eligible = reason in ["HTTP error", "timeout"] or (reason == "connection refused" and settings.retry_connrefused) or settings.retry_all_errors
    if !eligible { return result.code }
    warn(settings, f"Problem : {reason}. Retrying in {plural(wait_ms / 1000, "second", "seconds")}. {plural(left, "retry", "retries")} left.")
    time.sleep(time.millis(wait_ms))?
    left -= 1
    if settings.retry_delay == null { wait_ms = wait_ms * 2 }
  }
  0
}

# Strip a gzip content coding from a buffered body through a scratch file,
# because the codec works on paths.
proc gunzip(data: Bytes) [fs, error] -> Result[Bytes] {
  let scratch = fs.tempdir()?
  defer scratch.close()
  let dir = scratch.host_path()?
  fp"{dir}/in.gz".write(data)?
  compression.transform(fp"{dir}/in.gz", fp"{dir}/out", "gzip", true)?
  Ok(fp"{dir}/out".read_bytes()?)
}

# Run one request and deliver its output. The returned retry class is set for
# failures and statuses curl's --retry treats as transient.
proc attempt(settings: Settings, request: Request, dest: Path?, resume_from: Int, method: Str, jar: List[Cookie], buffered: Bool) [net, fs, process, io, error, time] -> Attempt {
  let host = http_transfer.url_host(request.url)
  let to_terminal = dest == null and unix.isatty(1)
  let meter = !settings.silent and !settings.no_progress and !settings.progress_bar and !to_terminal
  let bar = !settings.silent and !settings.no_progress and settings.progress_bar and !to_terminal
  let started = time.now()
  if settings.verbose {
    let route = http_transfer.url_path(request.url)
    emit(f"*   Trying {host}:{http_transfer.url_port(request.url)}...\n", settings.stderr_to_stdout)
    emit("* using HTTP/1.x\n", settings.stderr_to_stdout)
    emit(f"> {request.method} {route} HTTP/1.1\n", settings.stderr_to_stdout)
    for header in request.headers { emit(f"> {header.name}: {header.value}\n", settings.stderr_to_stdout) }
    emit("> \n", settings.stderr_to_stdout)
    if request.body.is_empty() {
      emit("* Request completely sent off\n", settings.stderr_to_stdout)
    } else {
      emit(f"}} [{request.body.len()} bytes data]\n* upload completely sent off: {request.body.len()} bytes\n", settings.stderr_to_stdout)
    }
  }
  if meter { emit(f"{METER_HEADER}\r{meter_line(0, 0, 0, 0)}", settings.stderr_to_stdout) }

  let streamed = dest != null and !buffered
  let result = if streamed { http_transfer.fetch_to(request, dest ?? p"") } else { http_transfer.fetch(request) }
  let elapsed = time.now() - started

  match result {
    Err(failure) => {
      let described = describe_failure(failure, request, host, elapsed, settings.location)
      if meter or bar { emit("\n", settings.stderr_to_stdout) }
      show_error(settings, described.code, described.message)
      if settings.write_out != null {
        let outcome: Outcome = {status: 0, reason: "", headers: [], bytes: 0, url: request.url, millis: elapsed, code: described.code}
        finish_write_out(settings, outcome, method)
      }
      return {code: described.code, again: described.again}
    }
    Ok(reply) => {
      var code = 0
      var body = reply.body
      if settings.verbose {
        emit(f"< {status_line(reply.status, reply.reason)}\n", settings.stderr_to_stdout)
        for header in reply.headers { emit(f"< {header.name}: {header.value}\n", settings.stderr_to_stdout) }
        emit("< \n", settings.stderr_to_stdout)
        if reply.bytes > 0 { emit(f"{{ [{reply.bytes} bytes data]\n", settings.stderr_to_stdout) }
        emit("* shutting down connection #0\n", settings.stderr_to_stdout)
      }
      if let name = settings.dump_header {
        let block = header_block(reply.status, reply.reason, reply.headers)
        if name == "-" {
          emit(block, true)
        } else {
          if let Err(_) = fp"{name}".write(block) {
            show_error(settings, 23, "Failed writing header")
            return {code: 23, again: null}
          }
        }
      }
      if resume_from > 0 and reply.status == 200 {
        show_error(settings, 33, "HTTP server doesn't seem to support byte ranges. Cannot resume.")
        return {code: 33, again: null}
      }
      if settings.compressed and http_transfer.header_value(reply.headers, "content-encoding") == "gzip" and !streamed {
        match gunzip(body) {
          Ok(plain) => body = plain
          Err(_) => {
            show_error(settings, 61, "Unrecognized content encoding type")
            return {code: 61, again: null}
          }
        }
      }
      if let limit = settings.max_filesize {
        if body.len() > limit {
          show_error(settings, 63, "Maximum file size exceeded")
          return {code: 63, again: null}
        }
      }
      var again: Str? = null
      if reply.status in TRANSIENT_STATUS { again = "HTTP error" }
      let failed = settings.fail and reply.status >= 400 and !(resume_from > 0 and reply.status == 416)
      if failed and !settings.fail_with_body {
        if meter or bar { emit("\n", settings.stderr_to_stdout) }
        if settings.head or settings.include {
          let block = header_block(reply.status, reply.reason, reply.headers)
          emit(block, true)
        }
        show_error(settings, 22, f"The requested URL returned error: {reply.status}")
        return {code: 22, again: again}
      }
      if failed { code = 22 }
      var out = b""
      if settings.include or settings.head {
        out = bytes.from_text(header_block(reply.status, reply.reason, reply.headers))
      }
      if !settings.head and !(resume_from > 0 and reply.status == 416) { out = bytes.concat([out, body]) }
      if !streamed {
        if let target = dest {
          var written = Ok(0)
          if resume_from > 0 and reply.status == 206 {
            written = bytes.write_at(target, resume_from, out, true)
          } else if resume_from == 0 or reply.status != 416 {
            written = match target.write(out) {
              Ok(_) => Ok(0)
              Err(failure) => Err(failure)
            }
          }
          if let Err(_) = written {
            show_error(settings, 23, f"client returned ERROR on write of {out.len()} bytes")
            return {code: 23, again: null}
          }
        } else {
          io.write_stdout_bytes(out)?
          io.flush_stdout()?
        }
      }
      if let jar_name = settings.cookie_jar {
        let now = time.now() / 1000
        var all: List[Cookie] = []
        for cookie in jar { if cookie.expires == 0 or cookie.expires >= now { all += [cookie] } }
        for header in reply.headers {
          if header.name.lower() == "set-cookie" {
            if let made = cookie_from(header.value, host, cookie_dir(http_transfer.url_path(request.url))) {
              var kept: List[Cookie] = []
              for old in all { if !(old.domain == made.domain and old.path == made.path and old.name == made.name) { kept += [old] } }
              all = kept.extend([made])
            }
          }
        }
        if jar_name == "-" {
          emit(jar_text(all), true)
        } else if let Err(_) = fp"{jar_name}".write(jar_text(all)) {
          show_error(settings, 23, "Failed writing cookie jar")
          return {code: 23, again: null}
        }
      }
      let shown = if streamed { reply.bytes } else { body.len() }
      if meter {
        let speed = if elapsed < 1 { shown * 1000 } else { shown * 1000 / elapsed }
        emit(f"\r{meter_line(100, shown, speed, elapsed)}\n", settings.stderr_to_stdout)
      }
      if bar { emit(f"\r{BAR} 100.0%\n", settings.stderr_to_stdout) }
      if settings.write_out != null {
        let outcome: Outcome = {status: reply.status, reason: reply.reason, headers: reply.headers, bytes: shown, url: request.url, millis: elapsed, code: code}
        finish_write_out(settings, outcome, method)
      }
      if failed { show_error(settings, 22, f"The requested URL returned error: {reply.status}") }
      return {code: code, again: again}
    }
  }
}

# The directory part of a request path, which RFC 6265 uses as the default
# cookie path.
pure cookie_dir(route: Str) -> Str {
  let parts = route.split("/")
  if parts.len() <= 2 { return "/" }
  var out = ""
  for index in range(parts.len() - 1) {
    if parts[index] != "" { out = f"{out}/{parts[index]}" }
  }
  if out == "" { "/" } else { out }
}

proc finish_write_out(settings: Settings, outcome: Outcome, method: Str) [fs, io, error] {
  var format = settings.write_out ?? ""
  if format.starts_with("@") {
    let name = format.byte_slice(1)
    let text = if name == "-" { io.stdin_text() } else { fp"{name}".read_text() }
    format = text ?? ""
  }
  let rendered = render_write_out(format, outcome, method, settings.location)
  if rendered.stdout != "" { emit(rendered.stdout, true) }
  if rendered.stderr != "" { emit(rendered.stderr, settings.stderr_to_stdout) }
}

proc main(...argv: List[Str]) [net, fs, process, env, io, error, time] {
  if argv.is_empty() {
    io.write_stderr("curl: try 'curl --help' or 'curl --manual' for more information\n")
    io.flush_stderr()
    exit 2
  }
  let settings = parse(argv)
  if settings.want_version {
    io.write_stdout(f"curl {VERSION} (XSH core, net module)\nProtocols: http https\nFeatures: Largefile SSL\n")
    io.flush_stdout()
    return
  }
  if settings.want_help {
    io.write_stdout(HELP)
    io.flush_stdout()
    return
  }
  if settings.urls.is_empty() {
    io.write_stderr("curl: (2) no URL specified\n")
    io.write_stderr("curl: try 'curl --help' or 'curl --manual' for more information\n")
    io.flush_stderr()
    exit 2
  }
  var code = 0
  var number = 0
  for url in settings.urls {
    let output = if number < settings.outputs.len() { settings.outputs[number] } else { "" }
    number += 1
    code = transfer(settings, url, output)
  }
  exit code
}
