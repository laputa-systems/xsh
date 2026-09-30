test test_ip_addr_smoke [process, env, error] { |ctx|
  let output = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- addr ?
  (output.count_chars() >= 0)
}

test test_ip_route_list_patterns_accept_exact_command_forms [process, env, error] { |ctx|
  let route = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- route ?
  let explicit = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- route show ?
  route == explicit
}

test test_ip_addr_alternatives_accept_address_show_and_device_forms [process, env, error] { |ctx|
  let short = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- addr ?
  for argv in [["address"], ["addr", "show"], ["address", "show"]] {
    let output = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- @argv ?
    output == short
  }
  let device = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- addr dev eth0 ?
  for argv in [["address", "dev", "eth0"], ["addr", "show", "dev", "eth0"], ["address", "show", "dev", "eth0"]] {
    let output = run.text XSH_LINUX_DRY_RUN=1 ${ctx.xsh_bin} fp"${ctx.core_dir}/ip.xsh" -- @argv ?
    output == device
  }
}
