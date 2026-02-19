# libazo-rs

> Slop coded.

AZO decompression library written in Rust.

AZO is an LZ77 variant with arithmetic coding, adaptive probability models,
and an optional x86 CALL/JMP address filter.

## Usage

```rust
use std::io::Cursor;

let compressed: &[u8] = /* AZO compressed data */;
let mut output = Vec::new();
let crc = libazo::extract_azo(
    &mut Cursor::new(compressed),
    &mut output,
    compressed.len() as u64,
    None, // optional decryption callback
)?;
```

With decryption:

```rust
let crc = libazo::extract_azo(
    &mut reader,
    &mut writer,
    compressed_size,
    Some(&mut |data: &mut [u8]| {
        my_decryptor.decrypt(data);
    }),
)?;
```

## License

BSD-2-Clause
