//! Read and decode blobs

extern crate byteorder;
extern crate protobuf;

use block::{HeaderBlock, PrimitiveBlock};
use byteorder::ReadBytesExt;
use error::{new_blob_error, new_protobuf_error, BlobError, Result};
use proto::fileformat;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use util::{parse_message_from_bytes, parse_message_from_reader};

#[cfg(feature = "system-libz")]
use flate2::read::ZlibDecoder;

#[cfg(not(feature = "system-libz"))]
use inflate::DeflateDecoder;

/// Maximum allowed `BlobHeader` size in bytes.
pub static MAX_BLOB_HEADER_SIZE: u64 = 64 * 1024;

/// Maximum allowed uncompressed `Blob` content size in bytes.
pub static MAX_BLOB_MESSAGE_SIZE: u64 = 32 * 1024 * 1024;

/// The content type of a blob.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlobType<'a> {
    /// Blob contains a `HeaderBlock`.
    OsmHeader,
    /// Blob contains a `PrimitiveBlock`.
    OsmData,
    /// An unknown blob type with the given string identifier.
    /// Parsers should ignore unknown blobs they do not expect.
    Unknown(&'a str),
}

//TODO rename variants to fit proto files
/// The decoded content of a blob (analogous to `BlobType`).
#[derive(Clone, Debug)]
pub enum BlobDecode<'a> {
    /// Blob contains a `HeaderBlock`.
    OsmHeader(Box<HeaderBlock>),
    /// Blob contains a `PrimitiveBlock`.
    OsmData(PrimitiveBlock),
    /// An unknown blob type with the given string identifier.
    /// Parsers should ignore unknown blobs they do not expect.
    Unknown(&'a str),
}

/// The offset of a blob in bytes from stream start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteOffset(pub u64);

/// A blob.
///
/// A PBF file consists of a sequence of blobs. This type supports decoding the content of a blob
/// to different types of blocks that are usually more interesting to the user.
#[derive(Clone, Debug)]
pub struct Blob {
    header: fileformat::BlobHeader,
    blob: fileformat::Blob,
    offset: Option<ByteOffset>,
}

impl Blob {
    fn new(
        header: fileformat::BlobHeader,
        blob: fileformat::Blob,
        offset: Option<ByteOffset>,
    ) -> Blob {
        Blob {
            header,
            blob,
            offset,
        }
    }

    /// Decodes the Blob and tries to obtain the inner content (usually a `HeaderBlock` or a
    /// `PrimitiveBlock`). This operation might involve an expensive decompression step.
    pub fn decode(&self) -> Result<BlobDecode> {
        match self.get_type() {
            BlobType::OsmHeader => {
                let block = Box::new(self.to_headerblock()?);
                Ok(BlobDecode::OsmHeader(block))
            }
            BlobType::OsmData => {
                let block = self.to_primitiveblock()?;
                Ok(BlobDecode::OsmData(block))
            }
            BlobType::Unknown(x) => Ok(BlobDecode::Unknown(x)),
        }
    }

    /// Returns the type of a blob without decoding its content.
    pub fn get_type(&self) -> BlobType {
        match self.header.get_field_type() {
            "OSMHeader" => BlobType::OsmHeader,
            "OSMData" => BlobType::OsmData,
            x => BlobType::Unknown(x),
        }
    }

    /// Returns the byte offset of the blob from the start of its source stream.
    /// This might be `None` if the source stream does not implement `Seek`.
    pub fn offset(&self) -> Option<ByteOffset> {
        self.offset
    }

    /// Tries to decode the blob to a `HeaderBlock`. This operation might involve an expensive
    /// decompression step.
    pub fn to_headerblock(&self) -> Result<HeaderBlock> {
        decode_blob(&self.blob).map(HeaderBlock::new)
    }

    /// Tries to decode the blob to a `PrimitiveBlock`. This operation might involve an expensive
    /// decompression step.
    pub fn to_primitiveblock(&self) -> Result<PrimitiveBlock> {
        decode_blob(&self.blob).map(PrimitiveBlock::new)
    }
}

/// A reader for PBF files that allows iterating over `Blob`s.
#[derive(Clone, Debug)]
pub struct BlobReader<R: Read> {
    reader: R,
    /// Current reader offset in bytes from the start of the stream.
    offset: Option<ByteOffset>,
    last_blob_ok: bool,
}

impl<R: Read> BlobReader<R> {
    /// Creates a new `BlobReader`.
    ///
    /// # Example
    /// ```
    /// use osmpbf::*;
    ///
    /// # fn foo() -> Result<()> {
    /// let f = std::fs::File::open("tests/test.osm.pbf")?;
    /// let buf_reader = std::io::BufReader::new(f);
    ///
    /// let reader = BlobReader::new(buf_reader);
    ///
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    pub fn new(reader: R) -> BlobReader<R> {
        BlobReader {
            reader,
            offset: None,
            last_blob_ok: true,
        }
    }
}

impl BlobReader<BufReader<File>> {
    /// Tries to open the file at the given path and constructs a `BlobReader` from this.
    ///
    /// # Errors
    /// Returns the same errors that `std::fs::File::open` returns.
    ///
    /// # Example
    /// ```
    /// use osmpbf::*;
    ///
    /// # fn foo() -> Result<()> {
    /// let reader = BlobReader::from_path("tests/test.osm.pbf")?;
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let f = File::open(path)?;
        let reader = BufReader::new(f);

        Ok(BlobReader {
            reader,
            offset: Some(ByteOffset(0)),
            last_blob_ok: true,
        })
    }
}

impl<R: Read> Iterator for BlobReader<R> {
    type Item = Result<Blob>;

    fn next(&mut self) -> Option<Self::Item> {
        // Stop iteration if there was an error.
        if !self.last_blob_ok {
            return None;
        }

        let prev_offset = self.offset;

        let header_size: u64 = match self.reader.read_u32::<byteorder::BigEndian>() {
            Ok(n) => {
                self.offset = self.offset.map(|x| ByteOffset(x.0 + 4));
                u64::from(n)
            }
            Err(e) => {
                self.offset = None;
                match e.kind() {
                    ::std::io::ErrorKind::UnexpectedEof => {
                        //TODO This also accepts corrupted files in the case of 1-3 available bytes
                        return None;
                    }
                    _ => {
                        self.last_blob_ok = false;
                        return Some(Err(new_blob_error(BlobError::InvalidHeaderSize)));
                    }
                }
            }
        };

        if header_size >= MAX_BLOB_HEADER_SIZE {
            self.last_blob_ok = false;
            return Some(Err(new_blob_error(BlobError::HeaderTooBig {
                size: header_size,
            })));
        }

        let header: fileformat::BlobHeader =
            match parse_message_from_reader(&mut self.reader.by_ref().take(header_size)) {
                Ok(header) => header,
                Err(e) => {
                    self.offset = None;
                    self.last_blob_ok = false;
                    return Some(Err(new_protobuf_error(e, "blob header")));
                }
            };

        let blob: fileformat::Blob = match parse_message_from_reader(
            &mut self.reader.by_ref().take(header.get_datasize() as u64),
        ) {
            Ok(blob) => blob,
            Err(e) => {
                self.offset = None;
                self.last_blob_ok = false;
                return Some(Err(new_protobuf_error(e, "blob content")));
            }
        };

        self.offset = self
            .offset
            .map(|x| ByteOffset(x.0 + header_size + header.get_datasize() as u64));

        Some(Ok(Blob::new(header, blob, prev_offset)))
    }
}

impl<R: Read + Seek> BlobReader<R> {
    /// Creates a new `BlobReader` from the given reader that is seekable and will be initialized
    /// with a valid offset.
    ///
    /// # Example
    /// ```
    /// use osmpbf::*;
    ///
    /// # fn foo() -> Result<()> {
    /// let f = std::fs::File::open("tests/test.osm.pbf")?;
    /// let buf_reader = std::io::BufReader::new(f);
    ///
    /// let mut reader = BlobReader::new_seekable(buf_reader)?;
    /// let first_blob = reader.next().unwrap()?;
    ///
    /// assert_eq!(first_blob.offset(), Some(ByteOffset(0)));
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    pub fn new_seekable(mut reader: R) -> Result<BlobReader<R>> {
        let pos = reader.seek(SeekFrom::Current(0))?;

        Ok(BlobReader {
            reader,
            offset: Some(ByteOffset(pos)),
            last_blob_ok: true,
        })
    }

    /// Seek to an offset in bytes from the start of the stream.
    ///
    /// # Example
    /// ```
    /// use osmpbf::*;
    ///
    /// # fn foo() -> Result<()> {
    /// let mut reader = BlobReader::from_path("tests/test.osm.pbf")?;
    /// let first_blob = reader.next().unwrap()?;
    /// let second_blob = reader.next().unwrap()?;
    ///
    /// reader.seek(first_blob.offset().unwrap())?;
    ///
    /// let first_blob_again = reader.next().unwrap()?;
    /// assert_eq!(first_blob.offset(), first_blob_again.offset());
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    pub fn seek(&mut self, pos: ByteOffset) -> Result<()> {
        match self.reader.seek(SeekFrom::Start(pos.0)) {
            Ok(offset) => {
                self.offset = Some(ByteOffset(offset));
                Ok(())
            }
            Err(e) => {
                self.offset = None;
                Err(e.into())
            }
        }
    }

    /// Seek to an offset in bytes. (See `std::io::Seek`)
    pub fn seek_raw(&mut self, pos: SeekFrom) -> Result<u64> {
        match self.reader.seek(pos) {
            Ok(offset) => {
                self.offset = Some(ByteOffset(offset));
                Ok(offset)
            }
            Err(e) => {
                self.offset = None;
                Err(e.into())
            }
        }
    }
}

impl BlobReader<BufReader<File>> {
    /// Creates a new `BlobReader` from the given path that is seekable and will be initialized
    /// with a valid offset.
    ///
    /// # Example
    /// ```
    /// use osmpbf::*;
    ///
    /// # fn foo() -> Result<()> {
    /// let mut reader = BlobReader::seekable_from_path("tests/test.osm.pbf")?;
    /// let first_blob = reader.next().unwrap()?;
    ///
    /// assert_eq!(first_blob.offset(), Some(ByteOffset(0)));
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    pub fn seekable_from_path<P: AsRef<Path>>(path: P) -> Result<BlobReader<BufReader<File>>> {
        let f = File::open(path.as_ref())?;
        let buf_reader = BufReader::new(f);
        Self::new_seekable(buf_reader)
    }
}

#[cfg(feature = "system-libz")]
pub(crate) fn decode_blob<T>(blob: &fileformat::Blob) -> Result<T>
where
    T: protobuf::Message,
{
    if blob.has_raw() {
        let size = blob.get_raw().len() as u64;
        if size < MAX_BLOB_MESSAGE_SIZE {
            parse_message_from_bytes(blob.get_raw())
                .map_err(|e| new_protobuf_error(e, "raw blob data"))
        } else {
            Err(new_blob_error(BlobError::MessageTooBig { size }))
        }
    } else if blob.has_zlib_data() {
        let mut decoder = ZlibDecoder::new(blob.get_zlib_data()).take(MAX_BLOB_MESSAGE_SIZE);
        parse_message_from_reader(&mut decoder).map_err(|e| new_protobuf_error(e, "blob zlib data"))
    } else {
        Err(new_blob_error(BlobError::Empty))
    }
}

#[cfg(not(feature = "system-libz"))]
pub(crate) fn decode_blob<T>(blob: &fileformat::Blob) -> Result<T>
where
    T: protobuf::Message,
{
    if blob.has_raw() {
        let size = blob.get_raw().len() as u64;
        if size < MAX_BLOB_MESSAGE_SIZE {
            parse_message_from_bytes(blob.get_raw())
                .map_err(|e| new_protobuf_error(e, "raw blob data"))
        } else {
            Err(new_blob_error(BlobError::MessageTooBig { size }))
        }
    } else if blob.has_zlib_data() {
        let mut decoder =
            DeflateDecoder::from_zlib(blob.get_zlib_data()).take(MAX_BLOB_MESSAGE_SIZE);
        parse_message_from_reader(&mut decoder).map_err(|e| new_protobuf_error(e, "blob zlib data"))
    } else {
        Err(new_blob_error(BlobError::Empty))
    }
}
