#![allow(dead_code)]

use super::{seal, tar_raw_header};

pub struct Spec<'a> {
    pub name: &'a [u8],
    pub flag: u8,
    pub body: &'a [u8],
    pub link: &'a [u8],
    pub mode: u32,
}

pub fn file<'a>(name: &'a [u8], body: &'a [u8]) -> Spec<'a> {
    Spec { name, flag: b'0', body, link: b"", mode: 0o644 }
}

pub fn dir(name: &[u8]) -> Spec<'_> {
    Spec { name, flag: b'5', body: b"", link: b"", mode: 0o755 }
}

pub fn symlink<'a>(name: &'a [u8], link: &'a [u8]) -> Spec<'a> {
    Spec { name, flag: b'2', body: b"", link, mode: 0o777 }
}

pub fn hardlink<'a>(name: &'a [u8], link: &'a [u8]) -> Spec<'a> {
    Spec { name, flag: b'1', body: b"", link, mode: 0o644 }
}

pub fn fifo(name: &[u8]) -> Spec<'_> {
    Spec { name, flag: b'6', body: b"", link: b"", mode: 0o644 }
}

pub fn with_mode(mut spec: Spec<'_>, mode: u32) -> Spec<'_> {
    spec.mode = mode;
    spec
}

pub fn tar_of(specs: &[Spec<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for spec in specs {
        let mut header = tar_raw_header(spec.name, spec.flag, spec.body.len() as u64, spec.link);
        header[100..107].copy_from_slice(format!("{:07o}", spec.mode).as_bytes());
        seal(&mut header);
        out.extend_from_slice(&header);
        out.extend_from_slice(spec.body);
        let rest = spec.body.len() % 512;
        if rest != 0 {
            out.resize(out.len() + 512 - rest, 0);
        }
    }
    out.resize(out.len() + 1024, 0);
    out
}
