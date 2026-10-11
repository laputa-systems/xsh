use super::{
    Evaluator, Arc, BTreeMap, ControlFlow, DryRunUeventStream, LoweredValue, NativeArgumentValues,
    PathBuf, PathValue, RecordMap, RuntimeError, RuntimeOp, Span, StreamValue, Value,
    linux_dry_run_partition_table, linux_file_attrs_record, linux_module, lowered_bool_arg_or,
    lowered_int_arg, lowered_int_arg_or, lowered_int_list_arg, lowered_optional_str_list,
    lowered_path_arg, lowered_path_list, lowered_record_arg, lowered_result_ok,
    lowered_runtime_result, lowered_runtime_value, lowered_str_arg_owned,
    lowered_stream_from_values, lowered_value_from_runtime_any, module_error, module_io_error,
    path_value_from_pathbuf, process_module, unix_require_arg, validate_linux_file_attrs_flags,
    validate_linux_file_version, validate_linux_sysctl_key, validate_mknod_args,
};

impl Evaluator {
    pub(super) fn eval_lowered_linux_interfaces_values(
        &mut self, _values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            if !self.linux_fake_active() {
                lowered_runtime_result(linux_module::interfaces(span), span)?
            } else {
                self.linux_fake_log("interfaces", &[], span)?;
                lowered_result_ok(lowered_stream_from_values(vec![LoweredValue::Record(
                    Arc::new(BTreeMap::from([
                        (Arc::from("name"), LoweredValue::Str("eth0".into())),
                        (
                            Arc::from("flags"),
                            LoweredValue::List(vec![
                                LoweredValue::Str("UP".into()),
                                LoweredValue::Str("BROADCAST".into()),
                                LoweredValue::Str("RUNNING".into()),
                            ]),
                        ),
                        (Arc::from("mtu"), LoweredValue::Int(1500)),
                        (
                            Arc::from("mac"),
                            LoweredValue::Str("02:00:00:00:00:01".into()),
                        ),
                        (
                            Arc::from("addresses"),
                            LoweredValue::List(vec![LoweredValue::Record(Arc::new(
                                BTreeMap::from([
                                    (Arc::from("family"), LoweredValue::Str("inet".into())),
                                    (Arc::from("addr"), LoweredValue::Str("192.0.2.10".into())),
                                    (Arc::from("prefix_len"), LoweredValue::Int(24)),
                                ]),
                            ))]),
                        ),
                    ])),
                )]))
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_linux_routes_values(
        &mut self, _values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            if !self.linux_fake_active() {
                lowered_runtime_result(linux_module::routes(span), span)?
            } else {
                self.linux_fake_log("routes", &[], span)?;
                lowered_result_ok(lowered_stream_from_values(vec![LoweredValue::Record(
                    Arc::new(BTreeMap::from([
                        (Arc::from("family"), LoweredValue::Str("inet".into())),
                        (Arc::from("dst"), LoweredValue::Str("default".into())),
                        (Arc::from("prefix_len"), LoweredValue::Int(0)),
                        (Arc::from("gateway"), LoweredValue::Str("192.0.2.1".into())),
                        (Arc::from("dev"), LoweredValue::Str("eth0".into())),
                        (Arc::from("metric"), LoweredValue::Int(100)),
                        (
                            Arc::from("flags"),
                            LoweredValue::List(vec![
                                LoweredValue::Str("UP".into()),
                                LoweredValue::Str("GATEWAY".into()),
                            ]),
                        ),
                    ])),
                )]))
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_linux_network_dump_values(
        &mut self, _values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            if !self.linux_fake_active() {
                lowered_runtime_result(linux_module::network_dump(span), span)?
            } else {
                self.linux_fake_log("network_dump", &[], span)?;
                let link = LoweredValue::Record(Arc::new(BTreeMap::from([
                    (Arc::from("ifindex"), LoweredValue::Int(2)),
                    (Arc::from("name"), LoweredValue::Str("eth0".into())),
                    (
                        Arc::from("name_bytes"),
                        LoweredValue::Bytes(b"eth0\0".to_vec().into()),
                    ),
                    (Arc::from("hardware_type"), LoweredValue::Int(1)),
                    (Arc::from("flags"), LoweredValue::Int(1)),
                    (Arc::from("mtu"), LoweredValue::Int(1500)),
                    (
                        Arc::from("address"),
                        LoweredValue::Bytes(vec![2, 0, 0, 0, 0, 1].into()),
                    ),
                    (Arc::from("broadcast"), LoweredValue::Null),
                    (Arc::from("master_ifindex"), LoweredValue::Null),
                    (Arc::from("lower_ifindex"), LoweredValue::Null),
                    (Arc::from("operstate"), LoweredValue::Int(6)),
                    (Arc::from("kind"), LoweredValue::Null),
                    (Arc::from("rx_bytes"), LoweredValue::Int(4096)),
                    (Arc::from("tx_bytes"), LoweredValue::Int(2048)),
                    (Arc::from("attributes"), LoweredValue::List(Vec::new())),
                ])));
                let address = LoweredValue::Record(Arc::new(BTreeMap::from([
                    (Arc::from("ifindex"), LoweredValue::Int(2)),
                    (Arc::from("family"), LoweredValue::Str("inet".into())),
                    (Arc::from("prefix_length"), LoweredValue::Int(24)),
                    (Arc::from("scope"), LoweredValue::Int(0)),
                    (Arc::from("flags"), LoweredValue::Int(0)),
                    (Arc::from("address"), LoweredValue::Str("192.0.2.10".into())),
                    (Arc::from("local"), LoweredValue::Str("192.0.2.10".into())),
                    (
                        Arc::from("broadcast"),
                        LoweredValue::Str("192.0.2.255".into()),
                    ),
                    (Arc::from("label"), LoweredValue::Str("eth0".into())),
                    (
                        Arc::from("preferred_lifetime_seconds"),
                        LoweredValue::Int(4_294_967_295),
                    ),
                    (
                        Arc::from("valid_lifetime_seconds"),
                        LoweredValue::Int(4_294_967_295),
                    ),
                    (Arc::from("attributes"), LoweredValue::List(Vec::new())),
                ])));
                let route = LoweredValue::Record(Arc::new(BTreeMap::from([
                    (Arc::from("family"), LoweredValue::Str("inet".into())),
                    (Arc::from("destination_prefix_length"), LoweredValue::Int(0)),
                    (Arc::from("source_prefix_length"), LoweredValue::Int(0)),
                    (
                        Arc::from("destination"),
                        LoweredValue::Str("0.0.0.0".into()),
                    ),
                    (Arc::from("source"), LoweredValue::Null),
                    (Arc::from("gateway"), LoweredValue::Str("192.0.2.1".into())),
                    (Arc::from("preferred_source"), LoweredValue::Null),
                    (Arc::from("output_ifindex"), LoweredValue::Int(2)),
                    (Arc::from("input_ifindex"), LoweredValue::Null),
                    (Arc::from("table"), LoweredValue::Int(254)),
                    (Arc::from("priority"), LoweredValue::Int(100)),
                    (Arc::from("route_type"), LoweredValue::Int(1)),
                    (Arc::from("protocol"), LoweredValue::Int(3)),
                    (Arc::from("scope"), LoweredValue::Int(0)),
                    (Arc::from("flags"), LoweredValue::Int(0)),
                    (Arc::from("nexthops"), LoweredValue::List(Vec::new())),
                    (Arc::from("attributes"), LoweredValue::List(Vec::new())),
                ])));
                let rule = LoweredValue::Record(Arc::new(BTreeMap::from([
                    (Arc::from("family"), LoweredValue::Str("inet".into())),
                    (Arc::from("destination_prefix_length"), LoweredValue::Int(0)),
                    (Arc::from("source_prefix_length"), LoweredValue::Int(0)),
                    (Arc::from("destination"), LoweredValue::Null),
                    (Arc::from("source"), LoweredValue::Null),
                    (Arc::from("input_name"), LoweredValue::Null),
                    (Arc::from("output_name"), LoweredValue::Null),
                    (Arc::from("priority"), LoweredValue::Int(32766)),
                    (Arc::from("table"), LoweredValue::Int(254)),
                    (Arc::from("fwmark"), LoweredValue::Null),
                    (Arc::from("fwmask"), LoweredValue::Null),
                    (Arc::from("action"), LoweredValue::Int(1)),
                    (Arc::from("flags"), LoweredValue::Int(0)),
                    (Arc::from("attributes"), LoweredValue::List(Vec::new())),
                ])));
                lowered_result_ok(LoweredValue::Record(Arc::new(BTreeMap::from([
                    (Arc::from("state"), LoweredValue::Str("complete".into())),
                    (Arc::from("links"), LoweredValue::List(vec![link])),
                    (Arc::from("addresses"), LoweredValue::List(vec![address])),
                    (Arc::from("routes"), LoweredValue::List(vec![route])),
                    (Arc::from("rules"), LoweredValue::List(vec![rule])),
                    (Arc::from("issues"), LoweredValue::List(Vec::new())),
                ]))))
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_linux_set_ipv4_address_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let netmask = lowered_str_arg_owned(
                values.get(2).cloned(),
                "",
                "linux.set_ipv4_address",
                span,
            )?;
            let address = lowered_str_arg_owned(
                values.get(1).cloned(),
                "",
                "linux.set_ipv4_address",
                span,
            )?;
            let interface = lowered_str_arg_owned(
                values.first().cloned(),
                "",
                "linux.set_ipv4_address",
                span,
            )?;
            if !self.linux_fake_active() {
                lowered_runtime_result(
                    linux_module::set_ipv4_address(&interface, &address, &netmask, span),
                    span,
                )?
            } else {
                self.linux_fake_log(
                    "set_ipv4_address",
                    &[
                        ("interface", interface),
                        ("address", address),
                        ("netmask", netmask),
                    ],
                    span,
                )?;
                lowered_result_ok(LoweredValue::Unit)
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_linux_add_default_ipv4_route_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let interface = lowered_str_arg_owned(
                values.get(1).cloned(),
                "",
                "linux.add_default_ipv4_route",
                span,
            )?;
            let gateway = lowered_str_arg_owned(
                values.first().cloned(),
                "",
                "linux.add_default_ipv4_route",
                span,
            )?;
            if !self.linux_fake_active() {
                lowered_runtime_result(
                    linux_module::add_default_ipv4_route(&gateway, &interface, span),
                    span,
                )?
            } else {
                self.linux_fake_log(
                    "add_default_ipv4_route",
                    &[("gateway", gateway), ("interface", interface)],
                    span,
                )?;
                lowered_result_ok(LoweredValue::Unit)
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_linux_del_default_ipv4_route_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let interface = lowered_str_arg_owned(
                values.get(1).cloned(),
                "",
                "linux.del_default_ipv4_route",
                span,
            )?;
            let gateway = lowered_str_arg_owned(
                values.first().cloned(),
                "",
                "linux.del_default_ipv4_route",
                span,
            )?;
            if !self.linux_fake_active() {
                lowered_runtime_result(
                    linux_module::del_default_ipv4_route(&gateway, &interface, span),
                    span,
                )?
            } else {
                self.linux_fake_log(
                    "del_default_ipv4_route",
                    &[("gateway", gateway), ("interface", interface)],
                    span,
                )?;
                lowered_result_ok(LoweredValue::Unit)
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_linux_dhcp_send_release_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let server_id = lowered_str_arg_owned(
                values.get(2).cloned(),
                "",
                "linux.dhcp_send_release",
                span,
            )?;
            let address = lowered_str_arg_owned(
                values.get(1).cloned(),
                "",
                "linux.dhcp_send_release",
                span,
            )?;
            let interface = lowered_str_arg_owned(
                values.first().cloned(),
                "",
                "linux.dhcp_send_release",
                span,
            )?;
            if !self.linux_fake_active() {
                lowered_runtime_result(
                    linux_module::dhcp_send_release(&interface, &address, &server_id, span),
                    span,
                )?
            } else {
                self.linux_fake_log(
                    "dhcp_send_release",
                    &[
                        ("interface", interface),
                        ("address", address),
                        ("server_id", server_id),
                    ],
                    span,
                )?;
                lowered_result_ok(LoweredValue::Unit)
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_linux_write_device_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            if !self.linux_fake_active() {
                let device = lowered_path_arg(values.remove(0), "linux.write_device", span)?;
                let source = lowered_path_arg(values.remove(0), "linux.write_device", span)?;
                let host_device = self.host_path(&device);
                let host_source = self.host_path(&source);
                lowered_runtime_result(
                    linux_module::write_device(&host_device, &host_source, span),
                    span,
                )?
            } else {
                let device = lowered_path_arg(values.remove(0), "linux.write_device", span)?;
                let source = lowered_path_arg(values.remove(0), "linux.write_device", span)?;
                self.linux_fake_log(
                    "write_device",
                    &[("device", device.display()), ("source", source.display())],
                    span,
                )?;
                lowered_result_ok(LoweredValue::Unit)
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_linux_read_device_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            if !self.linux_fake_active() {
                let device = lowered_path_arg(values.remove(0), "linux.read_device", span)?;
                let dest = lowered_path_arg(values.remove(0), "linux.read_device", span)?;
                let host_device = self.host_path(&device);
                let host_dest = self.host_path(&dest);
                let bytes = lowered_int_arg(Some(values.remove(0)), "linux.read_device", span)?;
                lowered_runtime_result(
                    linux_module::read_device(&host_device, &host_dest, bytes, span),
                    span,
                )?
            } else {
                let device = lowered_path_arg(values.remove(0), "linux.read_device", span)?;
                let dest = lowered_path_arg(values.remove(0), "linux.read_device", span)?;
                let bytes = lowered_int_arg(Some(values.remove(0)), "linux.read_device", span)?;
                if !(0..=1024 * 1024).contains(&bytes) {
                    return Ok(ControlFlow::Continue(lowered_runtime_value(
                        module_error(
                            "linux-read-device",
                            "bytes must be between 0 and 1048576 under the linux fake",
                            span,
                        ),
                        span,
                    )?));
                }
                let host_dest = self.host_path(&dest);
                match std::fs::write(host_dest, vec![0_u8; bytes as usize]) {
                    Ok(()) => {
                        self.linux_fake_log(
                            "read_device",
                            &[
                                ("device", device.display()),
                                ("dest", dest.display()),
                                ("bytes", bytes.to_string()),
                            ],
                            span,
                        )?;
                        lowered_result_ok(LoweredValue::Unit)
                    }
                    Err(error) => lowered_runtime_value(
                        module_io_error("linux-read-device", error, span),
                        span,
                    )?,
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    fn linux_dry_run_file_attrs_flags(&self, span: Span) -> Result<i64, RuntimeError> {
        let value = self.linux_fake_value("file_attrs_flags", "48");
        let flags = value.parse::<i64>().map_err(|_| {
            RuntimeError::new("linux-file-attrs", "invalid linux fake file_attrs_flags")
                .with_span(span)
        })?;
        validate_linux_file_attrs_flags(flags, span)?;
        Ok(flags)
    }

    fn linux_dry_run_file_version(&self, span: Span) -> Result<i64, RuntimeError> {
        let value = self.linux_fake_value("file_version", "0");
        let version = value.parse::<i64>().map_err(|_| {
            RuntimeError::new("linux-file-version", "invalid linux fake file_version")
                .with_span(span)
        })?;
        validate_linux_file_version(version, span)?;
        Ok(version)
    }

    pub(super) fn eval_lowered_linux_call(
        &mut self,
        op: RuntimeOp,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let out = self.eval_linux_call_value(op, values, span)?;
        lowered_value_from_runtime_any(&out).ok_or_else(|| {
            RuntimeError::new(
                "type-error",
                format!("cannot lower linux result {}", out.type_name()),
            )
            .with_span(span)
        })
    }

    pub(super) fn eval_linux_call_value(
        &mut self,
        op: RuntimeOp,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        if !self.linux_fake_active() {
            return match op {
                RuntimeOp::LinuxRootDevice => linux_module::root_device(span),
                RuntimeOp::LinuxMemInfo => linux_module::meminfo(span),
                RuntimeOp::LinuxModules => linux_module::modules(span),
                RuntimeOp::LinuxDmesg => linux_module::dmesg(span),
                RuntimeOp::LinuxIsMountpoint => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.is_mountpoint", span)?,
                        "linux.is_mountpoint",
                        span,
                    )?;
                    let host_path = self.host_path(&path);
                    linux_module::is_mountpoint(&host_path, span)
                }
                RuntimeOp::LinuxDiskUsage => {
                    let path = values
                        .first()
                        .cloned()
                        .map(|value| lowered_path_arg(value, "linux.disk_usage", span))
                        .transpose()?;
                    let host_path = path.as_ref().map(|pv| self.host_path(pv));
                    linux_module::disk_usage(host_path.as_deref(), span)
                }
                RuntimeOp::LinuxSysctlGet => {
                    let key = lowered_str_arg_owned(
                        values.first().cloned(),
                        "",
                        "linux.sysctl_get",
                        span,
                    )?;
                    linux_module::sysctl_get(&key, span)
                }
                RuntimeOp::LinuxFileAttrs => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.file_attrs", span)?,
                        "linux.file_attrs",
                        span,
                    )?;
                    let host_path = self.host_path(&path);
                    linux_module::file_attrs(&host_path, span)
                }
                RuntimeOp::LinuxFileVersion => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.file_version", span)?,
                        "linux.file_version",
                        span,
                    )?;
                    let host_path = self.host_path(&path);
                    linux_module::file_version(&host_path, span)
                }
                RuntimeOp::LinuxLoopList => linux_module::loop_list(span),
                RuntimeOp::LinuxOpenFiles => {
                    let pid = values
                        .first()
                        .cloned()
                        .map(|value| lowered_int_arg(Some(value), "linux.open_files", span))
                        .transpose()?;
                    linux_module::open_files(pid, span)
                }
                RuntimeOp::LinuxBlockDevices => linux_module::block_devices(span),
                RuntimeOp::LinuxBlkid => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.blkid", span)?,
                        "linux.blkid",
                        span,
                    )?;
                    let host_path = self.host_path(&device);
                    linux_module::blkid(&host_path, span)
                }
                RuntimeOp::LinuxModinfo => {
                    let name =
                        lowered_str_arg_owned(values.first().cloned(), "", "linux.modinfo", span)?;
                    linux_module::modinfo(&name, span)
                }
                RuntimeOp::LinuxPartitionTable => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.partition_table", span)?,
                        "linux.partition_table",
                        span,
                    )?;
                    let host_path = self.host_path(&device);
                    linux_module::partition_table(&host_path, span)
                }
                RuntimeOp::LinuxUeventStream => linux_module::uevent_stream(span),
                RuntimeOp::LinuxChroot => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.chroot", span)?,
                        "linux.chroot",
                        span,
                    )?;
                    let host_path = self.host_path(&path);
                    linux_module::chroot(&host_path, span)
                }
                // Boot / privileged operations are not safe in a non-privileged
                // container; return Ok(()) so scripts can feature-gate on errors.
                RuntimeOp::LinuxDepmod => {
                    let version =
                        lowered_str_arg_owned(values.first().cloned(), "", "linux.depmod", span)?;
                    linux_module::depmod(&version, span)
                }
                RuntimeOp::LinuxFsck => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.fsck", span)?,
                        "linux.fsck",
                        span,
                    )?;
                    let fstype =
                        lowered_str_arg_owned(values.get(1).cloned(), "", "linux.fsck", span)?;
                    let repair =
                        lowered_bool_arg_or(values.get(2).cloned(), false, "linux.fsck", span)?;
                    let host_device = self.host_path(&device);
                    linux_module::fsck(&host_device, &fstype, repair, span)
                }
                RuntimeOp::LinuxHalt => linux_module::halt(span),
                RuntimeOp::LinuxHwclock => linux_module::hwclock(span),
                RuntimeOp::LinuxInsmod => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.insmod", span)?,
                        "linux.insmod",
                        span,
                    )?;
                    let params =
                        lowered_str_arg_owned(values.get(1).cloned(), "", "linux.insmod", span)?;
                    let host_path = self.host_path(&path);
                    linux_module::insmod(&host_path, &params, span)
                }
                RuntimeOp::LinuxKillAll => {
                    let signal_str = lowered_str_arg_owned(
                        values.first().cloned(),
                        "TERM",
                        "linux.kill_all",
                        span,
                    )?;
                    let signal = process_module::signal_info(&signal_str, span)?.number;
                    let except_pid1 =
                        lowered_bool_arg_or(values.get(1).cloned(), false, "linux.kill_all", span)?;
                    linux_module::kill_all(signal, except_pid1, span)
                }
                RuntimeOp::LinuxLoopAttach => {
                    let file = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.loop_attach", span)?,
                        "linux.loop_attach",
                        span,
                    )?;
                    let device = values
                        .get(1)
                        .cloned()
                        .map(|value| lowered_path_arg(value, "linux.loop_attach", span))
                        .transpose()?;
                    let host_file = self.host_path(&file);
                    let host_device = device.as_ref().map(|pv| self.host_path(pv));
                    linux_module::loop_attach(&host_file, host_device.as_deref(), span)
                }
                RuntimeOp::LinuxLoopDetach => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.loop_detach", span)?,
                        "linux.loop_detach",
                        span,
                    )?;
                    let host_device = self.host_path(&device);
                    linux_module::loop_detach(&host_device, span)
                }
                RuntimeOp::LinuxMknod => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.mknod", span)?,
                        "linux.mknod",
                        span,
                    )?;
                    let kind =
                        lowered_str_arg_owned(values.get(1).cloned(), "", "linux.mknod", span)?;
                    let major = lowered_int_arg(values.get(2).cloned(), "linux.mknod", span)?;
                    let minor = lowered_int_arg(values.get(3).cloned(), "linux.mknod", span)?;
                    let host_path = self.host_path(&path);
                    linux_module::mknod(&host_path, &kind, major, minor, span)
                }
                RuntimeOp::LinuxMkswap => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.mkswap", span)?,
                        "linux.mkswap",
                        span,
                    )?;
                    let host_device = self.host_path(&device);
                    linux_module::mkswap(&host_device, span)
                }
                RuntimeOp::LinuxBlockSignatures => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.block_signatures", span)?, "linux.block_signatures", span)?;
                    linux_module::block_signatures(&self.host_path(&path), span)
                }
                RuntimeOp::LinuxWipeBlockSignatures => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.wipe_block_signatures", span)?, "linux.wipe_block_signatures", span)?;
                    linux_module::wipe_block_signatures(&self.host_path(&path), &lowered_int_list_arg(values.get(1).cloned(), "linux.wipe_block_signatures", span)?, span)
                }
                RuntimeOp::LinuxFileProject => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.file_project", span)?, "linux.file_project", span)?;
                    linux_module::file_project(&self.host_path(&path), span)
                }
                RuntimeOp::LinuxSetFileProject => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.set_file_project", span)?, "linux.set_file_project", span)?;
                    linux_module::set_file_project(&self.host_path(&path), lowered_int_arg(values.get(1).cloned(), "linux.set_file_project", span)?, span)
                }
                RuntimeOp::LinuxModulePlan => {
                    let name = lowered_str_arg_owned(values.first().cloned(), "", "linux.module_plan", span)?;
                    let params = lowered_str_arg_owned(values.get(1).cloned(), "", "linux.module_plan", span)?;
                    let remove = lowered_bool_arg_or(values.get(2).cloned(), false, "linux.module_plan", span)?;
                    linux_module::module_plan(&name, &params, remove, span)
                }
                RuntimeOp::LinuxUmount => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.umount", span)?, "linux.umount", span)?;
                    linux_module::umount(&self.host_path(&path), lowered_bool_arg_or(values.get(1).cloned(), false, "linux.umount", span)?, lowered_bool_arg_or(values.get(2).cloned(), false, "linux.umount", span)?, span)
                }
                RuntimeOp::LinuxBlockdevInfo => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.blockdev_info", span)?, "linux.blockdev_info", span)?;
                    linux_module::blockdev_info(&self.host_path(&path), span)
                }
                RuntimeOp::LinuxBlockdevSetReadOnly => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.blockdev_set_read_only", span)?, "linux.blockdev_set_read_only", span)?;
                    linux_module::blockdev_set_read_only(&self.host_path(&path), lowered_bool_arg_or(values.get(1).cloned(), false, "linux.blockdev_set_read_only", span)?, span)
                }
                RuntimeOp::LinuxBlockdevFlush => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.blockdev_flush", span)?, "linux.blockdev_flush", span)?;
                    linux_module::blockdev_flush(&self.host_path(&path), span)
                }
                RuntimeOp::LinuxBlockdevRereadPartitionTable => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.blockdev_reread_partition_table", span)?, "linux.blockdev_reread_partition_table", span)?;
                    linux_module::blockdev_reread_partition_table(&self.host_path(&path), span)
                }
                RuntimeOp::LinuxFstrim => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.fstrim", span)?, "linux.fstrim", span)?;
                    linux_module::fstrim(&self.host_path(&path), lowered_int_arg_or(values.get(1).cloned(), 0, "linux.fstrim", span)?, match values.get(2).cloned() { None | Some(LoweredValue::Null) => None, Some(value) => Some(lowered_int_arg(Some(value), "linux.fstrim", span)?) }, lowered_int_arg_or(values.get(3).cloned(), 0, "linux.fstrim", span)?, span)
                }
                RuntimeOp::LinuxFsfreeze => {
                    let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.fsfreeze", span)?, "linux.fsfreeze", span)?;
                    linux_module::fsfreeze(&self.host_path(&path), lowered_bool_arg_or(values.get(1).cloned(), false, "linux.fsfreeze", span)?, span)
                }
            RuntimeOp::LinuxModprobe => {
                    let name =
                        lowered_str_arg_owned(values.first().cloned(), "", "linux.modprobe", span)?;
                    let params =
                        lowered_str_arg_owned(values.get(1).cloned(), "", "linux.modprobe", span)?;
                    linux_module::modprobe(&name, &params, lowered_bool_arg_or(values.get(2).cloned(), false, "linux.modprobe", span)?, span)
                }
                RuntimeOp::LinuxMount => {
                    let source =
                        lowered_str_arg_owned(values.first().cloned(), "", "linux.mount", span)?;
                    let target = lowered_path_arg(
                        unix_require_arg(values.get(1).cloned(), "linux.mount", span)?,
                        "linux.mount",
                        span,
                    )?;
                    let fstype =
                        lowered_str_arg_owned(values.get(2).cloned(), "", "linux.mount", span)?;
                    let options =
                        lowered_optional_str_list(values.get(3).cloned(), "linux.mount", span)?;
                    let host_target = self.host_path(&target);
                    linux_module::mount(&source, &host_target, &fstype, &options, span)
                }
                RuntimeOp::LinuxMountAll => linux_module::mount_all(span),
                RuntimeOp::LinuxPivotRoot => {
                    let new_root = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.pivot_root", span)?,
                        "linux.pivot_root",
                        span,
                    )?;
                    let put_old = lowered_path_arg(
                        unix_require_arg(values.get(1).cloned(), "linux.pivot_root", span)?,
                        "linux.pivot_root",
                        span,
                    )?;
                    let host_new_root = self.host_path(&new_root);
                    let host_put_old = self.host_path(&put_old);
                    linux_module::pivot_root(&host_new_root, &host_put_old, span)
                }
                RuntimeOp::LinuxPoweroff => linux_module::poweroff(span),
                RuntimeOp::LinuxReboot => linux_module::reboot_system(span),
                RuntimeOp::LinuxRfkillBlock => {
                    let id = lowered_int_arg(values.first().cloned(), "linux.rfkill_block", span)?;
                    linux_module::rfkill_set(id, true, span)
                }
                RuntimeOp::LinuxRfkillList => linux_module::rfkill_list(span),
                RuntimeOp::LinuxRfkillUnblock => {
                    let id =
                        lowered_int_arg(values.first().cloned(), "linux.rfkill_unblock", span)?;
                    linux_module::rfkill_set(id, false, span)
                }
                RuntimeOp::LinuxRmmod => {
                    let name =
                        lowered_str_arg_owned(values.first().cloned(), "", "linux.rmmod", span)?;
                    let force =
                        lowered_bool_arg_or(values.get(1).cloned(), false, "linux.rmmod", span)?;
                    linux_module::rmmod(&name, force, span)
                }
                RuntimeOp::LinuxSetFileAttrs => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.set_file_attrs", span)?,
                        "linux.set_file_attrs",
                        span,
                    )?;
                    let flags =
                        lowered_int_arg(values.get(1).cloned(), "linux.set_file_attrs", span)?;
                    let host_path = self.host_path(&path);
                    linux_module::set_file_attrs(&host_path, flags, span)
                }
                RuntimeOp::LinuxSetFileVersion => {
                    let path = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.set_file_version", span)?,
                        "linux.set_file_version",
                        span,
                    )?;
                    let version =
                        lowered_int_arg(values.get(1).cloned(), "linux.set_file_version", span)?;
                    let host_path = self.host_path(&path);
                    linux_module::set_file_version(&host_path, version, span)
                }
                RuntimeOp::LinuxSetHwclock => {
                    let epoch_ms =
                        lowered_int_arg(values.first().cloned(), "linux.set_hwclock", span)?;
                    linux_module::set_hwclock(epoch_ms, span)
                }
                RuntimeOp::LinuxSetSystemClock => {
                    let epoch_ms =
                        lowered_int_arg(values.first().cloned(), "linux.set_system_clock", span)?;
                    linux_module::set_system_clock(epoch_ms, span)
                }
                RuntimeOp::LinuxSwapon => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.swapon", span)?,
                        "linux.swapon",
                        span,
                    )?;
                    let priority =
                        lowered_int_arg_or(values.get(1).cloned(), -1, "linux.swapon", span)?;
                    let host_device = self.host_path(&device);
                    linux_module::swapon(&host_device, priority, span)
                }
                RuntimeOp::LinuxSwaponAll => linux_module::swapon_all(span),
                RuntimeOp::LinuxSwapoff => {
                    let device = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.swapoff", span)?,
                        "linux.swapoff",
                        span,
                    )?;
                    let host_device = self.host_path(&device);
                    linux_module::swapoff(&host_device, span)
                }
                RuntimeOp::LinuxSwapoffAll => linux_module::swapoff_all(span),
                RuntimeOp::LinuxSwitchRoot => {
                    let new_root = lowered_path_arg(
                        unix_require_arg(values.first().cloned(), "linux.switch_root", span)?,
                        "linux.switch_root",
                        span,
                    )?;
                    let init = lowered_path_arg(
                        unix_require_arg(values.get(1).cloned(), "linux.switch_root", span)?,
                        "linux.switch_root",
                        span,
                    )?;
                    let host_new_root = self.host_path(&new_root);
                    let host_init = self.host_path(&init);
                    linux_module::switch_root(&host_new_root, &host_init, span)
                }
                RuntimeOp::LinuxSysctlLoadDirs => {
                    let dirs =
                        lowered_path_list(values.first().cloned(), "linux.sysctl_load_dirs", span)?;
                    let fallback = values
                        .get(1)
                        .cloned()
                        .map(|value| lowered_path_arg(value, "linux.sysctl_load_dirs", span))
                        .transpose()?;
                    let host_dirs: Vec<PathBuf> =
                        dirs.iter().map(|pv| self.host_path(pv)).collect();
                    let host_fallback = fallback.as_ref().map(|pv| self.host_path(pv));
                    linux_module::sysctl_load_dirs(&host_dirs, host_fallback.as_deref(), span)
                }
                RuntimeOp::LinuxSysctlSet => {
                    let key = lowered_str_arg_owned(
                        values.first().cloned(),
                        "",
                        "linux.sysctl_set",
                        span,
                    )?;
                    let value = lowered_str_arg_owned(
                        values.get(1).cloned(),
                        "",
                        "linux.sysctl_set",
                        span,
                    )?;
                    linux_module::sysctl_set(&key, &value, span)
                }
                RuntimeOp::LinuxUmountAll => {
                    let types = lowered_optional_str_list(
                        values.first().cloned(),
                        "linux.umount_all",
                        span,
                    )?;
                    linux_module::umount_all(&types, span)
                }
                RuntimeOp::LinuxWritePartitionTable => {
                    let device = lowered_path_arg(
                        unix_require_arg(
                            values.first().cloned(),
                            "linux.write_partition_table",
                            span,
                        )?,
                        "linux.write_partition_table",
                        span,
                    )?;
                    let table = lowered_record_arg(
                        values.get(1).cloned(),
                        "linux.write_partition_table",
                        span,
                    )?;
                    let host_device = self.host_path(&device);
                    linux_module::write_partition_table(&host_device, &table, span)
                }
                _ => unreachable!(
                    "new linux RuntimeOp routed to eval_linux_call_value must be handled"
                ),
            };
        }
        match op {
            RuntimeOp::LinuxUeventStream => {
                self.linux_fake_log("uevent_stream", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_live(
                    "linux.uevent_stream.dry_run",
                    DryRunUeventStream::default(),
                ))))
            }
            RuntimeOp::LinuxMount => {
                let source =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.mount", span)?;
                let target = lowered_path_arg(
                    unix_require_arg(values.get(1).cloned(), "linux.mount", span)?,
                    "linux.mount",
                    span,
                )?;
                let fstype =
                    lowered_str_arg_owned(values.get(2).cloned(), "", "linux.mount", span)?;
                let options =
                    lowered_optional_str_list(values.get(3).cloned(), "linux.mount", span)?;
                self.linux_fake_log(
                    "mount",
                    &[
                        ("source", source),
                        ("target", target.display()),
                        ("fstype", fstype),
                        ("options", options.join(",")),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxMountAll => {
                self.linux_fake_log("mount_all", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxUmountAll => {
                let types =
                    lowered_optional_str_list(values.first().cloned(), "linux.umount_all", span)?;
                self.linux_fake_log("umount_all", &[("types", types.join(","))], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSwaponAll => {
                self.linux_fake_log("swapon_all", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSwapoffAll => {
                self.linux_fake_log("swapoff_all", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxRootDevice => {
                let root = self.linux_fake_value("root_device", "rootfs");
                self.linux_fake_log("root_device", &[("device", root.clone())], span)?;
                Ok(Value::ok(Value::Str(root.into())))
            }
            RuntimeOp::LinuxMemInfo => {
                self.linux_fake_log("meminfo", &[], span)?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("total"), Value::Int(1024 * 1024 * 1024)),
                    (Arc::from("free"), Value::Int(256 * 1024 * 1024)),
                    (Arc::from("available"), Value::Int(512 * 1024 * 1024)),
                    (Arc::from("buffers"), Value::Int(64 * 1024 * 1024)),
                    (Arc::from("cached"), Value::Int(128 * 1024 * 1024)),
                    (Arc::from("shared"), Value::Int(0)),
                    (Arc::from("sreclaimable"), Value::Int(0)),
                    (Arc::from("swap_total"), Value::Int(512 * 1024 * 1024)),
                    (Arc::from("swap_free"), Value::Int(384 * 1024 * 1024)),
                ]))))
            }
            RuntimeOp::LinuxModules => {
                self.linux_fake_log("modules", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.modules",
                    vec![Value::Record(RecordMap::from([
                        (Arc::from("name"), Value::Str("xsh_demo".into())),
                        (Arc::from("size"), Value::Int(4096)),
                        (Arc::from("ref_count"), Value::Int(1)),
                        (
                            Arc::from("used_by"),
                            Value::List(vec![Value::Str("xsh_dep".into())]),
                        ),
                    ]))],
                ))))
            }
            RuntimeOp::LinuxDmesg => {
                self.linux_fake_log("dmesg", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.dmesg",
                    vec![Value::Str("xsh dry-run kernel message".into())],
                ))))
            }
            RuntimeOp::LinuxIsMountpoint => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.is_mountpoint", span)?,
                    "linux.is_mountpoint",
                    span,
                )?;
                self.linux_fake_log("is_mountpoint", &[("path", path.display())], span)?;
                Ok(Value::ok(Value::Bool(
                    path.display() == "/" || path.display() == "/proc",
                )))
            }
            RuntimeOp::LinuxDiskUsage => {
                let path = match values.first().cloned() {
                    Some(value) => {
                        Some(lowered_path_arg(value, "linux.disk_usage", span)?.display())
                    }
                    None => None,
                };
                let mount = path.unwrap_or_else(|| "/".to_string());
                self.linux_fake_log("disk_usage", &[("path", mount.clone())], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.disk_usage",
                    vec![Value::Record(RecordMap::from([
                        (Arc::from("device"), Value::Str("rootfs".into())),
                        (Arc::from("mount"), Value::Str(mount.into())),
                        (Arc::from("fstype"), Value::Str("tmpfs".into())),
                        (Arc::from("total"), Value::Int(1024 * 1024 * 1024)),
                        (Arc::from("used"), Value::Int(256 * 1024 * 1024)),
                        (Arc::from("available"), Value::Int(768 * 1024 * 1024)),
                    ]))],
                ))))
            }
            RuntimeOp::LinuxBlockDevices => {
                self.linux_fake_log("block_devices", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.block_devices",
                    vec![
                        Value::Record(RecordMap::from([
                            (Arc::from("name"), Value::Str("vda".into())),
                            (
                                Arc::from("path"),
                                Value::Path(path_value_from_pathbuf(PathBuf::from("/dev/vda"))?),
                            ),
                            (Arc::from("size"), Value::Int(128 * 1024 * 1024)),
                            (Arc::from("sectors"), Value::Int(262144)),
                            (Arc::from("sector_size"), Value::Int(512)),
                            (Arc::from("removable"), Value::Bool(false)),
                            (Arc::from("rotational"), Value::Bool(false)),
                            (Arc::from("partitioned"), Value::Bool(false)),
                            (Arc::from("partitions"), Value::List(Vec::new())),
                        ])),
                        Value::Record(RecordMap::from([
                            (Arc::from("name"), Value::Str("vdb".into())),
                            (
                                Arc::from("path"),
                                Value::Path(path_value_from_pathbuf(PathBuf::from("/dev/vdb"))?),
                            ),
                            (Arc::from("size"), Value::Int(128 * 1024 * 1024)),
                            (Arc::from("sectors"), Value::Int(262144)),
                            (Arc::from("sector_size"), Value::Int(512)),
                            (Arc::from("removable"), Value::Bool(false)),
                            (Arc::from("rotational"), Value::Bool(false)),
                            (Arc::from("partitioned"), Value::Bool(true)),
                            (
                                Arc::from("partitions"),
                                Value::List(vec![Value::Path(path_value_from_pathbuf(
                                    PathBuf::from("/dev/vdb1"),
                                )?)]),
                            ),
                        ])),
                    ],
                ))))
            }
            RuntimeOp::LinuxSysctlGet => {
                let key =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.sysctl_get", span)?;
                if let Err(error) = validate_linux_sysctl_key(&key, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.linux_fake_log("sysctl_get", &[("key", key)], span)?;
                Ok(Value::ok(Value::Str(
                    self.linux_fake_value("sysctl_value", "1").into(),
                )))
            }
            RuntimeOp::LinuxSysctlSet => {
                let key =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.sysctl_set", span)?;
                let value =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "linux.sysctl_set", span)?;
                if let Err(error) = validate_linux_sysctl_key(&key, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.linux_fake_log("sysctl_set", &[("key", key), ("value", value)], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxFileAttrs => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.file_attrs", span)?,
                    "linux.file_attrs",
                    span,
                )?;
                let flags = match self.linux_dry_run_file_attrs_flags(span) {
                    Ok(flags) => flags,
                    Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
                };
                self.linux_fake_log("file_attrs", &[("path", path.display())], span)?;
                Ok(Value::ok(linux_file_attrs_record(flags)))
            }
            RuntimeOp::LinuxSetFileAttrs => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.set_file_attrs", span)?,
                    "linux.set_file_attrs",
                    span,
                )?;
                let flags = lowered_int_arg(values.get(1).cloned(), "linux.set_file_attrs", span)?;
                if let Err(error) = validate_linux_file_attrs_flags(flags, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.linux_fake_log(
                    "set_file_attrs",
                    &[("path", path.display()), ("flags", flags.to_string())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxFileVersion => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.file_version", span)?,
                    "linux.file_version",
                    span,
                )?;
                let version = self.linux_dry_run_file_version(span)?;
                self.linux_fake_log("file_version", &[("path", path.display())], span)?;
                Ok(Value::ok(Value::Int(version)))
            }
            RuntimeOp::LinuxSetFileVersion => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.set_file_version", span)?,
                    "linux.set_file_version",
                    span,
                )?;
                let version =
                    lowered_int_arg(values.get(1).cloned(), "linux.set_file_version", span)?;
                if let Err(error) = validate_linux_file_version(version, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.linux_fake_log(
                    "set_file_version",
                    &[("path", path.display()), ("version", version.to_string())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSysctlLoadDirs => {
                let dirs =
                    lowered_path_list(values.first().cloned(), "linux.sysctl_load_dirs", span)?;
                let fallback = match values.get(1).cloned() {
                    Some(value) => {
                        lowered_path_arg(value, "linux.sysctl_load_dirs", span)?.display()
                    }
                    None => String::new(),
                };
                let dir_text = dirs
                    .iter()
                    .map(PathValue::display)
                    .collect::<Vec<_>>()
                    .join(",");
                self.linux_fake_log(
                    "sysctl_load_dirs",
                    &[("dirs", dir_text), ("fallback", fallback)],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxKillAll => {
                let signal =
                    lowered_str_arg_owned(values.first().cloned(), "TERM", "linux.kill_all", span)?;
                if let Err(error) = process_module::signal_info(&signal, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                let except_pid1 =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "linux.kill_all", span)?;
                self.linux_fake_log(
                    "kill_all",
                    &[("signal", signal), ("except_pid1", except_pid1.to_string())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxChroot => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.chroot", span)?,
                    "linux.chroot",
                    span,
                )?;
                self.linux_fake_log("chroot", &[("path", path.display())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxMknod => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.mknod", span)?,
                    "linux.mknod",
                    span,
                )?;
                let kind = lowered_str_arg_owned(values.get(1).cloned(), "", "linux.mknod", span)?;
                let major = lowered_int_arg(values.get(2).cloned(), "linux.mknod", span)?;
                let minor = lowered_int_arg(values.get(3).cloned(), "linux.mknod", span)?;
                if let Err(error) = validate_mknod_args(&kind, major, minor, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.linux_fake_log(
                    "mknod",
                    &[
                        ("path", path.display()),
                        ("kind", kind),
                        ("major", major.to_string()),
                        ("minor", minor.to_string()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxInsmod => {
                let path = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.insmod", span)?,
                    "linux.insmod",
                    span,
                )?;
                let params =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "linux.insmod", span)?;
                self.linux_fake_log(
                    "insmod",
                    &[("path", path.display()), ("params", params)],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxRmmod => {
                let name = lowered_str_arg_owned(values.first().cloned(), "", "linux.rmmod", span)?;
                let force =
                    lowered_bool_arg_or(values.get(1).cloned(), false, "linux.rmmod", span)?;
                self.linux_fake_log(
                    "rmmod",
                    &[("name", name), ("force", force.to_string())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxPivotRoot => {
                let new_root = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.pivot_root", span)?,
                    "linux.pivot_root",
                    span,
                )?;
                let put_old = lowered_path_arg(
                    unix_require_arg(values.get(1).cloned(), "linux.pivot_root", span)?,
                    "linux.pivot_root",
                    span,
                )?;
                self.linux_fake_log(
                    "pivot_root",
                    &[
                        ("new_root", new_root.display()),
                        ("put_old", put_old.display()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSwitchRoot => {
                let new_root = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.switch_root", span)?,
                    "linux.switch_root",
                    span,
                )?;
                let init = lowered_path_arg(
                    unix_require_arg(values.get(1).cloned(), "linux.switch_root", span)?,
                    "linux.switch_root",
                    span,
                )?;
                self.linux_fake_log(
                    "switch_root",
                    &[("new_root", new_root.display()), ("init", init.display())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxHwclock => {
                self.linux_fake_log("hwclock", &[], span)?;
                let epoch_ms = self
                    .linux_fake_value("hwclock_epoch_ms", "0")
                    .parse::<i64>()
                    .unwrap_or(0);
                Ok(Value::ok(Value::Int(epoch_ms)))
            }
            RuntimeOp::LinuxSetHwclock => {
                let epoch_ms = lowered_int_arg(values.first().cloned(), "linux.set_hwclock", span)?;
                self.linux_fake_log("set_hwclock", &[("epoch_ms", epoch_ms.to_string())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSetSystemClock => {
                let epoch_ms =
                    lowered_int_arg(values.first().cloned(), "linux.set_system_clock", span)?;
                self.linux_fake_log(
                    "set_system_clock",
                    &[("epoch_ms", epoch_ms.to_string())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxRfkillList => {
                self.linux_fake_log("rfkill_list", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.rfkill_list",
                    vec![Value::Record(RecordMap::from([
                        (Arc::from("id"), Value::Int(0)),
                        (Arc::from("name"), Value::Str("phy0".into())),
                        (Arc::from("type"), Value::Str("wlan".into())),
                        (Arc::from("soft_blocked"), Value::Bool(false)),
                        (Arc::from("hard_blocked"), Value::Bool(false)),
                    ]))],
                ))))
            }
            RuntimeOp::LinuxRfkillBlock | RuntimeOp::LinuxRfkillUnblock => {
                let id = lowered_int_arg(values.first().cloned(), "linux.rfkill", span)?;
                if id < 0 {
                    return Ok(module_error("linux-rfkill", "id cannot be negative", span));
                }
                let op_name = if op == RuntimeOp::LinuxRfkillBlock {
                    "rfkill_block"
                } else {
                    "rfkill_unblock"
                };
                self.linux_fake_log(op_name, &[("id", id.to_string())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxLoopAttach => {
                let file = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.loop_attach", span)?,
                    "linux.loop_attach",
                    span,
                )?;
                let device = match values.get(1).cloned() {
                    Some(value) => lowered_path_arg(value, "linux.loop_attach", span)?.display(),
                    None => "/dev/loop0".to_string(),
                };
                self.linux_fake_log(
                    "loop_attach",
                    &[("file", file.display()), ("device", device.clone())],
                    span,
                )?;
                path_value_from_pathbuf(PathBuf::from(device))
                    .map(Value::Path)
                    .map(Value::ok)
                    .map_err(|error| error.with_span(span))
            }
            RuntimeOp::LinuxLoopDetach => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.loop_detach", span)?,
                    "linux.loop_detach",
                    span,
                )?;
                self.linux_fake_log("loop_detach", &[("device", device.display())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxLoopList => {
                self.linux_fake_log("loop_list", &[], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.loop_list",
                    vec![Value::Record(RecordMap::from([
                        (
                            Arc::from("device"),
                            Value::Path(path_value_from_pathbuf(PathBuf::from("/dev/loop0"))?),
                        ),
                        (
                            Arc::from("file"),
                            Value::Path(path_value_from_pathbuf(PathBuf::from("/tmp/disk.img"))?),
                        ),
                        (Arc::from("offset"), Value::Int(0)),
                        (Arc::from("size"), Value::Int(0)),
                    ]))],
                ))))
            }
            RuntimeOp::LinuxMkswap => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.mkswap", span)?,
                    "linux.mkswap",
                    span,
                )?;
                self.linux_fake_log("mkswap", &[("device", device.display())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSwapon => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.swapon", span)?,
                    "linux.swapon",
                    span,
                )?;
                let priority =
                    lowered_int_arg_or(values.get(1).cloned(), -1, "linux.swapon", span)?;
                self.linux_fake_log(
                    "swapon",
                    &[
                        ("device", device.display()),
                        ("priority", priority.to_string()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxSwapoff => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.swapoff", span)?,
                    "linux.swapoff",
                    span,
                )?;
                self.linux_fake_log("swapoff", &[("device", device.display())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxBlkid => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.blkid", span)?,
                    "linux.blkid",
                    span,
                )?;
                self.linux_fake_log("blkid", &[("device", device.display())], span)?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("type"), Value::Str("ext4".into())),
                    (
                        Arc::from("uuid"),
                        Value::Str("00000000-0000-0000-0000-000000000001".into()),
                    ),
                    (Arc::from("label"), Value::Str("rootfs".into())),
                    (Arc::from("part_table_type"), Value::Str("gpt".into())),
                    (
                        Arc::from("part_entry_uuid"),
                        Value::Str("00000000-0000-0000-0000-000000000002".into()),
                    ),
                ]))))
            }
            RuntimeOp::LinuxModinfo => {
                let name =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.modinfo", span)?;
                self.linux_fake_log("modinfo", &[("name", name.clone())], span)?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("name"), Value::Str(name.into())),
                    (
                        Arc::from("filename"),
                        Value::Path(path_value_from_pathbuf(PathBuf::from(
                            "/lib/modules/dry-run/demo.ko",
                        ))?),
                    ),
                    (
                        Arc::from("description"),
                        Value::Str("dry-run module".into()),
                    ),
                    (Arc::from("license"), Value::Str("GPL".into())),
                    (Arc::from("version"), Value::Str("1".into())),
                    (Arc::from("fields"), Value::List([("description", "dry-run module"), ("license", "GPL"), ("version", "1"), ("parm", "debug:enable debug"), ("parmtype", "debug:bool")].into_iter().map(|(name, value)| Value::Record(RecordMap::from([(Arc::from("name"), Value::Str(name.into())), (Arc::from("value"), Value::Str(value.into()))]))).collect())),
                    (
                        Arc::from("params"),
                        Value::List(vec![Value::Record(RecordMap::from([
                            (Arc::from("name"), Value::Str("debug".into())),
                            (Arc::from("type"), Value::Str("bool".into())),
                            (Arc::from("description"), Value::Str("enable debug".into())),
                        ]))]),
                    ),
                ]))))
            }
            RuntimeOp::LinuxUmount | RuntimeOp::LinuxBlockdevInfo | RuntimeOp::LinuxBlockdevSetReadOnly
            | RuntimeOp::LinuxBlockdevFlush | RuntimeOp::LinuxBlockdevRereadPartitionTable
            | RuntimeOp::LinuxFstrim | RuntimeOp::LinuxFsfreeze | RuntimeOp::LinuxBlockSignatures
            | RuntimeOp::LinuxWipeBlockSignatures => {
                Err(RuntimeError::new("linux-fake-unsupported", "this block operation has no Linux fake implementation").with_span(span))
            }
            RuntimeOp::LinuxFileProject | RuntimeOp::LinuxSetFileProject => {
                let path = lowered_path_arg(unix_require_arg(values.first().cloned(), "linux.file_project", span)?, "linux.file_project", span)?;
                let project = if op == RuntimeOp::LinuxSetFileProject { lowered_int_arg(values.get(1).cloned(), "linux.set_file_project", span)? } else { self.linux_fake_value("file_project", "0").parse::<i64>().map_err(|_| RuntimeError::new("linux-file-project", "invalid fake project ID").with_span(span))? };
                if !(0..=i64::from(u32::MAX)).contains(&project) { return Ok(Value::err(Value::Error(Box::new(RuntimeError::new("linux-file-project", "project ID must fit in u32").with_span(span))))); }
                self.linux_fake_log(if op == RuntimeOp::LinuxFileProject { "file_project" } else { "set_file_project" }, &[("path", path.display()), ("project", project.to_string())], span)?;
                Ok(Value::ok(if op == RuntimeOp::LinuxFileProject { Value::Int(project) } else { Value::Unit }))
            }
                RuntimeOp::LinuxModulePlan => {
                let name = lowered_str_arg_owned(values.first().cloned(), "", "linux.module_plan", span)?;
                let params = lowered_str_arg_owned(values.get(1).cloned(), "", "linux.module_plan", span)?;
                let remove = lowered_bool_arg_or(values.get(2).cloned(), false, "linux.module_plan", span)?;
                self.linux_fake_log("module_plan", &[("name", name.clone()), ("params", params.clone()), ("remove", remove.to_string())], span)?;
                Ok(Value::ok(Value::List(vec![Value::Record(RecordMap::from([
                    (Arc::from("name"), Value::Str(name.into())),
                    (Arc::from("filename"), Value::Path(path_value_from_pathbuf(PathBuf::from("/lib/modules/dry-run/demo.ko"))?)),
                    (Arc::from("params"), Value::Str(params.into())),
                    (Arc::from("loaded"), Value::Bool(false)),
                ]))])))
            }
            RuntimeOp::LinuxModprobe => {
                let name =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.modprobe", span)?;
                let params =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "linux.modprobe", span)?;
                let remove = lowered_bool_arg_or(values.get(2).cloned(), false, "linux.modprobe", span)?;
                self.linux_fake_log("modprobe", &[("name", name), ("params", params), ("remove", remove.to_string())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxDepmod => {
                let version =
                    lowered_str_arg_owned(values.first().cloned(), "", "linux.depmod", span)?;
                self.linux_fake_log("depmod", &[("version", version)], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxOpenFiles => {
                let pid = match values.first().cloned() {
                    Some(value) => Some(lowered_int_arg(Some(value), "linux.open_files", span)?),
                    None => None,
                };
                let pid_value = pid.unwrap_or(123);
                self.linux_fake_log("open_files", &[("pid", pid_value.to_string())], span)?;
                Ok(Value::ok(Value::stream(StreamValue::from_values_live(
                    "linux.open_files",
                    vec![Value::Record(RecordMap::from([
                        (Arc::from("pid"), Value::Int(pid_value)),
                        (Arc::from("command"), Value::Str("xsh".into())),
                        (Arc::from("fd"), Value::Int(1)),
                        (Arc::from("fd_label"), Value::Str("1".into())),
                        (Arc::from("access"), Value::Str("w".into())),
                        (Arc::from("dev"), Value::Null),
                        (Arc::from("type"), Value::Str("file".into())),
                        (
                            Arc::from("path"),
                            Value::Path(path_value_from_pathbuf(PathBuf::from("/tmp/xsh.log"))?),
                        ),
                        (Arc::from("inode"), Value::Int(0)),
                        (Arc::from("protocol"), Value::Str("".into())),
                        (Arc::from("local"), Value::Str("".into())),
                        (Arc::from("remote"), Value::Str("".into())),
                    ]))],
                ))))
            }
            RuntimeOp::LinuxPartitionTable => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.partition_table", span)?,
                    "linux.partition_table",
                    span,
                )?;
                self.linux_fake_log("partition_table", &[("device", device.display())], span)?;
                Ok(Value::ok(linux_dry_run_partition_table()))
            }
            RuntimeOp::LinuxWritePartitionTable => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.write_partition_table", span)?,
                    "linux.write_partition_table",
                    span,
                )?;
                let _table = lowered_record_arg(
                    values.get(1).cloned(),
                    "linux.write_partition_table",
                    span,
                )?;
                self.linux_fake_log(
                    "write_partition_table",
                    &[("device", device.display())],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxFsck => {
                let device = lowered_path_arg(
                    unix_require_arg(values.first().cloned(), "linux.fsck", span)?,
                    "linux.fsck",
                    span,
                )?;
                let fstype = lowered_str_arg_owned(values.get(1).cloned(), "", "linux.fsck", span)?;
                let repair =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "linux.fsck", span)?;
                self.linux_fake_log(
                    "fsck",
                    &[
                        ("device", device.display()),
                        ("fstype", fstype),
                        ("repair", repair.to_string()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("status"), Value::Int(0)),
                    (Arc::from("errors"), Value::List(Vec::new())),
                ]))))
            }
            RuntimeOp::LinuxHalt => {
                self.linux_fake_log("halt", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxPoweroff => {
                self.linux_fake_log("poweroff", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::LinuxReboot => {
                self.linux_fake_log("reboot", &[], span)?;
                Ok(Value::ok(Value::Unit))
            }
            _ => unreachable!("linux dry-run operation expected"),
        }
    }
}
