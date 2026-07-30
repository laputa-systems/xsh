let url = args[0]
var requests = []
var count = 0

while count < 10000 {
  requests = requests.push({method: "GET", url: url, pool: "benchmark", max_body_bytes: 1024})
  count += 1
}

var total = 0

for request in requests {
  let response = net.request(request)?
  total += response.bytes
}

print $total
