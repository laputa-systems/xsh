# Tests run `udevadm` against a synthetic sysfs and udev database tree
# selected with `XSH_UDEVADM_ROOT`, so no real device is read or written. The
# expected text was captured from eudev's `udevadm` reading an equivalent
# tree; attributes are sorted by name here, where eudev prints directory order.

const TREE_FILES = [
  {path: "sys/devices/pci0000:00/0000:00:1f.2/uevent", text: "DRIVER=ahci\nPCI_ID=8086:2922\nMODALIAS=pci:v00008086d00002922\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/vendor", text: "0x8086\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/device", text: "0x2922\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/label", text: "two\nlines\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/power/control", text: "on\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/block/sda/uevent", text: "MAJOR=8\nMINOR=0\nDEVNAME=sda\nDEVTYPE=disk\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/block/sda/dev", text: "8:0\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/block/sda/size", text: "41943040\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/block/sda/ro", text: "0\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/block/sda/sda1/uevent", text: "MAJOR=8\nMINOR=1\nDEVNAME=sda1\nDEVTYPE=partition\nPARTN=1\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/block/sda/sda1/dev", text: "8:1\n"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/block/sda/sda1/partition", text: "1\n"},
  {path: "sys/devices/virtual/net/lo/uevent", text: "INTERFACE=lo\nIFINDEX=1\n"},
  {path: "sys/devices/virtual/net/lo/mtu", text: "65536\n"},
  {path: "sys/devices/virtual/net/lo/note", text: "tab\there\n"},
  {path: "sys/devices/virtual/mem/null/uevent", text: "MAJOR=1\nMINOR=3\nDEVNAME=null\nDEVMODE=0666\n"},
  {path: "sys/devices/virtual/mem/null/dev", text: "1:3\n"},
  {path: "run/udev/data/b8:1", text: "S:disk/by-uuid/1234\nS:disk/by-label/ROOT\nL:-1\nE:ID_FS_UUID=1234\nE:ID_FS_LABEL=ROOT\nG:systemd\nI:99\nQ:systemd\n"},
  {path: "run/udev/data/c1:3", text: "S:mynull\nE:ID_NODE=null\n"},
  {path: "run/udev/data/n1", text: "E:ID_NET_NAME=lo\nI:5\n"},
  {path: "run/udev/data/+pci:0000:00:1f.2", text: "E:ID_PCI_CLASS=storage\nG:pci\n"},
]

const TREE_LINKS = [
  {path: "sys/devices/pci0000:00/0000:00:1f.2/subsystem", target: "../../../bus/pci"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/driver", target: "../../../bus/pci/drivers/ahci"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/block/sda/subsystem", target: "../../../../../../class/block"},
  {path: "sys/devices/pci0000:00/0000:00:1f.2/block/sda/sda1/subsystem", target: "../../../../../../../class/block"},
  {path: "sys/devices/virtual/net/lo/subsystem", target: "../../../../class/net"},
  {path: "sys/devices/virtual/mem/null/subsystem", target: "../../../../class/mem"},
  {path: "sys/class/block/sda", target: "../../devices/pci0000:00/0000:00:1f.2/block/sda"},
  {path: "sys/class/block/sda1", target: "../../devices/pci0000:00/0000:00:1f.2/block/sda/sda1"},
  {path: "sys/class/net/lo", target: "../../devices/virtual/net/lo"},
  {path: "sys/class/mem/null", target: "../../devices/virtual/mem/null"},
  {path: "sys/bus/pci/devices/0000:00:1f.2", target: "../../../devices/pci0000:00/0000:00:1f.2"},
  {path: "sys/dev/block/8:0", target: "../../devices/pci0000:00/0000:00:1f.2/block/sda"},
  {path: "sys/dev/block/8:1", target: "../../devices/pci0000:00/0000:00:1f.2/block/sda/sda1"},
  {path: "sys/dev/char/1:3", target: "../../devices/virtual/mem/null"},
]

type Ran = {status: Int, stdout: Str, stderr: Str}

# A device list in the shape sysfs presents: class links, a bus device link,
# `dev` number links, a controller with a disk and a partition below it, a
# network interface, and a character device, with database records for some.
proc ensure_directory(root: FsRoot, directory: Path) [fs, error] -> Result[Unit] {
  if ! root.exists(directory)? {
    root.mkdir(directory, parents: true)?
  }
}

proc build_tree(ctx: TestContext) [fs, error] -> Result[Path] {
  let base = test.temp_dir(ctx, name: "udev-tree")?
  let root = fs.open_root(base)?
  for item in TREE_FILES {
    ensure_directory(root, fp"{item.path}".dirname())?
    root.write(fp"{item.path}", item.text)?
  }

  for item in TREE_LINKS {
    ensure_directory(root, fp"{item.path}".dirname())?
    root.symlink(target: fp"{item.target}", path: fp"{item.path}")?
  }

  ensure_directory(root, p"sys/bus/pci/drivers/ahci")?
  # Class directories also hold plain files, which scans must step over.
  root.write(p"sys/class/net/bonding_masters", "")?
  # A write-only attribute that the walk must not read.
  root.write(p"sys/devices/pci0000:00/0000:00:1f.2/secret", "x")?
  fp"{base}/sys/devices/pci0000:00/0000:00:1f.2/secret".chmod(0o200)?
  Ok(base)
}

proc udevadm(ctx: TestContext, tree: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let scratch = test.temp_dir(ctx, name: "udevadm-run")?
  let output = fp"{scratch}/stdout"
  let errors = fp"{scratch}/stderr"
  let script = fp"{ctx.core_dir}/udevadm.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--"].extend(args),
    scratch, {XSH_UDEVADM_ROOT: tree.display()}, b"", output, errors, timeout: 20s))?
  Ok({status: status.exit_code()?, stdout: output.read_text()?, stderr: errors.read_text()?})
}

test test_udevadm_info_prints_device_record_from_sysfs_and_database { |ctx|
  let tree = build_tree(ctx)?
  let ran = udevadm(ctx, tree, ["info", "-p", "/class/block/sda1"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == """P: /devices/pci0000:00/0000:00:1f.2/block/sda/sda1
N: sda1
L: -1
S: disk/by-label/ROOT
S: disk/by-uuid/1234
E: DEVLINKS=/dev/disk/by-label/ROOT /dev/disk/by-uuid/1234
E: DEVNAME=/dev/sda1
E: DEVPATH=/devices/pci0000:00/0000:00:1f.2/block/sda/sda1
E: DEVTYPE=partition
E: ID_FS_LABEL=ROOT
E: ID_FS_UUID=1234
E: MAJOR=8
E: MINOR=1
E: PARTN=1
E: SUBSYSTEM=block
E: TAGS=:systemd:
E: USEC_INITIALIZED=99

"""
  assert udevadm(ctx, tree, ["info", "-q", "all", "-p", "/class/block/sda1"])?.stdout == ran.stdout
  # Without a database record the properties come from sysfs alone.
  let bare = udevadm(ctx, tree, ["info", "/sys/devices/pci0000:00/0000:00:1f.2/block/sda"])?
  assert bare.stdout == """P: /devices/pci0000:00/0000:00:1f.2/block/sda
N: sda
E: DEVNAME=/dev/sda
E: DEVPATH=/devices/pci0000:00/0000:00:1f.2/block/sda
E: DEVTYPE=disk
E: MAJOR=8
E: MINOR=0
E: SUBSYSTEM=block

"""
  # A device with no node has no N: line, and the database is found by
  # `+subsystem:sysname` rather than by number.
  let pci = udevadm(ctx, tree, ["info", "-p", "/devices/pci0000:00/0000:00:1f.2"])?
  assert pci.stdout == """P: /devices/pci0000:00/0000:00:1f.2
E: DEVPATH=/devices/pci0000:00/0000:00:1f.2
E: DRIVER=ahci
E: ID_PCI_CLASS=storage
E: MODALIAS=pci:v00008086d00002922
E: PCI_ID=8086:2922
E: SUBSYSTEM=pci
E: TAGS=:pci:

"""
}

test test_udevadm_info_queries_select_one_field { |ctx|
  let tree = build_tree(ctx)?
  let target = ["-p", "/class/block/sda1"]
  assert udevadm(ctx, tree, ["info", "-q", "name"].extend(target))?.stdout == "sda1\n"
  assert udevadm(ctx, tree, ["info", "-r", "-q", "name"].extend(target))?.stdout == "/dev/sda1\n"
  assert udevadm(ctx, tree, ["info", "-q", "symlink"].extend(target))?.stdout == "disk/by-label/ROOT disk/by-uuid/1234\n"
  assert udevadm(ctx, tree, ["info", "-r", "-q", "symlink"].extend(target))?.stdout == "/dev/disk/by-label/ROOT /dev/disk/by-uuid/1234\n"
  assert udevadm(ctx, tree, ["info", "-q", "path"].extend(target))?.stdout == "/devices/pci0000:00/0000:00:1f.2/block/sda/sda1\n"
  let properties = udevadm(ctx, tree, ["info", "-q", "property"].extend(target))?.stdout
  assert properties.starts_with("DEVLINKS=/dev/disk/by-label/ROOT /dev/disk/by-uuid/1234\nDEVNAME=/dev/sda1\n")
  assert properties.ends_with("TAGS=:systemd:\nUSEC_INITIALIZED=99\n")
  let exported = udevadm(ctx, tree, ["info", "-q", "property", "-x"].extend(target))?.stdout
  assert exported.starts_with("DEVLINKS='/dev/disk/by-label/ROOT /dev/disk/by-uuid/1234'\nDEVNAME='/dev/sda1'\n")
  let prefixed = udevadm(ctx, tree, ["info", "-q", "property", "-x", "-P", "ID_"].extend(target))?.stdout
  assert "ID_MAJOR='8'\n" in prefixed
  # A device without symlinks prints an empty line, as eudev does.
  assert udevadm(ctx, tree, ["info", "-q", "symlink", "-p", "/class/net/lo"])?.stdout == "\n"
}

test test_udevadm_info_finds_devices_by_node_and_symlink_name { |ctx|
  let tree = build_tree(ctx)?
  assert udevadm(ctx, tree, ["info", "-q", "path", "-n", "sda1"])?.stdout == "/devices/pci0000:00/0000:00:1f.2/block/sda/sda1\n"
  assert udevadm(ctx, tree, ["info", "-q", "path", "-n", "/dev/sda"])?.stdout == "/devices/pci0000:00/0000:00:1f.2/block/sda\n"
  assert udevadm(ctx, tree, ["info", "-q", "path", "-n", "disk/by-uuid/1234"])?.stdout == "/devices/pci0000:00/0000:00:1f.2/block/sda/sda1\n"
  assert udevadm(ctx, tree, ["info", "-q", "path", "/dev/mynull"])?.stdout == "/devices/virtual/mem/null\n"
  let missing = udevadm(ctx, tree, ["info", "-n", "nosuch"])?
  assert missing.status == 2
  assert missing.stderr == "device node not found\n"
  let no_node = udevadm(ctx, tree, ["info", "-q", "name", "-p", "/class/net/lo"])?
  assert no_node.status == 5
  assert no_node.stderr == "no device node found\n"
}

test test_udevadm_info_attribute_walk_climbs_parents { |ctx|
  let tree = build_tree(ctx)?
  let ran = udevadm(ctx, tree, ["info", "-a", "-p", "/class/block/sda1"])?
  assert ran.status == 0, ran.stderr
  # eudev opens the walk with a blank line, which a block string would drop.
  assert ran.stdout == "\n" + """Udevadm info starts with the device specified by the devpath and then
walks up the chain of parent devices. It prints for every device
found, all possible attributes in the udev rules key format.
A rule to match, can be composed by the attributes of the device
and the attributes from one single parent device.

  looking at device '/devices/pci0000:00/0000:00:1f.2/block/sda/sda1':
    KERNEL=="sda1"
    SUBSYSTEM=="block"
    DRIVER==""
    ATTR{partition}=="1"

  looking at parent device '/devices/pci0000:00/0000:00:1f.2/block/sda':
    KERNELS=="sda"
    SUBSYSTEMS=="block"
    DRIVERS==""
    ATTRS{ro}=="0"
    ATTRS{size}=="41943040"

  looking at parent device '/devices/pci0000:00/0000:00:1f.2':
    KERNELS=="0000:00:1f.2"
    SUBSYSTEMS=="pci"
    DRIVERS=="ahci"
    ATTRS{device}=="0x2922"
    ATTRS{vendor}=="0x8086"

"""
  # Multi-line, unprintable, write-only, and link attributes are not printed.
  let lo = udevadm(ctx, tree, ["info", "-a", "-p", "/class/net/lo"])?.stdout
  assert "ATTR{mtu}==\"65536\"" in lo
  assert "note" not in lo
  assert "secret" not in ran.stdout
  assert "label" not in ran.stdout
}

test test_udevadm_info_export_db_lists_scanned_devices_and_cleanup_removes_records { |ctx|
  let tree = build_tree(ctx)?
  let all = udevadm(ctx, tree, ["info", "-e"])?
  assert all.status == 0, all.stderr
  let blocks = all.stdout.lines() |> where .starts_with("P: ") |> collect()
  assert blocks == [
    "P: /devices/pci0000:00/0000:00:1f.2",
    "P: /devices/pci0000:00/0000:00:1f.2/block/sda",
    "P: /devices/pci0000:00/0000:00:1f.2/block/sda/sda1",
    "P: /devices/virtual/mem/null",
    "P: /devices/virtual/net/lo",
  ]
  fp"{tree}/run/udev/queue.bin".write("")
  fp"{tree}/run/udev/links/disk/by-uuid".mkdir(parents: true)?
  fp"{tree}/run/udev/links/disk/by-uuid/1234".write("")
  let sticky = fp"{tree}/run/udev/data/c1:3"
  sticky.chmod(0o1644)?
  let cleaned = udevadm(ctx, tree, ["info", "-c"])?
  assert cleaned.status == 0, cleaned.stderr
  assert ! fp"{tree}/run/udev/queue.bin".exists()?
  assert ! fp"{tree}/run/udev/data/b8:1".exists()?
  assert ! fp"{tree}/run/udev/links/disk".exists()?
  # A record whose sticky bit is set survives, as it does in eudev.
  assert sticky.exists()?
}

test test_udevadm_info_device_id_of_file { |ctx|
  let tree = build_tree(ctx)?
  let file = test.temp_file(ctx, name: "probe", contents: b"x")?
  let expected = fs.stat(file)?.dev
  let ran = udevadm(ctx, tree, ["info", "-d", file.display()])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == f"{fs.dev_major(expected)}:{fs.dev_minor(expected)}\n"
  let missing = udevadm(ctx, tree, ["info", "-d", fp"{tree}/absent".display()])?
  assert missing.status == 1
  assert "absent" in missing.stderr
}

test test_udevadm_info_rejects_selectors_and_modifiers_that_would_do_nothing { |ctx|
  let tree = build_tree(ctx)?
  let target = ["-p", "/class/block/sda1"]
  let export_all = udevadm(ctx, tree, ["info", "-x"].extend(target))?
  assert export_all.status == 1
  assert "applies only to '--query=property'" in export_all.stderr
  let root_all = udevadm(ctx, tree, ["info", "-r"].extend(target))?
  assert root_all.status == 1
  assert "applies only to '--query=name' and '--query=symlink'" in root_all.stderr
  let prefix = udevadm(ctx, tree, ["info", "-q", "property", "-P", "X_"].extend(target))?
  assert prefix.status == 1
  assert "requires '--export'" in prefix.stderr
  let both = udevadm(ctx, tree, ["info", "-a", "-q", "path"].extend(target))?
  assert both.status == 1
  assert "mutually exclusive" in both.stderr
  let twice = udevadm(ctx, tree, ["info", "-n", "sda", "-p", "/class/block/sda1"])?
  assert twice.status == 2
  assert twice.stderr == "device already specified\n"
  let unknown_query = udevadm(ctx, tree, ["info", "-q", "bogus"].extend(target))?
  assert unknown_query.status == 3
  assert unknown_query.stderr == "unknown query type\n"
  let unknown_device = udevadm(ctx, tree, ["info", "relative/path"])?
  assert unknown_device.status == 4
  let missing_path = udevadm(ctx, tree, ["info", "-p", "/devices/absent"])?
  assert missing_path.status == 2
  assert missing_path.stderr == "syspath not found\n"
  let plain_file = udevadm(ctx, tree, ["info", "-p", "/class/net/bonding_masters"])?
  assert plain_file.status == 2
  assert plain_file.stderr == "syspath not found\n"
  let no_device = udevadm(ctx, tree, ["info"])?
  assert no_device.status == 2
  assert no_device.stdout.starts_with("udevadm info [OPTIONS] [DEVPATH|FILE]\n")
}

# `-v` prints the sysfs path of every device a trigger addresses; the write
# that makes the kernel emit the event is the uevent file taking the action.
test test_udevadm_trigger_writes_the_action_to_each_selected_uevent { |ctx|
  let tree = build_tree(ctx)?
  let ran = udevadm(ctx, tree, ["trigger", "-v", "-c", "add", "-s", "block"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == "/sys/devices/pci0000:00/0000:00:1f.2/block/sda\n/sys/devices/pci0000:00/0000:00:1f.2/block/sda/sda1\n"
  assert fp"{tree}/sys/devices/pci0000:00/0000:00:1f.2/block/sda/uevent".read_text()? == "add"
  assert fp"{tree}/sys/devices/pci0000:00/0000:00:1f.2/block/sda/sda1/uevent".read_text()? == "add"
  assert fp"{tree}/sys/devices/virtual/net/lo/uevent".read_text()? == "INTERFACE=lo\nIFINDEX=1\n"
}

test test_udevadm_trigger_default_action_is_change_and_dry_run_writes_nothing { |ctx|
  let tree = build_tree(ctx)?
  let dry = udevadm(ctx, tree, ["trigger", "-n", "-v", "-s", "net"])?
  assert dry.status == 0, dry.stderr
  assert dry.stdout == "/sys/devices/virtual/net/lo\n"
  assert fp"{tree}/sys/devices/virtual/net/lo/uevent".read_text()? == "INTERFACE=lo\nIFINDEX=1\n"
  let real = udevadm(ctx, tree, ["trigger", "-s", "net"])?
  assert real.status == 0, real.stderr
  assert real.stdout == ""
  assert fp"{tree}/sys/devices/virtual/net/lo/uevent".read_text()? == "change"
}

# Lists the devices a dry-run trigger with `args` would address.
proc listed(ctx: TestContext, tree: Path, args: List[Str]) [fs, process, error] -> Result[List[Str]] {
  let ran = udevadm(ctx, tree, ["trigger", "-v", "-n"].extend(args))?
  assert ran.status == 0, ran.stderr
  Ok(ran.stdout.lines().collect())
}

test test_udevadm_trigger_match_options_combine_as_eudev_does { |ctx|
  let tree = build_tree(ctx)?
  let sda = "/sys/devices/pci0000:00/0000:00:1f.2/block/sda"
  let sda1 = "/sys/devices/pci0000:00/0000:00:1f.2/block/sda/sda1"
  let pci = "/sys/devices/pci0000:00/0000:00:1f.2"
  assert listed(ctx, tree, ["-s", "net", "-s", "mem"])? == ["/sys/devices/virtual/mem/null", "/sys/devices/virtual/net/lo"]
  assert listed(ctx, tree, ["-s", "b*"])? == [sda, sda1]
  assert listed(ctx, tree, ["-S", "block", "-S", "net", "-S", "mem"])? == [pci]
  assert listed(ctx, tree, ["-s", "block", "-S", "block"])? == []
  assert listed(ctx, tree, ["-a", "size=41943040"])? == [sda]
  assert listed(ctx, tree, ["-a", "partition"])? == [sda1]
  assert listed(ctx, tree, ["-A", "partition", "-s", "block"])? == [sda]
  assert listed(ctx, tree, ["-a", "size", "-a", "ro=1"])? == []
  assert listed(ctx, tree, ["-p", "MAJOR=8", "-p", "MAJOR=1"])? == [sda, sda1, "/sys/devices/virtual/mem/null"]
  assert listed(ctx, tree, ["-p", "PARTN=*"])? == [sda1]
  assert listed(ctx, tree, ["-y", "sda"])? == [sda]
  assert listed(ctx, tree, ["-y", "sda", "-y", "lo"])? == [sda, "/sys/devices/virtual/net/lo"]
  assert listed(ctx, tree, ["-g", "systemd"])? == [sda1]
  assert listed(ctx, tree, ["-b", "/devices/pci0000:00/0000:00:1f.2/block/sda"])? == [sda, sda1]
  assert listed(ctx, tree, ["/sys/class/net/lo"])? == ["/sys/devices/virtual/net/lo"]
  assert listed(ctx, tree, ["--name-match=mynull"])? == ["/sys/devices/virtual/mem/null"]
}

test test_udevadm_trigger_subsystems_type_addresses_bus_and_driver_directories { |ctx|
  let tree = build_tree(ctx)?
  let ran = udevadm(ctx, tree, ["trigger", "-n", "-v", "-t", "subsystems"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == "/sys/bus/pci\n/sys/bus/pci/drivers/ahci\n"
  let drivers = udevadm(ctx, tree, ["trigger", "-n", "-v", "-t", "subsystems", "-s", "drivers"])?
  assert drivers.stdout == "/sys/bus/pci/drivers/ahci\n"
  # Attribute and property matches describe devices, so combining them with
  # subsystem triggers is refused instead of ignored.
  let refused = udevadm(ctx, tree, ["trigger", "-t", "subsystems", "-a", "size"])?
  assert refused.status == 1
  assert "only --subsystem-match and --subsystem-nomatch apply" in refused.stderr
}

test test_udevadm_trigger_rejects_unknown_values_before_writing { |ctx|
  let tree = build_tree(ctx)?
  let kind = udevadm(ctx, tree, ["trigger", "-t", "bogus"])?
  assert kind.status == 2
  assert kind.stderr == "unknown type --type=bogus\n"
  let action = udevadm(ctx, tree, ["trigger", "-c", "bogus"])?
  assert action.status == 2
  assert action.stderr == "unknown action 'bogus'\n"
  let property = udevadm(ctx, tree, ["trigger", "-p", "MAJOR"])?
  assert property.status == 1
  let device = udevadm(ctx, tree, ["trigger", "-b", "nosuch"])?
  assert device.status == 2
  assert device.stderr == "unable to open the device 'nosuch'\n"
  assert fp"{tree}/sys/devices/virtual/net/lo/uevent".read_text()? == "INTERFACE=lo\nIFINDEX=1\n"
}

# `settle` waits while `run/udev/queue` exists. The control marker stands in
# for a daemon: a privileged caller that finds no daemon has nothing to wait for.
test test_udevadm_settle_waits_for_queue_until_timeout_or_marker { |ctx|
  let tree = build_tree(ctx)?
  fp"{tree}/run/udev/control".write("")
  let idle = udevadm(ctx, tree, ["settle", "-t", "1"])?
  assert idle.status == 0, idle.stderr
  fp"{tree}/run/udev/queue".write("")
  let timed_out = udevadm(ctx, tree, ["settle", "-t", "0"])?
  assert timed_out.status == 1
  assert timed_out.stdout == "" and timed_out.stderr == ""
  let marker = fp"{tree}/stop-waiting"
  marker.write("")
  let stopped = udevadm(ctx, tree, ["settle", "-t", "5", "-E", marker.display()])?
  assert stopped.status == 0, stopped.stderr
  fp"{tree}/run/udev/queue".remove()?
  assert udevadm(ctx, tree, ["settle", "-t", "5"])?.status == 0
}

test test_udevadm_settle_rejects_bad_timeout_and_extra_operands { |ctx|
  let tree = build_tree(ctx)?
  let text = udevadm(ctx, tree, ["settle", "-t", "soon"])?
  assert text.status == 1
  assert text.stderr == "Invalid timeout value 'soon': Invalid argument\n"
  let negative = udevadm(ctx, tree, ["settle", "-t", "-3"])?
  assert negative.status == 1
  assert "Invalid timeout value '-3'" in negative.stderr
  let extra = udevadm(ctx, tree, ["settle", "now"])?
  assert extra.status == 1
  assert extra.stderr == "Extraneous argument: 'now'\n"
}

test test_udevadm_monitor_prints_kernel_events_from_the_uevent_stream { |ctx|
  let source = fp"{ctx.core_dir}/udevadm.xsh".read_text()?
  test.linux_fake(ctx, {})
  let plain = test.run_script(ctx, source, ["monitor"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "udevadm")?
  assert plain.success, plain.stderr
  let lines = plain.stdout.lines().collect()
  assert lines[0] == "monitor will print the received events for:"
  assert lines[1] == "KERNEL - the kernel uevent"
  assert lines[2] == ""
  assert lines[3].starts_with("KERNEL[")
  assert lines[3].ends_with("] add      /devices/virtual/block/sda (block)")

  let detailed = test.run_script(ctx, source, ["monitor", "-k", "-p"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "udevadm")?
  assert detailed.success, detailed.stderr
  assert detailed.stdout.ends_with("ACTION=add\nDEVNAME=sda\nDEVPATH=/devices/virtual/block/sda\nSUBSYSTEM=block\n\n")

  let filtered = test.run_script(ctx, source, ["monitor", "-s", "net"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "udevadm")?
  assert filtered.stdout == "monitor will print the received events for:\nKERNEL - the kernel uevent\n\n"
  let matched = test.run_script(ctx, source, ["monitor", "-s", "block"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "udevadm")?
  assert "(block)" in matched.stdout
}

test test_udevadm_monitor_and_control_refuse_what_needs_a_daemon { |ctx|
  let tree = build_tree(ctx)?
  for args in [["monitor", "-u"], ["monitor", "--udev"], ["monitor", "-t", "x"], ["monitor", "--tag-match=x"]] {
    let ran = udevadm(ctx, tree, args)?
    assert ran.status == 1, args.join(" ")
    assert "is not supported" in ran.stderr, args.join(" ")
    assert ran.stdout == ""
  }

  for args in [["control", "--reload"], ["control", "-R"], ["control", "--log-level=3"], ["control", "-l", "3"], ["control", "--exit"], ["control", "-s"], ["control", "-S"], ["control", "-p", "A=B"], ["control", "-m", "2"], ["control", "--timeout=3"]] {
    let ran = udevadm(ctx, tree, args)?
    assert ran.status == 1, args.join(" ")
    assert "is not supported: there is no udev daemon to control" in ran.stderr, args.join(" ")
  }

  let bare = udevadm(ctx, tree, ["control"])?
  assert bare.status == 1
  assert bare.stderr == "Option missing\n"
}

test test_udevadm_top_level_commands_help_version_and_unsupported { |ctx|
  let tree = build_tree(ctx)?
  assert udevadm(ctx, tree, ["version"])?.stdout == "251\n"
  assert udevadm(ctx, tree, ["--version"])?.stdout == "251\n"
  assert udevadm(ctx, tree, ["info", "--version"])?.stdout == "251\n"
  let help = udevadm(ctx, tree, ["--help"])?
  assert help.status == 0
  assert help.stdout.starts_with("udevadm [--help] [--version] [--debug] COMMAND [COMMAND OPTIONS]\n")
  assert "  test-builtin  Test a built-in command\n" in help.stdout
  let debug = udevadm(ctx, tree, ["--debug", "version"])?
  assert debug.stderr == "calling: version\n"
  let none = udevadm(ctx, tree, [])?
  assert none.status == 2
  assert none.stderr == "udevadm: missing or unknown command\n"
  let unknown = udevadm(ctx, tree, ["frobnicate"])?
  assert unknown.status == 2
  for command in ["hwdb", "test", "test-builtin"] {
    let ran = udevadm(ctx, tree, [command])?
    assert ran.status == 1, command
    assert "is not supported" in ran.stderr, command
  }
}
