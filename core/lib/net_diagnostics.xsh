##! Network diagnostic option parsing, query policy and factual presentation.
use gnu

error DiagnosticError = Usage : Usage | Network : Generic

type DnsOptions = {name: Str, record: Str, server: Str, port: Int?, timeout: Duration, attempts: Int, short: Bool, help: Bool, version: Bool}
type Answer = {name: Str, record: Str, value: Str, ttl: Int}

pure server_port(server: Str, port: Int?) -> Result[Str] {
  if port == null { return Ok(server) }
  let number = port
  if number < 1 or number > 65535 { return Err(DiagnosticError.Usage("port must be between 1 and 65535")) }
  if server.starts_with("[") {
    let close = server.find("]")
    if close == null { return Err(DiagnosticError.Usage("invalid IPv6 server address")) }
    return Ok(f"{server.byte_slice(0, length: close + 1)}:{number}")
  }
  let parts = server.split(":")
  if parts.len() > 2 { return Ok(f"[{server}]:{number}") }
  Ok(f"{parts[0]}:{number}")
}

pure record_name(raw: Str) -> Result[Str] {
  let kind = raw.upper()
  if kind not in ["A", "AAAA"] {
    return Err(DiagnosticError.Usage(f"unsupported query type '{raw}'"))
  }
  Ok(kind)
}

## Parse dig's query selection and the supported presentation modifiers.
export proc dig_options(argv: List[Str]) [error] -> Result[DnsOptions, Error] {
  var name = ""
  var record = "A"
  var server = ""
  var port: Int? = null
  var timeout = 5s
  var attempts = 3
  var short = false
  var help = false
  var version = false
  for token in cli.tokens(argv, ["p", "t"])? {
    if token.kind != "operand" {
      match token.name {
        "p" => port = token.value.parse_int()?
        "t" => record = record_name(token.value)?
        "h" | "help" => help = true
        "v" | "version" => version = true
        _ => return Err(DiagnosticError.Usage(f"unsupported option '{token.name}'"))
      }
      continue
    }
    let word = token.name
    if word.starts_with("@") {
      if word == "@" { return Err(DiagnosticError.Usage("missing nameserver after @")) }
      server = word.byte_slice(1)
    } else if word == "+short" { short = true } else if word == "+noshort" { short = false } else if word.starts_with("+time=") { timeout = time.millis(word.byte_slice(6).parse_int()? * 1000) } else if word.starts_with("+tries=") { attempts = word.byte_slice(7).parse_int()? } else if word.starts_with("+") { return Err(DiagnosticError.Usage(f"unsupported modifier '{word}'")) } else if word.upper() == "IN" { continue } else if word.upper() in ["A", "AAAA"] { record = record_name(word)? } else if name == "" { name = word } else { return Err(DiagnosticError.Usage("only one question is supported per invocation")) }
  }
  if name == "" and ! help and ! version { return Err(DiagnosticError.Usage("expected NAME [A|AAAA]")) }
  if timeout <= 0ms or attempts < 1 { return Err(DiagnosticError.Usage("timeout and tries must be positive")) }
  Ok({name: name, record: record, server: server, port: port, timeout: timeout, attempts: attempts, short: short, help: help, version: version})
}

## Parse nslookup's noninteractive query, server, port and timeout options.
export proc nslookup_options(argv: List[Str]) [error] -> Result[DnsOptions, Error] {
  var name = ""
  var record = "A"
  var server = ""
  var port: Int? = null
  var timeout = 5s
  var attempts = 3
  var help = false
  var version = false
  for word in argv {
    if word.starts_with("-type=") { record = record_name(word.byte_slice(6))? } else if word.starts_with("-query=") { record = record_name(word.byte_slice(7))? } else if word.starts_with("-port=") { port = word.byte_slice(6).parse_int()? } else if word.starts_with("-timeout=") { timeout = time.millis(word.byte_slice(9).parse_int()? * 1000) } else if word.starts_with("-retry=") { attempts = word.byte_slice(7).parse_int()? } else if word in ["-h", "--help"] { help = true } else if word in ["-version", "--version"] { version = true } else if word.starts_with("-") { return Err(DiagnosticError.Usage(f"unsupported option '{word}'")) } else if name == "" { name = word } else if server == "" { server = word } else { return Err(DiagnosticError.Usage("expected NAME [SERVER]")) }
  }
  if name == "" and ! help and ! version { return Err(DiagnosticError.Usage("expected NAME [SERVER]")) }
  if timeout <= 0ms or attempts < 1 { return Err(DiagnosticError.Usage("timeout and retry must be positive")) }
  Ok({name: name, record: record, server: server, port: port, timeout: timeout, attempts: attempts, short: false, help: help, version: version})
}

## Format only records actually returned by the typed DNS transport.
export pure answer_text(answer: Answer, short: Bool) -> Str {
  if short { return answer.value + "\n" }
  let name = if answer.name.ends_with(".") { answer.name } else { answer.name + "." }
  f"{name}\t{answer.ttl}\tIN\t{answer.record}\t{answer.value}\n"
}

proc query(opts: DnsOptions) [net, error] -> Result[List[Answer]] {
  var server = opts.server
  if server == "" {
    let configured = dns.nameservers()?
    if configured.is_empty() { return Err(DiagnosticError.Network("no nameserver is configured")) }
    server = configured[0]
  }
  server = server_port(server, opts.port)?
  var result = dns.lookup(opts.name, opts.record, server, opts.timeout)
  var attempt = 1
  while result is Err(_) and attempt < opts.attempts {
    result = dns.lookup(opts.name, opts.record, server, opts.timeout)
    attempt += 1
  }
  result
}

## Execute dig using DNS answers, without fabricating protocol metadata.
export proc dig(argv: List[Str]) [net, process, env, io, error] -> Unit {
  guard let opts = dig_options(argv) else { |failure| gnu.usage_error(failure.message, 1); return }
  if opts.help { gnu.help("Usage: dig [@SERVER] [-p PORT] [-t A|AAAA] [+short] [+time=N] [+tries=N] NAME [TYPE]"); return }
  if opts.version { gnu.version("dig"); return }
  match query(opts) {
    Ok(answers) => {
      if ! opts.short { gnu.write_text(f";; QUESTION SECTION:\n;{opts.name}\tIN\t{opts.record}\n\n;; ANSWER SECTION:\n") }
      for answer in answers { gnu.write_text(answer_text(answer, opts.short)) }
    }
    Err(failure) => { gnu.error(failure.message); exit 9 }
  }
}

## Execute nslookup's noninteractive query using the same typed DNS engine.
export proc nslookup(argv: List[Str]) [net, process, env, io, error] -> Unit {
  guard let opts = nslookup_options(argv) else { |failure| gnu.usage_error(failure.message, 1); return }
  if opts.help { gnu.help("Usage: nslookup [-type=TYPE] [-port=PORT] [-timeout=SECONDS] NAME [SERVER]"); return }
  if opts.version { gnu.version("nslookup"); return }
  match query(opts) {
    Ok(answers) => {
      gnu.write_text(f"Name:\t{opts.name}\n")
      for answer in answers { gnu.write_text(if answer.record in ["A", "AAAA"] { f"Address: {answer.value}\n" } else { answer_text(answer, false) }) }
    }
    Err(failure) => { gnu.error(failure.message); exit 1 }
  }
}
