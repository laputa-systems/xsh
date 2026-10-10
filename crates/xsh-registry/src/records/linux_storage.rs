use crate::types::Type;

fn record(fields: impl IntoIterator<Item = (&'static str, Type)>) -> Type {
    Type::Record(fields.into_iter().map(|(name, ty)| (name.to_string(), ty)).collect())
}

pub fn linux_sg_io_type() -> Type {
    record([
        ("status", Type::UInt),
        ("host_status", Type::UInt),
        ("driver_status", Type::UInt),
        ("sense", Type::Bytes),
        ("data", Type::Bytes),
        ("resid", Type::UInt),
    ])
}

pub fn linux_ata_result_type() -> Type {
    record([
        ("data", Type::Bytes),
        ("status", Type::UInt),
        ("error", Type::UInt),
        ("sector_count", Type::UInt),
        ("lba_low", Type::UInt),
        ("lba_mid", Type::UInt),
        ("lba_high", Type::UInt),
        ("device", Type::UInt),
        ("sense", Type::Bytes),
    ])
}

pub fn linux_ata_smart_status_type() -> Type {
    record([
        ("passed", Type::Bool),
        ("status", Type::UInt),
        ("error", Type::UInt),
        ("lba_mid", Type::UInt),
        ("lba_high", Type::UInt),
    ])
}

pub fn linux_nvme_command_type() -> Type {
    record([("status", Type::UInt), ("result", Type::UInt), ("data", Type::Bytes)])
}

pub fn linux_storage_candidate_type() -> Type {
    let optional = |ty: Type| Type::Optional(Box::new(ty));
    record([
        ("name", Type::Str),
        ("path", Type::Path),
        ("kind", Type::Str),
        ("protocol", Type::Str),
        ("model", optional(Type::Str)),
        ("serial", optional(Type::Str)),
        ("firmware", optional(Type::Str)),
        ("size_bytes", optional(Type::UInt)),
        ("rotational", optional(Type::Bool)),
        ("removable", optional(Type::Bool)),
        ("controller", optional(Type::Path)),
    ])
}
