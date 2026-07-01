let hosts = dns.resolve_host("localhost")?
let pool = net.pool("docs", 2, 5s)?
let refused = net.request({method: "GET", url: "ftp://example.invalid/file", pool: "docs"})

match refused {
  Err(error) => print (hosts.len() > 0) $pool.name $pool.max_idle_per_host $error.message
}

let _closed = net.close_pool("docs")?
