#![allow(clippy::module_inception)]

#[cfg(test)]
mod tests {
    use crate::modules::compression::{Compression, copy_compressed};
    use crate::modules::linux::api::{blkid, partition_table};
    use crate::modules::linux::block::{
        fsck_impl_with_path, page_size, write_partition_table_impl,
    };
    use crate::modules::linux::kernel::{
        depmod_impl_in_root, modinfo_impl_in_root, module_info_record, test_module_plan,
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
    #[cfg(target_os = "linux")]
    use crate::runtime::value::LiveStream;
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
        let mut metadata = Vec::new();
        for field in fields {
            metadata.extend_from_slice(field.as_bytes());
            metadata.push(0);
        }
        let strings = b"\0.shstrtab\0.modinfo\0";
        let mut bytes = vec![0u8; 64 + 3 * 64];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        bytes[16..18].copy_from_slice(&1u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        bytes[40..48].copy_from_slice(&64u64.to_le_bytes());
        bytes[52..54].copy_from_slice(&64u16.to_le_bytes());
        bytes[58..60].copy_from_slice(&64u16.to_le_bytes());
        bytes[60..62].copy_from_slice(&3u16.to_le_bytes());
        bytes[62..64].copy_from_slice(&1u16.to_le_bytes());
        let strings_offset = bytes.len() as u64;
        bytes.extend_from_slice(strings);
        let metadata_offset = bytes.len() as u64;
        bytes.extend_from_slice(&metadata);
        for (index, name, kind, offset, size) in [
            (1, 1u32, 3u32, strings_offset, strings.len() as u64),
            (2, 11u32, 1u32, metadata_offset, metadata.len() as u64),
        ] {
            let start = 64 + index * 64;
            bytes[start..start + 4].copy_from_slice(&name.to_le_bytes());
            bytes[start + 4..start + 8].copy_from_slice(&kind.to_le_bytes());
            bytes[start + 24..start + 32].copy_from_slice(&offset.to_le_bytes());
            bytes[start + 32..start + 40].copy_from_slice(&size.to_le_bytes());
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
                str_value(if label == "dos" { "0x12345678" } else { "11111111-2222-3333-4444-555555555555" }),
            ),
            (Arc::from("sector_size"), Value::Int(512)),
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
                ("end", Value::Int(6143)),
                ("type", str_value("83")),
                ("uuid", str_value("")),
                ("name", str_value("")),
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
                ("size", Value::Int(2048)),
                ("type", str_value(type_guid.to_string())),
                ("uuid", str_value(part_guid.to_string())),
                ("name", str_value("rootfs")),
            ])],
        );

        write_partition_table_impl(&gpt, &gpt_table, span()).expect("write gpt");
        // Byte-level format checks use an independent CRC implementation so
        // matching bugs in the native reader and writer cannot satisfy them.
        let bytes = fs::read(&gpt).expect("read GPT bytes");
        let oracle_crc = |data: &[u8]| {
            let mut crc = !0_u32;
            for byte in data {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb88320 } else { crc >> 1 };
                }
            }
            !crc
        };
        let primary = &bytes[512..1024];
        let backup = &bytes[bytes.len() - 512..];
        let entries = &bytes[1024..1024 + 128 * 128];
        assert_eq!(bytes[450], 0xee);
        assert_eq!(&bytes[510..512], &[0x55, 0xaa]);
        for header in [primary, backup] {
            assert_eq!(&header[..8], b"EFI PART");
            let stored = u32::from_le_bytes(header[16..20].try_into().unwrap());
            let mut checked = header[..92].to_vec();
            checked[16..20].fill(0);
            assert_eq!(oracle_crc(&checked), stored);
            assert_eq!(oracle_crc(entries), u32::from_le_bytes(header[88..92].try_into().unwrap()));
        }
        assert_eq!(entries, &bytes[bytes.len() - 512 - 128 * 128..bytes.len() - 512]);
        assert_eq!(u64::from_le_bytes(primary[32..40].try_into().unwrap()), (bytes.len() / 512 - 1) as u64);
        assert_eq!(u64::from_le_bytes(backup[32..40].try_into().unwrap()), 1);
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
        let mut corrupt = bytes.clone();
        corrupt[1024 + 32] ^= 1;
        fs::write(&gpt, &corrupt).unwrap();
        assert!(matches!(partition_table(&gpt, span()).unwrap(), Value::Result(ResultValue::Err(_))));
        corrupt = bytes;
        corrupt[512 + 16] ^= 1;
        fs::write(&gpt, &corrupt).unwrap();
        assert!(matches!(partition_table(&gpt, span()).unwrap(), Value::Result(ResultValue::Err(_))));
    }

    #[test]
    fn partition_writer_rejects_invalid_input_without_mutation() {
        let root = TempDir::new().expect("tempdir");
        let image = root.path().join("disk.img");
        let original = vec![0xa5; 1024 * 1024];
        fs::write(&image, &original).unwrap();
        let valid = partition_record(&[
            ("index", Value::Int(1)), ("start", Value::Int(64)),
            ("end", Value::Int(127)), ("size", Value::Int(64)),
            ("type", str_value("83")), ("uuid", str_value("")), ("name", str_value("")),
        ]);
        let mut invalid = Vec::new();
        for (field, value) in [
            ("index", Value::Int(0)), ("index", Value::Int(5)),
            ("start", Value::Int(-1)), ("size", Value::Int(0)),
            ("end", Value::Int(128)), ("type", str_value("bad")),
            ("type", str_value("05")),
        ] {
            let mut part = record_value(&valid).clone();
            part.insert(Arc::from(field), value);
            invalid.push(table_record("dos", vec![Value::Record(part)]));
        }
        invalid.push(table_record("dos", vec![valid.clone(); 5]));
        invalid.push(table_record("dos", vec![valid.clone(), valid.clone()]));
        let mut wrong_sector = table_record("dos", vec![valid]);
        wrong_sector.insert(Arc::from("sector_size"), Value::Int(4096));
        invalid.push(wrong_sector);
        for table in invalid {
            assert!(write_partition_table_impl(&image, &table, span()).is_err(), "accepted {table:?}");
            assert_eq!(fs::read(&image).unwrap(), original);
        }
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
    fn module_metadata_preserves_only_ordered_modinfo_fields() {
        let root = TempDir::new().expect("tempdir");
        let mut bytes = module_bytes(&["alias=pci:first", "description=Démo", "alias=pci:second"]);
        bytes.extend_from_slice(b"\0alias=outside-section\0");
        fs::write(root.path().join("demo.ko"), bytes).expect("module fixture");
        let record = modinfo_impl_in_root("demo", root.path(), span()).expect("modinfo");
        let fields = list_field(record_value(&record), "fields");
        let observed = fields.iter().map(|field| {
            let field = record_value(field);
            (str_field(field, "name"), str_field(field, "value"))
        }).collect::<Vec<_>>();
        assert_eq!(observed, vec![
            ("alias".into(), "pci:first".into()),
            ("description".into(), "Démo".into()),
            ("alias".into(), "pci:second".into()),
        ]);
    }

    #[test]
    fn module_metadata_reads_elf32_big_endian_and_rejects_overflowing_tables() {
        let root = TempDir::new().expect("tempdir");
        let strings = b"\0.shstrtab\0.modinfo\0";
        let metadata = b"description=big endian module\0";
        let mut image = vec![0u8; 52 + 3 * 40];
        image[..7].copy_from_slice(b"\x7fELF\x01\x02\x01");
        for (offset, value) in [(16, 1u16), (18, 40), (40, 52), (46, 40), (48, 3), (50, 1)] {
            image[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }
        image[20..24].copy_from_slice(&1u32.to_be_bytes());
        image[32..36].copy_from_slice(&52u32.to_be_bytes());
        let strings_offset = image.len() as u32;
        image.extend_from_slice(strings);
        let metadata_offset = image.len() as u32;
        image.extend_from_slice(metadata);
        for (index, name, kind, offset, length) in [(1, 1u32, 3u32, strings_offset, strings.len() as u32), (2, 11, 1, metadata_offset, metadata.len() as u32)] {
            let start = 52 + index * 40;
            for (field, value) in [(0, name), (4, kind), (16, offset), (20, length)] {
                image[start + field..start + field + 4].copy_from_slice(&value.to_be_bytes());
            }
        }
        let path = root.path().join("big.ko");
        fs::write(&path, &image).unwrap();
        let value = modinfo_impl_in_root("big", root.path(), span()).unwrap();
        assert_eq!(str_field(record_value(&value), "description"), "big endian module");
        image[32..36].copy_from_slice(&u32::MAX.to_be_bytes());
        fs::write(&path, image).unwrap();
        assert!(modinfo_impl_in_root("big", root.path(), span()).is_err());
    }

    #[test]
    fn depmod_rejects_missing_dependencies_without_rewriting_indices() {
        let root = TempDir::new().expect("tempdir");
        fs::write(root.path().join("demo.ko"), module_bytes(&["depends=missing"]))
            .expect("module fixture");
        fs::write(root.path().join("modules.dep"), "retained\n").expect("prior index");
        let failure = depmod_impl_in_root(root.path(), span()).expect_err("missing dependency");
        assert!(failure.message.contains("missing"));
        assert_eq!(fs::read_to_string(root.path().join("modules.dep")).unwrap(), "retained\n");
    }

    #[test]
    fn module_plan_resolves_alias_options_and_soft_dependencies_without_loading() {
        let root = TempDir::new().expect("tempdir");
        for (name, fields) in [
            ("pre", vec!["description=pre"]),
            ("hard", vec!["description=hard"]),
            ("post", vec!["description=post", "depends=demo"]),
            ("demo", vec!["alias=pci:v1234*", "depends=hard", "softdep=pre: pre post: post"]),
        ] {
            fs::write(root.path().join(format!("{name}.ko")), module_bytes(&fields)).unwrap();
        }
        let config = "alias test-alias demo\noptions demo debug=1\noptions test-alias mode=2\noptions hard limit=3\n";
        let plan = test_module_plan(root.path(), config, "test-alias", "extra=4", false).unwrap();
        assert_eq!(plan, vec![
            ("pre".into(), "".into()), ("hard".into(), "limit=3".into()),
            ("demo".into(), "debug=1 mode=2 extra=4".into()), ("post".into(), "".into()),
        ]);
        let removal = test_module_plan(root.path(), config, "demo", "", true).unwrap();
        assert_eq!(removal.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(), vec!["post", "demo", "hard", "pre"]);
        let alias = test_module_plan(root.path(), "", "pci:v1234dFFFF", "", false).unwrap();
        assert_eq!(alias[2].0, "demo");
        assert!(test_module_plan(root.path(), "blacklist demo\n", "pci:v1234dFFFF", "", false).is_err());
        assert!(test_module_plan(root.path(), "blacklist demo\n", "demo", "", false).is_ok());
        assert!(test_module_plan(root.path(), "install demo /bin/false\n", "demo", "", false).is_err());
        assert!(test_module_plan(root.path(), "alias a b\nalias b a\n", "a", "", false).is_err());
    }

    #[test]
    fn module_plan_reports_missing_dependencies_cycles_and_builtin_dependencies() {
        let root = TempDir::new().expect("tempdir");
        fs::write(root.path().join("demo.ko"), module_bytes(&["depends=missing"])).unwrap();
        assert!(test_module_plan(root.path(), "", "demo", "", false).unwrap_err().contains("missing"));
        fs::write(root.path().join("modules.builtin"), "kernel/missing.ko\n").unwrap();
        assert_eq!(test_module_plan(root.path(), "", "demo", "", false).unwrap(), vec![("demo".into(), "".into())]);
        assert!(test_module_plan(root.path(), "", "missing", "", true).unwrap().is_empty());
        fs::write(root.path().join("modules.builtin"), "").unwrap();
        fs::write(root.path().join("missing.ko"), module_bytes(&["depends=demo"])).unwrap();
        assert!(test_module_plan(root.path(), "", "demo", "", false).unwrap_err().contains("cycle"));
        assert!(depmod_impl_in_root(root.path(), span()).unwrap_err().message.contains("cycle"));
    }

    #[test]
    fn depmod_writes_alias_and_softdep_indices_and_preserves_declared_fields() {
        let root = TempDir::new().expect("tempdir");
        fs::write(root.path().join("demo.ko"), module_bytes(&["alias=pci:a*", "alias=pci:b*", "softdep=pre: before post: after", "parm=debug:Debug", "parmtype=debug:bool"])).unwrap();
        let record = modinfo_impl_in_root("demo", root.path(), span()).unwrap();
        assert_eq!(str_field(record_value(&list_field(record_value(&record), "params")[0]), "type"), "bool");
        depmod_impl_in_root(root.path(), span()).unwrap();
        assert_eq!(fs::read_to_string(root.path().join("modules.alias")).unwrap(), "alias pci:a* demo\nalias pci:b* demo\n");
        assert_eq!(fs::read_to_string(root.path().join("modules.softdep")).unwrap(), "softdep demo pre: before post: after\n");
        fs::write(root.path().join("broken.ko"), b"not ELF").unwrap();
        assert!(ModuleIndex::scan(root.path()).is_err());
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
        let mut records = open_files_impl(Some(pid), span()).expect("open files");
        let mut found = false;
        while let Some(record) = records.next(span()).expect("read open file") {
            let record = record_value(&record);
            if int_field(record, "pid") == pid
                && int_field(record, "fd") >= 0
                && !str_field(record, "type").is_empty()
            {
                found = true;
                break;
            }
        }
        assert!(found, "no descriptor found for current process");
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
