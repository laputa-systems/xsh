const format_version = 1
const retry_delays = [100ms, 500ms, 1s]
const header = {version: format_version, delays: retry_delays}
var configured = header
configured.delays += [2s]
print $format_version
print header.delays.len()
print configured.delays.len()
