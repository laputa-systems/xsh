let doc = json.decode("""{"version": 3, "features": ["ipv6"]}""")?
let version = if let {version: v is Int, ..} = doc { v } else { 0 }
print f"config version {version}"
