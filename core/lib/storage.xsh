##! Conventional storage command presentation over typed Linux APIs and collectors.
use gnu
use sys_block as block
use sys_mount as mounts
use system_report as report

# Names of the libfdisk partition-type tables, one `KEY name` per line.
const DOS_TYPE_NAMES = """
  0 Empty
  1 FAT12
  2 XENIX root
  3 XENIX usr
  4 FAT16 <32M
  5 Extended
  6 FAT16
  7 HPFS/NTFS/exFAT
  8 AIX
  9 AIX bootable
  a OS/2 Boot Manager
  b W95 FAT32
  c W95 FAT32 (LBA)
  e W95 FAT16 (LBA)
  f W95 Ext'd (LBA)
  10 OPUS
  11 Hidden FAT12
  12 Compaq diagnostics
  14 Hidden FAT16 <32M
  16 Hidden FAT16
  17 Hidden HPFS/NTFS
  18 AST SmartSleep
  1b Hidden W95 FAT32
  1c Hidden W95 FAT32 (LBA)
  1e Hidden W95 FAT16 (LBA)
  24 NEC DOS
  27 Hidden NTFS WinRE
  39 Plan 9
  3c PartitionMagic recovery
  40 Venix 80286
  41 PPC PReP Boot
  42 SFS
  4d QNX4.x
  4e QNX4.x 2nd part
  4f QNX4.x 3rd part
  50 OnTrack DM
  51 OnTrack DM6 Aux1
  52 CP/M
  53 OnTrack DM6 Aux3
  54 OnTrackDM6
  55 EZ-Drive
  56 Golden Bow
  5c Priam Edisk
  61 SpeedStor
  63 GNU HURD or SysV
  64 Novell Netware 286
  65 Novell Netware 386
  70 DiskSecure Multi-Boot
  75 PC/IX
  80 Old Minix
  81 Minix / old Linux
  82 Linux swap / Solaris
  83 Linux
  84 OS/2 hidden or Intel hibernation
  85 Linux extended
  86 NTFS volume set
  87 NTFS volume set
  88 Linux plaintext
  8e Linux LVM
  93 Amoeba
  94 Amoeba BBT
  9f BSD/OS
  a0 IBM Thinkpad hibernation
  a5 FreeBSD
  a6 OpenBSD
  a7 NeXTSTEP
  a8 Darwin UFS
  a9 NetBSD
  ab Darwin boot
  af HFS / HFS+
  b7 BSDI fs
  b8 BSDI swap
  bb Boot Wizard hidden
  bc Acronis FAT32 LBA
  be Solaris boot
  bf Solaris
  c1 DRDOS/sec (FAT-12)
  c4 DRDOS/sec (FAT-16 < 32M)
  c6 DRDOS/sec (FAT-16)
  c7 Syrinx
  da Non-FS data
  db CP/M / CTOS / ...
  de Dell Utility
  df BootIt
  e1 DOS access
  e3 DOS R/O
  e4 SpeedStor
  ea Linux extended boot
  eb BeOS fs
  ee GPT
  ef EFI (FAT-12/16/32)
  f0 Linux/PA-RISC boot
  f1 SpeedStor
  f4 SpeedStor
  f2 DOS secondary
  f8 EBBR protective
  fb VMware VMFS
  fc VMware VMKCORE
  fd Linux raid autodetect
  fe LANstep
  ff BBT
  """

const GPT_TYPE_NAMES = """
  C12A7328-F81F-11D2-BA4B-00A0C93EC93B EFI System
  024DEE41-33E7-11D3-9D69-0008C781F39F MBR partition scheme
  D3BFE2DE-3DAF-11DF-BA40-E3A556D89593 Intel Fast Flash
  21686148-6449-6E6F-744E-656564454649 BIOS boot
  F4019732-066E-4E12-8273-346C5641494F Sony boot partition
  BFBFAFE7-A34F-448A-9A5B-6213EB736C22 Lenovo boot partition
  9E1A2D38-C612-4316-AA26-8B49521E5A8B PowerPC PReP boot
  7412F7D5-A156-4B13-81DC-867174929325 ONIE boot
  D4E6E2CD-4469-46F3-B5CB-1BFF57AFC149 ONIE config
  E3C9E316-0B5C-4DB8-817D-F92DF00215AE Microsoft reserved
  EBD0A0A2-B9E5-4433-87C0-68B6B72699C7 Microsoft basic data
  5808C8AA-7E8F-42E0-85D2-E1E90434CFB3 Microsoft LDM metadata
  AF9B60A0-1431-4F62-BC68-3311714A69AD Microsoft LDM data
  DE94BBA4-06D1-4D40-A16A-BFD50179D6AC Windows recovery environment
  37AFFC90-EF7D-4E96-91C3-2D7AE055B174 IBM General Parallel Fs
  E75CAF8F-F680-4CEE-AFA3-B001E56EFC2D Microsoft Storage Spaces
  75894C1E-3AEB-11D3-B7C1-7B03A0000000 HP-UX data
  E2A1E728-32E3-11D6-A682-7B03A0000000 HP-UX service
  0657FD6D-A4AB-43C4-84E5-0933C84B4F4F Linux swap
  0FC63DAF-8483-4772-8E79-3D69D8477DE4 Linux filesystem
  3B8F8425-20E0-4F3B-907F-1A25A76F98E8 Linux server data
  44479540-F297-41B2-9AF7-D131D5F0458A Linux root (x86)
  4F68BCE3-E8CD-4DB1-96E7-FBCAF984B709 Linux root (x86-64)
  6523F8AE-3EB1-4E2A-A05A-18B695AE656F Linux root (Alpha)
  D27F46ED-2919-4CB8-BD25-9531F3C16534 Linux root (ARC)
  69DAD710-2CE4-4E3C-B16C-21A1D49ABED3 Linux root (ARM)
  B921B045-1DF0-41C3-AF44-4C6F280D3FAE Linux root (ARM-64)
  993D8D3D-F80E-4225-855A-9DAF8ED7EA97 Linux root (IA-64)
  77055800-792C-4F94-B39A-98C91B762BB6 Linux root (LoongArch-64)
  37C58C8A-D913-4156-A25F-48B1B64E07F0 Linux root (MIPS-32 LE)
  700BDA43-7A34-4507-B179-EEB93D7A7CA3 Linux root (MIPS-64 LE)
  1AACDB3B-5444-4138-BD9E-E5C2239B2346 Linux root (HPPA/PARISC)
  1DE3F1EF-FA98-47B5-8DCD-4A860A654D78 Linux root (PPC)
  912ADE1D-A839-4913-8964-A10EEE08FBD2 Linux root (PPC64)
  C31C45E6-3F39-412E-80FB-4809C4980599 Linux root (PPC64LE)
  60D5A7FE-8E7D-435C-B714-3DD8162144E1 Linux root (RISC-V-32)
  72EC70A6-CF74-40E6-BD49-4BDA08E8F224 Linux root (RISC-V-64)
  08A7ACEA-624C-4A20-91E8-6E0FA67D23F9 Linux root (S390)
  5EEAD9A9-FE09-4A1E-A1D7-520D00531306 Linux root (S390X)
  C50CDD70-3862-4CC3-90E1-809A8C93EE2C Linux root (TILE-Gx)
  8DA63339-0007-60C0-C436-083AC8230908 Linux reserved
  933AC7E1-2EB4-4F13-B844-0E14E2AEF915 Linux home
  A19D880F-05FC-4D3B-A006-743F0F84911E Linux RAID
  E6D6D379-F507-44C2-A23C-238F2A3DF928 Linux LVM
  4D21B016-B534-45C2-A9FB-5C16E091FD2D Linux variable data
  7EC6F557-3BC5-4ACA-B293-16EF5DF639D1 Linux temporary data
  75250D76-8CC6-458E-BD66-BD47CC81A812 Linux /usr (x86)
  8484680C-9521-48C6-9C11-B0720656F69E Linux /usr (x86-64)
  E18CF08C-33EC-4C0D-8246-C6C6FB3DA024 Linux /usr (Alpha)
  7978A683-6316-4922-BBEE-38BFF5A2FECC Linux /usr (ARC)
  7D0359A3-02B3-4F0A-865C-654403E70625 Linux /usr (ARM)
  B0E01050-EE5F-4390-949A-9101B17104E9 Linux /usr (ARM-64)
  4301D2A6-4E3B-4B2A-BB94-9E0B2C4225EA Linux /usr (IA-64)
  E611C702-575C-4CBE-9A46-434FA0BF7E3F Linux /usr (LoongArch-64)
  0F4868E9-9952-4706-979F-3ED3A473E947 Linux /usr (MIPS-32 LE)
  C97C1F32-BA06-40B4-9F22-236061B08AA8 Linux /usr (MIPS-64 LE)
  DC4A4480-6917-4262-A4EC-DB9384949F25 Linux /usr (HPPA/PARISC)
  7D14FEC5-CC71-415D-9D6C-06BF0B3C3EAF Linux /usr (PPC)
  2C9739E2-F068-46B3-9FD0-01C5A9AFBCCA Linux /usr (PPC64)
  15BB03AF-77E7-4D4A-B12B-C0D084F7491C Linux /usr (PPC64LE)
  B933FB22-5C3F-4F91-AF90-E2BB0FA50702 Linux /usr (RISC-V-32)
  BEAEC34B-8442-439B-A40B-984381ED097D Linux /usr (RISC-V-64)
  CD0F869B-D0FB-4CA0-B141-9EA87CC78D66 Linux /usr (S390)
  8A4F5770-50AA-4ED3-874A-99B710DB6FEA Linux /usr (S390X)
  55497029-C7C1-44CC-AA39-815ED1558630 Linux /usr (TILE-Gx)
  D13C5D3B-B5D1-422A-B29F-9454FDC89D76 Linux root verity (x86)
  2C7357ED-EBD2-46D9-AEC1-23D437EC2BF5 Linux root verity (x86-64)
  FC56D9E9-E6E5-4C06-BE32-E74407CE09A5 Linux root verity (Alpha)
  24B2D975-0F97-4521-AFA1-CD531E421B8D Linux root verity (ARC)
  7386CDF2-203C-47A9-A498-F2ECCE45A2D6 Linux root verity (ARM)
  DF3300CE-D69F-4C92-978C-9BFB0F38D820 Linux root verity (ARM-64)
  86ED10D5-B607-45BB-8957-D350F23D0571 Linux root verity (IA-64)
  F3393B22-E9AF-4613-A948-9D3BFBD0C535 Linux root verity (LoongArch-64)
  D7D150D2-2A04-4A33-8F12-16651205FF7B Linux root verity (MIPS-32 LE)
  16B417F8-3E06-4F57-8DD2-9B5232F41AA6 Linux root verity (MIPS-64 LE)
  D212A430-FBC5-49F9-A983-A7FEEF2B8D0E Linux root verity (HPPA/PARISC)
  98CFE649-1588-46DC-B2F0-ADD147424925 Linux root verity (PPC)
  9225A9A3-3C19-4D89-B4F6-EEFF88F17631 Linux root verity (PPC64)
  906BD944-4589-4AAE-A4E4-DD983917446A Linux root verity (PPC64LE)
  AE0253BE-1167-4007-AC68-43926C14C5DE Linux root verity (RISC-V-32)
  B6ED5582-440B-4209-B8DA-5FF7C419EA3D Linux root verity (RISC-V-64)
  7AC63B47-B25C-463B-8DF8-B4A94E6C90E1 Linux root verity (S390)
  B325BFBE-C7BE-4AB8-8357-139E652D2F6B Linux root verity (S390X)
  966061EC-28E4-4B2E-B4A5-1F0A825A1D84 Linux root verity (TILE-Gx)
  8F461B0D-14EE-4E81-9AA9-049B6FB97ABD Linux /usr verity (x86)
  77FF5F63-E7B6-4633-ACF4-1565B864C0E6 Linux /usr verity (x86-64)
  8CCE0D25-C0D0-4A44-BD87-46331BF1DF67 Linux /usr verity (Alpha)
  FCA0598C-D880-4591-8C16-4EDA05C7347C Linux /usr verity (ARC)
  C215D751-7BCD-4649-BE90-6627490A4C05 Linux /usr verity (ARM)
  6E11A4E7-FBCA-4DED-B9E9-E1A512BB664E Linux /usr verity (ARM-64)
  6A491E03-3BE7-4545-8E38-83320E0EA880 Linux /usr verity (IA-64)
  F46B2C26-59AE-48F0-9106-C50ED47F673D Linux /usr verity (LoongArch-64)
  46B98D8D-B55C-4E8F-AAB3-37FCA7F80752 Linux /usr verity (MIPS-32 LE)
  3C3D61FE-B5F3-414D-BB71-8739A694A4EF Linux /usr verity (MIPS-64 LE)
  5843D618-EC37-48D7-9F12-CEA8E08768B2 Linux /usr verity (HPPA/PARISC)
  DF765D00-270E-49E5-BC75-F47BB2118B09 Linux /usr verity (PPC)
  BDB528A5-A259-475F-A87D-DA53FA736A07 Linux /usr verity (PPC64)
  EE2B9983-21E8-4153-86D9-B6901A54D1CE Linux /usr verity (PPC64LE)
  CB1EE4E3-8CD0-4136-A0A4-AA61A32E8730 Linux /usr verity (RISC-V-32)
  8F1056BE-9B05-47C4-81D6-BE53128E5B54 Linux /usr verity (RISC-V-64)
  B663C618-E7BC-4D6D-90AA-11B756BB1797 Linux /usr verity (S390)
  31741CC4-1A2A-4111-A581-E00B447D2D06 Linux /usr verity (S390X)
  2FB4BF56-07FA-42DA-8132-6B139F2026AE Linux /usr verity (TILE-Gx)
  5996FC05-109C-48DE-808B-23FA0830B676 Linux root verity sign. (x86)
  41092B05-9FC8-4523-994F-2DEF0408B176 Linux root verity sign. (x86-64)
  D46495B7-A053-414F-80F7-700C99921EF8 Linux root verity sign. (Alpha)
  143A70BA-CBD3-4F06-919F-6C05683A78BC Linux root verity sign. (ARC)
  42B0455F-EB11-491D-98D3-56145BA9D037 Linux root verity sign. (ARM)
  6DB69DE6-29F4-4758-A7A5-962190F00CE3 Linux root verity sign. (ARM-64)
  E98B36EE-32BA-4882-9B12-0CE14655F46A Linux root verity sign. (IA-64)
  5AFB67EB-ECC8-4F85-AE8E-AC1E7C50E7D0 Linux root verity sign. (LoongArch-64)
  C919CC1F-4456-4EFF-918C-F75E94525CA5 Linux root verity sign. (MIPS-32 LE)
  904E58EF-5C65-4A31-9C57-6AF5FC7C5DE7 Linux root verity sign. (MIPS-64 LE)
  15DE6170-65D3-431C-916E-B0DCD8393F25 Linux root verity sign. (HPPA/PARISC)
  1B31B5AA-ADD9-463A-B2ED-BD467FC857E7 Linux root verity sign. (PPC)
  F5E2C20C-45B2-4FFA-BCE9-2A60737E1AAF Linux root verity sign. (PPC64)
  D4A236E7-E873-4C07-BF1D-BF6CF7F1C3C6 Linux root verity sign. (PPC64LE)
  3A112A75-8729-4380-B4CF-764D79934448 Linux root verity sign. (RISC-V-32)
  EFE0F087-EA8D-4469-821A-4C2A96A8386A Linux root verity sign. (RISC-V-64)
  3482388E-4254-435A-A241-766A065F9960 Linux root verity sign. (S390)
  C80187A5-73A3-491A-901A-017C3FA953E9 Linux root verity sign. (S390X)
  B3671439-97B0-4A53-90F7-2D5A8F3AD47B Linux root verity sign. (TILE-Gx)
  974A71C0-DE41-43C3-BE5D-5C5CCD1AD2C0 Linux /usr verity sign. (x86)
  E7BB33FB-06CF-4E81-8273-E543B413E2E2 Linux /usr verity sign. (x86-64)
  5C6E1C76-076A-457A-A0FE-F3B4CD21CE6E Linux /usr verity sign. (Alpha)
  94F9A9A1-9971-427A-A400-50CB297F0F35 Linux /usr verity sign. (ARC)
  D7FF812F-37D1-4902-A810-D76BA57B975A Linux /usr verity sign. (ARM)
  C23CE4FF-44BD-4B00-B2D4-B41B3419E02A Linux /usr verity sign. (ARM-64)
  8DE58BC2-2A43-460D-B14E-A76E4A17B47F Linux /usr verity sign. (IA-64)
  B024F315-D330-444C-8461-44BBDE524E99 Linux /usr verity sign. (LoongArch-64)
  3E23CA0B-A4BC-4B4E-8087-5AB6A26AA8A9 Linux /usr verity sign. (MIPS-32 LE)
  F2C2C7EE-ADCC-4351-B5C6-EE9816B66E16 Linux /usr verity sign. (MIPS-64 LE)
  450DD7D1-3224-45EC-9CF2-A43A346D71EE Linux /usr verity sign. (HPPA/PARISC)
  7007891D-D371-4A80-86A4-5CB875B9302E Linux /usr verity sign. (PPC)
  0B888863-D7F8-4D9E-9766-239FCE4D58AF Linux /usr verity sign. (PPC64)
  C8BFBD1E-268E-4521-8BBA-BF314C399557 Linux /usr verity sign. (PPC64LE)
  C3836A13-3137-45BA-B583-B16C50FE5EB4 Linux /usr verity sign. (RISC-V-32)
  D2F9000A-7A18-453F-B5CD-4D32F77A7B32 Linux /usr verity sign. (RISC-V-64)
  17440E4F-A8D0-467F-A46E-3912AE6EF2C5 Linux /usr verity sign. (S390)
  3F324816-667B-46AE-86EE-9B0C0C6C11B4 Linux /usr verity sign. (S390X)
  4EDE75E2-6CCC-4CC8-B9C7-70334B087510 Linux /usr verity sign. (TILE-Gx)
  BC13C2FF-59E6-4262-A352-B275FD6F7172 Linux extended boot
  773F91EF-66D4-49B5-BD83-D683BF40AD16 Linux user's home
  516E7CB4-6ECF-11D6-8FF8-00022D09712B FreeBSD data
  83BD6B9D-7F41-11DC-BE0B-001560B84F0F FreeBSD boot
  516E7CB5-6ECF-11D6-8FF8-00022D09712B FreeBSD swap
  516E7CB6-6ECF-11D6-8FF8-00022D09712B FreeBSD UFS
  516E7CBA-6ECF-11D6-8FF8-00022D09712B FreeBSD ZFS
  516E7CB8-6ECF-11D6-8FF8-00022D09712B FreeBSD Vinum
  48465300-0000-11AA-AA11-00306543ECAC Apple HFS/HFS+
  7C3457EF-0000-11AA-AA11-00306543ECAC Apple APFS
  55465300-0000-11AA-AA11-00306543ECAC Apple UFS
  52414944-0000-11AA-AA11-00306543ECAC Apple RAID
  52414944-5F4F-11AA-AA11-00306543ECAC Apple RAID offline
  426F6F74-0000-11AA-AA11-00306543ECAC Apple boot
  4C616265-6C00-11AA-AA11-00306543ECAC Apple label
  5265636F-7665-11AA-AA11-00306543ECAC Apple TV recovery
  53746F72-6167-11AA-AA11-00306543ECAC Apple Core storage
  69646961-6700-11AA-AA11-00306543ECAC Apple Silicon boot
  52637672-7900-11AA-AA11-00306543ECAC Apple Silicon recovery
  6A82CB45-1DD2-11B2-99A6-080020736631 Solaris boot
  6A85CF4D-1DD2-11B2-99A6-080020736631 Solaris root
  6A898CC3-1DD2-11B2-99A6-080020736631 ZFS pool member
  6A87C46F-1DD2-11B2-99A6-080020736631 Solaris swap
  6A8B642B-1DD2-11B2-99A6-080020736631 Solaris backup
  6A8EF2E9-1DD2-11B2-99A6-080020736631 Solaris /var
  6A90BA39-1DD2-11B2-99A6-080020736631 Solaris /home
  6A9283A5-1DD2-11B2-99A6-080020736631 Solaris alternate sector
  6A945A3B-1DD2-11B2-99A6-080020736631 Solaris reserved 1
  6A9630D1-1DD2-11B2-99A6-080020736631 Solaris reserved 2
  6A980767-1DD2-11B2-99A6-080020736631 Solaris reserved 3
  6A96237F-1DD2-11B2-99A6-080020736631 Solaris reserved 4
  6A8D2AC7-1DD2-11B2-99A6-080020736631 Solaris reserved 5
  49F48D32-B10E-11DC-B99B-0019D1879648 NetBSD swap
  49F48D5A-B10E-11DC-B99B-0019D1879648 NetBSD FFS
  49F48D82-B10E-11DC-B99B-0019D1879648 NetBSD LFS
  2DB519C4-B10F-11DC-B99B-0019D1879648 NetBSD concatenated
  2DB519EC-B10F-11DC-B99B-0019D1879648 NetBSD encrypted
  49F48DAA-B10E-11DC-B99B-0019D1879648 NetBSD RAID
  FE3A2A5D-4F32-41A7-B725-ACCC3285A309 ChromeOS kernel
  3CB8E202-3B7E-47DD-8A3C-7FF2A13CFCEC ChromeOS root fs
  2E0A753D-9E48-43B0-8337-B15192CB1B5E ChromeOS reserved
  CAB6E88E-ABF3-4102-A07A-D4BB9BE3C1D3 ChromeOS firmware
  09845860-705F-4BB5-B16C-8A8A099CAF52 ChromeOS miniOS
  3F0F8318-F146-4E6B-8222-C28C8F02E0D5 ChromeOS hibernate
  85D5E45A-237C-11E1-B4B3-E89A8F7FC3A7 MidnightBSD data
  85D5E45E-237C-11E1-B4B3-E89A8F7FC3A7 MidnightBSD boot
  85D5E45B-237C-11E1-B4B3-E89A8F7FC3A7 MidnightBSD swap
  0394EF8B-237E-11E1-B4B3-E89A8F7FC3A7 MidnightBSD UFS
  85D5E45D-237C-11E1-B4B3-E89A8F7FC3A7 MidnightBSD ZFS
  85D5E45C-237C-11E1-B4B3-E89A8F7FC3A7 MidnightBSD Vinum
  45B0969E-9B03-4F30-B4C6-B4B80CEFF106 Ceph Journal
  45B0969E-9B03-4F30-B4C6-5EC00CEFF106 Ceph Encrypted Journal
  4FBD7E29-9D25-41B8-AFD0-062C0CEFF05D Ceph OSD
  4FBD7E29-9D25-41B8-AFD0-5EC00CEFF05D Ceph crypt OSD
  89C57F98-2FE5-4DC0-89C1-F3AD0CEFF2BE Ceph disk in creation
  89C57F98-2FE5-4DC0-89C1-5EC00CEFF2BE Ceph crypt disk in creation
  AA31E02A-400F-11DB-9590-000C2911D1B8 VMware VMFS
  9D275380-40AD-11DB-BF97-000C2911D1B8 VMware Diagnostic
  381CFCCC-7288-11E0-92EE-000C2911D0B2 VMware Virtual SAN
  77719A0C-A4A0-11E3-A47E-000C29745A24 VMware Virsto
  9198EFFC-31C0-11DB-8F78-000C2911D1B8 VMware Reserved
  824CC7A0-36A8-11E3-890A-952519AD3F61 OpenBSD data
  CEF5A9AD-73BC-4601-89F3-CDEEEEE321A1 QNX6 file system
  C91818F9-8025-47AF-89D2-F030D7000C2C Plan 9 partition
  5B193300-FC78-40CD-8002-E86C45580B47 HiFive FSBL
  2E54B353-1271-4842-806F-E436D6AF6985 HiFive BBL
  42465331-3BA3-10F1-802A-4861696B7521 Haiku BFS
  6828311A-BA55-42A4-BCDE-A89BB5EDECAE Marvell Armada 3700 Boot partition
  9D087404-1CA5-11DC-8817-01301BB8A9F5 DragonFlyBSD Label32
  9D58FDBD-1CA5-11DC-8817-01301BB8A9F5 DragonFlyBSD Swap
  9D94CE7C-1CA5-11DC-8817-01301BB8A9F5 DragonFlyBSD UFS1
  9DD4478F-1CA5-11DC-8817-01301BB8A9F5 DragonFlyBSD Vinum
  DBD5211B-1CA5-11DC-8817-01301BB8A9F5 DragonFlyBSD CCD
  3D48CE54-1D16-11DC-8696-01301BB8A9F5 DragonFlyBSD Label64
  BD215AB2-1D16-11DC-8696-01301BB8A9F5 DragonFlyBSD Legacy
  61DC63AC-6E38-11DC-8513-01301BB8A9F5 DragonFlyBSD HAMMER
  5CBB9AD1-862D-11DC-A94D-01301BB8A9F5 DragonFlyBSD HAMMER2
  3DE21764-95BD-54BD-A5C3-4ABE786F38A8 U-Boot environment
  734E5AFE-F61A-11E6-BC64-92361F002671 Atari TOS basic data
  35540011-B055-499F-842D-C69AECA357B7 Atari TOS raw data (XHDI)
  481B2A38-0561-420B-B72A-F1C4988EFC16 Minix filesystem
  """

# fdisk's own column names; only the columns of the label in use are valid.
const FDISK_COLUMNS = ["Device", "Boot", "Start", "End", "Sectors", "Size", "Id", "Type", "Type-UUID", "Attrs", "Name", "UUID"]

type Arguments = {flags: List[Str], values: Map[Str], multiple: Map[List[Str]], operands: List[Str]}

proc unsupported(value: Str) {
  gnu.usage_error(f"{value} is not supported")
}

# Only declared options reach controllers; reject a complete invocation before
# opening a device, including any unknown option after a valid operand.
proc arguments(argv: List[Str], booleans: List[Str], valued: List[Str]) -> Arguments {
  var flags: List[Str] = []
  var values: Map[Str] = {}
  var multiple: Map[List[Str]] = {}
  var operands: List[Str] = []
  var index = 0
  var ended = false
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if ended or ! word.starts_with("-") or word == "-" { operands += [word]; continue }
    if word == "--" { ended = true; continue }
    let pieces = word.split("=", maxsplit: 1)
    let option = pieces[0]
    var found = false
    for forms in booleans {
      let aliases = forms.split(" ")
      if option in aliases {
        if pieces.len() > 1 { gnu.usage_error(f"option {option} does not take an argument") }
        flags += [aliases[0]]
        found = true
        break
      }
    }
    if found { continue }
    for forms in valued {
      let aliases = forms.split(" ")
      var attached: Str? = null
      if option not in aliases and word.starts_with("-") and ! word.starts_with("--") and word.byte_len() > 2 and word.byte_slice(0, 2) in aliases {
        attached = word.byte_slice(2)
      }
      if option in aliases or attached != null {
        var value = attached ?? ""
        if attached == null {
          if pieces.len() == 2 { value = pieces[1] } else {
            if index >= argv.len() { gnu.usage_error(f"option {option} requires an argument") }
            value = argv[index]
            index += 1
          }
        }
        values = values.set(aliases[0], value)
        multiple = multiple.set(aliases[0], (multiple.get(aliases[0]) ?? []) + [value])
        found = true
        break
      }
    }
    if found { continue }
    if ! word.starts_with("--") and word.byte_len() > 2 {
      var bundled: List[Str] = []
      for char in word.byte_slice(1) {
        var canonical: Str? = null
        for forms in booleans {
          let aliases = forms.split(" ")
          if f"-{char}" in aliases { canonical = aliases[0]; break }
        }
        if canonical == null { unsupported(f"option {gnu.quote(word)}") }
        bundled += [canonical ?? ""]
      }
      flags += bundled
      continue
    }
    unsupported(f"option {gnu.quote(word)}")
  }
  {flags: flags, values: values, multiple: multiple, operands: operands}
}

# Name the operand that failed: with several operands, a bare kernel message
# cannot say which one was refused. Ends the applet with status 1.
proc operand_failed(name: Str, reason: Str) {
  gnu.error(f"{name}: {reason}")
  exit 1
}

proc require_operands(args: Arguments, minimum: Int, maximum: Int) {
  if args.operands.len() < minimum { gnu.usage_error("missing operand") }
  if maximum >= 0 and args.operands.len() > maximum { gnu.extra_operand(args.operands[maximum]) }
}

## Match util-linux filesystem lists; a leading no excludes the entire list.
export pure type_matches(filesystem: Str, filter: Str) -> Bool {
  if filter == "" { return true }
  let types = filter.split(",")
  if types[0].starts_with("no") {
    return ! (filesystem in filter.byte_slice(2).split(","))
  }
  filesystem in types
}

proc mount_table() -> mounts.MountTable {
  let root = fs.open_root(/)?
  defer root.close()
  let table = mounts.collect(root)
  if table.source_state != report.Observed { gnu.error("cannot read mount table"); exit 1 }
  table
}

pure mount_value(entry: mounts.MountEntry, column: Str) -> Str {
  match column {
    "TARGET" => entry.target,
    "SOURCE" => entry.source,
    "FSTYPE" => entry.filesystem,
    "OPTIONS" => (entry.mount_options + [item for item in entry.super_options if item not in entry.mount_options]).join(","),
    "VFS-OPTIONS" => entry.mount_options.join(","),
    "FS-OPTIONS" => entry.super_options.join(","),
    "MAJ:MIN" => f"{entry.major}:{entry.minor}",
    "FSROOT" => entry.root,
    "ID" => f"{entry.mount_id}",
    "PARENT" => f"{entry.parent_id}",
    _ => "",
  }
}

proc columns(value: Str, allowed: List[Str]) -> List[Str] {
  let selected = value.split(",")
  for column in selected { if column not in allowed { gnu.usage_error(f"unknown column: {column}") } }
  selected
}

proc json_fields(keys: List[Str], values: List[Str]) -> Str {
  var fields: List[Str] = []
  for index in range(keys.len()) { fields += [json.encode(keys[index].lower())? + ":" + json.encode(values[index])?] }
  "{" + fields.join(",") + "}"
}

# A containing-path lookup chooses the deepest mount at a component boundary.
# Repeated targets resolve to the newest visible mount rather than prefix text.
pure target_mount(entries: List[mounts.MountEntry], target: Str) -> Int? {
  var selected: Int? = null
  var length = -1
  for index in range(entries.len()) {
    let entry = entries[index]
    if (target == entry.target or entry.target == "/" or target.starts_with(entry.target + "/")) and entry.target.byte_len() >= length {
      selected = index
      length = entry.target.byte_len()
    }
  }
  selected
}

pure raw_field(value: Str) -> Str {
  value.replace("\\", with: "\\x5c").replace(" ", with: "\\x20").replace("\t", with: "\\x09").replace("\n", with: "\\x0a")
}

pure mount_children(entries: List[mounts.MountEntry], index: Int) -> List[Int] {
  [at for at in range(entries.len()) if entries[at].parent_id == entries[index].mount_id and at != index]
}

proc mount_json_row(entries: List[mounts.MountEntry], index: Int, cols: List[Str], ancestors: List[Int]) -> Str {
  let row = json_fields(cols, [mount_value(entries[index], column) for column in cols])
  var children: List[Str] = []
  for child in mount_children(entries, index) { if child not in ancestors { children += [mount_json_row(entries, child, cols, ancestors + [index])] } }
  if children.is_empty() { return row }
  row.byte_slice(0, row.byte_len() - 1) + ",\"children\":[" + children.join(",") + "]}"
}

proc mount_text_row(entries: List[mounts.MountEntry], index: Int, cols: List[Str], ancestors: List[Int], prefix: Str) {
  var values = [mount_value(entries[index], column) for column in cols]
  for at in range(cols.len()) { if cols[at] == "TARGET" { values[at] = prefix + values[at] } }
  gnu.write_text(values.join(" ") + "\n")
  for child in mount_children(entries, index) { if child not in ancestors { mount_text_row(entries, child, cols, ancestors + [index], prefix + "  ") } }
}

## Render a rooted mount table using decoded targets and mount identities.
export proc findmnt_from_root(root: FsRoot, argv: List[Str]) {
  let args = arguments(argv, ["-J --json", "-n --noheadings", "-r --raw", "-l --list"], ["-t --types", "-S --source", "-T --target", "-o --output"])
  require_operands(args, 0, 1)
  let cols = columns(args.values.get("-o") ?? "TARGET,SOURCE,FSTYPE,OPTIONS", ["TARGET", "SOURCE", "FSTYPE", "OPTIONS", "VFS-OPTIONS", "FS-OPTIONS", "MAJ:MIN", "FSROOT", "ID", "PARENT"])
  let table = mounts.collect(root)
  if table.source_state != report.Observed { gnu.error("cannot read mount table"); exit 1 }
  var entries = table.mounts
  let source: Str? = if "-S" in args.values { args.values.get("-S")? } else { null }
  if source != null { entries = [entry for entry in entries if entry.source == source] }
  let target: Str? = if "-T" in args.values { args.values.get("-T")? } else { null }
  if target != null {
    let absolute = fp"{target}".resolve()?
    let selected = target_mount(entries, f"{absolute}")
    entries = if selected == null { [] } else { [entries[selected]] }
  }
  if ! args.operands.is_empty() { entries = [entry for entry in entries if entry.source == args.operands[0] or entry.target == args.operands[0]] }
  entries = [entry for entry in entries if type_matches(entry.filesystem, args.values.get("-t") ?? "")]
  if entries.is_empty() { exit 1 }
  let tree = "-l" not in args.flags and "-r" not in args.flags and "TARGET" in cols
  var roots: List[Int] = []
  let ids = [entry.mount_id for entry in entries]
  for index in range(entries.len()) { if ! tree or entries[index].parent_id not in ids or entries[index].parent_id == entries[index].mount_id { roots += [index] } }
  if roots.is_empty() { gnu.error("mount table contains a cyclic hierarchy"); exit 1 }
  if "-J" in args.flags {
    let rows = if tree { [mount_json_row(entries, index, cols, []) for index in roots] } else { [json_fields(cols, [mount_value(entry, column) for column in cols]) for entry in entries] }
    gnu.write_text("{\"filesystems\":[" + rows.join(",") + "]}\n")
    return
  }
  if "-n" not in args.flags { gnu.write_text(cols.join(" ") + "\n") }
  if tree { for index in roots { mount_text_row(entries, index, cols, [], "") } } else {
    for entry in entries {
      let values = [mount_value(entry, column) for column in cols]
      gnu.write_text((if "-r" in args.flags { [raw_field(value) for value in values] } else { values }).join(" ") + "\n")
    }
  }
}

proc findmnt(argv: List[Str]) {
  let root = fs.open_root(/)?
  defer root.close()
  findmnt_from_root(root, argv)
}

pure block_children(devices: List[block.BlockDevice], parent: Int) -> List[Int] {
  var children: List[Int] = []
  for index in range(devices.len()) {
    if devices[index].parent_device_index == parent or parent in devices[index].slave_indices { children += [index] }
  }
  children
}

proc block_values(device: block.BlockDevice, table: mounts.MountTable, cols: List[Str], byte_sizes: Bool, paths: Bool) -> List[Str] {
  let mounted = [entry.target for entry in table.mounts if entry.major == device.major and entry.minor == device.minor]
  var metadata = {type: "", uuid: "", label: "", part_table_type: "", part_entry_uuid: ""}
  if "FSTYPE" in cols or "UUID" in cols or "LABEL" in cols {
    metadata = linux.blkid(fp"/dev/{device.name}")?
  }
  let size = device.size_bytes
  let kind = if device.kind == "partition" { "part" } else if device.name.starts_with("loop") { "loop" } else if ! device.slave_indices.is_empty() { "dm" } else { "disk" }
  let fields: Map[Str] = {
    "NAME": if paths { f"/dev/{device.name}" } else { device.name }, "KNAME": device.name, "PATH": f"/dev/{device.name}",
    "MAJ:MIN": if device.major != null and device.minor != null { f"{device.major}:{device.minor}" } else { "" },
    "SIZE": if size == null { "" } else if byte_sizes { f"{size}" } else { bytes.human(size) },
    "RM": if device.removable == null { "" } else if device.removable { "1" } else { "0" },
    "RO": if device.read_only == null { "" } else if device.read_only { "1" } else { "0" },
    "TYPE": kind, "PKNAME": device.parent_name ?? "", "MOUNTPOINT": if mounted.is_empty() { "" } else { mounted[-1] }, "MOUNTPOINTS": mounted.join("\n"),
    "FSTYPE": metadata.type, "UUID": metadata.uuid, "LABEL": metadata.label, "MODEL": device.model.value ?? "",
    "LOG-SEC": if device.logical_sector_bytes == null { "" } else { f"{device.logical_sector_bytes}" },
    "PHY-SEC": if device.physical_sector_bytes == null { "" } else { f"{device.physical_sector_bytes}" },
  }
  [fields.get(column) ?? "" for column in cols]
}

proc block_json_row(devices: List[block.BlockDevice], index: Int, table: mounts.MountTable, cols: List[Str], args: Arguments, ancestors: List[Int]) -> Str {
  let values = block_values(devices[index], table, cols, "-b" in args.flags, "-p" in args.flags)
  var fields: List[Str] = []
  for at in range(cols.len()) {
    let key = json.encode(cols[at].lower())?
    let value = values[at]
    var encoded = if value == "" { "null" } else { json.encode(value)? }
    if cols[at] in ["RM", "RO"] and value != "" { encoded = if value == "1" { "true" } else { "false" } }
    if (cols[at] in ["LOG-SEC", "PHY-SEC"] or (cols[at] == "SIZE" and "-b" in args.flags)) and value != "" { encoded = value }
    if cols[at] == "MOUNTPOINTS" { encoded = if value == "" { "[null]" } else { json.encode(value.split("\n"))? } }
    fields += [key + ":" + encoded]
  }
  if "-d" not in args.flags and "-l" not in args.flags and "-r" not in args.flags {
    var children: List[Str] = []
    for child in block_children(devices, index) {
      if child not in ancestors and child != index { children += [block_json_row(devices, child, table, cols, args, ancestors + [index])] }
    }
    if ! children.is_empty() { fields += ["\"children\":[" + children.join(",") + "]"] }
  }
  "{" + fields.join(",") + "}"
}

proc block_text_row(devices: List[block.BlockDevice], index: Int, table: mounts.MountTable, cols: List[Str], args: Arguments, ancestors: List[Int], prefix: Str) {
  var values = block_values(devices[index], table, cols, "-b" in args.flags, "-p" in args.flags)
  if "NAME" in cols and prefix != "" {
    for at in range(cols.len()) { if cols[at] == "NAME" { values[at] = prefix + values[at] } }
  }
  gnu.write_text((if "-r" in args.flags { [raw_field(value) for value in values] } else { values }).join(" ") + "\n")
  if "-d" not in args.flags and "-l" not in args.flags and "-r" not in args.flags {
    for child in block_children(devices, index) {
      if child not in ancestors and child != index { block_text_row(devices, child, table, cols, args, ancestors + [index], prefix + "  ") }
    }
  }
}

## Render one rooted inventory; device relationships come from collector indexes.
export proc lsblk_from_root(root: FsRoot, argv: List[Str]) {
  let args = arguments(argv, ["-a --all", "-b --bytes", "-d --nodeps", "-f --fs", "-J --json", "-l --list", "-n --noheadings", "-p --paths", "-r --raw"], ["-o --output"])
  let cols = columns(args.values.get("-o") ?? (if "-f" in args.flags { "NAME,FSTYPE,LABEL,UUID,MOUNTPOINTS" } else { "NAME,MAJ:MIN,RM,SIZE,RO,TYPE,MOUNTPOINTS" }), ["NAME", "KNAME", "PATH", "MAJ:MIN", "RM", "SIZE", "RO", "TYPE", "PKNAME", "MOUNTPOINT", "MOUNTPOINTS", "FSTYPE", "UUID", "LABEL", "MODEL", "LOG-SEC", "PHY-SEC"])
  let inventory = block.collect(root)
  if ! inventory.enumeration_succeeded { gnu.error("cannot enumerate block devices"); exit 1 }
  let table = mounts.collect(root)
  let devices = inventory.devices
  var selected: List[Int] = []
  for index in range(devices.len()) {
    let device = devices[index]
    if "-a" not in args.flags and device.size_bytes == 0 { continue }
    if ! args.operands.is_empty() {
      if f"/dev/{device.name}" in args.operands { selected += [index] }
    } else if "-d" in args.flags {
      if device.parent_device_index == null and device.slave_indices.is_empty() { selected += [index] }
    } else if "-l" in args.flags or "-r" in args.flags or (device.parent_device_index == null and device.slave_indices.is_empty()) { selected += [index] }
  }
  for operand in args.operands { if operand not in [f"/dev/{devices[index].name}" for index in selected] { gnu.error(f"{operand}: not a block device"); exit 1 } }
  if "-J" in args.flags { gnu.write_text("{\"blockdevices\":[" + [block_json_row(devices, index, table, cols, args, []) for index in selected].join(",") + "]}\n"); return }
  if "-n" not in args.flags { gnu.write_text(cols.join(" ") + "\n") }
  for index in selected { block_text_row(devices, index, table, cols, args, [], "") }
}

proc lsblk(argv: List[Str]) {
  let root = fs.open_root(/)?
  defer root.close()
  lsblk_from_root(root, argv)
}

pure hexadecimal(value: UInt) -> Str {
  var number: UInt = value
  var text = ""
  while number > 0 {
    let remainder: UInt = number % 16
    let digit: Int = remainder
    text = "0123456789abcdef".byte_slice(digit, length: 1) + text
    number /= 16
  }
  "0x" + (if text == "" { "0" } else { text })
}

# Zero-padded hexadecimal, the form wipefs prints in its erase report.
pure hex_offset(value: UInt, digits: Int) -> Str {
  var text = hexadecimal(value).byte_slice(2)
  while text.byte_len() < digits { text = "0" + text }
  "0x" + text
}

pure hex_bytes(data: Bytes) -> Str {
  var pairs: List[Str] = []
  for index in range(data.len()) {
    let value = data.byte_at(index) ?? 0
    pairs += ["0123456789abcdef".byte_slice(value / 16, length: 1) + "0123456789abcdef".byte_slice(value % 16, length: 1)]
  }
  pairs.join(" ")
}

# Columns padded to the widest cell, every column but a left-aligned last one.
# Empty cells keep their padding, so a trailing empty column leaves the
# separator behind exactly as the util-linux table printer does. A column with
# a minimum width widens to it only once its data outgrows its heading.
pure table_lines(headers: List[Str], right: List[Bool], minimums: List[Int], rows: List[List[Str]], headings: Bool) -> List[Str] {
  var widths: List[Int] = []
  for at in range(headers.len()) {
    var widest = 0
    for row in rows { if row[at].count_chars() > widest { widest = row[at].count_chars() } }
    let heading = headers[at].count_chars()
    var width = if headings and heading > widest { heading } else { widest }
    if widest > heading and minimums[at] > width { width = minimums[at] }
    widths += [width]
  }
  var lines: List[Str] = []
  for row in if headings { [headers] + rows } else { rows } {
    var cells: List[Str] = []
    for at in range(row.len()) {
      if right[at] { cells += [tui.left_pad(row[at], widths[at])] } else if at + 1 < row.len() { cells += [tui.right_pad(row[at], widths[at])] } else { cells += [row[at]] }
    }
    lines += [cells.join(" ")]
  }
  lines
}

# One probed magic. `recognized` marks the magics the native signature wiper
# owns; the others are the extra bytes of a multi-magic filesystem, which are
# erased by range.
type Signature = {offset: UInt, type: Str, usage: Str, magic: Bytes, recognized: Bool, uuid: Str, label: Str}

# The magics util-linux lists for a device, in its probing order: filesystem
# magics first, then partition tables. A FAT boot sector carries three (the
# FATnn type string, the leading jump byte and the 0x55aa trailer). Erasing
# only the type string leaves the trailer, which blkid then reads as a DOS
# partition table. A protective MBR is its own table type.
proc device_signatures(name: Str) -> Result[List[Signature]] {
  let node = fp"{name}"
  let probed = linux.block_signatures(node)?
  let identity = linux.blkid(node)?
  var systems: List[Signature] = []
  var tables: List[Signature] = []
  var fat = false
  for item in probed {
    if item.kind == "partition-table" {
      var table_type = item.type
      if table_type == "dos" {
        let slots = bytes.read_at(node, 446, 64)?
        for slot in range(4) { if (slots.byte_at(slot * 16 + 4) ?? 0) == 238 { table_type = "PMBR" } }
      }
      tables += [{offset: item.offset, type: table_type, usage: "partition-table", magic: item.magic, recognized: true, uuid: "", label: ""}]
      continue
    }
    let usage = if item.type == "swap" { "other" } else { "filesystem" }
    systems += [{offset: item.offset, type: item.type, usage: usage, magic: item.magic, recognized: true, uuid: identity.uuid, label: identity.label}]
    if item.type == "vfat" and ! fat {
      fat = true
      let lead = bytes.read_at(node, 0, 1)?
      let origin: UInt = 0
      let trailer: UInt = 510
      if lead == b"\xeb" or lead == b"\xe9" {
        systems += [{offset: origin, type: "vfat", usage: "filesystem", magic: lead, recognized: false, uuid: identity.uuid, label: identity.label}]
      }
      systems += [{offset: trailer, type: "vfat", usage: "filesystem", magic: b"\x55\xaa", recognized: false, uuid: identity.uuid, label: identity.label}]
    }
  }
  # A FAT boot sector's trailer is part of the filesystem, not a table.
  if fat { tables = [item for item in tables if item.offset != 510] }
  Ok(systems + [item for item in tables if item.type == "gpt"] + [item for item in tables if item.type != "gpt"])
}

proc wipefs(argv: List[Str]) {
  # --force is accepted because scripts pass it; this wiper never refuses a
  # partition table, so there is no refusal for it to override.
  let args = arguments(argv, ["-a --all", "-f --force", "-n --no-act", "-J --json", "--noheadings"], ["-o --offset", "-t --types", "-O --output"])
  require_operands(args, 1, -1)
  let cols = columns((args.values.get("-O") ?? "DEVICE,OFFSET,TYPE,UUID,LABEL").upper(), ["DEVICE", "OFFSET", "TYPE", "UUID", "LABEL", "LENGTH", "USAGE"])
  let given_offsets = args.multiple.get("-o") ?? []
  if "-a" in args.flags and ! given_offsets.is_empty() { gnu.usage_error("--all and --offset are mutually exclusive") }
  var offsets: List[UInt] = []
  for value in given_offsets {
    if value.starts_with("0x") {
      let digits = value.byte_slice(2)
      if digits == "" { gnu.usage_error("invalid signature offset") }
      var parsed: UInt = 0
      for char in digits.lower() {
        let at = "0123456789abcdef".find(char)
        if at == null { gnu.usage_error("invalid hexadecimal signature offset") }
        parsed = parsed * 16 + (at ?? 0) as UInt
      }
      offsets += [parsed]
    } else { offsets += [byte_count(value)] }
  }
  let erase = "-a" in args.flags or ! offsets.is_empty()
  if erase and "-J" in args.flags { gnu.usage_error("JSON cannot be combined with signature erasure") }
  # One table spans every operand, so column widths are shared.
  var rows: List[List[Str]] = []
  var json_rows: List[Str] = []
  for name in args.operands {
    guard let found = device_signatures(name) else { |failure|
      gnu.error(f"error: {name}: probing initialization failed: {gnu.strerror(failure)}")
      exit 1
    }
    let selected = [item for item in found if type_matches(item.type, args.values.get("-t") ?? "") and (offsets.is_empty() or item.offset in offsets)]
    for offset in offsets { if offset not in [item.offset for item in selected] { gnu.usage_error(f"no matching signature at offset {offset} on {name}") } }
    if erase {
      if "-n" not in args.flags {
        let native = [item.offset for item in selected if item.recognized]
        if ! native.is_empty() { linux.wipe_block_signatures(fp"{name}", native)? }
        for item in selected { if ! item.recognized { let _ = bytes.zero_at(fp"{name}", item.offset, item.magic.len())? } }
      }
      # A dry run reports the same text; the device is left untouched.
      for item in selected {
        let count = item.magic.len()
        gnu.write_text(f"{name}: {count} byte{if count == 1 { "" } else { "s" }} {if count == 1 { "was" } else { "were" }} erased at offset {hex_offset(item.offset, 8)} ({item.type}): {hex_bytes(item.magic)}\n")
      }
      continue
    }
    for item in selected {
      let values: Map[Str] = {"DEVICE": fp"{name}".basename(), "OFFSET": hexadecimal(item.offset), "TYPE": item.type, "UUID": item.uuid, "LABEL": item.label, "LENGTH": f"{item.magic.len()}", "USAGE": item.usage}
      let cells = [values.get(column) ?? "" for column in cols]
      rows += [cells]
      if "-J" in args.flags {
        var members: List[Str] = []
        for at in range(cols.len()) {
          let key = json.encode(cols[at].lower())?
          let present = cells[at] != "" or cols[at] in ["DEVICE", "OFFSET", "TYPE", "LENGTH", "USAGE"]
          members += [f"         {key}: {if present { json.encode(cells[at])? } else { "null" }}"]
        }
        json_rows += ["{\n" + members.join(",\n") + "\n      }"]
      }
    }
  }
  if erase { return }
  if "-J" in args.flags {
    gnu.write_text("{\n   \"signatures\": [\n" + (if json_rows.is_empty() { "\n" } else { "      " + json_rows.join(",") + "\n" }) + "   ]\n}\n")
    return
  }
  if rows.is_empty() { return }
  for line in table_lines(cols, [false for _ in cols], [0 for _ in cols], rows, "--noheadings" not in args.flags) { gnu.write_text(line + "\n") }
}


proc mount(argv: List[Str]) {
  let args = arguments(argv, ["-a --all", "-r --read-only", "-w --rw", "-B --bind", "-R --rbind", "-M --move"], ["-t --types", "-o --options"])
  if "-r" in args.flags and "-w" in args.flags { gnu.usage_error("read-only and read-write are mutually exclusive") }
  var types = args.values.get("-t") ?? ""
  var options = (args.values.get("-o") ?? "").split(",") |> where . != ""
  if "-B" in args.flags { options += ["bind"] }
  if "-R" in args.flags { options += ["rbind"] }
  if "-M" in args.flags { options += ["move"] }
  if "-r" in args.flags { options += ["ro"] }
  if "-w" in args.flags { options += ["rw"] }
  if "-a" in args.flags {
    require_operands(args, 0, 0)
    if types != "" or ! options.is_empty() { unsupported("filtered or option-overridden mount --all") }
    linux.mount_all()?
    return
  }
  if args.operands.is_empty() {
    if ! options.is_empty() { gnu.usage_error("missing mount source and target") }
    for entry in mount_table().mounts {
      if type_matches(entry.filesystem, types) { gnu.write_text(f"{entry.source} on {entry.target} type {entry.filesystem} ({mount_value(entry, "OPTIONS")})\n") }
    }
    return
  }
  if args.operands.len() == 1 and "remount" in options { linux.mount("none", fp"{args.operands[0]}", fstype: types, options: options)?; return }
  require_operands(args, 2, 2)
  if "," in types { unsupported("multiple filesystem types for a direct mount") }
  if (types == "" or types == "auto") and ! ("bind" in options or "rbind" in options or "move" in options) {
    types = linux.blkid(fp"{args.operands[0]}")?.type
    if types == "" { gnu.usage_error("cannot identify filesystem type; specify --types") }
  }
  linux.mount(args.operands[0], fp"{args.operands[1]}", fstype: types, options: options)?
}

proc blkid(argv: List[Str]) {
  let args = arguments(argv, ["-p --probe"], ["-o --output", "-s --match-tag"])
  let format = args.values.get("-o") ?? "full"
  if format not in ["full", "value", "export", "device"] { unsupported(f"output format {format}") }
  let selected_tags = args.multiple.get("-s") ?? []
  for tag in selected_tags { if tag not in ["TYPE", "UUID", "LABEL", "PTTYPE", "PART_ENTRY_UUID"] { unsupported(f"tag {tag}") } }
  var devices = args.operands
  if devices.is_empty() {
    for device in linux.block_devices()? { devices += [f"{device.path}"] + [f"{partition}" for partition in device.partitions] }
  }
  var observed = false
  for name in devices {
    let info = linux.blkid(fp"{name}")?
    let tags = ["UUID", "LABEL", "TYPE", "PTTYPE", "PART_ENTRY_UUID"]
    let values = [info.uuid, info.label, info.type, info.part_table_type, info.part_entry_uuid]
    var fields: List[Str] = []
    for index in range(tags.len()) {
      if values[index] == "" or (! selected_tags.is_empty() and tags[index] not in selected_tags) { continue }
      observed = true
      if format == "value" { fields += [values[index]] } else if format == "export" { fields += [f"{tags[index]}={values[index]}"] } else { fields += [tags[index] + "=" + json.encode(values[index])?] }
    }
    if fields.is_empty() { continue }
    if format == "device" { gnu.write_text(name + "\n") } else if format == "value" { gnu.write_text(fields.join("\n") + "\n") } else if format == "export" { gnu.write_text(f"DEVNAME={name}\n" + fields.join("\n") + "\n\n") } else { gnu.write_text(name + ": " + fields.join(" ") + "\n") }
  }
  if ! observed { exit 2 }
}

proc losetup(argv: List[Str]) {
  let args = arguments(argv, ["-a --all", "-l --list", "-f --find", "--show", "-d --detach", "-j --associated"], [])
  if "-d" in args.flags {
    if ! [flag for flag in args.flags if flag != "-d"].is_empty() { gnu.usage_error("detach cannot be combined with other modes") }
    require_operands(args, 1, -1)
    for device in args.operands { linux.loop_detach(fp"{device}")? }
    return
  }
  if "--show" in args.flags and "-f" not in args.flags { gnu.usage_error("--show requires --find") }
  if "-f" in args.flags {
    require_operands(args, 1, 1)
    if "-a" in args.flags or "-l" in args.flags or "-j" in args.flags { gnu.usage_error("incompatible loop modes") }
    let device = linux.loop_attach(fp"{args.operands[0]}")?
    if "--show" in args.flags { gnu.write_text(f"{device}\n") }
    return
  }
  if args.flags.is_empty() and args.operands.len() == 2 { let _ = linux.loop_attach(fp"{args.operands[1]}", device: fp"{args.operands[0]}")?; return }
  require_operands(args, if "-j" in args.flags { 1 } else { 0 }, 1)
  let listed = "-l" in args.flags
  if listed { gnu.write_text("NAME OFFSET SIZELIMIT BACK-FILE\n") }
  var found = false
  for entry in linux.loop_list()? {
    if ! args.operands.is_empty() and (("-j" in args.flags and f"{entry.file}" != args.operands[0]) or ("-j" not in args.flags and f"{entry.device}" != args.operands[0])) { continue }
    found = true
    if listed { gnu.write_text(f"{entry.device} {entry.offset} {entry.size} {entry.file}\n") } else { gnu.write_text(f"{entry.device}: ({entry.file}), offset {entry.offset}, sizelimit {entry.size}\n") }
  }
  if ! found and ! args.operands.is_empty() and "-j" not in args.flags { exit 1 }
}

proc swap_command(command: Str, argv: List[Str]) {
  let args = arguments(argv, if command == "mkswap" { [] } else { ["-a --all"] }, if command == "swapon" { ["-p --priority"] } else { [] })
  if "-a" in args.flags {
    require_operands(args, 0, 0)
    if ! args.values.is_empty() { unsupported("priority override with --all") }
    if command == "swapon" { linux.swapon_all()? } else { linux.swapoff_all()? }
    return
  }
  require_operands(args, 1, if command == "mkswap" { 1 } else { -1 })
  let priority = (args.values.get("-p") ?? "-1").parse_int()?
  if priority < -1 or priority > 32767 or ("-p" in args.values and priority < 0) { gnu.usage_error("priority must be between 0 and 32767") }
  for name in args.operands {
    if command == "mkswap" {
      let device = fp"{name}".resolve()?
      let metadata = fs.stat(device, follow_symlinks: true)?
      if metadata.kind not in ["file", "block"] { gnu.usage_error("swap target must be a regular file or block device") }
      linux.mkswap(device)?
    } else if command == "swapon" { linux.swapon(fp"{name}", priority: priority)? } else { linux.swapoff(fp"{name}")? }
  }
}

proc umount(argv: List[Str]) {
  let args = arguments(argv, ["-a --all", "-l --lazy", "-f --force"], ["-t --types"])
  if "-a" in args.flags {
    require_operands(args, 0, 0)
    if "-l" in args.flags or "-f" in args.flags { unsupported("lazy or forced --all unmount") }
    linux.umount_all(types: (args.values.get("-t") ?? "").split(",") |> where . != "")?
    return
  }
  require_operands(args, 1, -1)
  if "-t" in args.values { unsupported("filesystem type filter for explicit unmount targets") }
  # Every operand is attempted so one refused target does not hide the rest.
  var failed = false
  for name in args.operands {
    if let Err(failure) = linux.umount(fp"{name}", lazy: "-l" in args.flags, force: "-f" in args.flags) {
      gnu.error(f"{name}: {gnu.strerror(failure)}")
      failed = true
    }
  }
  if failed { exit 1 }
}

proc blockdev(argv: List[Str]) {
  let args = arguments(argv, ["--getsize64", "--getsz", "--getsize", "--getss", "--getpbsz", "--getro", "--setro", "--setrw", "--flushbufs", "--rereadpt"], [])
  require_operands(args, 1, -1)
  if args.flags.is_empty() { gnu.usage_error("missing block-device operation") }
  for name in args.operands {
    for operation in args.flags {
      if operation == "--setro" or operation == "--setrw" { linux.blockdev_set_read_only(fp"{name}", operation == "--setro")? } else if operation == "--flushbufs" { linux.blockdev_flush(fp"{name}")? } else if operation == "--rereadpt" { linux.blockdev_reread_partition_table(fp"{name}")? } else {
        let info = linux.blockdev_info(fp"{name}")?
        let value: UInt = if operation == "--getsize64" { info.size_bytes } else if operation == "--getss" { info.logical_sector_bytes } else if operation == "--getpbsz" { info.physical_sector_bytes } else if operation == "--getro" { if info.read_only { 1 } else { 0 } } else { info.size_bytes / 512 }
        gnu.write_text(f"{value}\n")
      }
    }
  }
}

proc byte_count(value: Str) -> UInt {
  if rx"^[0-9]+$".matches(value) { return value.parse_uint()? }
  if rx"^[0-9]+[KMGT]$".matches(value) {
    let number = value.byte_slice(0, value.byte_len() - 1).parse_uint()?
    let suffix = value.byte_slice(value.byte_len() - 1)
    let factor: UInt = if suffix == "K" { 1024 } else if suffix == "M" { 1048576 } else if suffix == "G" { 1073741824 } else { 1099511627776 }
    return number * factor
  }
  gnu.usage_error(f"invalid byte count: {gnu.quote(value)}")
  0
}

# fstrim names the option whose value it could not parse and, like the
# util-linux tool, does not follow that with a usage hint.
proc fstrim_bytes(value: Str, what: Str) -> UInt {
  if ! rx"^[0-9]+[KMGT]?$".matches(value) {
    gnu.error(f"failed to parse {what}: {gnu.quote(value)}: Invalid argument")
    exit 1
  }
  byte_count(value)
}

proc fstrim(argv: List[Str]) {
  let args = arguments(argv, ["-v --verbose"], ["-o --offset", "-l --length", "-m --minimum"])
  if args.operands.is_empty() { gnu.error("no mountpoint specified"); exit 1 }
  if args.operands.len() > 1 { gnu.usage_error("unexpected number of arguments") }
  let name = args.operands[0]
  let offset = fstrim_bytes(args.values.get("-o") ?? "0", "offset")
  let minimum = fstrim_bytes(args.values.get("-m") ?? "0", "minimum extent length")
  let length: UInt? = if "-l" in args.values { fstrim_bytes(args.values.get("-l")?, "length") } else { null }
  let result = linux.fstrim(fp"{name}", offset: offset, length: length, minlen: minimum)
  if let Err(failure) = result {
    # ENOTTY and EOPNOTSUPP both mean the filesystem has no discard support.
    let errno = failure.errno ?? -1
    if errno == 2 { gnu.error(f"stat of {name} failed: {gnu.strerror(failure)}"); exit 1 }
    if errno == 25 or errno == 95 { operand_failed(name, "the discard operation is not supported") }
    operand_failed(name, f"FITRIM ioctl failed: {gnu.strerror(failure)}")
  }
  let trimmed = result?
  if "-v" in args.flags { gnu.write_text(f"{name}: {trimmed} bytes trimmed\n") }
}

proc fsfreeze(argv: List[Str]) {
  let args = arguments(argv, ["-f --freeze", "-u --unfreeze"], [])
  require_operands(args, 1, 1)
  let freeze = "-f" in args.flags
  let thaw = "-u" in args.flags
  if freeze == thaw { gnu.usage_error("specify exactly one of --freeze and --unfreeze") }
  linux.fsfreeze(fp"{args.operands[0]}", "-f" in args.flags)?
}

proc partprobe(argv: List[Str]) {
  let args = arguments(argv, ["-d --dry-run", "-s --summary"], [])
  require_operands(args, 1, -1)
  for name in args.operands {
    let loaded = linux.partition_table(fp"{name}")
    if let Err(failure) = loaded { operand_failed(name, gnu.strerror(failure)) }
    let table = loaded?
    # Summarize only after the kernel accepted the reread, so a refused
    # device never prints a partition list.
    if "-d" not in args.flags {
      if let Err(failure) = linux.blockdev_reread_partition_table(fp"{name}") { operand_failed(name, gnu.strerror(failure)) }
    }
    if "-s" in args.flags { gnu.write_text(f"{name}: {table.label} partitions" + [f" {item.index}" for item in table.partitions].join("") + "\n") }
  }
}

pure partition_node(name: Str, index: Int) -> Str {
  let separator = if name.byte_slice(name.byte_len() - 1) in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"] { "p" } else { "" }
  name + separator + f"{index}"
}

type PartitionInput = {index: Int, start: Int, end: Int, size: Int, type: Str, uuid: Str, name: Str}

proc sector_count(value: Str, sector_size: Int) -> Int {
  let text = value.trim()
  if rx"^[0-9]+$".matches(text) { return text.parse_int()? }
  if rx"^[0-9]+[KMGT]$".matches(text) {
    let number = text.byte_slice(0, text.byte_len() - 1).parse_int()?
    let suffix = text.byte_slice(text.byte_len() - 1)
    let factor = if suffix == "K" { 1024 } else if suffix == "M" { 1048576 } else if suffix == "G" { 1073741824 } else { 1099511627776 }
    return (number * factor + sector_size - 1) / sector_size
  }
  unsupported(f"partition size {gnu.quote(value)} (use sectors or K/M/G/T)")
  0
}

# Parse the conventional sfdisk named-field input before issuing any write.
# Sequential explicit ranges keep every affected byte and partition visible.
proc write_sfdisk(name: Str, args: Arguments) {
  require_operands(args, 1, 1)
  let text = io.stdin_text()?
  var label = args.values.get("--label") ?? "dos"
  var id = ""
  var sector_size = 512
  var first_lba: Int? = null
  var last_lba: Int? = null
  var partitions: List[PartitionInput] = []
  for input in text.lines() {
    let line = input.trim()
    if line == "" or line.starts_with("#") { continue }
    if rx"^(label|label-id|unit|sector-size|device|first-lba|last-lba|table-length|grain):".matches(line) and ! partitions.is_empty() { gnu.usage_error("partition table headers must precede partition rows") }
    if line.starts_with("label:") {
      let supplied = line.byte_slice(6).trim()
      if "--label" in args.values and supplied != args.values.get("--label")? { gnu.usage_error("partition input label disagrees with --label") }
      label = supplied
      continue
    }
    if line.starts_with("label-id:") { id = line.byte_slice(9).trim(); continue }
    if line.starts_with("unit:") {
      if line.byte_slice(5).trim() != "sectors" { unsupported("partition input unit other than sectors") }
      continue
    }
    if line.starts_with("sector-size:") { sector_size = line.byte_slice(12).trim().parse_int()?; continue }
    if line.starts_with("device:") {
      if line.byte_slice(7).trim() != name { gnu.usage_error("partition input device disagrees with operand") }
      continue
    }
    # The usable range of a dump is checked against its partitions; the writer
    # picks the range itself, so only the entry count has to match its fixed 128.
    if line.starts_with("first-lba:") { first_lba = line.byte_slice(10).trim().parse_uint()?; continue }
    if line.starts_with("last-lba:") { last_lba = line.byte_slice(9).trim().parse_uint()?; continue }
    if line.starts_with("table-length:") {
      if line.byte_slice(13).trim() != "128" { unsupported("custom GPT table length") }
      continue
    }
    if line.starts_with("grain:") { continue }
    if label not in ["gpt", "dos"] { gnu.usage_error("partition label must be gpt or dos") }
    if sector_size <= 0 { gnu.usage_error("sector size must be positive") }
    var body = line
    var partition_index = partitions.len() + 1
    if ":" in body {
      let prefix = body.split(":", maxsplit: 1)
      let node = prefix[0].trim()
      let base = partition_node(name, 1).byte_slice(0, partition_node(name, 1).byte_len() - 1)
      if ! node.starts_with(base) { gnu.usage_error("partition node disagrees with device operand") }
      partition_index = node.byte_slice(base.byte_len()).parse_uint_positive()?
      if node != partition_node(name, partition_index) { gnu.usage_error("invalid partition node") }
      body = prefix[1].trim()
    }
    var start: Int? = null
    var size: Int? = null
    var kind = if label == "gpt" { "0fc63daf-8483-4772-8e79-3d69d8477de4" } else { "83" }
    var uuid = ""
    var part_name = ""
    var seen: Set[Str] = set.empty()
    for field in body.split(",") {
      let pieces = field.trim().split("=", maxsplit: 1)
      if pieces.len() != 2 { unsupported("positional or bootable sfdisk fields; use start=, size=, type=") }
      let key = pieces[0].trim()
      if key in seen { gnu.usage_error(f"duplicate partition field: {key}") }
      seen = seen.add(key)
      let value = pieces[1].trim()
      if key == "start" { start = sector_count(value, sector_size) } else if key == "size" { size = sector_count(value, sector_size) } else if key == "type" {
        kind = value
        if label == "dos" and rx"^[0-9a-fA-F]$".matches(value) { kind = "0" + value }
        if value == "L" { kind = if label == "gpt" { "0fc63daf-8483-4772-8e79-3d69d8477de4" } else { "83" } }
        if value == "U" { kind = if label == "gpt" { "c12a7328-f81f-11d2-ba4b-00a0c93ec93b" } else { "ef" } }
      } else if key == "uuid" { uuid = value } else if key == "name" {
        if value.starts_with("\"") and value.ends_with("\"") {
          part_name = json.decode(value)?.require(Str)?
        } else { part_name = value }
      } else { unsupported(f"partition input field {key}") }
    }
    if start == null or size == null { unsupported("implicit partition ranges; specify start= and size=") }
    let first = start ?? 0
    let length = size ?? 0
    if first <= 0 or length <= 0 { gnu.usage_error("partition start and size must be positive") }
    let last = first + length - 1
    for previous in partitions {
      if partition_index == previous.index { gnu.usage_error("duplicate partition index") }
      if first <= previous.end and last >= previous.start { gnu.usage_error("partition ranges overlap") }
    }
    if first < (first_lba ?? first) or last > (last_lba ?? last) { gnu.usage_error("partition lies outside the first-lba and last-lba range") }
    partitions += [{index: partition_index, start: first, end: last, size: length, type: kind, uuid: uuid, name: part_name}]
  }
  if label not in ["gpt", "dos"] { gnu.usage_error("partition label must be gpt or dos") }
  if partitions.is_empty() { gnu.usage_error("partition input is empty") }
  if (label == "dos" and partitions.len() > 4) or partitions.len() > 128 { gnu.usage_error("too many primary partitions") }
  let table = {label: label, id: id, sector_size: sector_size, partitions: partitions}
  if "-n" in args.flags { gnu.write_text(json.encode(table, pretty: true)? + "\n"); return }
  linux.write_partition_table(fp"{name}", table)?
}

# Sizes the way util-linux prints them: the largest binary unit that leaves a
# whole part, with one decimal rounded half up and dropped when it is zero.
# `spaced` selects the "64 MiB" heading form over the "10M" table form.
pure human_size(count: Int, spaced: Bool) -> Str {
  var unit = 1
  var exponent = 0
  while exponent < 6 and count / unit >= 1024 {
    unit *= 1024
    exponent += 1
  }
  var whole = count / unit
  var tenths = if exponent == 0 { 0 } else { (count % unit / (unit / 1024) + 50) / 100 }
  if tenths == 10 {
    whole += 1
    tenths = 0
  }
  let letter = "BKMGTPE".byte_slice(exponent, length: 1)
  let number = if tenths > 0 { f"{whole}.{tenths}" } else { f"{whole}" }
  if ! spaced { return number + letter }
  number + " " + letter + (if exponent == 0 { "" } else { "iB" })
}

pure type_name(table: Str, key: Str) -> Str {
  for line in table.lines() {
    let pieces = line.trim().split(" ", maxsplit: 1)
    if pieces.len() == 2 and pieces[0] == key { return pieces[1] }
  }
  "Unknown"
}

# The type code of a DOS partition as sfdisk and fdisk print it: lower-case
# hexadecimal without a leading zero.
pure dos_code(code: Str) -> Str {
  let lowered = code.lower()
  if lowered.byte_len() == 2 and lowered.starts_with("0") { lowered.byte_slice(1) } else { lowered }
}

# GPT attribute bits by name, ascending; the high word is the type-specific
# GUID:N range.
pure gpt_attribute_names(raw: Bytes) -> Str {
  var names: List[Str] = []
  for bit in range(64) {
    var mask = 1
    for _ in range(bit % 8) { mask *= 2 }
    if (raw.byte_at(bit / 8) ?? 0) / mask % 2 == 0 { continue }
    if bit == 0 { names += ["RequiredPartition"] } else if bit == 1 { names += ["NoBlockIOProtocol"] } else if bit == 2 { names += ["LegacyBIOSBootable"] } else { names += [f"GUID:{bit}"] }
  }
  names.join(" ")
}

type DiskPartition = {index: Int, start: Int, end: Int, size: Int, type: Str, uuid: Str, name: Str, bootable: Bool, attrs: Str}
type DiskLayout = {label: Str, id: Str, sector_size: Int, physical_sector: Int, total_bytes: Int, first_lba: Int, last_lba: Int, table_length: Int, partitions: List[DiskPartition]}

# The partition table of a device or image together with the facts the dump
# and listing formats carry that the native reader does not: the boot flags of
# a DOS table and the usable range, entry count and attributes of a GPT.
proc disk_layout(name: Str) -> Result[DiskLayout] {
  let node = fp"{name}"
  let table = linux.partition_table(node)?
  let info = fs.stat(node, follow_symlinks: true)?
  var total: Int = info.size
  var physical: Int = table.sector_size
  if info.kind == "block" {
    let device = linux.blockdev_info(node)?
    total = device.size_bytes
    physical = device.physical_sector_bytes
  }
  var partitions: List[DiskPartition] = []
  var first = 0
  var last = 0
  var length = 128
  if table.label == "dos" {
    let slots = bytes.read_at(node, 446, 64)?
    for item in table.partitions {
      let bootable = (slots.byte_at((item.index - 1) * 16) ?? 0) == 128
      partitions += [{index: item.index, start: item.start, end: item.end, size: item.size, type: dos_code(item.type), uuid: "", name: "", bootable: bootable, attrs: ""}]
    }
  } else if table.label == "gpt" {
    let header = bytes.read_at(node, table.sector_size, 92)?
    first = bytes.unpack_le(header, 8, 40)?
    last = bytes.unpack_le(header, 8, 48)?
    length = bytes.unpack_le(header, 4, 80)?
    let entries = bytes.unpack_le(header, 8, 72)? * table.sector_size
    let entry_size = bytes.unpack_le(header, 4, 84)?
    for item in table.partitions {
      let raw = bytes.read_at(node, entries + (item.index - 1) * entry_size + 48, 8)?
      partitions += [{index: item.index, start: item.start, end: item.end, size: item.size, type: item.type.upper(), uuid: item.uuid.upper(), name: item.name, bootable: false, attrs: gpt_attribute_names(raw)}]
    }
  }
  Ok({label: table.label, id: if table.label == "gpt" { table.id.upper() } else { table.id }, sector_size: table.sector_size, physical_sector: physical, total_bytes: total, first_lba: first, last_lba: last, table_length: length, partitions: partitions})
}

# Disks of at most 4 MiB are aligned to one sector rather than 1 MiB, which
# sfdisk records as a `grain` header.
pure small_disk(layout: DiskLayout) -> Bool { layout.total_bytes <= 4194304 }

pure dump_quoted(value: Str) -> Str {
  "\"" + value.replace("\\", with: "\\\\").replace("\"", with: "\\\"") + "\""
}

proc dump_layout(name: Str, layout: DiskLayout) {
  var text = f"label: {layout.label}\nlabel-id: {layout.id}\ndevice: {name}\nunit: sectors\n"
  if layout.label == "gpt" {
    text += f"first-lba: {layout.first_lba}\nlast-lba: {layout.last_lba}\n"
    if layout.table_length != 128 { text += f"table-length: {layout.table_length}\n" }
  }
  if small_disk(layout) { text += f"grain: {layout.sector_size}\n" }
  text += f"sector-size: {layout.sector_size}\n"
  if ! layout.partitions.is_empty() { text += "\n" }
  for item in layout.partitions {
    text += f"{partition_node(name, item.index)} : start={tui.left_pad(f"{item.start}", 12)}, size={tui.left_pad(f"{item.size}", 12)}, type={item.type}"
    if item.uuid != "" { text += f", uuid={item.uuid}" }
    if item.name != "" { text += f", name={dump_quoted(item.name)}" }
    if item.bootable { text += ", bootable" }
    if item.attrs != "" { text += f", attrs={dump_quoted(item.attrs)}" }
    text += "\n"
  }
  gnu.write_text(text)
}

proc json_layout(name: Str, layout: DiskLayout) {
  var members = [f"\"label\": {json.encode(layout.label)?}", f"\"id\": {json.encode(layout.id)?}", f"\"device\": {json.encode(name)?}", "\"unit\": \"sectors\""]
  if layout.label == "gpt" {
    members += [f"\"firstlba\": {layout.first_lba}", f"\"lastlba\": {layout.last_lba}"]
    if layout.table_length != 128 { members += [f"\"table-length\": \"{layout.table_length}\""] }
  }
  if small_disk(layout) { members += [f"\"grain\": \"{layout.sector_size}\""] }
  members += [f"\"sectorsize\": {layout.sector_size}"]
  var text = "{\n   \"partitiontable\": {\n"
  var rows: List[Str] = []
  for item in layout.partitions {
    var fields = [f"\"node\": {json.encode(partition_node(name, item.index))?}", f"\"start\": {item.start}", f"\"size\": {item.size}", f"\"type\": {json.encode(item.type)?}"]
    if item.uuid != "" { fields += [f"\"uuid\": {json.encode(item.uuid)?}"] }
    if item.name != "" { fields += [f"\"name\": {json.encode(item.name)?}"] }
    if item.bootable { fields += ["\"bootable\": true"] }
    if item.attrs != "" { fields += [f"\"attrs\": {json.encode(item.attrs)?}"] }
    rows += ["{\n            " + fields.join(",\n            ") + "\n         }"]
  }
  if ! rows.is_empty() { members += ["\"partitions\": [\n         " + rows.join(",") + "\n      ]"] }
  text += "      " + members.join(",\n      ") + "\n   }\n}\n"
  gnu.write_text(text)
}

pure fdisk_default_columns(label: Str) -> List[Str] {
  if label == "dos" { ["Device", "Boot", "Start", "End", "Sectors", "Size", "Id", "Type"] } else { ["Device", "Start", "End", "Sectors", "Size", "Type"] }
}

# Print one device the way `fdisk -l` does. A column request that the label
# cannot supply is reported after the disk heading, as fdisk does.
proc list_layout(name: Str, layout: DiskLayout, request: Str) {
  let sectors = layout.total_bytes / layout.sector_size
  var text = f"Disk {name}: {human_size(layout.total_bytes, true)}, {layout.total_bytes} bytes, {sectors} sectors\n"
  text += f"Units: sectors of 1 * {layout.sector_size} = {layout.sector_size} bytes\n"
  text += f"Sector size (logical/physical): {layout.sector_size} bytes / {layout.physical_sector} bytes\n"
  text += f"I/O size (minimum/optimal): {layout.physical_sector} bytes / {layout.physical_sector} bytes\n"
  if layout.label != "none" { text += f"Disklabel type: {layout.label}\nDisk identifier: {layout.id}\n" }
  gnu.write_text(text)
  if layout.label == "none" or layout.partitions.is_empty() { return }
  var cols: List[Str] = []
  var requested = request
  if requested.starts_with("+") or requested == "" { cols = fdisk_default_columns(layout.label) }
  if requested.starts_with("+") { requested = requested.byte_slice(1) }
  for word in requested.split(",") {
    if word == "" { continue }
    var canonical: Str? = null
    for known in FDISK_COLUMNS { if known.lower() == word.lower() { canonical = known } }
    let column = canonical ?? word
    let offered = if layout.label == "dos" { column in ["Device", "Boot", "Start", "End", "Sectors", "Size", "Id", "Type"] } else { column in ["Device", "Start", "End", "Sectors", "Size", "Type", "Type-UUID", "Attrs", "Name", "UUID"] }
    if ! offered { gnu.error(f"{layout.label} unknown column: {word}"); exit 1 }
    cols += [column]
  }
  var rows: List[List[Str]] = []
  for item in layout.partitions {
    let known: Map[Str] = {
      "Device": partition_node(name, item.index), "Boot": if item.bootable { "*" } else { "" },
      "Start": f"{item.start}", "End": f"{item.end}", "Sectors": f"{item.size}", "Size": human_size(item.size * layout.sector_size, false),
      "Id": item.type, "Type": if layout.label == "dos" { type_name(DOS_TYPE_NAMES, item.type) } else { type_name(GPT_TYPE_NAMES, item.type) },
      "Type-UUID": item.type, "Attrs": item.attrs, "Name": item.name, "UUID": item.uuid,
    }
    rows += [[known.get(column) ?? "" for column in cols]]
  }
  let numeric = ["Start", "End", "Sectors", "Size", "Id"]
  # Minimum widths of the fdisk columns; they apply once the data outgrows
  # the heading.
  let minimums: Map[Int] = {"Device": 10, "Start": 5, "End": 5, "Sectors": 5, "Size": 5, "Type-UUID": 36, "UUID": 36}
  gnu.write_text("\n")
  for line in table_lines(cols, [column in numeric for column in cols], [minimums.get(column) ?? 0 for column in cols], rows, true) { gnu.write_text(line + "\n") }
}

proc partition_command(command: Str, argv: List[Str]) {
  let booleans = if command == "sfdisk" { ["-l --list", "-J --json", "-d --dump", "-n --no-act"] } else if command == "fdisk" { ["-l --list"] } else { ["-l --list", "-s --show", "--noheadings"] }
  let valued = if command == "sfdisk" { ["--label"] } else { ["-o --output"] }
  let args = arguments(argv, booleans, valued)
  require_operands(args, 1, -1)
  if command == "fdisk" and "-l" not in args.flags { unsupported("interactive fdisk editing; use sfdisk for scripted partition tables") }
  if command == "sfdisk" and [flag for flag in args.flags if flag != "-n"].is_empty() { write_sfdisk(args.operands[0], args); return }
  if ("-n" in args.flags and command == "sfdisk") or "--label" in args.values { gnu.usage_error("write options cannot be combined with inspection modes") }
  if command == "partx" {
    let cols = columns(args.values.get("-o") ?? "NR,START,END,SECTORS,SIZE,NAME,UUID,TYPE", ["NR", "START", "END", "SECTORS", "SIZE", "NAME", "UUID", "TYPE"])
    for name in args.operands {
      let loaded = linux.partition_table(fp"{name}")
      if let Err(failure) = loaded { operand_failed(name, gnu.strerror(failure)) }
      let table = loaded?
      if table.label == "none" { operand_failed(name, "failed to read partition table") }
      if "--noheadings" not in args.flags { gnu.write_text(cols.join(" ") + "\n") }
      for item in table.partitions {
        let row: Map[Str] = {"NR": f"{item.index}", "START": f"{item.start}", "END": f"{item.end}", "SECTORS": f"{item.size}", "SIZE": f"{item.size * table.sector_size}", "NAME": item.name, "UUID": item.uuid, "TYPE": item.type}
        gnu.write_text([row.get(column) ?? "" for column in cols].join(" ") + "\n")
      }
    }
    return
  }
  var first = true
  for name in args.operands {
    let loaded = disk_layout(name)
    if let Err(failure) = loaded {
      gnu.error(f"cannot open {name}: {gnu.strerror(failure)}")
      exit 1
    }
    let layout = loaded?
    if "-J" in args.flags or "-d" in args.flags {
      if layout.label == "none" { operand_failed(name, "does not contain a recognized partition table") }
      if "-J" in args.flags { json_layout(name, layout) } else { dump_layout(name, layout) }
      continue
    }
    # util-linux separates the listings of several devices by two blank lines.
    if ! first { gnu.write_text("\n\n") }
    first = false
    list_layout(name, layout, args.values.get("-o") ?? "")
  }
}


pure usage(command: Str) -> Str {
  let details = match command {
    "lsblk" => "[-a -b -d -f -J -l -n -p -r] [-o COLUMNS] [DEVICE...]\nList block devices and typed partition/dependency relationships.",
    "blkid" => "[-p] [-o full|value|export|device] [-s TAG] [DEVICE...]\nProbe filesystem and partition tags directly. Repeated -s selects multiple tags.",
    "findmnt" => "[-J -l -n -r] [-t TYPES] [-S SOURCE] [-T PATH] [-o COLUMNS] [SOURCE|TARGET]\nRead the live mountinfo hierarchy. --fstab is not supported.",
    "mount" => "[-r|-w] [-B|-R|-M] [-t TYPE] [-o OPTIONS] SOURCE TARGET\n       mount [-t TYPES]\n       mount -a\nMount filesystems, bind/move mounts, or list mounts. A single fstab operand and filtered --all are not supported.",
    "umount" => "[-l|-f] TARGET...\n       umount -a [-t TYPES]\nUnmount named targets. Lazy/forced --all is not supported.",
    "losetup" => "[-a|-l] [LOOP]\n       losetup -f [--show] FILE\n       losetup LOOP FILE\n       losetup -d LOOP...\n       losetup -j FILE\nInspect, attach or detach loop devices. Offset, size-limit, read-only and partition-scan options are not supported.",
    "blockdev" => "OPERATION... DEVICE...\nOperations: --getsize64 --getsz --getsize --getss --getpbsz --getro --setro --setrw --flushbufs --rereadpt.",
    "wipefs" => "[-J|--noheadings] [-O DEVICE,OFFSET,TYPE,UUID,LABEL,LENGTH,USAGE] [-t TYPES] DEVICE...\n       wipefs [-n] [-f] [-a|-o OFFSET...] [-t TYPES] DEVICE...\nProbe known signatures or erase every magic of the selected ones. Repeated -o selects multiple offsets. Backup and forced nested-signature modes are not supported.",
    "partx" => "[-s|-l] [--noheadings] [-o COLUMNS] DEVICE...\nInspect partition table extents. Kernel partition add/delete/update is not supported.",
    "partprobe" => "[-d] [-s] DEVICE...\nRead partition tables and request a kernel reread for named devices. -d validates without rereading.",
    "fstrim" => "[-v] [-o OFFSET] [-l LENGTH] [-m MINIMUM] MOUNTPOINT\nTrim one explicitly named filesystem; byte counts accept K/M/G/T. --all is not supported.",
    "fsfreeze" => "(-f|-u) MOUNTPOINT\nFreeze or thaw one explicitly named filesystem.",
    "sfdisk" => "(--json|--dump|--list) DEVICE...\n       sfdisk [-n] [--label gpt|dos] DEVICE < INPUT\nWrite conventional named start=,size=,type=,uuid=,name= fields with explicit nonoverlapping ranges. GPT and DOS primary partitions are supported. Positional fields, implicit ranges, bootable flags, attrs, a table-length other than 128 and DOS logical/extended tables are not supported; first-lba and last-lba only bound the partitions, since the writer chooses the usable range itself.",
    "fdisk" => "-l [-o [+]COLUMNS] DEVICE...\nInspect GPT and DOS primary partition tables in the util-linux listing layout (columns Device, Boot, Start, End, Sectors, Size, Id, Type, Type-UUID, Attrs, Name, UUID). Interactive editing and DOS logical/extended tables are not supported; use sfdisk for scripted writes.",
    "swapon" => "[-p PRIORITY] DEVICE...\n       swapon -a\nActivate named swap devices/files, or fstab swap entries. Swap listing and discard options are not supported.",
    "swapoff" => "DEVICE...\n       swapoff -a\nDeactivate named swap devices/files, or all active swap.",
    "mkswap" => "DEVICE\nInitialize an existing swap device or file. Label, UUID, page-size and creation options are not supported.",
    _ => "[OPTIONS] [DEVICE...]",
  }
  f"Usage: {command} {details}\n"
}

## Dispatch storage applets, keeping unsupported kernel operations explicit.
export proc dispatch(command: Str, argv: List[Str]) {
  for argument in argv {
    if argument == "--" { break }
    if argument == "--help" or argument == "-h" { gnu.help(usage(command)); return }
    if argument == "--version" or argument == "-V" { gnu.version(command); return }
  }
  if let Err(failure) = execute(command, argv) { gnu.error(gnu.strerror(failure)); exit 1 }
}

proc execute(command: Str, argv: List[Str]) -> Result[Unit] {
  match command {
    "lsblk" => lsblk(argv),
    "blkid" => blkid(argv),
    "findmnt" => findmnt(argv),
    "mount" => mount(argv),
    "umount" => umount(argv),
    "blockdev" => blockdev(argv),
    "wipefs" => wipefs(argv),
    "fstrim" => fstrim(argv),
    "fsfreeze" => fsfreeze(argv),
    "partprobe" => partprobe(argv),
    "losetup" => losetup(argv),
    "swapon" => swap_command(command, argv),
    "swapoff" => swap_command(command, argv),
    "mkswap" => swap_command(command, argv),
    "sfdisk" => partition_command(command, argv),
    "fdisk" => partition_command(command, argv),
    "partx" => partition_command(command, argv),
    _ => unsupported(f"{command} operation"),
  }
}
