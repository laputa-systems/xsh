test test_net_module_with_mocks { |ctx|
  let response = {
    status: 200,
    reason: "OK",
    bytes: 2,
    headers: [
      {
        name: "content-type",
        value: "text/plain",
      },
    ],
    url: "https://example.test/",
    body: b"ok",
  }

  test.mock(ctx, "net.request", {url: "https://example.test/"}, Ok(response))

  test.mock(
    ctx,
    "net.request_many",
    {pool: "stdlib-test"},
    Ok([Ok(response)]),
  )

  test.mock(
    ctx,
    "net.download",
    {url: "https://example.test/file"},
    Ok({
      status: 200,
      reason: "OK",
      bytes: 2,
      headers: [{name: "content-type", value: "text/plain"}],
      url: "https://example.test/file",
    }),
  )

  test.mock(
    ctx,
    "net.download_many",
    {pool: "stdlib-test"},
    Ok(
      [
        Ok({
          status: 200,
          reason: "OK",
          bytes: 2,
          headers: [{name: "content-type", value: "text/plain"}],
          url: "https://example.test/file",
        }),
      ],
    ),
  )

  test.mock(
    ctx,
    "net.upload",
    {url: "https://example.test/upload"},
    Ok({
      status: 200,
      reason: "OK",
      bytes: 2,
      headers: [{name: "content-type", value: "text/plain"}],
      url: "https://example.test/upload",
    }),
  )

  assert net.request({method: "GET", url: "https://example.test/"})?.body == b"ok"
  let many = net.request_many({
    requests: [{method: "GET", url: "https://example.test/"}],
    pool: "stdlib-test",
  })?
  assert many[0]?.body == b"ok"
  assert net.download({method: "GET", url: "https://example.test/file", dest: p"out"})?.status == 200
  let downloaded = net.download_many({
    downloads: [{url: "https://example.test/file", dest: p"out"}],
    pool: "stdlib-test",
  })?
  assert downloaded[0]?.bytes == 2
  assert net.upload({method: "PUT", url: "https://example.test/upload", source: p"in"})?.bytes == 2
  let pool = net.pool(name: "stdlib-test", max_idle_per_host: 1, idle_timeout: 1s)?
  assert pool.name == "stdlib-test"
  assert pool.max_idle_per_host == 1
  net.close_pool("stdlib-test")
  net.close_all_pools()
  assert test.calls(ctx, "net.request")[0].args.method == "GET"
  assert test.calls(ctx, "net.request_many")[0].args.requests[0].method == "GET"
  assert test.calls(ctx, "net.download")[0].args.dest.require(Path)?.display() == "out"
  assert test.calls(ctx, "net.download_many")[0].args.downloads[0].url == "https://example.test/file"
  assert test.calls(ctx, "net.upload")[0].args.source.require(Path)?.display() == "in"
}

proc net_start_scoped_helper() [net] -> Result[NetJob] {
  net.start({
    method: "GET",
    url: "https://example.test/returned-job",
    max_body_bytes: 1,
  })
}

test test_net_start_mock_job_is_single_consumption { |ctx|
  let response = {
    status: 200,
    reason: "OK",
    bytes: 2,
    headers: [],
    url: "https://example.test/",
    body: b"ok",
  }
  test.mock(ctx, "net.start", {url: "https://example.test/"}, Ok(response))

  let job = net.start({method: "GET", url: "https://example.test/"})?
  assert job.wait()?.body == b"ok"
  test.error_kind(job.cancel(), "net-job-not-live")
}

test test_net_job_progresses_while_synchronous_request_waits {
  let url = env.get_or("XSH_NET_TEST_CONCURRENT_URL", "")?
  if url == "" {
    test.skip("requires concurrent NetJob transport fixture")
    return
  }

  # The fixture holds the synchronous response until the driver's job response
  # has been released. `net.start` therefore must keep progressing while this
  # evaluator is checkpoint-waiting in `net.request`.
  let job = net.start({
    method: "GET",
    url: f"{url}/job",
    headers: [{name: "Connection", value: "close"}],
  })?
  let foreground = net.request({
    method: "GET",
    url: f"{url}/sync",
    headers: [{name: "Connection", value: "close"}],
  })?

  assert foreground.body.utf8()? == "sync"
  assert job.wait()?.body.utf8()? == "job"
}

test test_net_start_transfers_returned_job_ownership { |ctx|
  let response = {
    status: 200,
    reason: "OK",
    bytes: 2,
    headers: [],
    url: "https://example.test/returned-job",
    body: b"ok",
  }
  test.mock(ctx, "net.start", {url: "https://example.test/returned-job"}, Ok(response))

  let job = net_start_scoped_helper()?
  assert job.wait()?.body == b"ok"
}

test test_net_start_aliases_share_one_consumption { |ctx|
  let response = {
    status: 204,
    reason: "No Content",
    bytes: 0,
    headers: [],
    url: "https://example.test/alias",
    body: b"",
  }
  test.mock(ctx, "net.start", {url: "https://example.test/alias"}, Ok(response))

  let job = net.start({method: "GET", url: "https://example.test/alias"})?
  let alias = job
  alias.cancel()
  test.error_kind(job.wait(), "net-job-not-live")
  assert test.calls(ctx, "net.start")[0].args.method == "GET"
}

test test_net_start_enforces_live_job_capacity { |ctx|
  let response = {
    status: 204,
    reason: "No Content",
    bytes: 0,
    headers: [],
    url: "https://example.test/capacity",
    body: b"",
  }
  test.mock(ctx, "net.start", {url: "https://example.test/capacity"}, Ok(response), 66)

  test.error_kind(
    net.start({
      method: "GET",
      url: "https://example.test/capacity",
      max_body_bytes: 67108865,
    }),
    "net-overload",
  )

  var jobs = [
    net.start({
      method: "GET",
      url: "https://example.test/capacity",
      max_body_bytes: 1,
    })?
    for _ in range(64)
  ]
  test.error_kind(
    net.start({method: "GET", url: "https://example.test/capacity", max_body_bytes: 1}),
    "net-overload",
  )
  for job in jobs {
    job.cancel()
  }
}

test test_net_start_scope_cleanup_releases_admission { |ctx|
  let response = {
    status: 204,
    reason: "No Content",
    bytes: 0,
    headers: [],
    url: "https://example.test/cleanup",
    body: b"",
  }
  test.mock(ctx, "net.start", {url: "https://example.test/cleanup"}, Ok(response), 65)

  let should_cleanup = ctx.keys().len() > 0
  if should_cleanup {
    let jobs = [
      net.start({
        method: "GET",
        url: "https://example.test/cleanup",
        max_body_bytes: 1,
      })?
      for _ in range(64)
    ]
    assert jobs.len() == 64
  }

  let released = net.start({
    method: "GET",
    url: "https://example.test/cleanup",
    max_body_bytes: 1,
  })?
  released.cancel()
}

test test_net_start_loop_control_cleans_lexical_job_scopes { |ctx|
  let response = {
    status: 204,
    reason: "No Content",
    bytes: 0,
    headers: [],
    url: "https://example.test/loop-cleanup",
    body: b"",
  }
  test.mock(
    ctx,
    "net.start",
    {url: "https://example.test/loop-cleanup"},
    Ok(response),
    66,
  )

  # Both control paths discard a statement block. Its unconsumed job must be
  # canceled before the next iteration, rather than staying live until this
  # function returns and exhausting the evaluator's job capacity.
  for _ in [0] {
    let _ = net.start({
      method: "GET",
      url: "https://example.test/loop-cleanup",
      max_body_bytes: 1,
    })?
    break
  }

  repeat 64 times {
    let _ = net.start({
      method: "GET",
      url: "https://example.test/loop-cleanup",
      max_body_bytes: 1,
    })?
    continue
  }

  let final_job = net.start({
    method: "GET",
    url: "https://example.test/loop-cleanup",
    max_body_bytes: 1,
  })?
  assert final_job.wait()?.status == 204
}

test test_net_job_trace_is_correlated_and_redacts_request_secrets { |ctx|
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
  } else {
    let source = """\nlet url = env.get_or("XSH_NET_TEST_URL", "")?
let job = net.start({
  method: "POST",
  url: url + "/echo",
  headers: [{name: "authorization", value: "Bearer trace-secret"}],
  body: b"trace-secret",
})?
let response = job.wait()?
print \${response.status}
"""
    let trace = test.run_xsht_trace(ctx, source, ["--trace", "--raw", "--trace-format", "jsonl"])?
    {
      let {success: assertion_condition, stderr: assertion_message, ..} = trace
      assert assertion_condition, assertion_message
    }
    assert trace.stdout == """200
"""
    assert "\"kind\":\"net.job.accepted\"" in trace.stderr
    assert "\"kind\":\"net.job.scheduled\"" in trace.stderr
    assert "\"kind\":\"net.transport.started\"" in trace.stderr
    assert "\"kind\":\"net.transport.completed\"" in trace.stderr
    assert "\"kind\":\"net.job.wait\"" in trace.stderr
    assert "\"api_id\":\"module.net.start\"" in trace.stderr
    assert "\"api_id\":\"method.NetJob.wait\"" in trace.stderr
    assert "\"job_id\":1" in trace.stderr
    assert "\"queue_duration_us\":" in trace.stderr
    assert "\"transport_duration_us\":" in trace.stderr
    {
      let assertion_condition = "trace-secret" not in trace.stderr
      let assertion_message = trace.stderr
      assert assertion_condition, assertion_message
    }
  }
}

test test_net_runtime_descriptors_do_not_survive_exec { |ctx|
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  let helper = env.get_or("XSH_NET_FD_HELPER", "")?
  if url == "" or helper == "" {
    test.skip("requires local network and descriptor fixtures")
    return
  }

  let root = test.temp_dir(ctx, name: "net-runtime-fds")?
  let output = fp"{root}/fds.txt"
  run ${helper} > output ?
  let inherited = output.read_text()?
  let job = net.start({method: "GET", url: url + "/hello"})?
  run ${helper} > output ?
  # Wait instead of cancelling: the harness asserts the fixture server saw the
  # request, and a cancel can win before the request is sent.
  assert job.wait()?.status == 200
  assert output.read_text()? == inherited
}

test test_net_transport_http_contracts { |ctx|
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
    return
  }

  let root = test.temp_dir(ctx, name: "net-http")?
  let upload_source = fp"{root}/upload.txt"
  let download_dest = fp"{root}/download.txt"
  let missing_ca = fp"{root}/missing-ca.pem"
  upload_source.write("upload-body")

  let pool = net.pool("fixture", 4, 1s)?
  let first = net.request({method: "GET", url: f"{url}/hello", pool: "fixture"})?
  let second = net.request({method: "GET", url: f"{url}/hello", pool: "fixture"})?
  let headed = net.request({method: "HEAD", url: f"{url}/hello", pool: "fixture"})?
  let redirected = net.request({method: "GET", url: f"{url}/redirect", redirects: 1, pool: "fixture"})?
  let posted = net.request({
    method: "POST",
    url: f"{url}/echo",
    headers: [{name: "X-Test", value: "one"}],
    body_text: "payload",
    pool: "fixture",
  })?
  let posted_file = net.request({
    method: "POST",
    url: f"{url}/echo",
    body_file: upload_source,
    pool: "fixture",
  })?
  let posted_bytes = net.request({
    method: "POST",
    url: f"{url}/echo",
    body: b"bytes",
    pool: "fixture",
  })?
  let status = net.request({method: "GET", url: f"{url}/status", pool: "fixture"})?
  let downloaded = net.download({
    url: f"{url}/header-file",
    dest: download_dest,
    headers: [{name: "X-Download", value: "yes"}],
    overwrite: true,
    pool: "fixture",
  })?
  let uploaded = net.upload({
    url: f"{url}/upload",
    source: upload_source,
    headers: [{name: "Authorization", value: "Bearer secret-token"}],
    pool: "fixture",
  })?

  assert pool.max_idle_per_host == 4
  assert pool.idle_timeout_ms == 1000
  assert first.reason == "OK"
  assert first.url == f"{url}/hello"
  assert first.body.utf8()? == "hello"
  assert second.body.utf8()? == "hello"
  assert headed.status == 200
  assert headed.bytes == 0
  assert redirected.body.utf8()? == "hello"
  assert posted.body.utf8()? == "echo:payload"
  assert posted_file.body.utf8()? == "echo:upload-body"
  assert posted_bytes.body.utf8()? == "echo:bytes"
  assert status.status == 404
  assert downloaded.status == 200
  assert downloaded.bytes == 11
  assert download_dest.read_text()? == """downloaded
"""
  assert uploaded.status == 201
  assert uploaded.reason == "Created"
  assert uploaded.bytes == 20
  assert uploaded.url == f"{url}/upload"
  assert first.headers[0].name == "Date"
  assert first.headers[1].value == "5"
  test.error_kind(net.request({method: "GET", url: "ftp://example.invalid/file"}), "net-scheme")
  test.error_kind(
    net.request({method: "GET", url: f"{url}/hello", ca_certificate: missing_ca}),
    "net-ca-certificate",
  )
  net.close_pool("fixture")
  net.close_all_pools()
}

test test_net_transport_error_contracts { |ctx|
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
    return
  }

  let root = test.temp_dir(ctx, name: "net-errors")?
  let existing = fp"{root}/existing.txt"
  let limited = fp"{root}/limited.txt"
  let in_place = fp"{root}/in-place.txt"
  existing.write("previous")
  limited.write("limited before")
  in_place.write("old")

  let redirect_limit = net.request({method: "GET", url: f"{url}/redirect", redirects: 0})
  let missing_location = net.request({method: "GET", url: f"{url}/redirect-missing", redirects: 1})
  let redirect_loop = net.request({method: "GET", url: f"{url}/redirect-loop", redirects: 1})
  let body_limit = net.request({method: "GET", url: f"{url}/hello", max_body_bytes: 4})
  let status = net.request({method: "GET", url: f"{url}/status", fail_status: true})
  let existing_result = net.download({url: f"{url}/file", dest: existing})
  let limited_result = net.download({
    url: f"{url}/hello",
    dest: limited,
    atomic: true,
    overwrite: true,
    max_body_bytes: 4,
  })
  let in_place_result = net.download({
    url: f"{url}/file",
    dest: in_place,
    atomic: false,
    overwrite: true,
  })?

  test.error_kind(redirect_limit, "net-redirect")
  test.error_kind(missing_location, "net-redirect")
  test.error_kind(redirect_loop, "net-redirect")
  test.error_kind(body_limit, "net-body-limit")
  test.error_kind(status, "net-status")
  test.error_kind(existing_result, "net-dest")
  test.error_kind(limited_result, "net-body-limit")
  assert existing.read_text()? == "previous"
  assert limited.read_text()? == "limited before"
  assert in_place_result.bytes == 11
  assert in_place.read_text()? == """downloaded
"""
}

proc assert_invalid_net_input(ctx: TestContext, source: Str, kind: Str) [error] {
  let _ = test.expect(ctx, source, status: 3, stderr: [kind])?
}

test test_net_transport_rejects_invalid_shapes { |ctx|
  let root = test.temp_dir(ctx, name: "net-invalid")?
  let missing_source = fp"{root}/missing.txt"

  test.error_kind(net.request({method: "TRACE", url: "http://127.0.0.1:9/"}), "net-method")
  test.error_kind(net.request({method: "GET", url: "ftp://example.test/"}), "net-scheme")
  test.error_kind(
    net.request({
      method: "GET",
      url: "http://127.0.0.1:9/",
      headers: [{name: "", value: "bad"}],
    }),
    "net-header",
  )
  test.error_kind(
    net.request({method: "POST", url: "http://127.0.0.1:9/", body_file: missing_source}),
    "net-body-file",
  )
  test.error_kind(
    net.upload({url: "http://127.0.0.1:9/", source: missing_source}),
    "net-source",
  )
  test.error_kind(net.request_many({requests: [], concurrency: 0}), "net-concurrency")
  test.error_kind(net.download_many({downloads: [], concurrency: -1}), "net-concurrency")
  test.error_kind(net.pool("invalid", -1), "net-pool")
  assert_invalid_net_input(
    ctx,
    """let _ = net.request({
  method: "POST",
  url: "http://127.0.0.1:9/",
  body: b"one",
  body_text: "two",
})
""",
    "net-body",
  )
  assert_invalid_net_input(
    ctx,
    """let _ = net.request({method: "GET", url: "http://127.0.0.1:9/", redirects: -1})
""",
    "range-error",
  )
  assert_invalid_net_input(
    ctx,
    """let _ = net.download({url: "http://127.0.0.1:9/", dest: p"missing", max_body_bytes: -1})
""",
    "range-error",
  )
  test.error_kind(net.request({method: "GET", url: "http://"}), "net-url")
  test.error_kind(
    net.upload({method: "TRACE", url: "http://127.0.0.1:9/", source: missing_source}),
    "net-method",
  )
}

test test_net_transport_timeout_contracts { |ctx|
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
    return
  }

  test.error_kind(net.request({method: "GET", url: f"{url}/slow", timeout: 50ms}), "net-timeout")
  let response = net.request({method: "GET", url: f"{url}/slow", connect_timeout: 50ms})?
  assert response.body.utf8()? == "slow"
  test.error_kind(
    net.request({method: "GET", url: f"{url}/slow", headers_timeout: 50ms}),
    "net-headers-timeout",
  )
  test.error_kind(
    net.request({method: "GET", url: f"{url}/slow-body", body_idle_timeout: 50ms}),
    "net-body-idle-timeout",
  )
  let root = test.temp_dir(ctx, name: "net-total-timeout")?
  let destination = fp"{root}/download.txt"
  test.error_kind(
    net.download({url: f"{url}/slow-body", dest: destination, timeout: 50ms}),
    "net-timeout",
  )
  assert ! destination.exists()?

  let tls_stall_url = env.get_or("XSH_NET_TEST_TLS_STALL_URL", "")?
  if tls_stall_url != "" {
    test.error_kind(
      net.request({method: "GET", url: tls_stall_url, tls_timeout: 50ms}),
      "net-tls-timeout",
    )
  }
}

test test_net_transport_batch_contracts { |ctx|
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
    return
  }

  let root = test.temp_dir(ctx, name: "net-batch")?
  let first = fp"{root}/first.txt"
  let second = fp"{root}/second.txt"
  let request_items = [
    {
      method: "GET",
      url: f"{url}/hello",
      headers: [
        {
          name: "Connection",
          value: "close",
        },
      ],
    },
    {
      method: "GET",
      url: "ftp://example.test/",
    },
    {
      method: "GET",
      url: f"{url}/status",
      headers: [
        {
          name: "Connection",
          value: "close",
        },
      ],
      fail_status: true,
    },
    {
      method: "GET",
      url: f"{url}/hello",
      headers: [
        {
          name: "Connection",
          value: "close",
        },
      ],
    },
  ]
  let download_items = [
    {
      url: f"{url}/hello",
      dest: first,
      overwrite: true,
    },
    {
      url: f"{url}/hello",
      dest: second,
      overwrite: true,
    },
  ]
  let requests = net.request_many({
    requests: request_items,
    concurrency: 2,
    pool: "batch",
  })?
  let downloads = net.download_many({
    downloads: download_items,
    concurrency: 2,
    pool: "batch-downloads",
  })?

  # A batch item is admitted only when its sliding-window slot opens. Its
  # total deadline must therefore not elapse while the preceding item runs.
  let queued_timeout_requests = net.request_many({
    requests: [{method: "GET", url: f"{url}/slow"}, {method: "GET", url: f"{url}/hello", timeout: 50ms}],
    concurrency: 1,
    pool: "batch-queued-timeout",
  })?

  assert requests[0]?.body.utf8()? == "hello"
  test.error_kind(requests[1], "net-scheme")
  test.error_kind(requests[2], "net-status")
  assert requests[3]?.body.utf8()? == "hello"
  assert downloads[0]?.bytes == 5
  assert downloads[1]?.bytes == 5
  assert first.read_text()? == "hello"
  assert second.read_text()? == "hello"
  assert queued_timeout_requests[0]?.body.utf8()? == "slow"
  assert queued_timeout_requests[1]?.body.utf8()? == "hello"
}

test test_net_transport_batch_download_error_contract { |ctx|
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
    return
  }

  let root = test.temp_dir(ctx, name: "net-batch-errors")?
  let redirected = fp"{root}/redirected.txt"
  let limited = fp"{root}/limited.txt"
  limited.write("previous")
  let download_items = [
    {
      url: f"{url}/redirect",
      dest: redirected,
      atomic: true,
      overwrite: true,
      redirects: 1,
      max_body_bytes: 1024,
    },
    {
      url: f"{url}/hello",
      dest: limited,
      atomic: true,
      overwrite: true,
      redirects: 1,
      max_body_bytes: 4,
    },
  ]
  let responses = net.download_many({
    downloads: download_items,
    concurrency: 2,
    pool: "batch-errors",
  })?

  assert responses[0]?.bytes == 5
  test.error_kind(responses[1], "net-body-limit")
  assert redirected.read_text()? == "hello"
  assert limited.read_text()? == "previous"
}

test test_net_transport_tls_contracts {
  let url = env.get_or("XSH_NET_TEST_TLS_URL", "")?
  let ca = env.get_or("XSH_NET_TEST_CA", "")?
  if url == "" or ca == "" {
    test.skip("requires TLS fixture")
    return
  }

  let rejected = net.request({method: "GET", url: f"{url}/secure"})
  let unverified = net.request({
    method: "GET",
    url: f"{url}/secure",
    tls_verify: false,
  })?
  let verified = net.request({
    method: "GET",
    url: f"{url}/secure",
    ca_certificate: fp"{ca}",
  })?

  test.error_kind(rejected, "net-tls")
  assert unverified.body.utf8()? == "secure"
  assert verified.body.utf8()? == "secure"
}

test test_net_transport_https_http1_contract {
  let url = env.get_or("XSH_NET_TEST_H1_URL", "")?
  let ca = env.get_or("XSH_NET_TEST_CA", "")?
  if url == "" or ca == "" {
    test.skip("requires HTTPS H1 fixture")
    return
  }

  let response = net.request({
    method: "GET",
    url: f"{url}/secure",
    ca_certificate: fp"{ca}",
    pool: "h1-alpn",
  })?

  assert response.body.utf8()? == "secure"
}

test test_net_transport_request_many_https_h2_contract {
  let url = env.get_or("XSH_NET_TEST_H2_URL", "")?
  let ca = env.get_or("XSH_NET_TEST_CA", "")?
  if url == "" or ca == "" {
    test.skip("requires HTTPS H2 fixture")
    return
  }

  let request_items = [
    {
      method: "GET",
      url: f"{url}/h2",
    },
    {
      method: "GET",
      url: f"{url}/h2",
    },
  ]
  let batch = {
    requests: request_items,
    concurrency: 1,
    ca_certificate: fp"{ca}",
    pool: "h2",
  }
  let requests = net.request_many(batch)?

  assert requests[0]?.body.utf8()? == "h2"
  assert requests[1]?.body.utf8()? == "h2"
}

test test_net_job_cancel_keeps_h2_siblings_and_pool_healthy {
  let url = env.get_or("XSH_NET_TEST_H2_CANCEL_URL", "")?
  let ca = env.get_or("XSH_NET_TEST_CA", "")?
  if url == "" or ca == "" {
    test.skip("requires HTTPS H2 cancellation fixture")
    return
  }

  let pool = "h2-cancel"
  let warmed = net.request_many({
    requests: [{method: "GET", url: f"{url}/warm"}],
    concurrency: 1,
    ca_certificate: fp"{ca}",
    pool: pool,
  })?
  assert warmed[0]?.body.utf8()? == "warm"

  let stalled = net.start({
    method: "GET",
    url: f"{url}/slow",
    ca_certificate: fp"{ca}",
    pool: pool,
  })?
  let sibling = net.start({
    method: "GET",
    url: f"{url}/fast",
    ca_certificate: fp"{ca}",
    pool: pool,
  })?
  let fast = sibling.wait()?
  assert fast.body.utf8()? == "fast"
  stalled.cancel()

  let later = net.request_many({
    requests: [{method: "GET", url: f"{url}/later"}],
    concurrency: 1,
    ca_certificate: fp"{ca}",
    pool: pool,
  })?
  assert later[0]?.body.utf8()? == "later"
}

test test_net_transport_download_many_https_h2_contract { |ctx|
  let url = env.get_or("XSH_NET_TEST_H2_URL", "")?
  let ca = env.get_or("XSH_NET_TEST_CA", "")?
  if url == "" or ca == "" {
    test.skip("requires HTTPS H2 fixture")
    return
  }

  let root = test.temp_dir(ctx, name: "net-h2")?
  let dest = fp"{root}/h2.txt"
  let download_items = [{url: f"{url}/h2", dest: dest, overwrite: true}]
  let batch = {
    downloads: download_items,
    ca_certificate: fp"{ca}",
    pool: "h2-download",
  }
  let downloads = net.download_many(batch)?

  assert downloads[0]?.bytes == 2
  assert dest.read_text()? == "h2"
}

test test_net_transport_linux_system_ca_dir {
  let url = env.get_or("XSH_NET_TEST_TLS_URL", "")?
  if url == "" {
    test.skip("requires Linux TLS fixture")
    return
  }

  let response = net.request({method: "GET", url: f"{url}/secure"})?
  assert response.status == 200
  assert response.body.utf8()? == "secure"
}
