##! HTTP transfer layer shared by the curl and wget applets.
##!
##! One request description, one executor over the net module, and one
##! classification of its failures, so both applets agree on what a refused
##! connection, a timeout or a certificate problem is and differ only in how
##! they word it. Classification reads the net module's `NetError` variant; the
##! message only carries wording.

## Why a transfer failed, independent of any applet's wording.
export error TransferFailure {
  Dns
  Connect
  ConnectTimeout
  Timeout
  Certificate
  Handshake
  EmptyReply
  Redirects
  Status(code: Int)
  Scheme
  Write
  TrustStore
  Other
}

## One HTTP header field.
export type Header = {name: Str, value: Str}

## A request the applets build from their options. `redirects` is the number of
## redirects the engine may follow; `return_redirects` hands a redirect it will
## not follow back as the reply instead of failing, so a caller can follow hop
## by hop with `redirect_request`. `method` must be one the net module sends
## (GET, HEAD, POST, PUT, PATCH, DELETE).
export type Request = {
  method: Str,
  url: Str,
  headers: List[Header],
  body: Bytes,
  timeout: Duration?,
  connect_timeout: Duration?,
  idle_timeout: Duration?,
  redirects: Int,
  return_redirects: Bool,
  verify: Bool,
  cacert: Path?,
  fail_status: Bool,
}

## The final response of a transfer. `body` is empty for a download to a file.
## `url` is the URL the response came from after redirects and `redirects` how
## many the engine followed to reach it.
export type Reply = {status: Int, reason: Str, headers: List[Header], bytes: Int, body: Bytes, url: Str, redirects: Int}

# Responses are held in memory when they are not downloaded to a file, so the
# cap only has to exceed anything a script would pipe through stdout.
const BUFFERED_LIMIT = 1099511627776

# The net record cannot carry a null duration and a record cannot be built a
# field at a time, so an unset limit is a limit no transfer reaches.
const NO_LIMIT = 8760h

## Map a net failure to its cause from its `NetError` variant. The message only
## carries the wording of the failure.
export pure classify(failure: Error) -> TransferFailure {
  let message = failure.message
  match failure {
    NetError.Status {status} => TransferFailure.Status(code: status ?? 0)
    NetError.Dns => TransferFailure.Dns(message)
    NetError.ConnectTimeout => TransferFailure.ConnectTimeout(message)
    NetError.Timeout => TransferFailure.Timeout(message)
    NetError.Connect => TransferFailure.Connect(message)
    NetError.TrustStore => TransferFailure.TrustStore(message)
    NetError.Certificate => TransferFailure.Certificate(message)
    NetError.Tls => TransferFailure.Handshake(message)
    NetError.EmptyReply => TransferFailure.EmptyReply(message)
    NetError.Redirect => TransferFailure.Redirects(message)
    NetError.Unsupported => TransferFailure.Scheme(message)
    NetError.Write => TransferFailure.Write(message)
    else => TransferFailure.Other(message)
  }
}

pure redirect_limit(request: Request) -> Str {
  if request.return_redirects { "return" } else { "error" }
}

pure header_records(headers: List[Header]) -> List[Record] {
  [{name: header.name, value: header.value} for header in headers]
}

pure reply_from(raw: Record, body: Bytes) -> Reply {
  {
    status: raw.status.require(Int) ?? 0,
    reason: raw.reason.require(Str) ?? "",
    url: raw.effective_url.require(Str) ?? "",
    redirects: raw.redirect_count.require(Int) ?? 0,
    headers: [
      {name: header.name.require(Str) ?? "", value: header.value.require(Str) ?? ""}
      for header in raw.headers.require(List[Record]) ?? []
    ],
    bytes: raw.bytes.require(Int) ?? 0,
    body: body,
  }
}

proc buffered(request: Request) [net, error] -> Result[Record, Error] {
  let headers = header_records(request.headers)
  let total = request.timeout ?? NO_LIMIT
  let connect = request.connect_timeout ?? NO_LIMIT
  let idle = request.idle_timeout ?? NO_LIMIT
  if let anchor = request.cacert {
    return net.request({
      method: request.method, url: request.url, headers: headers, body: request.body,
      timeout: total, connect_timeout: connect, dns_timeout: connect,
      headers_timeout: idle, body_idle_timeout: idle,
      redirects: request.redirects, redirect_limit: redirect_limit(request), fail_status: request.fail_status,
      tls_verify: request.verify, ca_certificate: anchor, max_body_bytes: BUFFERED_LIMIT,
    })
  }
  net.request({
    method: request.method, url: request.url, headers: headers, body: request.body,
    timeout: total, connect_timeout: connect, dns_timeout: connect,
      headers_timeout: idle, body_idle_timeout: idle,
    redirects: request.redirects, redirect_limit: redirect_limit(request), fail_status: request.fail_status,
    tls_verify: request.verify, max_body_bytes: BUFFERED_LIMIT,
  })
}

proc to_file(request: Request, dest: Path) [net, error] -> Result[Record, Error] {
  let headers = header_records(request.headers)
  let total = request.timeout ?? NO_LIMIT
  let connect = request.connect_timeout ?? NO_LIMIT
  let idle = request.idle_timeout ?? NO_LIMIT
  if let anchor = request.cacert {
    return net.download({
      url: request.url, headers: headers, dest: dest, overwrite: true, atomic: false,
      timeout: total, connect_timeout: connect, dns_timeout: connect,
      headers_timeout: idle, body_idle_timeout: idle,
      redirects: request.redirects, redirect_limit: redirect_limit(request), fail_status: request.fail_status,
      tls_verify: request.verify, ca_certificate: anchor,
    })
  }
  net.download({
    url: request.url, headers: headers, dest: dest, overwrite: true, atomic: false,
    timeout: total, connect_timeout: connect, dns_timeout: connect,
      headers_timeout: idle, body_idle_timeout: idle,
    redirects: request.redirects, redirect_limit: redirect_limit(request), fail_status: request.fail_status,
    tls_verify: request.verify,
  })
}

## Run a request and buffer the response body.
export proc fetch(request: Request) [net, error] -> Result[Reply, TransferFailure] {
  match buffered(request) {
    Ok(raw) => Ok(reply_from(raw, raw.body.require(Bytes) ?? b""))
    Err(failure) => Err(classify(failure))
  }
}

## Run a request and stream the response body into `dest`, replacing it. The
## engine writes as chunks arrive and removes a partial file on failure.
export proc fetch_to(request: Request, dest: Path) [net, error] -> Result[Reply, TransferFailure] {
  match to_file(request, dest) {
    Ok(raw) => Ok(reply_from(raw, b""))
    Err(failure) => Err(classify(failure))
  }
}

## The value of a response header, case-insensitively, or null.
export pure header_value(headers: List[Header], name: Str) -> Str? {
  let wanted = name.lower()
  for header in headers {
    if header.name.lower() == wanted { return header.value }
  }
  null
}

## Whether a header list sets `name`, case-insensitively.
export pure has_header(headers: List[Header], name: Str) -> Bool {
  header_value(headers, name) != null
}

## `headers` with every header called `name` removed.
export pure without_header(headers: List[Header], name: Str) -> List[Header] {
  let wanted = name.lower()
  [header for header in headers if header.name.lower() != wanted]
}

## The scheme of a URL, lowercased, or null when it has none.
export pure url_scheme(url: Str) -> Str? {
  let at = url.find("://")
  if at == null { return null }
  let scheme = url.byte_slice(0, at ?? 0)
  return null unless rx"^[A-Za-z][A-Za-z0-9+.-]*$".matches(scheme)

  scheme.lower()
}

# Everything between the scheme separator and the first path, query or
# fragment delimiter: userinfo, host and port.
pure authority(url: Str) -> Str {
  let at = url.find("://")
  if at == null { return "" }
  let rest = url.byte_slice((at ?? 0) + 3)
  var end = rest.byte_len()
  for mark in ["/", "?", "#"] {
    let found = rest.find(mark)
    if found != null and (found ?? end) < end { end = found ?? end }
  }
  rest.byte_slice(0, end)
}

## The authority of a URL as sent in a Host header: host and port exactly as
## written, without userinfo.
export pure url_authority(url: Str) -> Str {
  let full = authority(url)
  let at = full.find("@")
  if at == null { full } else { full.byte_slice((at ?? 0) + 1) }
}

## The host of a URL without userinfo or port; bracketed IPv6 literals keep
## their brackets stripped.
export pure url_host(url: Str) -> Str {
  var host = authority(url)
  let at = host.find("@")
  if at != null { host = host.byte_slice((at ?? 0) + 1) }
  if host.starts_with("[") {
    let close = host.find("]")
    return host.byte_slice(1, (close ?? host.byte_len()) - 1)
  }
  let colon = host.find(":")
  if colon != null { host = host.byte_slice(0, colon ?? 0) }
  host
}

## The port of a URL, or the scheme's default.
export pure url_port(url: Str) -> Int {
  let host = authority(url)
  var tail = host
  let at = tail.find("@")
  if at != null { tail = tail.byte_slice((at ?? 0) + 1) }
  let close = tail.find("]")
  if close != null { tail = tail.byte_slice((close ?? 0) + 1) }
  let colon = tail.find(":")
  if colon != null {
    let parsed = tail.byte_slice((colon ?? 0) + 1).parse_int() ?? -1
    if parsed >= 0 { return parsed }
  }
  if url_scheme(url) == "https" { 443 } else { 80 }
}

## The path component of a URL, always beginning with `/`.
export pure url_path(url: Str) -> Str {
  let at = url.find("://")
  if at == null { return "/" }
  let rest = url.byte_slice((at ?? 0) + 3)
  let slash = rest.find("/")
  var full = if slash == null { "/" } else { rest.byte_slice(slash ?? 0) }
  for mark in ["?", "#"] {
    let found = full.find(mark)
    if found != null { full = full.byte_slice(0, found ?? 0) }
  }
  full
}

## The final path segment of a URL: the name `curl -O` and `wget` save under.
## It is empty for a URL that ends in `/` or has no path.
export pure url_file_name(url: Str) -> Str {
  let parts = url_path(url).split("/")
  parts[-1]
}

# The printable ASCII characters in code order, to turn a byte back to text.
const PRINTABLE = r""" !"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\]^_`abcdefghijklmnopqrstuvwxyz{|}~"""

## Percent-encode bytes for a URL query or form value: unreserved bytes pass
## through, everything else becomes `%XX`, and a space is `+` when `plus_space`.
export pure percent_encode(data: Bytes, plus_space: Bool) -> Str {
  var out = ""
  let digits = "0123456789ABCDEF"
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    let plain = (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) or byte == 45 or byte == 46 or byte == 95 or byte == 126
    if plain {
      out = f"{out}{PRINTABLE.byte_slice(byte - 32, 1)}"
    } else if byte == 32 and plus_space {
      out = f"{out}+"
    } else {
      out = f"{out}%{digits.byte_slice(byte / 16, 1)}{digits.byte_slice(byte % 16, 1)}"
    }
  }
  out
}

## `Authorization: Basic ...` for `user:password`.
export pure basic_authorization(credentials: Str) -> Header {
  {name: "Authorization", value: f"Basic {bytes.from_text(credentials).base64()}"}
}

## Format a duration in whole or fractional seconds as `S.mmm` text.
export pure seconds_text(millis: Int) -> Str {
  let whole = millis / 1000
  let fraction = millis % 1000
  let pad = if fraction < 10 { "00" } else if fraction < 100 { "0" } else { "" }
  f"{whole}.{pad}{fraction}"
}

## The absolute URL a `Location` value names relative to the URL it answered.
export pure resolve_location(base: Str, location: Str) -> Str {
  if url_scheme(location) != null { return location }

  let scheme = url_scheme(base) ?? "http"
  if location.starts_with("//") { return f"{scheme}:{location}" }
  if location.starts_with("/") { return f"{scheme}://{authority(base)}{location}" }

  let route = url_path(base)
  let directory = route.byte_slice(0, route.byte_len() - url_file_name(base).byte_len())
  f"{scheme}://{authority(base)}{directory}{location}"
}

# Whether two URLs name the same scheme, host and port.
pure same_origin(left: Str, right: Str) -> Bool {
  url_scheme(left) == url_scheme(right) and url_host(left) == url_host(right) and url_port(left) == url_port(right)
}

## The request that follows a redirect `reply` to `request`, or null when the
## reply is not a redirect that names a `Location`. It applies the net module's
## own redirect policy for callers that follow hop by hop: a 301 or 302 answer
## to POST and a 303 answer to anything but GET and HEAD are repeated as GET
## without a body, 307 and 308 repeat the method and body, and credentials and
## an explicit Host header are dropped when the target is another origin.
export pure redirect_request(request: Request, reply: Reply) -> Request? {
  return null unless reply.status in [301, 302, 303, 307, 308]

  let location = header_value(reply.headers, "location")
  return null when location == null

  let target = resolve_location(request.url, location ?? "")
  let rewrite = (reply.status in [301, 302] and request.method == "POST") or (reply.status == 303 and request.method not in ["GET", "HEAD"])
  var headers = request.headers
  var method = request.method
  var body = request.body
  if rewrite {
    method = "GET"
    body = b""
    for name in ["content-length", "content-type", "content-encoding", "transfer-encoding"] {
      headers = without_header(headers, name)
    }
  }
  if !same_origin(request.url, target) {
    for name in ["host", "authorization", "proxy-authorization", "cookie"] {
      headers = without_header(headers, name)
    }
  }
  {...request, method: method, url: target, headers: headers, body: body}
}
