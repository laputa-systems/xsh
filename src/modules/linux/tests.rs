#![allow(clippy::module_inception)]

#[cfg(test)]
mod tests {
    use crate::modules::compression::{Compression, copy_compressed};
    use crate::modules::linux::api::{blkid, partition_table};
    use crate::modules::linux::block::{
        fsck_impl_with_path, page_size, write_partition_table_impl,
    };
    use crate::modules::linux::kernel::{
        depmod_impl_in_root, modinfo_impl_in_root, module_info_record,
    };
    #[cfg(target_os = "linux")]
    use crate::modules::linux::process::open_files_impl;
    use crate::modules::linux::{
        BTRFS_FSID_OFFSET, BTRFS_MAGIC_OFFSET, BTRFS_SUPER_OFFSET, BTRFS_SUPER_SIZE,
        EXT_FEATURE_INCOMPAT_OFFSET, EXT_LABEL_OFFSET, EXT_MAGIC_OFFSET, EXT_SUPER_OFFSET,
        EXT_SUPER_SIZE, EXT_UUID_OFFSET, EXT4_FEATURE_INCOMPAT_EXTENTS, ISO9660_LABEL_OFFSET,
        ISO9660_PVD_OFFSET, ISO9660_PVD_SIZE, ModuleIndex, VFAT_BOOT_SIZE, VFAT_LABEL32_OFFSET,
        VFAT_SERIAL32_OFFSET, XFS_LABEL_OFFSET, XFS_SUPER_SIZE, XFS_UUID_OFFSET, str_value,
    };
    use crate::runtime::value::{RecordMap, ResultValue, Value};
    use crate::source::{SourceId, Span};
    use std::fs::{self, File};
    use std::io::{Seek, SeekFrom, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn span() -> Span {
        Span::new(SourceId::new(0), 0, 0)
    }

    fn ok_record(value: Value) -> RecordMap {
        match value {
            Value::Result(ResultValue::Ok(value)) => match *value {
                Value::Record(record) => record,
                value => panic!("expected record, got {value:?}"),
            },
            value => panic!("expected Ok(record), got {value:?}"),
        }
    }

    fn str_field(record: &RecordMap, field: &str) -> String {
        match record.get(field) {
            Some(Value::Str(value)) => value.to_string(),
            value => panic!("expected string field {field}, got {value:?}"),
        }
    }

    fn int_field(record: &RecordMap, field: &str) -> i64 {
        match record.get(field) {
            Some(Value::Int(value)) => *value,
            value => panic!("expected int field {field}, got {value:?}"),
        }
    }

    fn list_field<'a>(record: &'a RecordMap, field: &str) -> &'a [Value] {
        match record.get(field) {
            Some(Value::List(values)) => values,
            value => panic!("expected list field {field}, got {value:?}"),
        }
    }

    fn record_value(value: &Value) -> &RecordMap {
        match value {
            Value::Record(record) => record,
            value => panic!("expected record, got {value:?}"),
        }
    }

    fn write_sparse(path: &Path, len: u64, writes: &[(u64, &[u8])]) {
        let mut file = File::create(path).expect("create sparse fixture");
        file.set_len(len).expect("set sparse fixture len");
        for (offset, bytes) in writes {
            file.seek(SeekFrom::Start(*offset)).expect("seek fixture");
            file.write_all(bytes).expect("write fixture");
        }
    }

    #[allow(clippy::single_call_fn)]
    fn write_bytes_at(bytes: &mut [u8], offset: usize, value: &[u8]) {
        bytes[offset..offset + value.len()].copy_from_slice(value);
    }

    #[allow(clippy::single_call_fn)]
    fn unique_bytes() -> [u8; 16] {
        [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10,
        ]
    }

    fn module_bytes(fields: &[&str]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for field in fields {
            bytes.extend_from_slice(field.as_bytes());
            bytes.push(0);
        }
        bytes
    }

    fn write_compressed_module(path: &Path, compression: Compression, fields: &[&str]) {
        let raw = path.with_extension("raw");
        let bytes = module_bytes(fields);
        fs::write(&raw, &bytes).expect("write raw module fixture");
        let input = File::open(&raw).expect("open raw module fixture");
        let output = File::create(path).expect("create compressed module fixture");
        copy_compressed(input, output, compression, 6, bytes.len() as u64, span())
            .expect("compress module fixture");
    }

    fn table_record(label: &str, partitions: Vec<Value>) -> RecordMap {
        RecordMap::from([
            (Arc::from("label"), str_value(label.to_string())),
            (
                Arc::from("id"),
                str_value("11111111-2222-3333-4444-555555555555"),
            ),
            (Arc::from("partitions"), Value::List(partitions)),
        ])
    }

    #[allow(clippy::single_call_fn)]
    fn partition_record(fields: &[(&str, Value)]) -> Value {
        Value::Record(
            fields
                .iter()
                .map(|(name, value)| (Arc::from(*name), value.clone()))
                .collect(),
        )
    }

    #[test]
    fn blkid_reads_seed_filesystem_magic_fixtures() {
        let root = TempDir::new().expect("tempdir");
        let uuid = unique_bytes();

        let ext = root.path().join("ext.img");
        let mut ext_super = [0_u8; EXT_SUPER_SIZE];
        ext_super[EXT_MAGIC_OFFSET..EXT_MAGIC_OFFSET + 2]
            .copy_from_slice(&0xef53_u16.to_le_bytes());
        write_bytes_at(
            &mut ext_super,
            EXT_FEATURE_INCOMPAT_OFFSET,
            &EXT4_FEATURE_INCOMPAT_EXTENTS.to_le_bytes(),
        );
        write_bytes_at(&mut ext_super, EXT_UUID_OFFSET, &uuid);
        write_bytes_at(&mut ext_super, EXT_LABEL_OFFSET, b"ROOTFS\0");
        write_sparse(
            &ext,
            EXT_SUPER_OFFSET + EXT_SUPER_SIZE as u64,
            &[(EXT_SUPER_OFFSET, &ext_super)],
        );

        let swap = root.path().join("swap.img");
        let page = page_size().expect("page size") as u64;
        write_sparse(
            &swap,
            page,
            &[
                (page - 10, b"SWAPSPACE2"),
                (1024 + 12, &uuid),
                (1024 + 28, b"SWAPVOL\0"),
            ],
        );

        let xfs = root.path().join("xfs.img");
        let mut xfs_super = [0_u8; XFS_SUPER_SIZE];
        write_bytes_at(&mut xfs_super, 0, b"XFSB");
        write_bytes_at(&mut xfs_super, XFS_UUID_OFFSET, &uuid);
        write_bytes_at(&mut xfs_super, XFS_LABEL_OFFSET, b"XFSVOL\0");
        write_sparse(&xfs, XFS_SUPER_SIZE as u64, &[(0, &xfs_super)]);

        let vfat = root.path().join("vfat.img");
        let mut vfat_boot = [0_u8; VFAT_BOOT_SIZE];
        write_bytes_at(&mut vfat_boot, 82, b"FAT32   ");
        write_bytes_at(
            &mut vfat_boot,
            VFAT_SERIAL32_OFFSET,
            &0x1234_abcd_u32.to_le_bytes(),
        );
        write_bytes_at(&mut vfat_boot, VFAT_LABEL32_OFFSET, b"SEEDVOL    ");
        vfat_boot[510] = 0x55;
        vfat_boot[511] = 0xaa;
        write_sparse(&vfat, VFAT_BOOT_SIZE as u64, &[(0, &vfat_boot)]);

        let btrfs = root.path().join("btrfs.img");
        let mut btrfs_super = [0_u8; BTRFS_SUPER_SIZE];
        write_bytes_at(&mut btrfs_super, BTRFS_FSID_OFFSET, &uuid);
        write_bytes_at(&mut btrfs_super, BTRFS_MAGIC_OFFSET, b"_BHRfS_M");
        write_sparse(
            &btrfs,
            BTRFS_SUPER_OFFSET + BTRFS_SUPER_SIZE as u64,
            &[(BTRFS_SUPER_OFFSET, &btrfs_super)],
        );

        let iso = root.path().join("iso.img");
        let mut descriptor = [0_u8; ISO9660_PVD_SIZE];
        descriptor[0] = 1;
        write_bytes_at(&mut descriptor, 1, b"CD001");
        descriptor[6] = 1;
        write_bytes_at(&mut descriptor, ISO9660_LABEL_OFFSET, b"ISO_LABEL");
        write_sparse(
            &iso,
            ISO9660_PVD_OFFSET + ISO9660_PVD_SIZE as u64,
            &[(ISO9660_PVD_OFFSET, &descriptor)],
        );

        let squash = root.path().join("squash.img");
        write_sparse(&squash, 512, &[(0, b"hsqs")]);

        let cases = [
            (
                &ext,
                "ext4",
                "ROOTFS",
                "01020304-0506-0708-090a-0b0c0d0e0f10",
            ),
            (
                &swap,
                "swap",
                "SWAPVOL",
                "01020304-0506-0708-090a-0b0c0d0e0f10",
            ),
            (
                &xfs,
                "xfs",
                "XFSVOL",
                "01020304-0506-0708-090a-0b0c0d0e0f10",
            ),
            (&vfat, "vfat", "SEEDVOL", "1234-ABCD"),
            (&btrfs, "btrfs", "", "01020304-0506-0708-090a-0b0c0d0e0f10"),
            (&iso, "iso9660", "ISO_LABEL", ""),
            (&squash, "squashfs", "", ""),
        ];

        for (path, fstype, label, uuid) in cases {
            let record = ok_record(blkid(path, span()).expect("blkid"));
            assert_eq!(str_field(&record, "type"), fstype, "{path:?}");
            assert_eq!(str_field(&record, "label"), label, "{path:?}");
            assert_eq!(str_field(&record, "uuid"), uuid, "{path:?}");
        }
    }

    #[test]
    fn partition_table_reads_and_writes_dos_and_gpt_file_images() {
        let root = TempDir::new().expect("tempdir");
        let mbr = root.path().join("mbr.img");
        File::create(&mbr)
            .expect("create mbr")
            .set_len(4 * 1024 * 1024)
            .expect("size mbr");
        let mbr_table = table_record(
            "dos",
            vec![partition_record(&[
                ("index", Value::Int(1)),
                ("start", Value::Int(2048)),
                ("size", Value::Int(4096)),
                ("type", str_value("83")),
            ])],
        );

        write_partition_table_impl(&mbr, &mbr_table, span()).expect("write mbr");
        let record = ok_record(partition_table(&mbr, span()).expect("read mbr"));
        assert_eq!(str_field(&record, "label"), "dos");
        let parts = list_field(&record, "partitions");
        let part = record_value(&parts[0]);
        assert_eq!(int_field(part, "index"), 1);
        assert_eq!(int_field(part, "start"), 2048);
        assert_eq!(int_field(part, "end"), 6143);
        assert_eq!(int_field(part, "size"), 4096);
        assert_eq!(str_field(part, "type"), "83");
        let blkid_record = ok_record(blkid(&mbr, span()).expect("blkid mbr"));
        assert_eq!(str_field(&blkid_record, "part_table_type"), "dos");

        let gpt = root.path().join("gpt.img");
        File::create(&gpt)
            .expect("create gpt")
            .set_len(16 * 1024 * 1024)
            .expect("size gpt");
        let type_guid = "0fc63daf-8483-4772-8e79-3d69d8477de4";
        let part_guid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let gpt_table = table_record(
            "gpt",
            vec![partition_record(&[
                ("index", Value::Int(1)),
                ("start", Value::Int(2048)),
                ("end", Value::Int(4095)),
                ("type", str_value(type_guid.to_string())),
                ("uuid", str_value(part_guid.to_string())),
                ("name", str_value("rootfs")),
            ])],
        );

        write_partition_table_impl(&gpt, &gpt_table, span()).expect("write gpt");
        let record = ok_record(partition_table(&gpt, span()).expect("read gpt"));
        assert_eq!(str_field(&record, "label"), "gpt");
        assert_eq!(
            str_field(&record, "id"),
            "11111111-2222-3333-4444-555555555555"
        );
        let parts = list_field(&record, "partitions");
        let part = record_value(&parts[0]);
        assert_eq!(int_field(part, "start"), 2048);
        assert_eq!(int_field(part, "end"), 4095);
        assert_eq!(str_field(part, "type"), type_guid);
        assert_eq!(str_field(part, "uuid"), part_guid);
        assert_eq!(str_field(part, "name"), "rootfs");
        let blkid_record = ok_record(blkid(&gpt, span()).expect("blkid gpt"));
        assert_eq!(str_field(&blkid_record, "part_table_type"), "gpt");
        assert_eq!(
            str_field(&blkid_record, "part_entry_uuid"),
            "11111111-2222-3333-4444-555555555555"
        );
    }

    #[test]
    fn module_metadata_fixtures_cover_modinfo_and_depmod() {
        let root = TempDir::new().expect("tempdir");
        let module_dir = root.path().join("kernel/drivers");
        fs::create_dir_all(&module_dir).expect("create module dir");
        fs::write(
            module_dir.join("dep.ko"),
            module_bytes(&["description=dep module", "license=GPL", "version=1"]),
        )
        .expect("write dep module");
        fs::write(
            module_dir.join("demo-name.ko"),
            module_bytes(&[
                "description=Demo module",
                "license=MIT",
                "version=2",
                "depends=dep",
                "parm=debug:Enable debug (bool)",
            ]),
        )
        .expect("write demo module");

        let index = ModuleIndex::scan(root.path()).expect("scan module tree");
        let entry = index.get("demo-name").expect("demo module");
        let record = module_info_record(entry, span()).expect("module info");
        let record = record_value(&record);
        assert_eq!(str_field(record, "name"), "demo_name");
        assert_eq!(str_field(record, "description"), "Demo module");
        assert_eq!(str_field(record, "license"), "MIT");
        assert_eq!(str_field(record, "version"), "2");
        let params = list_field(record, "params");
        let param = record_value(&params[0]);
        assert_eq!(str_field(param, "name"), "debug");
        assert_eq!(str_field(param, "description"), "Enable debug");
        assert_eq!(str_field(param, "type"), "bool");

        let modinfo_record =
            modinfo_impl_in_root("demo-name", root.path(), span()).expect("modinfo");
        let modinfo_record = record_value(&modinfo_record);
        assert_eq!(str_field(modinfo_record, "name"), "demo_name");
        depmod_impl_in_root(root.path(), span()).expect("depmod");
        assert_eq!(
            fs::read_to_string(root.path().join("modules.dep")).expect("read modules.dep"),
            "kernel/drivers/demo-name.ko: kernel/drivers/dep.ko\nkernel/drivers/dep.ko:\n"
        );
    }

    #[test]
    fn module_metadata_reads_supported_compressed_modules() {
        let root = TempDir::new().expect("tempdir");
        let module_dir = root.path().join("kernel/drivers");
        fs::create_dir_all(&module_dir).expect("create module dir");
        write_compressed_module(
            &module_dir.join("gzip-demo.ko.gz"),
            Compression::Gz,
            &["description=gzip module", "license=GPL"],
        );
        write_compressed_module(
            &module_dir.join("xz-demo.ko.xz"),
            Compression::Xz,
            &["description=xz module", "license=MIT"],
        );
        write_compressed_module(
            &module_dir.join("bzip-demo.ko.bz2"),
            Compression::Bz2,
            &["description=bzip module", "license=Apache-2.0"],
        );

        let index = ModuleIndex::scan(root.path()).expect("scan module tree");
        let gzip_record = module_info_record(index.get("gzip-demo").expect("gzip module"), span())
            .expect("gzip module info");
        let xz_record =
            module_info_record(index.get("xz-demo").expect("xz module"), span()).expect("xz info");
        let bzip_record = module_info_record(index.get("bzip-demo").expect("bzip module"), span())
            .expect("bzip module info");

        assert_eq!(
            str_field(record_value(&gzip_record), "description"),
            "gzip module"
        );
        assert_eq!(
            str_field(record_value(&xz_record), "description"),
            "xz module"
        );
        assert_eq!(
            str_field(record_value(&bzip_record), "description"),
            "bzip module"
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn open_files_reports_current_process_descriptors() {
        let pid = std::process::id() as i64;
        let records = open_files_impl(Some(pid), span()).expect("open files");
        assert!(
            records.iter().any(|record| {
                let record = record_value(record);
                int_field(record, "pid") == pid
                    && int_field(record, "fd") >= 0
                    && !str_field(record, "type").is_empty()
            }),
            "{records:?}"
        );
    }

    #[test]
    fn fsck_dispatches_to_filesystem_specific_checker() {
        let root = TempDir::new().expect("tempdir");
        let bin = root.path().join("bin");
        fs::create_dir_all(&bin).expect("create bin");
        let checker = bin.join("fsck.xshparity");
        fs::write(
            &checker,
            "#!/bin/sh\nprintf 'args:%s\\n' \"$*\" >&2\nexit 4\n",
        )
        .expect("write checker");
        let mut permissions = fs::metadata(&checker).expect("stat checker").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&checker, permissions).expect("chmod checker");

        let path_value = bin.into_os_string();
        let record = fsck_impl_with_path(
            Path::new("/dev/null"),
            "xshparity",
            false,
            Some(&path_value),
            span(),
        )
        .expect("fsck");
        let record = record_value(&record);
        assert_eq!(int_field(record, "status"), 4);
        let errors = list_field(record, "errors");
        assert!(matches!(&errors[0], Value::Str(line) if line.as_ref() == "args:-n /dev/null"));
    }
}
