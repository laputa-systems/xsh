let url = "https://example.com"
let response = net.request({method: "GET", url: url})?
print $response.status
