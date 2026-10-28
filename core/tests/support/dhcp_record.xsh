##! Test support: records the frames of one interface into a pcap file until
##! it is killed. Arguments: interface, output file.
use tests.support.dhcp_pcap as pcap

proc main(...argv: List[Str]) [fs, process, net, time, error] {
  pcap.record_interface(argv[0], fp"{argv[1]}")
}
