use crate::error::{Error, Result};
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::os::unix::fs::FileTypeExt;

pub trait ReadSeek: Read + Seek + Send {}

impl<T: Read + Seek + Send> ReadSeek for T {}

pub enum Input {
    Seekable(Box<dyn ReadSeek>),
    Sequential(Box<dyn Read + Send>),
}

impl Input {
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Seekable(_) => "seekable",
            Self::Sequential(_) => "sequential",
        }
    }
}

pub struct Source {
    pub input: Input,
    pub name: Option<Vec<u8>>,
}

impl Source {
    pub fn from_file(file: File, name: Option<Vec<u8>>) -> Result<Self> {
        let file_type = file
            .metadata()
            .map_err(|error| Error::from_open(&error))?
            .file_type();
        let sequential =
            file_type.is_fifo() || file_type.is_socket() || file_type.is_char_device();
        let input = if sequential {
            Input::Sequential(Box::new(file))
        } else {
            Input::Seekable(Box::new(file))
        };
        Ok(Self { input, name })
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            input: Input::Seekable(Box::new(Cursor::new(bytes))),
            name: None,
        }
    }

    pub fn from_reader(reader: impl Read + Send + 'static) -> Self {
        Self {
            input: Input::Sequential(Box::new(reader)),
            name: None,
        }
    }
}
