use crate::budget::{Budget, Metered};
use crate::compression::{self, BUFFER_SIZE, Codec, Compression};
use crate::cpio_reader::CpioCursor;
use crate::cursor::{Completion, Cursor, Entry, read_up_to};
use crate::entry::Format;
use crate::error::{Error, Result};
use crate::input::{Input, Source};
use crate::tar_reader::TarCursor;
use crate::zip_reader::ZipCursor;
use rawzip::ZipArchive;
use std::io::{BufReader, Cursor as MemoryCursor, Read};
use std::sync::Arc;

const SNIFF: usize = 64;

pub struct Archive {
    cursor: Box<dyn Cursor>,
    format: Format,
    codec: Option<Codec>,
}

impl Archive {
    pub fn open(
        source: Source,
        format: Format,
        compression: Compression,
        budget: Arc<Budget>,
    ) -> Result<Self> {
        match format {
            Format::Zip => open_zip(source, format, compression, budget),
            Format::Tar | Format::Cpio => open_sequential(source, format, compression, budget),
        }
    }

    pub const fn format(&self) -> Format {
        self.format
    }

    pub const fn codec(&self) -> Option<Codec> {
        self.codec
    }

    pub fn next_entry(&mut self) -> Result<Option<Entry<'_>>> {
        self.cursor.next_entry()
    }

    pub fn finish(&mut self, payload_consumed: bool) -> Result<Completion> {
        let finish = self.cursor.finish()?;
        let mut completion = Completion::INSPECTED;
        if payload_consumed {
            completion = completion.with(Completion::PAYLOAD_CONSUMED);
        }
        if finish.checksum_verified {
            completion = completion.with(Completion::CHECKSUM_VERIFIED);
        }
        if finish.structure_complete {
            completion = completion
                .with(Completion::STRUCTURE_COMPLETE)
                .with(Completion::OUTER_COMPLETE);
        }
        Ok(completion)
    }
}

fn open_zip(
    source: Source,
    format: Format,
    compression: Compression,
    budget: Arc<Budget>,
) -> Result<Archive> {
    if !matches!(compression, Compression::Auto | Compression::Plain) {
        return Err(Error::unsupported(
            "zip compresses its entries itself and cannot sit behind another compression",
        ));
    }
    let reader = match source.input {
        Input::Seekable(reader) => reader,
        Input::Sequential(_) => {
            return Err(Error::new(
                crate::ErrorKind::NeedsSeek,
                "zip needs a seekable input; a pipe or byte stream cannot be read as zip",
            ));
        }
    };
    let mut buffer = vec![0_u8; rawzip::RECOMMENDED_BUFFER_SIZE];
    let archive = ZipArchive::from_seekable(reader, &mut buffer)
        .map_err(|error| Error::corrupt(error.to_string()).with_context("zip central directory"))?;
    let cursor = ZipCursor::new(archive, budget)?;
    Ok(Archive {
        cursor: Box::new(cursor),
        format,
        codec: None,
    })
}

fn open_sequential(
    source: Source,
    format: Format,
    compression: Compression,
    budget: Arc<Budget>,
) -> Result<Archive> {
    let mut reader: Box<dyn Read + Send> = match source.input {
        Input::Seekable(reader) => Box::new(reader),
        Input::Sequential(reader) => reader,
    };
    let mut peeked = Vec::with_capacity(SNIFF);
    let mut sniff = [0_u8; SNIFF];
    let filled = read_up_to(&mut reader, &mut sniff)?;
    peeked.extend_from_slice(&sniff[..filled]);
    let codec = compression.resolve_read(&peeked, source.name.as_deref());
    let chained = BufReader::with_capacity(BUFFER_SIZE, MemoryCursor::new(peeked.clone()).chain(reader));
    let stream: Box<dyn Read + Send> = match codec {
        None => Box::new(Metered::new(chained, Arc::clone(&budget))),
        Some(codec) => {
            let workspace = budget.limits().workspace_bytes;
            if codec == Codec::Xz
                && let Some(dictionary) = compression::xz_dictionary_bytes(&peeked)
                && dictionary > workspace
            {
                return Err(Error::limit(format!(
                    "xz dictionary of {dictionary} bytes exceeds the {workspace} byte decoder workspace"
                )));
            }
            let decoded = compression::decoder(codec, chained, workspace)?;
            Box::new(Metered::new(decoded, Arc::clone(&budget)))
        }
    };
    let cursor: Box<dyn Cursor> = match format {
        Format::Tar => Box::new(TarCursor::new(stream, budget)),
        Format::Cpio => Box::new(CpioCursor::new(stream, budget)),
        Format::Zip => unreachable!("zip is opened by open_zip"),
    };
    Ok(Archive {
        cursor,
        format,
        codec,
    })
}
