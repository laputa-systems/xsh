use core.lib.sys_ipc as ipc

test test_sys_ipc_system_v_schema_and_partial_fields {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc/sysvipc", parents: true)
  root.write(p"proc/sysvipc/msg", "key msqid perms cbytes qnum lspid lrpid uid gid cuid cgid stime rtime ctime\n-1 7 1600 42 invalid 13 14 10 11 12 15 100 0 200\n0 8 bad 0 1 0 0 10 11 12 15 0 0 0\nshort row\n")
  let records = ipc.collect_system_v(ipc.MessageQueues, root.host_path()?)
  assert records.len() == 2
  assert records[0].id == 7
  let identity = records[0].identity
  assert identity != null
  assert identity.key == -1 and identity.mode == 0o1600
  assert identity.uid == 10 and identity.gid == 11 and identity.creator_uid == 12 and identity.creator_gid == 15
  match records[0].details {
    ipc.Queue(queue) => {
      assert queue.used_bytes == 42 and queue.messages == null
      assert queue.sender_pid == 13 and queue.receiver_pid == 14
      assert queue.send_seconds == 100 and queue.receive_seconds == 0 and queue.change_seconds == 200
    }
    else => assert false
  }
  assert records[1].id == 8 and records[1].identity == null
}

test test_sys_ipc_shared_memory_and_semaphore_abi_fields {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc/sysvipc", parents: true)
  root.write(p"proc/sysvipc/shm", "key shmid perms size cpid lpid nattch uid gid cuid cgid atime dtime ctime\n0 21 600 8192 31 32 2 10 11 12 13 100 200 300\n")
  root.write(p"proc/sysvipc/sem", "key semid perms nsems uid gid cuid cgid otime ctime\n0 22 640 3 10 11 12 13 400 500\n")
  let memory = ipc.collect_system_v(ipc.MemorySegments, root.host_path()?)[0]
  match memory.details {
    ipc.SharedMemory(segment) => {
      assert segment.size_bytes == 8192 and segment.creator_pid == 31 and segment.last_pid == 32
      assert segment.attachments == 2 and segment.attach_seconds == 100 and segment.detach_seconds == 200 and segment.change_seconds == 300
    }
    else => assert false
  }
  let semaphore = ipc.collect_system_v(ipc.SemaphoreSets, root.host_path()?)[0]
  match semaphore.details {
    ipc.SemaphoreSet(semaphores) => {
      assert semaphores.elements == 3 and semaphores.operation_seconds == 400 and semaphores.change_seconds == 500
    }
    else => assert false
  }
}

test test_sys_ipc_limits_preserve_raw_text_and_native_read_errors {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc/sys/kernel", parents: true)
  root.write(p"proc/sys/kernel/shmmax", "18446744073692774399\n")
  root.write(p"proc/sys/kernel/sem", "32000\t1024000000 500 32000\n")
  assert ipc.read_limit_text(ipc.SharedMemoryBytes, root.host_path()?) == "18446744073692774399"
  for spelling in ["0", "00042", "9223372036854775808", "18446744073709551615"] {
    root.write(p"proc/sys/kernel/shmmax", f"{spelling}\n")
    assert ipc.read_limit_text(ipc.SharedMemoryBytes, root.host_path()?) == spelling
  }
  for raw in ["", "-1", "+1", "0x10", "18446744073709551616", "1 2", "bad"] {
    root.write(p"proc/sys/kernel/shmmax", f"  {raw}\n")
    let original = fp"{root.host_path()?}/proc/sys/kernel/shmmax".read_text()?.trim()
    assert ipc.read_limit_text(ipc.SharedMemoryBytes, root.host_path()?) == original
  }
  root.write(p"proc/sys/kernel/msgmni", "17\n")
  assert ipc.read_limit_text(ipc.MessageQueueCount, root.host_path()?) == "17"
  root.write(p"proc/sys/kernel/msgmni", "18\n")
  assert ipc.read_limit_text(ipc.MessageQueueCount, root.host_path()?) == "18"
  match try { ipc.read_limit_text(ipc.MessageBytes, root.host_path()?) } {
    Err(failure) => assert failure.errno == 2
    Ok(_) => assert false
  }
  let limits = ipc.read_semaphore_limits(root.host_path()?)
  assert limits.elements_per_set == 32000 and limits.total_elements == 1024000000
  assert limits.operations_per_call == 500 and limits.sets == 32000
  let missing = try { ipc.collect_system_v(ipc.MessageQueues, root.host_path()?) }
  match missing {
    Err(failure) => assert failure.errno == 2
    Ok(_) => assert false
  }
  root.write(p"proc/sys/kernel/sem", "bad 2 3 4\n")
  assert try { ipc.read_semaphore_limits(root.host_path()?) } is Err(_)
}

test test_sys_ipc_process_arguments_preserve_native_bytes_and_read_failure {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc/41", parents: true)
  root.write(p"proc/41/cmdline", b"keeper\0\xff\0tail")
  assert ipc.read_process_arguments(41, root.host_path()?) == b"keeper\0\xff\0tail"
  match try { ipc.read_process_arguments(42, root.host_path()?) } {
    Err(failure) => assert failure.errno == 2
    Ok(_) => assert false
  }
}

test test_sys_ipc_posix_shared_memory_separates_semaphore_names_and_retains_metadata {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"dev/shm", parents: true)
  root.write(p"dev/shm/segment", "contents")
  root.write(p"dev/shm/sem.other", "excluded")
  let observations = ipc.collect_posix(ipc.PosixMemory, root.host_path()?)
  assert observations.len() == 1
  let segment = observations[0]?
  assert segment.name == "segment"
  let stat = fs.stat(fp"{root.host_path()?}/dev/shm/segment")?
  assert segment.uid == stat.uid and segment.gid == stat.gid and segment.mode == stat.mode
  assert segment.modified_ns == stat.mtime_ns
  match segment.details {
    ipc.PosixSharedMemory(size_bytes) => assert size_bytes == 8
    else => assert false
  }
  assert !ipc.posix_directory_available(ipc.PosixQueues, root.host_path()?)
  assert try { ipc.collect_posix(ipc.PosixQueues, root.host_path()?) } is Err(_)
}


test test_sys_ipc_posix_queue_failures_remain_per_resource {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"dev/mqueue", parents: true)
  root.write(p"dev/mqueue/not-a-queue", "not a queue status")
  root.write(p"dev/mqueue/invalid-size", "QSIZE: invalid NOTIFY:0 SIGNO:0 NOTIFY_PID:0\n")
  let observations = ipc.collect_posix(ipc.PosixQueues, root.host_path()?)
  assert observations.len() == 2
  assert observations[0] is Err(_) and observations[1] is Err(_)
}
