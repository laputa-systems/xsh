#!/bin/xsh
use lib.http_transfer
use lib.gnu

const VERSION = "1.25.0"

# Wget exit statuses.
const EXIT_GENERIC = 1
const EXIT_USAGE = 2
const EXIT_IO = 3
const EXIT_NETWORK = 4
const EXIT_TLS = 5
const EXIT_AUTH = 6
const EXIT_SERVER = 8

type Header = http_transfer.Header

type Options = {
  urls: List[Str],
  output: Str?,
  quiet: Bool,
  terse: Bool,
  resume: Bool,
  tries: Int,
  timeout: Duration?,
  headers: List[Header],
  agent: Str?,
  referer: Str?,
  verify: Bool,
  prefix: Str?,
  spider: Bool,
  server_response: Bool,
  user: Str?,
  password: Str?,
  post_data: Str?,
  no_clobber: Bool,
  max_redirect: Int,
  input_file: Str?,
  ca_certificate: Path?,
  help: Bool,
  version: Bool,
}

# One accepted option: long name, short letter ("" for none), and whether it
# takes a value.
type Spec = {long: Str, short: Str, value: Bool}

const SPECS: List[Spec] = [
  {long: "output-document", short: "O", value: true},
  {long: "quiet", short: "q", value: false},
  {long: "no-verbose", short: "", value: false},
  {long: "continue", short: "c", value: false},
  {long: "tries", short: "t", value: true},
  {long: "timeout", short: "T", value: true},
  {long: "header", short: "", value: true},
  {long: "user-agent", short: "U", value: true},
  {long: "referer", short: "", value: true},
  {long: "no-check-certificate", short: "", value: false},
  {long: "directory-prefix", short: "P", value: true},
  {long: "spider", short: "", value: false},
  {long: "server-response", short: "S", value: false},
  {long: "user", short: "", value: true},
  {long: "password", short: "", value: true},
  {long: "post-data", short: "", value: true},
  {long: "no-clobber", short: "", value: false},
  {long: "max-redirect", short: "", value: true},
  {long: "input-file", short: "i", value: true},
  {long: "ca-certificate", short: "", value: true},
  {long: "progress", short: "", value: true},
  {long: "version", short: "V", value: false},
  {long: "help", short: "h", value: false},
]

# Real wget options this applet does not implement; each is refused rather
# than accepted and ignored.
const UNSUPPORTED: List[Str] = [
  "recursive", "r", "mirror", "m", "no-parent", "np", "level", "l", "span-hosts", "H",
  "timestamping", "N", "background", "b", "execute", "e", "append-output", "a", "output-file", "o",
  "load-cookies", "save-cookies", "keep-session-cookies", "post-file", "content-disposition",
  "convert-links", "k", "page-requisites", "p", "limit-rate", "wait", "w", "waitretry", "random-wait",
  "bind-address", "inet4-only", "4", "inet6-only", "6", "retry-connrefused", "no-proxy", "proxy",
  "http-user", "http-password", "no-cache", "default-page", "adjust-extension", "E", "force-html",
  "F", "base", "B", "accept", "A", "reject", "R", "include-directories", "I", "exclude-directories",
  "X", "no-directories", "nd", "force-directories", "x", "no-host-directories", "nH", "cut-dirs",
  "certificate", "private-key", "secure-protocol", "https-only", "ignore-length", "debug", "d",
  "dns-timeout", "connect-timeout", "read-timeout", "start-pos", "compression", "trust-server-names",
  "method", "body-data", "body-file", "if-modified-since", "quota", "Q", "delete-after", "unlink",
  "save-headers", "ask-password", "use-askpass", "xattr", "config", "no-config", "hsts-file", "no-hsts",
  "local-encoding", "remote-encoding", "unlink", "restrict-file-names", "ignore-case", "i",
  "relative", "L", "follow-ftp", "ftp-user", "ftp-password", "no-passive-ftp", "preserve-permissions",
]

const HELP = """GNU Wget 1.25.0 (XSH core), a non-interactive network retriever.
Usage: wget [OPTION]... [URL]...

  -V,  --version           display the version of Wget and exit
  -h,  --help              print this help
  -q,  --quiet             quiet (no output)
       --no-verbose        turn off verboseness, without being quiet
  -O,  --output-document=FILE  write documents to FILE
  -c,  --continue          resume getting a partially-downloaded file
  -t,  --tries=NUMBER      set number of retries to NUMBER (0 unlimits)
  -T,  --timeout=SECONDS   set all timeout values to SECONDS
  -P,  --directory-prefix=PREFIX  save files to PREFIX/..
  -U,  --user-agent=AGENT  identify as AGENT instead of Wget/VERSION
       --header=STRING     insert STRING among the headers
       --referer=URL       include 'Referer: URL' header in HTTP request
       --user=USER, --password=PASS  set HTTP credentials
       --post-data=STRING  use the POST method; send STRING as the data
       --no-check-certificate  don't validate the server's certificate
       --ca-certificate=FILE  file with the bundle of CA's
       --spider            don't download anything
  -S,  --server-response   print server response
  -nc, --no-clobber        skip downloads that would download to existing files
       --max-redirect=NUMBER  maximum redirections allowed per page
  -i,  --input-file=FILE   download URLs found in local or external FILE
"""

# `Usage` text wget prints after a usage error, with the status it exits with.
proc usage_failure(message: Str, status: Int) [io, error] {
  io.write_stderr(f"wget: {message}\nUsage: wget [OPTION]... [URL]...\n\nTry `wget --help' for more options.\n")
  io.flush_stderr()
  exit status
}

proc log(options: Options, text: Str) [io, error] {
  if options.quiet { return }
  let _ = io.write_stderr(text)
  let _ = io.flush_stderr()
}

# Error lines survive --no-verbose but not --quiet.
proc log_verbose(options: Options, text: Str) [io, error] {
  if options.terse { return }
  log(options, text)
}

pure initial() -> Options {
  {
    urls: [], output: null, quiet: false, terse: false, resume: false, tries: 20, timeout: null,
    headers: [], agent: null, referer: null, verify: true, prefix: null, spider: false,
    server_response: false, user: null, password: null, post_data: null, no_clobber: false,
    max_redirect: 20, input_file: null, ca_certificate: null, help: false, version: false,
  }
}

proc number(option: Str, text: Str) [io, error] -> Int {
  let parsed = text.parse_int() ?? -1
  if parsed < 0 {
    io.write_stderr(f"wget: {option}: Invalid number '{text}'.\n")
    io.flush_stderr()
    exit EXIT_USAGE
  }
  parsed
}

proc seconds(option: Str, text: Str) [io, error] -> Duration {
  let parsed = text.parse_float()
  match parsed {
    Ok(value) => {
      if value >= 0.0 { return time.millis((value * 1000.0).round() ?? 0) }
    }
    Err(_) => {}
  }
  io.write_stderr(f"wget: {option}: Invalid time period '{text}'\n")
  io.flush_stderr()
  exit EXIT_USAGE
}

proc apply(options: Options, spec: Spec, value: Str) [io, error] -> Options {
  match spec.long {
    "output-document" => {...options, output: value}
    "quiet" => {...options, quiet: true}
    "no-verbose" => {...options, terse: true}
    "continue" => {...options, resume: true}
    "tries" => {...options, tries: number("--tries", value)}
    "timeout" => {...options, timeout: seconds("--timeout", value)}
    "header" => {
      let colon = value.find(":")
      if colon == null {
        io.write_stderr(f"wget: --header: Invalid header '{value}'.\n")
        io.flush_stderr()
        exit EXIT_USAGE
      }
      let at = colon ?? 0
      {...options, headers: options.headers.extend([{name: value.byte_slice(0, at).trim(), value: value.byte_slice(at + 1).trim()}])}
    }
    "user-agent" => {...options, agent: value}
    "referer" => {...options, referer: value}
    "no-check-certificate" => {...options, verify: false}
    "directory-prefix" => {...options, prefix: value}
    "spider" => {...options, spider: true}
    "server-response" => {...options, server_response: true}
    "user" => {...options, user: value}
    "password" => {...options, password: value}
    "post-data" => {...options, post_data: value}
    "no-clobber" => {...options, no_clobber: true}
    "max-redirect" => {...options, max_redirect: number("--max-redirect", value)}
    "input-file" => {...options, input_file: value}
    "ca-certificate" => {...options, ca_certificate: fp"{value}"}
    "progress" => {
      if value not in ["dot", "bar", "dot:default", "bar:force"] {
        io.write_stderr(f"wget: --progress: Invalid progress type '{value}'.\n")
        io.flush_stderr()
        exit EXIT_USAGE
      }
      options
    }
    "version" => {...options, version: true}
    "help" => {...options, help: true}
    _ => options
  }
}

proc lookup_long(name: Str) -> Spec? {
  for spec in SPECS {
    if spec.long == name { return spec }
  }
  var found: Spec? = null
  for spec in SPECS {
    if spec.long.starts_with(name) {
      if found != null { return null }
      found = spec
    }
  }
  found
}

proc lookup_short(letter: Str) -> Spec? {
  for spec in SPECS {
    if spec.short == letter { return spec }
  }
  null
}

proc refuse(name: Str, display: Str) [io, error] {
  if name in UNSUPPORTED {
    io.write_stderr(f"wget: {display}: this option is not supported by this implementation\n")
    io.flush_stderr()
    exit EXIT_USAGE
  }
}

proc parse(argv: List[Str]) [io, error] -> Options {
  var options = initial()
  var index = 0
  var operands_only = false
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if operands_only or word == "-" or !word.starts_with("-") {
      options = {...options, urls: options.urls.extend([word])}
      continue
    }
    if word == "--" {
      operands_only = true
      continue
    }
    if word == "-nv" {
      options = {...options, terse: true}
      continue
    }
    if word == "-nc" {
      options = {...options, no_clobber: true}
      continue
    }
    if word.starts_with("--") {
      let equals = word.find("=")
      let name = if equals == null { word.byte_slice(2) } else { word.byte_slice(2, (equals ?? 0) - 2) }
      let found = lookup_long(name)
      if found == null {
        refuse(name, f"--{name}")
        usage_failure(f"unrecognized option '{word}'", EXIT_USAGE)
      }
      let spec = found ?? {long: "", short: "", value: false}
      var value = ""
      if spec.value {
        if equals != null {
          value = word.byte_slice((equals ?? 0) + 1)
        } else {
          if index >= argv.len() { usage_failure(f"option '--{spec.long}' requires an argument", EXIT_USAGE) }
          value = argv[index]
          index += 1
        }
      } else if equals != null {
        usage_failure(f"option '--{spec.long}' doesn't allow an argument", EXIT_USAGE)
      }
      options = apply(options, spec, value)
      continue
    }
    var position = 1
    while position < word.byte_len() {
      let letter = word.byte_slice(position, 1)
      let found = lookup_short(letter)
      if found == null {
        refuse(letter, f"-{letter}")
        usage_failure(f"invalid option -- '{letter}'", EXIT_USAGE)
      }
      let spec = found ?? {long: "", short: "", value: false}
      if spec.value {
        var value = word.byte_slice(position + 1)
        if value == "" {
          if index >= argv.len() { usage_failure(f"option requires an argument -- '{letter}'", EXIT_USAGE) }
          value = argv[index]
          index += 1
        }
        options = apply(options, spec, value)
        break
      }
      options = apply(options, spec, "")
      position += 1
    }
  }
  options
}

pure unit_rate(rate: Float) -> Str {
  var value = rate
  var unit = "B/s"
  for next in ["KB/s", "MB/s", "GB/s", "TB/s"] {
    if value >= 1024.0 {
      value = value / 1024.0
      unit = next
    }
  }
  let precision = if value >= 99.95 { 0 } else if value >= 9.995 { 1 } else { 2 }
  f"{value.format(precision)} {unit}"
}

# Rate in bytes per second; a transfer faster than the clock tick counts as
# one millisecond, as wget substitutes the clock resolution.
pure rate_of(total: Int, millis: Int) -> Float {
  let span = if millis < 1 { 1 } else { millis }
  total.float() * 1000.0 / span.float()
}

# The letter-suffixed 5-column speed of a dot progress row.
pure dot_rate(rate: Float) -> Str {
  var value = rate
  var unit = ""
  for next in ["K", "M", "G", "T"] {
    if value >= 1024.0 {
      value = value / 1024.0
      unit = next
    }
  }
  let precision = if value >= 99.95 { 0 } else if value >= 9.995 { 1 } else { 2 }
  f"{value.format(precision)}{unit}"
}

# `Length:` sizes: 1500 -> 1.5K, 20000 -> 20K, 2500000 -> 2.4M.
pure human_size(size: Int) -> Str {
  if size < 1024 { return f"{size}" }
  var value = size.float()
  var unit = ""
  for next in ["K", "M", "G", "T"] {
    if value >= 1024.0 {
      value = value / 1024.0
      unit = next
    }
  }
  f"{value.format(if value < 9.995 { 1 } else { 0 })}{unit}"
}

pure elapsed_text(millis: Int) -> Str {
  let value = millis.float() / 1000.0
  let precision = if value >= 100.0 { 0 } else if value >= 10.0 { 1 } else if value >= 1.0 { 2 } else { 3 }
  var text = value.format(precision)
  if text.find(".") != null {
    while text.ends_with("0") { text = text.byte_slice(0, text.byte_len() - 1) }
    if text.ends_with(".") { text = text.byte_slice(0, text.byte_len() - 1) }
  }
  if text == "" { "0s" } else { f"{text}s" }
}

# The dot-style progress wget prints on a stream that is not a terminal: one
# dot per kilobyte, ten to a cluster, fifty to a row, with the percentage and
# speed on each row and the elapsed time on the last.
pure progress_rows(total: Int?, counted: Int, fetched: Int, skipped: Int, millis: Int) -> Str {
  let dots = counted / 1024
  let rate = rate_of(fetched, millis)
  var rows = if skipped > 0 and skipped < 1024 { f"{0:>6}K\n" } else { "" }
  var row = 0
  while row * 50 <= dots {
    var text = ""
    var count = 0
    while count < 50 and row * 50 + count < dots {
      text = if row * 50 + count < skipped / 1024 { f"{text}," } else { f"{text}." }
      if count % 10 == 9 and count < 49 and row * 50 + count + 1 < dots { text = f"{text} " }
      count += 1
    }
    let last = (row + 1) * 50 > dots
    let label = f"{row * 50:>6}K"
    var padded = text
    while padded.byte_len() < 54 { padded = f"{padded} " }
    var tail = ""
    if last {
      if total == null {
        tail = f" {dot_rate(rate):>5}={elapsed_text(millis)}"
      } else {
        tail = f"{100:>3}% {dot_rate(rate):>5}={elapsed_text(millis)}"
      }
    } else {
      let size = total ?? counted
      let done = (row + 1) * 50 * 1024
      let percent = if size > 0 { done * 100 / size } else { 100 }
      tail = f"{percent:>3}% {dot_rate(rate):>5} 0s"
    }
    rows = f"{rows}{label} {padded}{tail}\n"
    row += 1
  }
  rows
}

proc stamp() [time, error] -> Str {
  time.format(time.now() * 1000000, "%Y-%m-%d %H:%M:%S") ?? ""
}

# How a failed attempt is reported: the text that completes the current line,
# wget's exit status, and whether wget tries again.
type Plan = {status: Int, line: Str, retry: Bool}

# The outcome of one URL: wget's exit status and what it downloaded.
type Retrieved = {status: Int, bytes: Int, millis: Int, saved: Bool}

# Where a download goes: a path to create, or standard output.
type Target = {path: Path?, label: Str, stdout: Bool}

# The first of NAME, NAME.1, NAME.2, ... that does not exist, as wget picks
# when it would otherwise overwrite; `reuse` keeps NAME itself.
proc unique_name(directory: Path?, base: Str, reuse: Bool) [fs] -> Path {
  let first = if let dir = directory { fp"{dir}/{base}" } else { fp"{base}" }
  if reuse or !(first.exists() ?? false) { return first }
  var suffix = 1
  while true {
    let candidate = if let dir = directory { fp"{dir}/{base}.{suffix}" } else { fp"{base}.{suffix}" }
    if !(candidate.exists() ?? false) { return candidate }
    suffix += 1
  }
  first
}

# One URL start to finish. Returns wget's exit status for it.
proc retrieve(options: Options, raw_url: Str) [net, fs, process, env, io, error, time] -> Retrieved {
  var url = raw_url
  let scheme = http_transfer.url_scheme(url)
  if scheme == null {
    url = f"http://{url}"
  } else if scheme not in ["http", "https"] {
    log(options, f"{raw_url}: Unsupported scheme '{scheme ?? ""}'.\n")
    return {status: EXIT_GENERIC, bytes: 0, millis: 0, saved: false}
  }
  let host = http_transfer.url_host(url)
  let port = http_transfer.url_port(url)

  var target: Target = {path: null, label: "", stdout: false}
  if let named = options.output {
    if named == "-" {
      target = {path: null, label: "STDOUT", stdout: true}
    } else {
      target = {path: fp"{named}", label: named, stdout: false}
    }
  } else if !options.spider {
    var base = http_transfer.url_file_name(url)
    if base == "" { base = "index.html" }
    var directory: Path? = null
    if let prefix = options.prefix { directory = fp"{prefix}" }
    let chosen = unique_name(directory, base, options.no_clobber or options.resume)
    if options.no_clobber and (chosen.exists() ?? false) {
      log(options, f"File '{chosen}' already there; not retrieving.\n\n")
      return {status: 0, bytes: 0, millis: 0, saved: false}
    }
    target = {path: chosen, label: f"{chosen}", stdout: false}
  }

  var resume_from = 0
  if options.resume {
    if let target_path = target.path {
      let known = target_path.metadata()
      match known {
        Ok(info) => resume_from = info.size
        Err(_) => resume_from = 0
      }
    }
  }

  var headers: List[Header] = []
  headers += [{name: "Host", value: http_transfer.url_authority(url)}]
  headers += [{name: "User-Agent", value: options.agent ?? f"Wget/{VERSION}"}]
  headers += [{name: "Accept", value: "*/*"}]
  headers += [{name: "Accept-Encoding", value: "identity"}]
  if let from = options.referer { headers += [{name: "Referer", value: from}] }
  if let name = options.user {
    headers += [http_transfer.basic_authorization(f"{name}:{options.password ?? ""}")]
  }
  if resume_from > 0 { headers += [{name: "Range", value: f"bytes={resume_from}-"}] }
  var body = b""
  var method = "GET"
  if let data = options.post_data {
    body = bytes.from_text(data)
    method = "POST"
    headers += [{name: "Content-Type", value: "application/x-www-form-urlencoded"}]
    headers += [{name: "Content-Length", value: f"{body.len()}"}]
  }
  for custom in options.headers {
    headers = http_transfer.without_header(headers, custom.name)
    headers += [custom]
  }
  if options.spider { method = "HEAD" }

  # Creating the destination up front reports an unwritable path before any
  # request is made, the way wget opens its -O file first.
  if let named = options.output {
    if named != "-" and resume_from == 0 {
      match fp"{named}".write(b"") {
        Ok(_) => {}
        Err(failure) => {
          log(options, f"{named}: {gnu.strerror(failure)}\n")
          return {status: EXIT_GENERIC, bytes: 0, millis: 0, saved: false}
        }
      }
    }
  } else if let directory = options.prefix {
    let _ = fp"{directory}".mkdir(parents: true)
  }

  var attempt = 1
  while true {
    let started = time.now()
    let tag = if attempt > 1 { f"(try:{attempt:>2})  " } else { "" }
    if options.spider and attempt == 1 {
      log_verbose(options, "Spider mode enabled. Check if remote file exists.\n")
    }
    log_verbose(options, f"--{stamp()}--  {tag}{url}\n")
    var announce = ""
    if looks_numeric(host) {
      announce = f"Connecting to {host}:{port}... "
    } else {
      match dns.resolve_host(host) {
        Ok(found) => {
          var addresses: List[Str] = []
          for entry in found { addresses += [entry.addr] }
          log_verbose(options, f"Resolving {host} ({host})... {addresses.join(", ")}\n")
          announce = f"Connecting to {host} ({host})|{addresses[0]}|:{port}... "
        }
        Err(_) => {
          log_verbose(options, f"Resolving {host} ({host})... failed: Name does not resolve.\n")
          log(options, f"wget: unable to resolve host address '{host}'\n")
          return {status: EXIT_NETWORK, bytes: 0, millis: 0, saved: false}
        }
      }
    }
    log_verbose(options, announce)
    let request: http_transfer.Request = {
      method: method,
      url: url,
      headers: headers,
      body: body,
      timeout: null,
      connect_timeout: options.timeout,
      idle_timeout: options.timeout,
      redirects: options.max_redirect,
      verify: options.verify,
      cacert: options.ca_certificate,
      fail_status: false,
    }
    let buffered = target.stdout or options.spider or resume_from > 0 or options.server_response
    let result = if buffered {
      http_transfer.fetch(request)
    } else {
      http_transfer.fetch_to(request, target.path ?? p"/dev/null")
    }
    let elapsed = time.now() - started
    match result {
      Err(failure) => {
        let plan = failure_plan(failure, host, options.max_redirect)
        log_verbose(options, plan.line)
        if !plan.retry or (options.tries != 0 and attempt >= options.tries) {
          if plan.retry { log_verbose(options, "Giving up.\n\n") }
          return {status: plan.status, bytes: 0, millis: 0, saved: false}
        }
        log_verbose(options, "Retrying.\n\n")
        let wait_for = if attempt > 10 { 10 } else { attempt }
        time.sleep(time.seconds(wait_for))?
        attempt += 1
        continue
      }
      Ok(reply) => {
        log_verbose(options, "connected.\n")
        if options.server_response {
          log_verbose(options, "HTTP request sent, awaiting response... \n")
          log_verbose(options, f"  HTTP/1.1 {reply.status} {reply.reason}\n")
          for header in reply.headers { log_verbose(options, f"  {header.name}: {header.value}\n") }
        } else {
          log_verbose(options, f"HTTP request sent, awaiting response... {reply.status} {reply.reason}\n")
        }
        if reply.status == 401 and options.user == null {
          log_verbose(options, "\nUsername/Password Authentication Failed.\n")
          return {status: EXIT_AUTH, bytes: 0, millis: 0, saved: false}
        }
        let ranged = resume_from > 0
        if ranged and reply.status == 416 {
          log_verbose(options, "\n    The file is already fully retrieved; nothing to do.\n\n")
          return {status: 0, bytes: 0, millis: 0, saved: false}
        }
        if reply.status >= 400 {
          if let target_path = target.path {
            if options.output == null { let _ = target_path.remove(missing_ok: true) }
          }
          if options.terse {
            log(options, f"{stamp()} ERROR {reply.status}: {reply.reason}.\n")
          } else {
            log(options, f"{stamp()} ERROR {reply.status}: {reply.reason}.\n\n")
          }
          return {status: EXIT_SERVER, bytes: 0, millis: 0, saved: false}
        }
        let length = content_length(reply.headers)
        let kind = http_transfer.header_value(reply.headers, "content-type")
        var total = length
        if ranged and reply.status == 206 {
          total = range_total(reply.headers)
          if total == null and length != null { total = (length ?? 0) + resume_from }
        }
        if options.spider {
          log_verbose(options, spider_summary(length, kind))
          return {status: 0, bytes: 0, millis: 0, saved: false}
        }
        var payload = reply.body
        let received = if buffered { payload.len() } else { reply.bytes }
        if target.stdout {
          let _ = io.write_stdout_bytes(payload)
          let _ = io.flush_stdout()
        } else if buffered {
          if let target_path = target.path {
            var written = Ok(0)
            if ranged and reply.status == 206 {
              written = bytes.write_at(target_path, resume_from, payload, true)
            } else {
              written = match target_path.write(payload) {
                Ok(_) => Ok(0)
                Err(failure) => Err(failure)
              }
            }
            if let Err(failure) = written {
              log(options, f"\nCannot write to '{target.label}' ({gnu.strerror(failure)}).\n")
              return {status: EXIT_IO, bytes: 0, millis: 0, saved: false}
            }
          }
        }
        log_verbose(options, length_line(total, kind, resume_from, ranged and reply.status == 206))
        log_verbose(options, f"Saving to: '{target.label}'\n\n")
        let shown_total = if ranged and reply.status == 206 { total } else { length }
        let counted = if ranged and reply.status == 206 { received + resume_from } else { received }
        log_verbose(options, progress_rows(shown_total, counted, received, if ranged and reply.status == 206 { resume_from } else { 0 }, elapsed))
        let rate = unit_rate(rate_of(received, elapsed))
        let verb = if target.stdout { "written to stdout" } else { f"'{target.label}' saved" }
        let sizes = if let size = shown_total { f"[{counted}/{size}]" } else { f"[{counted}]" }
        if options.terse {
          log(options, f"{stamp()} URL:{url} {sizes} -> \"{target.label}\" [1]\n")
        } else {
          log(options, f"\n{stamp()} ({rate}) - {verb} {sizes}\n\n")
        }
        return {status: 0, bytes: received, millis: elapsed, saved: true}
      }
    }
  }
  {status: 0, bytes: 0, millis: 0, saved: false}
}

pure looks_numeric(host: Str) -> Bool {
  host.find(":") != null or rx"^[0-9.]+$".matches(host)
}

pure content_length(headers: List[Header]) -> Int? {
  let text = http_transfer.header_value(headers, "content-length")
  if let value = text {
    let parsed = value.trim().parse_int() ?? -1
    if parsed >= 0 { return parsed }
  }
  null
}

# The complete size in `Content-Range: bytes FROM-TO/TOTAL`.
pure range_total(headers: List[Header]) -> Int? {
  let text = http_transfer.header_value(headers, "content-range")
  if let value = text {
    let slash = value.find("/")
    if slash != null {
      let parsed = value.byte_slice((slash ?? 0) + 1).trim().parse_int() ?? -1
      if parsed >= 0 { return parsed }
    }
  }
  null
}

pure sized(count: Int) -> Str {
  if count >= 1024 { f"{count} ({human_size(count)})" } else { f"{count}" }
}

pure length_line(total: Int?, kind: Str?, resume_from: Int, partial: Bool) -> Str {
  let label = if let type_name = kind { f" [{type_name}]" } else { "" }
  if let size = total {
    if partial {
      return f"Length: {sized(size)}, {sized(size - resume_from)} remaining{label}\n"
    }
    return f"Length: {sized(size)}{label}\n"
  }
  f"Length: unspecified{label}\n"
}

pure spider_summary(total: Int?, kind: Str?) -> Str {
  f"{length_line(total, kind, 0, false)}Remote file exists and could contain further links,\nbut recursion is disabled -- not retrieving.\n\n"
}

# What completes the connect and response line for each cause of failure.
pure failure_plan(failure: http_transfer.TransferFailure, host: Str, redirects: Int) -> Plan {
  match failure {
    http_transfer.TransferFailure.Dns => {status: EXIT_NETWORK, line: f"failed: Name does not resolve.\nwget: unable to resolve host address '{host}'\n", retry: false}
    http_transfer.TransferFailure.Connect => {status: EXIT_NETWORK, line: "failed: Connection refused.\n", retry: false}
    http_transfer.TransferFailure.ConnectTimeout => {status: EXIT_NETWORK, line: "failed: Operation timed out.\n", retry: true}
    http_transfer.TransferFailure.Timeout => {status: EXIT_NETWORK, line: "connected.\nHTTP request sent, awaiting response... Read error (Operation timed out) in headers.\n", retry: true}
    http_transfer.TransferFailure.EmptyReply => {status: EXIT_NETWORK, line: "connected.\nHTTP request sent, awaiting response... No data received.\n", retry: true}
    http_transfer.TransferFailure.Certificate => {
      status: EXIT_TLS,
      line: f"connected.\nERROR: cannot verify {host}'s certificate, issued by unknown authority:\n  Unable to locally verify the issuer's authority.\nTo connect to {host} insecurely, use `--no-check-certificate'.\n",
      retry: false,
    }
    http_transfer.TransferFailure.Handshake => {status: EXIT_TLS, line: "connected.\nUnable to establish SSL connection.\n", retry: false}
    http_transfer.TransferFailure.TrustStore => {status: EXIT_TLS, line: "failed: Cannot read the CA certificate file.\n", retry: false}
    http_transfer.TransferFailure.Redirects => {status: EXIT_SERVER, line: f"connected.\nHTTP request sent, awaiting response... \n{redirects} redirections exceeded.\n", retry: false}
    http_transfer.TransferFailure.Write {message} => {status: EXIT_IO, line: f"{message}\n", retry: false}
    http_transfer.TransferFailure.Scheme {message} => {status: EXIT_GENERIC, line: f"{message}\n", retry: false}
    http_transfer.TransferFailure.Status {code} => {status: EXIT_SERVER, line: f"ERROR {code}.\n", retry: false}
    http_transfer.TransferFailure.Other {message} => {status: EXIT_NETWORK, line: f"failed: {message}.\n", retry: false}
  }
}

proc main(...argv: List[Str]) [net, fs, process, env, io, error, time] {
  let options = parse(argv)
  if options.version {
    gnu.write_text(f"GNU Wget {VERSION} (XSH core, net module)\n")
    return
  }
  if options.help {
    gnu.write_text(HELP)
    return
  }
  var urls = options.urls
  if let list = options.input_file {
    let text = if list == "-" { io.stdin_text() } else { fp"{list}".read_text() }
    match text {
      Ok(content) => {
        for line in content.lines() {
          if line.trim() != "" { urls += [line.trim()] }
        }
      }
      Err(failure) => {
        log(options, f"{list}: {gnu.strerror(failure)}\n")
        exit EXIT_GENERIC
      }
    }
  }
  if urls.is_empty() { usage_failure("missing URL", EXIT_GENERIC) }
  let began = time.now()
  var status = 0
  var files = 0
  var total = 0
  var spent = 0
  for url in urls {
    let outcome = retrieve(options, url)
    if outcome.status != 0 and status == 0 { status = outcome.status }
    if outcome.saved {
      files += 1
      total += outcome.bytes
      spent += outcome.millis
    }
  }
  if urls.len() > 1 {
    let wall = time.now() - began
    log_verbose(options, f"FINISHED --{stamp()}--\nTotal wall clock time: {elapsed_text(wall)}\nDownloaded: {files} files, {human_size(total)} in {elapsed_text(spent)} ({unit_rate(rate_of(total, spent))})\n")
  }
  exit status
}
