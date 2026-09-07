//! A minimal, read-only GGUF header parser: enough to read
//! `general.architecture` and the handful of per-architecture metadata keys
//! ([`GgufMetadata`]) the KV-cache formula in [`crate::hardware`] needs, for
//! a local `.gguf` file scanned off disk.
//!
//! Hand-rolled rather than pulling in a crate: the GGUF header is a small,
//! simple typed KV format (see
//! <https://github.com/ggml-org/ggml/blob/master/docs/gguf.md>), and we only
//! ever need to *read* a handful of scalar values out of it, never write one
//! or touch tensor data.
//!
//! Reads stop the instant every metadata KV pair has been consumed -- the
//! (much larger) tensor info table and tensor data that follow are never
//! touched, so even a multi-gigabyte model file only costs a read of its
//! (typically single-digit-MB) metadata section.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use whspr_core::{Result, WhsprError};

use crate::hardware::{estimated_llm_footprint, LlmShape};

/// The GGUF magic, as the raw bytes on disk (mirrors
/// [`crate::models::classify`]'s magic-byte check).
const GGUF_MAGIC: [u8; 4] = *b"GGUF";

/// A single metadata string is never legitimately anywhere near this large
/// (the biggest realistic one is a license blurb); a length field claiming
/// more than this is almost certainly a corrupt or hostile file, so bail
/// out instead of attempting a multi-gigabyte allocation.
const MAX_STRING_LEN: u64 = 16 * 1024 * 1024;

/// GGUF metadata value type tags (`enum gguf_metadata_value_type` in the
/// GGUF spec) -- only used to size/skip values we don't care about.
mod value_type {
    pub const UINT8: u32 = 0;
    pub const INT8: u32 = 1;
    pub const UINT16: u32 = 2;
    pub const INT16: u32 = 3;
    pub const UINT32: u32 = 4;
    pub const INT32: u32 = 5;
    pub const FLOAT32: u32 = 6;
    pub const BOOL: u32 = 7;
    pub const STRING: u32 = 8;
    pub const ARRAY: u32 = 9;
    pub const UINT64: u32 = 10;
    pub const INT64: u32 = 11;
    pub const FLOAT64: u32 = 12;
}

/// The architecture shape read out of a GGUF file's metadata -- exactly what
/// [`LlmShape`] needs (see [`GgufMetadata::shape`]), plus the architecture
/// name for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufMetadata {
    /// `general.architecture`, e.g. `"qwen2"` or `"llama"`.
    pub architecture: String,
    /// `<arch>.block_count`.
    pub n_layers: u32,
    /// `<arch>.embedding_length`.
    pub n_embd: u32,
    /// `<arch>.attention.head_count`.
    pub n_head: u32,
    /// `<arch>.attention.head_count_kv` -- defaults to [`Self::n_head`] when
    /// the key is absent (ordinary multi-head attention, no GQA).
    pub n_head_kv: u32,
    /// `<arch>.context_length`.
    pub context_length: u32,
}

impl GgufMetadata {
    /// This file's architecture shape, for [`crate::hardware::kv_cache_bytes`].
    pub fn shape(&self) -> LlmShape {
        LlmShape {
            n_layers: u64::from(self.n_layers),
            n_embd: u64::from(self.n_embd),
            n_head: u64::from(self.n_head),
            n_head_kv: u64::from(self.n_head_kv),
            context_length: u64::from(self.context_length),
        }
    }
}

/// Reads `path`'s GGUF header and returns the metadata the KV-cache formula
/// needs. Errors if the file isn't a GGUF (bad magic), is truncated or
/// otherwise malformed, or is missing one of the required keys
/// (`general.architecture`, `<arch>.block_count`, `<arch>.embedding_length`,
/// `<arch>.attention.head_count`, `<arch>.context_length` --
/// `attention.head_count_kv` alone is optional, see
/// [`GgufMetadata::n_head_kv`]).
pub fn read_metadata(path: &Path) -> Result<GgufMetadata> {
    let file = File::open(path)
        .map_err(|e| WhsprError::Other(format!("failed to open {}: {e}", path.display())))?;
    let mut r = BufReader::new(file);
    parse(&mut r).map_err(|e| WhsprError::Other(format!("{}: {e}", path.display())))
}

/// Convenience: reads `path`'s GGUF metadata and computes its exact,
/// KV-cache-aware footprint for `size_bytes` of on-disk weights (see
/// [`crate::hardware::estimated_llm_footprint`]). `None` if the header can't
/// be read or parsed (not a GGUF, truncated, or missing a required key) --
/// callers fall back to [`crate::hardware::estimated_footprint`]'s coarse,
/// size-only estimate in that case.
pub fn estimated_footprint_for_file(path: &Path, size_bytes: u64) -> Option<u64> {
    let meta = read_metadata(path).ok()?;
    Some(estimated_llm_footprint(size_bytes, meta.shape()))
}

/// The actual header walk, kept separate from [`read_metadata`] so every
/// error site can just use `?` against a plain [`io::Result`] and the public
/// function does the one path-prefixing translation into [`WhsprError`].
fn parse(r: &mut impl Read) -> io::Result<GgufMetadata> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if magic != GGUF_MAGIC {
        return Err(invalid("not a GGUF file (bad magic)"));
    }

    let version = read_u32(r)?;
    if !(2..=3).contains(&version) {
        return Err(invalid(format!("unsupported GGUF version {version}")));
    }

    let _tensor_count = read_u64(r)?;
    let kv_count = read_u64(r)?;

    let mut architecture: Option<String> = None;
    let mut n_layers: Option<u32> = None;
    let mut n_embd: Option<u32> = None;
    let mut n_head: Option<u32> = None;
    let mut n_head_kv: Option<u32> = None;
    let mut context_length: Option<u32> = None;

    for _ in 0..kv_count {
        let key = read_string(r)?;
        let vtype = read_u32(r)?;

        if key == "general.architecture" && vtype == value_type::STRING {
            architecture = Some(read_string(r)?);
            continue;
        }

        let is_uint = matches!(
            vtype,
            value_type::UINT32 | value_type::INT32 | value_type::UINT64 | value_type::INT64
        );
        let slot = if !is_uint {
            None
        } else if key.ends_with(".block_count") {
            Some(&mut n_layers)
        } else if key.ends_with(".embedding_length") {
            Some(&mut n_embd)
        } else if key.ends_with(".attention.head_count_kv") {
            Some(&mut n_head_kv)
        } else if key.ends_with(".attention.head_count") {
            Some(&mut n_head)
        } else if key.ends_with(".context_length") {
            Some(&mut context_length)
        } else {
            None
        };

        match slot {
            Some(slot) => *slot = Some(read_uint_value(r, vtype)? as u32),
            None => skip_value(r, vtype)?,
        }
    }

    Ok(GgufMetadata {
        architecture: architecture.ok_or_else(|| invalid("missing general.architecture"))?,
        n_layers: n_layers.ok_or_else(|| invalid("missing <arch>.block_count"))?,
        n_embd: n_embd.ok_or_else(|| invalid("missing <arch>.embedding_length"))?,
        n_head: n_head.ok_or_else(|| invalid("missing <arch>.attention.head_count"))?,
        // Ordinary (non-GQA) multi-head attention omits head_count_kv
        // entirely -- it's simply equal to head_count.
        n_head_kv: n_head_kv.unwrap_or(n_head.unwrap_or(0)),
        context_length: context_length.ok_or_else(|| invalid("missing <arch>.context_length"))?,
    })
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

/// Reads a fixed-width integer value already known (by [`parse`]) to be one
/// of the `UINT32`/`INT32`/`UINT64`/`INT64` tags -- GGUF only ever stores
/// small non-negative counts in these keys, so the signed/unsigned
/// distinction doesn't matter, only the byte width does.
fn read_uint_value(r: &mut impl Read, vtype: u32) -> io::Result<u64> {
    match vtype {
        value_type::UINT32 | value_type::INT32 => Ok(u64::from(read_u32(r)?)),
        value_type::UINT64 | value_type::INT64 => read_u64(r),
        _ => Err(invalid("read_uint_value called with a non-integer type")),
    }
}

/// Reads a GGUF string (`uint64` length prefix + that many UTF-8 bytes, not
/// null-terminated). Lossy-decodes rather than erroring on invalid UTF-8 --
/// we only care about a handful of well-formed ASCII keys/values, and a
/// mangled human-readable field elsewhere in the file shouldn't block
/// reading those.
fn read_string(r: &mut impl Read) -> io::Result<String> {
    let len = read_u64(r)?;
    if len > MAX_STRING_LEN {
        return Err(invalid(format!("string length {len} exceeds sane bound")));
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Discards a value of `vtype` without interpreting it -- how [`parse`] skips
/// over every metadata key it doesn't care about (including, notably, the
/// often-multi-megabyte tokenizer vocabulary array).
fn skip_value(r: &mut impl Read, vtype: u32) -> io::Result<()> {
    match vtype {
        value_type::UINT8 | value_type::INT8 | value_type::BOOL => skip_bytes(r, 1),
        value_type::UINT16 | value_type::INT16 => skip_bytes(r, 2),
        value_type::UINT32 | value_type::INT32 | value_type::FLOAT32 => skip_bytes(r, 4),
        value_type::UINT64 | value_type::INT64 | value_type::FLOAT64 => skip_bytes(r, 8),
        value_type::STRING => read_string(r).map(|_| ()),
        value_type::ARRAY => skip_array(r),
        other => Err(invalid(format!("unknown GGUF value type {other}"))),
    }
}

/// Skips a GGUF array value: an element-type tag, a `uint64` element count,
/// then that many values of the element type -- recursing through
/// [`skip_value`] per element (the spec permits nested arrays, though no
/// real-world file uses them).
fn skip_array(r: &mut impl Read) -> io::Result<()> {
    let elem_type = read_u32(r)?;
    let len = read_u64(r)?;
    for _ in 0..len {
        skip_value(r, elem_type)?;
    }
    Ok(())
}

fn skip_bytes(r: &mut impl Read, n: u64) -> io::Result<()> {
    io::copy(&mut r.by_ref().take(n), &mut io::sink()).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny GGUF-byte-format builder, just expressive enough for these
    /// tests: a magic/version/tensor_count(0) header followed by whatever
    /// metadata KV pairs the test pushes.
    #[derive(Default)]
    struct Fixture {
        kvs: Vec<u8>,
        count: u64,
    }

    impl Fixture {
        fn push_u32(&mut self, key: &str, value: u32) -> &mut Self {
            self.push_key(key);
            self.kvs
                .extend_from_slice(&value_type::UINT32.to_le_bytes());
            self.kvs.extend_from_slice(&value.to_le_bytes());
            self
        }

        fn push_string(&mut self, key: &str, value: &str) -> &mut Self {
            self.push_key(key);
            self.kvs
                .extend_from_slice(&value_type::STRING.to_le_bytes());
            self.push_raw_string(value);
            self
        }

        /// A `tokenizer.ggml.tokens`-shaped array-of-strings, to prove
        /// [`skip_array`] correctly steps over a multi-element array without
        /// throwing off the keys that follow it.
        fn push_string_array(&mut self, key: &str, values: &[&str]) -> &mut Self {
            self.push_key(key);
            self.kvs.extend_from_slice(&value_type::ARRAY.to_le_bytes());
            self.kvs
                .extend_from_slice(&value_type::STRING.to_le_bytes());
            self.kvs
                .extend_from_slice(&(values.len() as u64).to_le_bytes());
            for v in values {
                self.push_raw_string(v);
            }
            self
        }

        fn push_key(&mut self, key: &str) {
            self.count += 1;
            self.push_raw_string(key);
        }

        fn push_raw_string(&mut self, s: &str) {
            self.kvs.extend_from_slice(&(s.len() as u64).to_le_bytes());
            self.kvs.extend_from_slice(s.as_bytes());
        }

        fn build(&self) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(&GGUF_MAGIC);
            out.extend_from_slice(&3u32.to_le_bytes()); // version
            out.extend_from_slice(&0u64.to_le_bytes()); // tensor_count
            out.extend_from_slice(&self.count.to_le_bytes()); // metadata_kv_count
            out.extend_from_slice(&self.kvs);
            out
        }
    }

    fn write_fixture(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.gguf");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn parses_a_gqa_model_with_a_vocab_array_in_between() {
        let mut fx = Fixture::default();
        fx.push_string("general.architecture", "testarch")
            .push_string_array("tokenizer.ggml.tokens", &["<s>", "</s>", "hi"])
            .push_u32("testarch.block_count", 4)
            .push_u32("testarch.embedding_length", 64)
            .push_u32("testarch.attention.head_count", 8)
            .push_u32("testarch.attention.head_count_kv", 2)
            .push_u32("testarch.context_length", 2048);
        let (_dir, path) = write_fixture(&fx.build());

        let meta = read_metadata(&path).expect("should parse");
        assert_eq!(
            meta,
            GgufMetadata {
                architecture: "testarch".to_string(),
                n_layers: 4,
                n_embd: 64,
                n_head: 8,
                n_head_kv: 2,
                context_length: 2048,
            }
        );
    }

    #[test]
    fn missing_head_count_kv_defaults_to_head_count() {
        let mut fx = Fixture::default();
        fx.push_string("general.architecture", "testarch")
            .push_u32("testarch.block_count", 4)
            .push_u32("testarch.embedding_length", 64)
            .push_u32("testarch.attention.head_count", 8)
            .push_u32("testarch.context_length", 2048);
        let (_dir, path) = write_fixture(&fx.build());

        let meta = read_metadata(&path).expect("should parse");
        assert_eq!(meta.n_head_kv, meta.n_head);
    }

    #[test]
    fn rejects_a_bad_magic() {
        let (_dir, path) = write_fixture(b"NOPE1234");
        assert!(read_metadata(&path).is_err());
    }

    #[test]
    fn rejects_a_truncated_file() {
        let mut bytes = Fixture::default()
            .push_string("general.architecture", "testarch")
            .build();
        bytes.truncate(bytes.len() - 2);
        let (_dir, path) = write_fixture(&bytes);
        assert!(read_metadata(&path).is_err());
    }

    #[test]
    fn errors_on_a_missing_required_key() {
        let mut fx = Fixture::default();
        fx.push_string("general.architecture", "testarch")
            .push_u32("testarch.block_count", 4);
        // embedding_length/head_count/context_length are never set.
        let (_dir, path) = write_fixture(&fx.build());
        assert!(read_metadata(&path).is_err());
    }

    #[test]
    fn estimated_footprint_for_file_falls_back_to_none_on_a_bad_file() {
        let (_dir, path) = write_fixture(b"NOPE1234");
        assert_eq!(estimated_footprint_for_file(&path, 1024), None);
    }

    #[test]
    fn estimated_footprint_for_file_matches_the_shared_formula() {
        let mut fx = Fixture::default();
        fx.push_string("general.architecture", "testarch")
            .push_u32("testarch.block_count", 4)
            .push_u32("testarch.embedding_length", 64)
            .push_u32("testarch.attention.head_count", 8)
            .push_u32("testarch.attention.head_count_kv", 2)
            .push_u32("testarch.context_length", 2048);
        let (_dir, path) = write_fixture(&fx.build());

        let meta = read_metadata(&path).unwrap();
        let expected = estimated_llm_footprint(4096, meta.shape());
        assert_eq!(estimated_footprint_for_file(&path, 4096), Some(expected));
    }
}
