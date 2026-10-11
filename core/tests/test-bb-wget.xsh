use support.uu

type HttpFixture = {base: Str, handle: ProcessHandle}

# A loopback response makes download destinations reproducible while retaining
# the explicit root path and empty path forms accepted by the HTTP client.
const HTTP_FIXTURE = r"""
use strict;
use warnings;
use IO::Socket::INET;
my ($port_path, $request_path) = @ARGV;
my $listener = IO::Socket::INET->new(LocalAddr => '127.0.0.1', LocalPort => 0,
    Listen => 1, Proto => 'tcp') or die "socket: $!";
open my $port, '>', "$port_path.ready" or die "port: $!";
print $port $listener->sockport;
close $port or die "port close: $!";
rename "$port_path.ready", $port_path or die "publish port: $!";
my $connection = $listener->accept or die "accept: $!";
my $request = <$connection>;
open my $record, '>', $request_path or die "request: $!";
print $record $request;
close $record or die "request close: $!";
while (my $header = <$connection>) { last if $header eq "\r\n"; }
my $body = "<html><body>local download fixture</body></html>\n";
print $connection "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: ",
    length($body), "\r\nConnection: close\r\n\r\n", $body;
close $connection;
"""

proc serve(s: uu.Scene) [fs, process, time, error] -> Result[HttpFixture, Error] {
  let script = uu.at(s, "http-fixture.pl")
  let port = uu.at(s, "port")
  let request = uu.at(s, "request")
  script.write(HTTP_FIXTURE)?
  let handle = spawn run perl $script $port $request ?
  let started = time.now()
  while !port.exists()? {
    if time.now() - started > 5000 {
      handle.cancel(signal: "KILL", kill_after: 0ms)?
      return Err(error.failure("HTTP fixture did not publish its port"))
    }
    time.sleep(20ms)?
  }
  Ok({base: f"http://127.0.0.1:{port.read_text()?.trim()}", handle: handle})
}

proc response_is(s: uu.Scene, destination: Str) [fs, error] {
  uu.file_is(s, destination, "<html><body>local download fixture</body></html>\n")
  uu.file_is(s, "request", "GET / HTTP/1.1\r\n")
}

# origin: busybox wget/wget--O-overrides--P
test test_bb_wget_wget_O_overrides_P_576a1b1a { |ctx|
  let s = uu.scene(ctx)?
  let fixture = serve(s)?
  defer fixture.handle.cancel(kill_after: 100ms)
  uu.mkdir(s, "foo")?
  let r = uu.invoke(s, "wget", ["-q", "-O", "index.html", "-P", "foo", f"{fixture.base}/"], timeout: 20s)?
  uu.succeeds(r)
  response_is(s, "index.html")
}

# origin: busybox wget/wget-handles-empty-path
test test_bb_wget_wget_handles_empty_path_6fd8b16f { |ctx|
  let s = uu.scene(ctx)?
  let fixture = serve(s)?
  defer fixture.handle.cancel(kill_after: 100ms)
  let r = uu.invoke(s, "wget", [fixture.base], timeout: 20s)?
  uu.succeeds(r)
  response_is(s, "index.html")
}

# origin: busybox wget/wget-retrieves-google-index
test test_bb_wget_wget_retrieves_google_index_1baa9083 { |ctx|
  let s = uu.scene(ctx)?
  let fixture = serve(s)?
  defer fixture.handle.cancel(kill_after: 100ms)
  let r = uu.invoke(s, "wget", ["-q", "-O", "foo", f"{fixture.base}/"], timeout: 20s)?
  uu.succeeds(r)
  response_is(s, "foo")
}

# origin: busybox wget/wget-supports--P
test test_bb_wget_wget_supports_P_5d6738e6 { |ctx|
  let s = uu.scene(ctx)?
  let fixture = serve(s)?
  defer fixture.handle.cancel(kill_after: 100ms)
  uu.mkdir(s, "foo")?
  let r = uu.invoke(s, "wget", ["-q", "-P", "foo", f"{fixture.base}/"], timeout: 20s)?
  uu.succeeds(r)
  response_is(s, "foo/index.html")
}
