# platform: linux
type Address = {family: Str, local: Str, prefixlen: Int}
type Link = {ifname: Str, operstate: Str, addr_info: List[Address]}

let links = json.decode(run.text ip -j addr show ?)?.require(List[Link])?

for link in links |> sort-by .ifname {
  let v4 = [f"{a.local}/{a.prefixlen}" for a in link.addr_info if a.family == "inet"]
  print f"{link.ifname:<12} {link.operstate.lower():<8} {v4.join(", ")}"
}
