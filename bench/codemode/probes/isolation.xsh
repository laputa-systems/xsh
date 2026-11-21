proc listing(dir: Path) -> Str {
  match fs.children(dir) {
    Ok(entries) => (entries |> map .name |> sort()).join(" ")
    Err(error) => f"unreadable: {error.message}"
  }
}

print f"/ = {listing(p"/")}"
print f"/task = {listing(p"/task")}"
print f"/work = {listing(p"/work")}"
print f"/home = {listing(p"/home")}"
print f"/root = {listing(p"/root")}"
let reach = net.request({method: "GET", url: "http://1.1.1.1/", timeout: 2s})
print f"external network reachable: {reach is Ok(_)}"
let service = net.request({method: "GET", url: "http://127.0.0.1:8080/", timeout: 2s})
print f"loopback service reachable: {service is Ok(_)}"
let python = try run.capture --text python3 -c "print(1)"
print f"python3 runs: {python is Ok(_) and python?.status.ok}"
let perl = try run.capture --text perl -e "print 1"
print f"perl runs: {perl is Ok(_) and perl?.status.ok}"
let mounts = p"/proc/self/mountinfo".read_text()?.lines() |> where { |l| " /task" in l or " /work" in l or "/usr/local/bin/xsh" in l } |> map { |l| l.fields()[4] }
print f"bind mounts: {mounts.join(" ")}"
let names = env.entries()? |> map { |e| e.name.utf8() ?? "?" } |> sort()
print f"environment: {names.join(" ")}"
let write_root = try { p"/etc/probe".write("x")? }
print f"root filesystem writable: {write_root is Ok(_)}"
