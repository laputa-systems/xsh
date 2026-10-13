stream delayed_clock() -> Stream[Int] {
  yield time.now()
}

stream delayed_file_marker(marker: Path) [fs, error] -> Stream[Int] {
  marker.write("pulled")?
  yield 7
}

stream delayed_process_marker(marker: Path) [process, error] -> Stream[Int] {
  run sh -c "echo $$ > $1" sh $marker
  yield 8
}

stream delayed_http_status() [net, error] -> Stream[Int] {
  let response = net.request({
    method: "GET",
    url: "https://example.test/lexical-effect-bounds",
  })?
  yield response.status
}

test test_effectful_stream_creation_is_rejected_inside_without { |ctx|
  test.expect(
    ctx,
    r"""
stream delayed_clock() -> Stream[Int] { yield time.now() }
without time { let pending = delayed_clock() }
""",
    status: 2,
    stderr: ["check.effect-violation", "without time"],
  )?
}

test test_effectful_callbacks_are_rejected_inside_without { |ctx|
  test.expect(
    ctx,
    r"""
proc main() [time] {
  without time {
    let values = [1] |> map { |value| time.now() }
  }
}
""",
    status: 2,
    stderr: ["check.effect-violation", "without time"],
  )?
}

test test_filesystem_effects_of_an_external_stream_are_charged_at_creation { |ctx|
  let marker = test.temp_path(ctx, name: "lexical-stream-fs")
  let rows = delayed_file_marker(marker)
  assert ! marker.exists()?, "calling a producer must not run its body"

  var values: List[Int] = []
  without fs {
    values = rows |> collect()
  }
  assert values == [7]
  assert marker.read_text()? == "pulled"
}

test test_process_effects_of_an_external_stream_are_charged_at_creation { |ctx|
  let marker = test.temp_path(ctx, name: "lexical-stream-process")
  let rows = delayed_process_marker(marker)
  assert ! marker.exists()?, "calling a producer must not run its body"

  var values: List[Int] = []
  without process {
    values = rows |> collect()
  }
  assert values == [8]
  assert marker.exists()?, "the delayed command must run when the stream is pulled"
}

test test_network_effects_of_an_external_stream_are_charged_at_creation { |ctx|
  let url = "https://example.test/lexical-effect-bounds"
  let response = {
    status: 200,
    reason: "OK",
    bytes: 0,
    headers: [],
    url: url,
    effective_url: url,
    redirect_count: 0,
    body: b"",
  }
  test.mock(ctx, "net.request", {url: url}, Ok(response))

  let rows = delayed_http_status()
  assert test.calls(ctx, "net.request").len() == 0

  var values: List[Int] = []
  without net {
    values = rows |> collect()
  }
  assert values == [200]
  assert test.calls(ctx, "net.request").len() == 1
}
