test test_linux_ipc_name_bytes_reject_embedded_nul_without_truncation {
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux IPC"); return }
  for result in [
    linux.posix_semaphore_value(b"missing\0suffix"),
    linux.posix_message_queue_messages(b"/missing\0suffix"),
  ] {
    match result {
      Err(failure) => assert failure.errno == 22
      Ok(_) => assert false, "an IPC name containing NUL must not address its prefix"
    }
  }
}

test test_linux_system_v_semaphore_element_rejects_unrepresentable_ids_and_indices {
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux IPC"); return }
  for result in [
    linux.system_v_semaphore_element(-1, 0),
    linux.system_v_semaphore_element(0, -1),
    linux.system_v_semaphore_element(2147483648, 0),
    linux.system_v_semaphore_element(0, 2147483648),
  ] {
    match result {
      Err(failure) => assert failure.errno == 22
      Ok(_) => assert false, "invalid native IPC integers must not wrap"
    }
  }
}

test test_linux_ipc_fake_does_not_invent_live_resource_values { |ctx|
  test.linux_fake(ctx, {})
  test.error_kind(linux.posix_semaphore_value(b"xsh"), "linux-ipc-observation")
  test.error_kind(linux.posix_message_queue_messages(b"/xsh"), "linux-ipc-observation")
  test.error_kind(linux.system_v_semaphore_element(0, 0), "linux-ipc-observation")
}
