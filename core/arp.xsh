#!/bin/xsh
use lib.gnu
use lib.nettools

const USAGE = """Usage:
  arp [-vn]  [<HW>] [-i <if>] [-a] [<hostname>]             <-Display ARP cache
  arp [-v]          [-i <if>] -d  <host> [pub]               <-Delete ARP entry
  arp [-vnD] [<HW>] [-i <if>] -f  [<filename>]            <-Add entry from file
  arp [-v]   [<HW>] [-i <if>] -s  <host> <hwaddr> [temp]            <-Add entry
  arp [-v]   [<HW>] [-i <if>] -Ds <host> <if> [netmask <nm>] pub          <-''-

        -a                       display (all) hosts in alternative (BSD) style
        -e                       display (all) hosts in default (Linux) style
        -s, --set                set a new ARP entry
        -d, --delete             delete a specified entry
        -v, --verbose            be verbose
        -n, --numeric            don't resolve names
        -i, --device             specify network interface (e.g. eth0)
        -D, --use-device         read <hwaddr> from given device
        -A, -p, --protocol       specify protocol family
        -f, --file               read new entries from file or from /etc/ethers

  <HW>=Use '-H <hw>' to specify hardware address type. Default: ether
  List of possible hardware types (which support ARP):
    ether (Ethernet) netrom (AMPR NET/ROM)
"""

# ARPHRD numbers of the hardware types arp can filter by or set.
const HW_TYPES = [{name: "ether", number: 1}, {name: "netrom", number: 0}]

type Options = {
  bsd: Bool, set: Bool, delete: Bool, file: Bool, verbose: Bool, numeric: Bool, use_device: Bool,
  device: Str?, hw: Int, hw_given: Bool, operands: List[Str],
}

proc usage_error() [process, error] {
  eprint nettools.chomp(USAGE)
  exit 3
}

# inet_aton's decimal forms: a, a.b, a.b.c, and a.b.c.d, the last part filling
# the remaining bytes.
pure parse_address(word: Str) -> Bytes? {
  let parts = word.split(".")
  if parts.len() < 1 or parts.len() > 4 { return null }
  var values: List[Int] = []
  for part in parts {
    let value = nettools.decimal(part)
    if value == null { return null }
    values += [value ?? 0]
  }
  var octets: List[Int] = []
  for index in range(parts.len() - 1) {
    if values[index] > 255 { return null }
    octets += [values[index]]
  }
  var tail = values[parts.len() - 1]
  let room = 4 - octets.len()
  var limit = 1
  for _ in range(room) { limit = limit * 256 }
  if tail >= limit { return null }
  var last: List[Int] = []
  for _ in range(room) {
    last = [tail % 256] + last
    tail = tail / 256
  }
  match bytes.from_ints(octets + last) {
    Ok(raw) => raw
    Err(_) => null
  }
}

proc resolve(word: Str) [fs, process, error] -> Bytes {
  if let raw = parse_address(word) { return raw }
  if let address = nettools.hosts_address(word) {
    if let raw = nettools.ipv4_parse(address) { return raw }
  }
  eprint f"{word}: Unknown host"
  exit 255
  b""
}

pure hw_number(name: Str) -> Int? {
  for entry in HW_TYPES {
    if entry.name == name { return entry.number }
  }
  null
}

# One struct arpreq: protocol address, hardware address, flags, netmask, and
# the interface name, 68 bytes in all.
proc arp_request(address: Bytes, hw_family: Int, mac: Bytes, flags: Int, mask: Bytes?, device: Str?) [process, error] -> Bytes {
  let mask_part = if let raw = mask {
    bytes.concat([bytes.pack_le(2, 2)?, bytes.zero(2)?, raw, bytes.zero(8)?])
  } else {
    bytes.zero(16)?
  }
  var name = bytes.zero(16)?
  if let device_name = device {
    let raw = bytes.from_text(device_name)
    if raw.len() > 15 {
      eprint f"arp: {device_name}: interface name too long"
      exit 255
    }
    name = bytes.concat([raw, bytes.zero(16 - raw.len())?])
  }
  bytes.concat(
    [
      bytes.pack_le(2, 2)?, bytes.zero(2)?, address, bytes.zero(8)?,
      bytes.pack_le(hw_family, 2)?, mac, bytes.zero(8)?,
      bytes.pack_le(flags, 4)?,
      mask_part,
      name,
    ],
  )
}

# SIOCSARP for one host. `hardware` is an address text, or with -D an
# interface name whose hardware address is copied.
proc set_entry(options: Options, host: Str, hardware: Str, words: List[Str]) [fs, process, env, error] {
  let c = linux.net_constants()
  let address = resolve(host)
  var mac = b""
  if options.use_device {
    let socket = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
    defer unix.close_fd(socket)
    let lookup = nettools.ifreq(hardware, b"") ?? b""
    match linux.ioctl(socket, c.SIOCGIFHWADDR, lookup, 40) {
      Ok(reply) => mac = reply.slice(18, 6)
      Err(failure) => {
        eprint f"arp: cant get HW-Address for `{hardware}': {gnu.strerror(failure)}."
        exit 255
      }
    }
  } else {
    let parsed = nettools.ether_parse(hardware)
    if parsed == null {
      eprint "arp: invalid hardware address"
      exit 255
    }
    mac = parsed ?? b""
  }
  var flags = 6
  var mask: Bytes? = null
  var at = 0
  while at < words.len() {
    let word = words[at]
    at += 1
    match word {
      "temp" => flags = flags.clear_bits(4)
      "pub" => flags = flags.bit_or(8)
      "trail" => flags = flags.bit_or(16)
      "dontpub" => flags = flags.bit_or(64)
      "netmask" => {
        if at >= words.len() { usage_error() }
        let raw = nettools.netmask_parse(words[at])
        if raw == null {
          eprint f"{words[at]}: Unknown host"
          exit 255
        }
        mask = raw
        flags = flags.bit_or(32)
        at += 1
      }
      else => usage_error()
    }
  }
  var device: Str? = options.device
  if options.use_device { device = hardware }
  let request = arp_request(address, options.hw, mac, flags, mask, device)
  let socket = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(socket)
  if options.verbose { eprint "arp: SIOCSARP()" }
  match linux.ioctl(socket, c.SIOCSARP, request, 0) {
    Ok(_) => {}
    Err(failure) => {
      eprint f"SIOCSARP: {gnu.strerror(failure)}"
      exit 255
    }
  }
}

# SIOCDARP: first without the proxy flag, then with it, as the legacy tool
# does, so that `arp -d HOST` removes either kind of entry.
proc delete_entry(options: Options, host: Str, words: List[Str]) [fs, process, env, error] {
  let c = linux.net_constants()
  var proxy = false
  for word in words {
    if word != "pub" { usage_error() }
    proxy = true
  }
  let address = resolve(host)
  let socket = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(socket)
  let tries = if proxy { [8] } else { [0, 8] }
  for flags in tries {
    let request = arp_request(address, options.hw, bytes.zero(6)?, flags, null, options.device)
    if options.verbose { eprint f"arp: SIOCDARP({if flags == 8 { "pub" } else { "dontpub" }})" }
    match linux.ioctl(socket, c.SIOCDARP, request, 0) {
      Ok(_) => return
      Err(failure) => {
        # ENXIO or ENOENT: no such entry, rather than a failure of the request.
        if gnu.errno(failure) != 6 and gnu.errno(failure) != 2 {
          eprint f"SIOCDARP({if flags == 8 { "pub" } else { "dontpub" }}): {gnu.strerror(failure)}"
          exit 255
        }
      }
    }
  }
  eprint f"No ARP entry for {host}"
  exit 255
}

proc show(options: Options) [fs, process, env, io, error] {
  let entries = nettools.neighbours() ?? { |failure|
    eprint f"arp: cannot read the neighbour table: {gnu.strerror(failure)}"
    exit 255
  }
  var wanted: Str? = null
  var wanted_address = ""
  if !options.operands.is_empty() {
    let raw = parse_address(options.operands[0])
    guard let bytes_ = raw else {
      eprint f"{options.operands[0]}: Unknown host"
      exit 255
    }
    wanted = options.operands[0]
    wanted_address = nettools.ipv4_text(bytes_, 0)
  }
  var names: Map[Str, Str] = {}
  if !options.numeric { names = nettools.hosts_names() }
  var shown = 0
  var skipped = 0
  var header = false
  for entry in entries {
    var keep = true
    if let device = options.device {
      if entry.iface != device { keep = false }
    }
    if options.hw_given and entry.hwtype != options.hw { keep = false }
    if wanted != null and entry.address != wanted_address { keep = false }
    if !keep {
      skipped += 1
      continue
    }
    shown += 1
    let name: Str = names.get(entry.address) ?? ""
    if options.bsd {
      gnu.write_text(nettools.arp_bsd_row(entry, if name == "" { "?" } else { name }) + "\n")
    } else {
      if !header {
        gnu.write_text(nettools.ARP_HEADER + "\n")
        header = true
      }
      gnu.write_text(nettools.arp_row(entry, if name == "" { entry.address } else { name }) + "\n")
    }
  }
  if options.verbose {
    gnu.write_text(f"Entries: {entries.len()}\tSkipped: {skipped}\tFound: {shown}\n")
  }
  if shown == 0 {
    if wanted != null and !options.bsd {
      gnu.write_text(f"{wanted ?? ""} ({wanted_address}) -- no entry\n")
    } else if wanted != null or options.device != null or options.hw_given {
      gnu.write_text(f"arp: in {entries.len()} entries no match found.\n")
    }
  }
}

proc load_file(options: Options, file: Str) [fs, process, env, error] {
  let text = fp"{file}".read_text() ?? { |_|
    eprint f"arp: cannot open etherfile {file} !"
    exit 255
  }
  for line in text.lines() {
    let words = line.split("#")[0].words()
    continue when words.len() < 2
    set_entry(options, words[0], words[1], words[2..])
  }
}

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  var options: Options = {
    bsd: false, set: false, delete: false, file: false, verbose: false, numeric: false, use_device: false,
    device: null, hw: 1, hw_given: false, operands: [],
  }
  var at = 0
  var operands: List[Str] = []
  while at < argv.len() {
    let word = argv[at]
    at += 1
    if word == "--" {
      operands += argv[at..]
      break
    }
    if !word.starts_with("-") or word == "-" {
      operands += [word]
      continue
    }
    if word.starts_with("--") {
      match word {
        "--set" => options = {...options, set: true}
        "--delete" => options = {...options, delete: true}
        "--verbose" => options = {...options, verbose: true}
        "--numeric" => options = {...options, numeric: true}
        "--use-device" => options = {...options, use_device: true}
        "--file" => options = {...options, file: true}
        "--ascii" => options = {...options, bsd: true}
        "--version" => {
          gnu.version("arp")
          return
        }
        "--help" => {
          gnu.help(USAGE)
          return
        }
        "--device" | "--protocol" | "--hw-type" => {
          if at >= argv.len() { usage_error() }
          let value = argv[at]
          at += 1
          if word == "--device" {
            options = {...options, device: value}
          } else if word == "--protocol" {
            if value != "inet" {
              eprint f"arp: {value}: kernel only supports 'inet'."
              exit 255
            }
          } else {
            let number = hw_number(value)
            if number == null {
              eprint f"arp: {value}: unknown hardware type."
              exit 255
            }
            options = {...options, hw: number ?? 1, hw_given: true}
          }
        }
        else => {
          eprint f"arp: unrecognized option: {word}"
          usage_error()
        }
      }
      continue
    }
    let letters = word.byte_slice(1).split("")
    var position = 0
    while position < letters.len() {
      let letter = letters[position]
      position += 1
      match letter {
        "a" => options = {...options, bsd: true}
        "e" => options = {...options, bsd: false}
        "s" => options = {...options, set: true}
        "d" => options = {...options, delete: true}
        "v" => options = {...options, verbose: true}
        "n" => options = {...options, numeric: true}
        "D" => options = {...options, use_device: true}
        "f" => options = {...options, file: true}
        "V" => {
          gnu.version("arp")
          return
        }
        "h" | "?" => {
          gnu.help(USAGE)
          return
        }
        "i" | "A" | "p" | "H" | "t" => {
          # The value is the rest of the cluster (-id0) or the next word.
          var value = ""
          if position < letters.len() {
            value = letters[position..].join("")
            position = letters.len()
          } else {
            if at >= argv.len() { usage_error() }
            value = argv[at]
            at += 1
          }
          if letter == "i" {
            options = {...options, device: value}
          } else if letter == "A" or letter == "p" {
            if value != "inet" {
              eprint f"arp: {value}: kernel only supports 'inet'."
              exit 255
            }
          } else {
            let number = hw_number(value)
            if number == null {
              eprint f"arp: {value}: unknown hardware type."
              exit 255
            }
            options = {...options, hw: number ?? 1, hw_given: true}
          }
        }
        else => {
          eprint f"arp: unrecognized option: {letter}"
          usage_error()
        }
      }
    }
  }
  options = {...options, operands: operands}

  if options.set {
    if operands.is_empty() {
      eprint "arp: need host name"
      exit 255
    }
    if operands.len() < 2 {
      eprint "arp: need hardware address"
      exit 255
    }
    set_entry(options, operands[0], operands[1], operands[2..])
    return
  }
  if options.delete {
    if operands.is_empty() {
      eprint "arp: need host name"
      exit 255
    }
    delete_entry(options, operands[0], operands[1..])
    return
  }
  if options.file {
    load_file(options, if operands.is_empty() { "/etc/ethers" } else { operands[0] })
    return
  }
  if operands.len() > 1 { usage_error() }
  show(options)
}
