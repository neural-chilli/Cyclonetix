use serde::{Deserialize, Serialize};
use std::error::Error;
use crate::utils::config::SerializationFormat;

pub fn serialize<T>(
    value: &T,
    format: &SerializationFormat,
) -> Result<Vec<u8>, Box<dyn Error>>
where
    T: Serialize,
{
    match format {
        SerializationFormat::Json => {
            // Serialize JSON as UTF‑8 bytes.
            Ok(serde_json::to_vec(value)?)
        }
        SerializationFormat::Binary => {
            // 1. Serialize with MessagePack.
            let bin = rmp_serde::to_vec(value)?;
            // 2. Prepend an 8‑byte header with the payload length.
            let mut data = Vec::with_capacity(8 + bin.len());
            data.extend_from_slice(&(bin.len() as u64).to_le_bytes());
            data.extend_from_slice(&bin);
            // 3. Compress the framed data with zstd.
            let compressed = zstd::stream::encode_all(&data[..], 0)?;
            Ok(compressed)
        }
    }
}

pub fn deserialize<T>(
    raw: &[u8],
    format: &SerializationFormat,
) -> Result<T, Box<dyn Error>>
where
    T: for<'de> Deserialize<'de>,
{
    match format {
        SerializationFormat::Json => {
            let s = std::str::from_utf8(raw)?;
            Ok(serde_json::from_str(s)?)
        }
        SerializationFormat::Binary => {
            if raw.is_empty() {
                return Err("Empty payload received".into());
            }
            let decompressed = zstd::stream::decode_all(raw)?;
            if decompressed.len() < 8 {
                return Err("Decompressed data is too short to contain a valid header".into());
            }
            let (header, payload) = decompressed.split_at(8);
            let expected_len = u64::from_le_bytes(header.try_into().unwrap()) as usize;
            if payload.len() != expected_len {
                return Err(format!(
                    "Payload length mismatch: expected {} bytes but got {} bytes",
                    expected_len,
                    payload.len()
                )
                    .into());
            }
            let obj = rmp_serde::from_slice(payload)?;
            Ok(obj)
        }
    }
}

