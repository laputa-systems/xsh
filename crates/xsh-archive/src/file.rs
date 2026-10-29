use crate::budget::{Budget, Metered};
use crate::compression::{self, BUFFER_SIZE, Codec, Compression, Encode, Encoder, SNIFF_LEN};
use crate::cursor::read_up_to;
use crate::error::{Error, Result};
use crate::extract::Collision;
use crate::output::PendingOutput;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Cursor, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::Arc;

pub fn compress_file(
    source: &Path,
    dest: &Path,
    compression: Compression,
    level: Option<i64>,
    collision: Collision,
    budget: &Arc<Budget>,
) -> Result<()> {
    let leaf = dest
        .file_name()
        .ok_or_else(|| Error::options("destination has no final name"))?;
    let codec = compression
        .resolve_write(leaf.as_bytes())
        .ok_or_else(|| Error::options("compression format required"))?;
    let mut encode = Encode::new(codec, level)?;
    let input = File::open(source).map_err(|error| Error::from_open(&error))?;
    let size = input
        .metadata()
        .map_err(|error| Error::from_open(&error))?
        .len();
    encode = encode.with_size_hint(size);
    let (pending, file) = PendingOutput::open(dest)?;
    let written = (|| {
        let mut encoder = Encoder::new(Some(&encode), BufWriter::with_capacity(BUFFER_SIZE, file))?;
        let mut input = Metered::new(input, Arc::clone(budget));
        io::copy(&mut input, &mut encoder).map_err(|error| Error::from_read(&error))?;
        encoder
            .finish()?
            .into_inner()
            .map(drop)
            .map_err(|error| Error::from_host(error.error()))
    })();
    match written {
        Ok(()) => pending.publish(collision),
        Err(error) => Err(pending.discard(error)),
    }
}

pub fn decompress_file(
    source: &Path,
    dest: &Path,
    compression: Compression,
    collision: Collision,
    budget: &Arc<Budget>,
) -> Result<()> {
    let mut reader = open_decoded(source, compression, budget)?;
    let (pending, file) = PendingOutput::open(dest)?;
    let written = (|| {
        let mut sink = BufWriter::with_capacity(BUFFER_SIZE, file);
        io::copy(&mut reader, &mut sink).map_err(|error| Error::from_read(&error))?;
        sink.into_inner()
            .map(drop)
            .map_err(|error| Error::from_host(error.error()))
    })();
    match written {
        Ok(()) => pending.publish(collision),
        Err(error) => Err(pending.discard(error)),
    }
}

pub fn decompress_to_memory(
    source: &Path,
    compression: Compression,
    budget: &Arc<Budget>,
) -> Result<Vec<u8>> {
    let mut reader = open_decoded(source, compression, budget)?;
    let mut output = Vec::new();
    reader
        .read_to_end(&mut output)
        .map_err(|error| Error::from_read(&error))?;
    Ok(output)
}

fn open_decoded(
    source: &Path,
    compression: Compression,
    budget: &Arc<Budget>,
) -> Result<Box<dyn Read + Send>> {
    let mut file = File::open(source).map_err(|error| Error::from_open(&error))?;
    let mut head = [0_u8; SNIFF_LEN];
    let filled = read_up_to(&mut file, &mut head)?;
    let peeked = head[..filled].to_vec();
    let codec: Option<Codec> = compression.resolve_read(&peeked, Some(source.as_os_str().as_bytes()));
    let codec = codec.ok_or_else(|| Error::options("compression format required"))?;
    let chained = BufReader::with_capacity(BUFFER_SIZE, Cursor::new(peeked).chain(file));
    let decoded = compression::decoder(codec, chained, budget.limits().workspace_bytes)?;
    Ok(Box::new(Metered::new(decoded, Arc::clone(budget))))
}
