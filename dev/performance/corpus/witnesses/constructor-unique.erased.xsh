error ProbeError = InvalidCpuList

pure cpu_list_error(message: Str) {
  .InvalidCpuList(message)
}

print cpu_list_error("invalid CPU list").message
