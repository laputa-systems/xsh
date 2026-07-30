let url = args[0]
var requests = []
var count = 0

while count < 10000 {
  requests = requests.push({method: "GET", url: url, pool: "benchmark", max_body_bytes: 1024})
  count += 1
}

let responses = net.request_many({requests: requests, concurrency: 8, pool: "benchmark"})?
var total = 0

for response in responses {
  total += response?.bytes
}

print $total
