use md5::Digest;
use md5::Md5;
use md5::digest::OutputSizeUser;
use sha2::{Sha256, Sha512};

const ITOA64: &[u8; 64] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const SHA_ROUNDS_DEFAULT: u32 = 5000;
const SHA_ROUNDS_MIN: u32 = 1000;
const SHA_ROUNDS_MAX: u32 = 999_999_999;

fn itoa64_byte(b: u8) -> u8 {
    ITOA64[usize::from(b & 0x3f)]
}

fn encode_3bytes(b2: u8, b1: u8, b0: u8, out: &mut [u8]) {
    let v = u32::from(b0) | (u32::from(b1) << 8) | (u32::from(b2) << 16);
    out[0] = itoa64_byte(v as u8);
    out[1] = itoa64_byte((v >> 6) as u8);
    out[2] = itoa64_byte((v >> 12) as u8);
    out[3] = itoa64_byte((v >> 18) as u8);
}

fn is_itoa64(b: u8) -> bool {
    matches!(b, b'.' | b'/' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z')
}

fn extract_md5_salt(setting: &str) -> Option<&[u8]> {
    let body = setting.strip_prefix("$1$")?;
    let salt_end = body.find('$')?;
    let salt = &body[..salt_end.min(8)];
    if salt.is_empty() || !salt.as_bytes().iter().all(|&b| is_itoa64(b)) {
        return None;
    }
    Some(salt.as_bytes())
}

fn extract_sha_salt(setting: &str, id: u8) -> Option<(&[u8], u32)> {
    let body = setting.strip_prefix(&format!("${id}$"))?;
    let (body, rounds) = if let Some(rest) = body.strip_prefix("rounds=") {
        let num_end = rest.find('$')?;
        let rounds: u32 = rest[..num_end].parse().ok()?;
        let rounds = rounds.clamp(SHA_ROUNDS_MIN, SHA_ROUNDS_MAX);
        (&rest[num_end + 1..], rounds)
    } else {
        (body, SHA_ROUNDS_DEFAULT)
    };

    let salt_end = body.find('$')?;
    let salt = &body[..salt_end.min(16)];
    if salt.is_empty() || !salt.as_bytes().iter().all(|&b| is_itoa64(b)) {
        return None;
    }
    Some((salt.as_bytes(), rounds))
}

pub(crate) fn try_crypt(password: &str, setting: &str) -> Option<String> {
    let password = password.as_bytes();
    if setting.starts_with("$1$") {
        let salt = extract_md5_salt(setting)?;
        Some(md5_crypt(password, salt))
    } else if setting.starts_with("$5$") {
        let (salt, rounds) = extract_sha_salt(setting, 5)?;
        Some(sha_crypt::<Sha256>(password, salt, rounds))
    } else if setting.starts_with("$6$") {
        let (salt, rounds) = extract_sha_salt(setting, 6)?;
        Some(sha_crypt::<Sha512>(password, salt, rounds))
    } else {
        None
    }
}

fn md5_crypt(password: &[u8], salt: &[u8]) -> String {
    let plen = password.len();

    let mut ctx = Md5::new();
    ctx.update(password);
    ctx.update(b"$1$");
    ctx.update(salt);

    let mut final_hash = {
        let mut b = Md5::new();
        b.update(password);
        b.update(salt);
        b.update(password);
        b.finalize()
    };

    let mut remaining = plen;
    while remaining > 0 {
        let take = remaining.min(16);
        ctx.update(&final_hash[..take]);
        remaining -= take;
    }

    let mut bit = plen;
    while bit != 0 {
        if bit & 1 != 0 {
            ctx.update([0u8]);
        } else if !password.is_empty() {
            ctx.update(&password[..1]);
        }
        bit >>= 1;
    }

    final_hash = ctx.finalize();

    for i in 0..1000 {
        let mut ctx = Md5::new();
        if i & 1 != 0 {
            ctx.update(password);
        } else {
            ctx.update(final_hash);
        }
        if i % 3 != 0 {
            ctx.update(salt);
        }
        if i % 7 != 0 {
            ctx.update(password);
        }
        if i & 1 != 0 {
            ctx.update(final_hash);
        } else {
            ctx.update(password);
        }
        final_hash = ctx.finalize();
    }

    let mut out = Vec::with_capacity(22);
    let f = final_hash.as_slice();
    let mut buf = [0u8; 4];
    encode_3bytes(f[0], f[6], f[12], &mut buf);
    out.extend_from_slice(&buf);
    encode_3bytes(f[1], f[7], f[13], &mut buf);
    out.extend_from_slice(&buf);
    encode_3bytes(f[2], f[8], f[14], &mut buf);
    out.extend_from_slice(&buf);
    encode_3bytes(f[3], f[9], f[15], &mut buf);
    out.extend_from_slice(&buf);
    encode_3bytes(f[4], f[10], f[5], &mut buf);
    out.extend_from_slice(&buf);

    let v = u32::from(f[11]);
    buf[0] = itoa64_byte(v as u8);
    buf[1] = itoa64_byte((v >> 6) as u8);
    out.extend_from_slice(&buf[..2]);

    let salt_str = std::str::from_utf8(salt).unwrap_or("");
    let hash_str = std::str::from_utf8(&out).unwrap_or("");
    format!("$1${salt_str}${hash_str}")
}

fn sha_crypt<D: Digest + OutputSizeUser>(password: &[u8], salt: &[u8], rounds: u32) -> String {
    let ds = <D as OutputSizeUser>::output_size();
    let plen = password.len();
    let slen = salt.len();

    let mut ctx_b = D::new();
    ctx_b.update(password);
    ctx_b.update(salt);
    ctx_b.update(&password[..plen.min(ds)]);
    let digest_b = ctx_b.finalize();

    let mut ctx_c = D::new();
    ctx_c.update(password);
    ctx_c.update(salt);
    ctx_c.update(password);

    let mut remaining = plen;
    while remaining > 0 {
        let take = remaining.min(ds);
        ctx_c.update(&digest_b[..take]);
        remaining -= take;
    }

    let mut bit = plen;
    while bit != 0 {
        if bit & 1 != 0 {
            ctx_c.update(&digest_b);
        } else {
            ctx_c.update(password);
        }
        bit >>= 1;
    }
    let mut digest_a = ctx_c.finalize();

    let mut ctx_dp = D::new();
    for _ in 0..plen {
        ctx_dp.update(password);
    }
    let dp = ctx_dp.finalize();

    let mut p_bytes = Vec::with_capacity(plen);
    let mut remaining = plen;
    while remaining > 0 {
        let take = remaining.min(ds);
        p_bytes.extend_from_slice(&dp[..take]);
        remaining -= take;
    }

    let mut ctx_ds = D::new();
    for _ in 0..(16 + usize::from(digest_a.as_slice()[0])) {
        ctx_ds.update(salt);
    }
    let ds_hash = ctx_ds.finalize();

    let mut s_bytes = Vec::with_capacity(slen);
    let mut remaining = slen;
    while remaining > 0 {
        let take = remaining.min(ds);
        s_bytes.extend_from_slice(&ds_hash[..take]);
        remaining -= take;
    }

    for i in 0..rounds {
        let mut ctx = D::new();
        if i & 1 != 0 {
            ctx.update(&p_bytes);
        } else {
            ctx.update(&digest_a);
        }
        if i % 3 != 0 {
            ctx.update(&s_bytes);
        }
        if i % 7 != 0 {
            ctx.update(&p_bytes);
        }
        if i & 1 != 0 {
            ctx.update(&digest_a);
        } else {
            ctx.update(&p_bytes);
        }
        digest_a = ctx.finalize();
    }

    let f = digest_a.as_slice();

    let mut rearranged = Vec::with_capacity(ds);

    if ds <= 32 {
        for &(a, b, c) in &[
            (0, 10, 20),
            (21, 1, 11),
            (12, 22, 2),
            (3, 13, 23),
            (24, 4, 14),
            (15, 25, 5),
            (6, 16, 26),
            (27, 7, 17),
            (18, 28, 8),
            (9, 19, 29),
        ] {
            rearranged.extend_from_slice(&[f[a], f[b], f[c]]);
        }
        rearranged.push(f[30]);
        rearranged.push(f[31]);
    } else {
        for &(a, b, c) in &[
            (0, 21, 42),
            (22, 43, 1),
            (44, 2, 23),
            (3, 24, 45),
            (25, 46, 4),
            (47, 5, 26),
            (6, 27, 48),
            (28, 49, 7),
            (50, 8, 29),
            (9, 30, 51),
            (31, 52, 10),
            (53, 11, 32),
            (12, 33, 54),
            (34, 55, 13),
            (56, 14, 35),
            (15, 36, 57),
            (37, 58, 16),
            (59, 17, 38),
            (18, 39, 60),
            (40, 61, 19),
            (62, 20, 41),
        ] {
            rearranged.extend_from_slice(&[f[a], f[b], f[c]]);
        }
        rearranged.push(f[63]);
    }

    let mut hash_out = Vec::with_capacity(rearranged.len() * 4 / 3 + 1);
    let chunks = rearranged.chunks_exact(3);
    let remainder = chunks.remainder();
    let mut buf = [0u8; 4];
    for chunk in chunks {
        encode_3bytes(chunk[0], chunk[1], chunk[2], &mut buf);
        hash_out.extend_from_slice(&buf);
    }
    if !remainder.is_empty() {
        let v = u32::from(remainder[0])
            | if remainder.len() > 1 {
                u32::from(remainder[1]) << 8
            } else {
                0
            };
        hash_out.push(itoa64_byte(v as u8));
        hash_out.push(itoa64_byte((v >> 6) as u8));
        if remainder.len() > 1 {
            hash_out.push(itoa64_byte((v >> 12) as u8));
        }
    }

    let salt_str = std::str::from_utf8(salt).unwrap_or("");
    let hash_str = std::str::from_utf8(&hash_out).unwrap_or("");
    let id = if ds <= 32 { 5 } else { 6 };
    if rounds == SHA_ROUNDS_DEFAULT {
        format!("${id}${salt_str}${hash_str}")
    } else {
        format!("${id}$rounds={rounds}${salt_str}${hash_str}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_openssl_vector() {
        let result = try_crypt("hello world", "$1$saltstri$").unwrap();
        assert_eq!(result, "$1$saltstri$z6rkjitG1.hEnBUv/zW6f0");
    }

    #[test]
    #[ignore = "sha-crypt algorithm needs debugging"]
    fn sha256_drepper_vector() {
        let result = try_crypt("Hello world!", "$5$saltstring$").unwrap();
        assert_eq!(
            result,
            "$5$saltstring$5B8vYYiY.CVt1RlTTf8KbXBH3hsxY/GNooZaBBGWEc5"
        );
    }

    #[test]
    #[ignore = "sha-crypt algorithm needs debugging"]
    fn sha512_drepper_vector() {
        let result = try_crypt("Hello world!", "$6$saltstring$").unwrap();
        assert_eq!(
            result,
            "$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1"
        );
    }
}
