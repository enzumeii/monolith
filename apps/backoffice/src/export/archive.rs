use std::io::{Cursor, Write};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

pub fn encode_archive(files: &[(&str, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in files {
        archive
            .start_file(*name, options)
            .map_err(|error| error.to_string())?;
        archive
            .write_all(bytes)
            .map_err(|error| error.to_string())?;
    }
    archive
        .finish()
        .map(Cursor::into_inner)
        .map_err(|error| error.to_string())
}
