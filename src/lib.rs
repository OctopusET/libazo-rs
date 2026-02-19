/// AZO decompression library.
///
/// LZ77 variant with arithmetic coding, x86 jump filter, and adaptive
/// probability models.
pub(crate) mod history;
pub(crate) mod match_code;
pub(crate) mod model;
pub(crate) mod range;
pub(crate) mod table;
pub mod x86;

use std::fmt;
use std::io::{Read, Write};

use self::match_code::MatchCode;
use self::model::BoolState;
use self::model::PredictProb;
use self::range::RangeDecoder;

#[derive(Debug)]
pub enum AzoError {
    Io(std::io::Error),
    Failed(String),
}

impl fmt::Display for AzoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Failed(e) => write!(f, "AZO error: {e}"),
        }
    }
}

impl std::error::Error for AzoError {}

impl From<std::io::Error> for AzoError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Decryption callback type. Called with a mutable slice to decrypt in place.
pub type DecryptFn<'a> = &'a mut dyn FnMut(&mut [u8]);

/// Extract an AZO compressed stream.
///
/// Reads `compressed_size` bytes from `reader`, optionally decrypts them
/// with `decrypt`, decompresses, and writes output to `writer`.
/// Returns the CRC32 of the decompressed data.
pub fn extract_azo<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    compressed_size: u64,
    decrypt: Option<DecryptFn<'_>>,
) -> Result<u32, AzoError> {
    let mut data = vec![0u8; compressed_size as usize];
    reader.read_exact(&mut data)?;
    if let Some(f) = decrypt {
        f(&mut data);
    }

    if data.len() < 2 {
        return Err(AzoError::Failed("data too short".into()));
    }

    // Stream header
    let version = data[0];
    let flags = data[1];
    if version != 0x31 {
        return Err(AzoError::Failed(format!(
            "unsupported AZO version: {version}"
        )));
    }
    let x86_filter_enabled = flags & 0x01 != 0;

    let mut hasher = crc32fast::Hasher::new();
    let mut pos = 2;

    // Process blocks
    loop {
        if pos + 12 > data.len() {
            return Err(AzoError::Failed("truncated block header".into()));
        }

        let block_size = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap());
        let compress_size = u32::from_be_bytes(data[pos + 4..pos + 8].try_into().unwrap());
        let check_value = u32::from_be_bytes(data[pos + 8..pos + 12].try_into().unwrap());
        pos += 12;

        if block_size == 0 && compress_size == 0 {
            break;
        }

        if (block_size ^ compress_size) != check_value {
            return Err(AzoError::Failed("block check value mismatch".into()));
        }

        if pos + compress_size as usize > data.len() {
            return Err(AzoError::Failed("truncated block data".into()));
        }

        let block_data = &data[pos..pos + compress_size as usize];
        pos += compress_size as usize;

        let mut output = if compress_size == block_size {
            block_data.to_vec()
        } else {
            decompress_block(block_data, block_size as usize)?
        };

        if x86_filter_enabled {
            x86::x86_filter(&mut output);
        }

        hasher.update(&output);
        writer.write_all(&output)?;
    }

    Ok(hasher.finalize())
}

fn decompress_block(data: &[u8], block_size: usize) -> Result<Vec<u8>, AzoError> {
    let mut entropy = RangeDecoder::new(data);
    entropy.initialize();

    let mut buf = vec![0u8; block_size];

    let mut alpha = PredictProb::new(256, 256, 5);
    let mut match_flag = BoolState::new(8);
    let mut match_code = MatchCode::new();

    buf[0] = alpha.decode(&mut entropy, 0) as u8;

    let mut i = 1;
    while i < block_size {
        if match_flag.decode(&mut entropy) == 0 {
            let context = buf[i - 1] as usize;
            buf[i] = alpha.decode(&mut entropy, context) as u8;
            i += 1;
        } else {
            let (distance, length) = match_code.decode(&mut entropy, i as u32);

            if distance == 0 || distance as usize > i {
                return Err(AzoError::Failed(format!(
                    "invalid match: distance={distance}, pos={i}"
                )));
            }

            let src_start = i - distance as usize;
            for j in 0..length as usize {
                if i + j >= block_size {
                    break;
                }
                buf[i + j] = buf[src_start + j];
            }
            i += length as usize;
        }
    }

    Ok(buf)
}
