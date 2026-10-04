type Hit = {url: Str, size: Int}

const hits: List[Hit] = [
  {url: "/a", size: 120},
  {url: "/b/c", size: 4000},
]

pure budget(hit: Hit) -> Int {
  hit.url.byte_len() * 1000
}

# begin example
let by_url = hits |> count { .url }
let over_budget = hits |> where .size > budget(.)
let depths = hits |> map { .url.split("/") |> where . != "" |> count() }
let port = "eighty".parse_int() ?? { .message.byte_len() }
# end example
