type Disk = {mount: Str, used: Int, size: Int}

const inventory: List[Disk] = [
  {mount: "/", used: 45, size: 50},
  {mount: "/var", used: 99, size: 100},
  {mount: "/home", used: 10, size: 100},
]

pure usage(d: Disk) {
  d.used * 100 / d.size
}

pure hottest(disks: List[Disk], over = 80) {
  disks
    |> where { |d| usage(d) >= over }
    |> sort-by(desc: true, block: usage)
    |> first()
}

proc mount_of(target: Path) {
  let m = fs.mount_for(target)?
  Disk(mount: m.mounted_on.display(), used: m.used_1k, size: m.blocks_1k)
}

let worst = hottest(inventory)?
print f"{worst.mount} at {usage(worst)}%"
print f"anything at 100%: {hottest(inventory, over: 100) is Ok(_)}"
print f"root has capacity: {mount_of(/)?.size > 0}"
