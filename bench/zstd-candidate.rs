use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() == 2 && args[1] == "--version" {
        println!("libzstd {} through zstd 0.13.3", zstd::zstd_safe::version_string());
        return Ok(());
    }
    if args.len() != 4 { return Err(io::Error::other("expected encode|decode SOURCE DESTINATION")); }
    let mut input = BufReader::with_capacity(65536, File::open(&args[2])?);
    let output = BufWriter::with_capacity(65536, File::create(&args[3])?);
    if args[1] == "encode" {
        let mut encoder = zstd::stream::write::Encoder::new(output, 3)?;
        encoder.include_checksum(true)?;
        io::copy(&mut input, &mut encoder)?;
        encoder.finish()?.flush()
    } else if args[1] == "decode" {
        let mut decoder = zstd::stream::read::Decoder::with_buffer(input)?;
        let mut output = output;
        io::copy(&mut decoder, &mut output)?;
        output.flush()
    } else { Err(io::Error::other("expected encode or decode")) }
}
