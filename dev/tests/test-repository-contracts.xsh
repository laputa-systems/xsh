proc check_repository_contract(script: Str, args: List[Str] = []) [process, error] {
  let command = process.command_argv("python3", ["python3", script] + args)
  let status = process.run(command)?
  assert status.exited_with(0), f"repository contract failed: {script}"
}

test test_core_options_do_not_gain_discard_buckets {
  check_repository_contract("dev/checks/check_ignored_options.py")
}

test test_core_kernel_state_uses_typed_domain_apis {
  check_repository_contract("dev/checks/check_kernel_reads.py")
}

test test_native_origin_exclusions_are_exact_and_explained {
  check_repository_contract("dev/checks/check_exclusions.py")
}

test test_command_inventory_matches_sources_and_pinned_scope {
  check_repository_contract("dev/checks/check_inventory.py", ["--check"])
}

test test_native_origins_cover_the_frozen_behavior {
  check_repository_contract("core/tests/origins/check_origins.py", ["--strict"])
}
