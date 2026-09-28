// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2004-2026 The Stockfish developers (see notices/upstream/Pikafish-AUTHORS)
// Copyright (c) 2026 SpenserCai and PikaRust contributors
// Rust adaptation and modifications, 2026; see NOTICE.md for upstream sources.
// 2026-09-28: current Pikafish architecture and explicit legacy model decoding.
// Distributed without warranty; see LICENSE and notices/upstream/Pikafish-COPYRIGHT.

use std::io::Read;
use std::path::Path;

use thiserror::Error;

pub const VERSION: u32 = 0x6A44_8AFA;
const LEGACY_VERSION: u32 = 0x7AF3_2F20;
pub const TRANSFORMED_DIMS: usize = 1024;
pub const PSQ_DIMS: usize = 16_536;
pub const THREAT_DIMS: usize = 45_547;
pub const PSQT_BUCKETS: usize = 16;
pub const LAYER_STACKS: usize = 16;
pub const L2_BIG: usize = 32;
pub const L3_BIG: usize = 32;
pub const WEIGHT_SCALE_BITS: u32 = 6;
pub const OUTPUT_SCALE: i32 = 16;
#[allow(dead_code)]
pub const PS_NB: usize = 689;
#[allow(dead_code)]
pub const ATTACK_BUCKET_NB: usize = 4;
#[allow(dead_code)]
pub const KING_BUCKET_NB: usize = 6;

const FC0_OUTPUTS: usize = L2_BIG;
const FC1_INPUTS_PADDED: usize = 64;
// Upstream NetworkArchitecture::get_hash_value(). Both supported architectures
// have the same affine output sizes, so their architecture hashes coincide.
// Version and feature-transformer hashes must also agree to select a format.
const ARCHITECTURE_HASH: u32 = 0x6333_7116;

const LEB128_MAGIC: &[u8; 17] = b"COMPRESSED_LEB128";

/// Supported on-disk NNUE architectures, selected by the file version and hashes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelFormat {
    /// Pikafish's 1024/32/32 network with concatenated hidden activations.
    Current,
    /// The 1024/31/32 network retained for existing model users.
    Legacy,
}

impl ModelFormat {
    const fn feature_hash(self) -> u32 {
        let threat_hash = match self {
            Self::Current => super::features::full_threats::HASH_VALUE,
            Self::Legacy => 0x8F23_4CB8,
        };
        threat_hash.rotate_left(1)
            ^ super::features::half_ka_v2_hm::HASH_VALUE
            ^ (TRANSFORMED_DIMS as u32 * 2)
    }

    const fn fc2_inputs(self) -> usize {
        match self {
            Self::Current => 128,
            Self::Legacy => 32,
        }
    }
}

#[derive(Debug, Error)]
pub enum NnueError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "unsupported NNUE version 0x{got:08X}; expected current 0x{:08X} or legacy 0x{:08X}",
        VERSION,
        LEGACY_VERSION
    )]
    InvalidVersion { got: u32 },
    #[error("invalid {section} NNUE hash: expected 0x{expected:08X}, got 0x{got:08X}")]
    InvalidHash {
        section: &'static str,
        expected: u32,
        got: u32,
    },
    #[error("zstd decompression failed: {0}")]
    Zstd(std::io::Error),
    #[error("unexpected end of data at offset {offset}")]
    UnexpectedEof { offset: usize },
    #[error("invalid LEB128 magic at offset {offset}")]
    InvalidLeb128Magic { offset: usize },
    #[error("LEB128 decode produced {decoded} values, expected {expected}")]
    Leb128CountMismatch { decoded: usize, expected: usize },
    #[error("invalid signed LEB128 integer for {bits} bits")]
    InvalidLeb128 { bits: u32 },
    #[error("unexpected trailing data at offset {offset}")]
    TrailingData { offset: usize },
}

pub struct FeatureTransformerWeights {
    pub biases: Box<[i16; TRANSFORMED_DIMS]>,
    pub weights: Box<[i8]>,
    pub psqt_weights: Box<[i32]>,
    pub threat_weights: Box<[i8]>,
    pub threat_psqt_weights: Box<[i32]>,
}

pub struct LayerStackWeights {
    pub fc0_biases: Box<[i32; FC0_OUTPUTS]>,
    pub fc0_weights: Box<[i8]>,
    pub fc1_biases: Box<[i32; L3_BIG]>,
    pub fc1_weights: Box<[i8]>,
    pub fc2_biases: Box<[i32; 1]>,
    pub fc2_weights: Box<[i8]>,
}

pub struct NnueModel {
    /// Architecture used to decode and evaluate these weights.
    pub format: ModelFormat,
    pub description: String,
    pub ft: FeatureTransformerWeights,
    pub layer_stacks: Vec<LayerStackWeights>,
}

impl NnueModel {
    /// Load a current or retained legacy model, validating all structural hashes.
    ///
    /// Unknown versions, incompatible architectures, malformed parameter blocks,
    /// and trailing data return an error without falling back to another format.
    pub fn load(path: &Path) -> Result<Self, NnueError> {
        let compressed = std::fs::read(path)?;
        let decompressed =
            zstd::stream::decode_all(std::io::Cursor::new(&compressed)).map_err(NnueError::Zstd)?;

        Self::decode(&decompressed)
    }

    fn decode(data: &[u8]) -> Result<Self, NnueError> {
        let mut pos = 0;

        let version = read_u32(data, &mut pos)?;
        let format = match version {
            VERSION => ModelFormat::Current,
            LEGACY_VERSION => ModelFormat::Legacy,
            got => return Err(NnueError::InvalidVersion { got }),
        };
        read_hash(
            data,
            &mut pos,
            format.feature_hash() ^ ARCHITECTURE_HASH,
            "network",
        )?;
        let desc_size = read_u32(data, &mut pos)? as usize;
        let description = read_string(data, &mut pos, desc_size)?;

        let ft = read_feature_transformer(data, &mut pos, format)?;
        let mut layer_stacks = Vec::with_capacity(LAYER_STACKS);
        for _ in 0..LAYER_STACKS {
            layer_stacks.push(read_layer_stack(data, &mut pos, format)?);
        }
        if pos != data.len() {
            return Err(NnueError::TrailingData { offset: pos });
        }

        Ok(Self {
            format,
            description,
            ft,
            layer_stacks,
        })
    }
}

/// Transpose weight matrix from file order `[out_dim][in_dim]` to SIMD order `[in_dim][out_dim]`.
fn transpose_weights(src: &[i8], out_dim: usize, in_dim: usize) -> Box<[i8]> {
    let mut dst = vec![0i8; out_dim * in_dim];
    for o in 0..out_dim {
        for i in 0..in_dim {
            dst[i * out_dim + o] = src[o * in_dim + i];
        }
    }
    dst.into_boxed_slice()
}

fn read_feature_transformer(
    data: &[u8],
    pos: &mut usize,
    format: ModelFormat,
) -> Result<FeatureTransformerWeights, NnueError> {
    read_hash(data, pos, format.feature_hash(), "feature transformer")?;

    let mut biases = Box::new([0i16; TRANSFORMED_DIMS]);
    let consumed = read_leb128_i16(data, *pos, biases.as_mut_slice())?;
    *pos += consumed;

    let threat_weight_count = THREAT_DIMS * TRANSFORMED_DIMS;
    let mut threat_weights = vec![0i8; threat_weight_count].into_boxed_slice();
    read_i8_slice(data, pos, &mut threat_weights)?;

    let threat_psqt_count = THREAT_DIMS * PSQT_BUCKETS;
    let mut threat_psqt_weights = vec![0i32; threat_psqt_count].into_boxed_slice();
    if format == ModelFormat::Current {
        *pos += read_leb128_i32(data, *pos, &mut threat_psqt_weights)?;
    }

    let psq_weight_count = PSQ_DIMS * TRANSFORMED_DIMS;
    let mut weights = vec![0i8; psq_weight_count].into_boxed_slice();
    read_i8_slice(data, pos, &mut weights)?;

    let mut psqt_weights = vec![0i32; PSQ_DIMS * PSQT_BUCKETS].into_boxed_slice();
    if format == ModelFormat::Current {
        *pos += read_leb128_i32(data, *pos, &mut psqt_weights)?;
    } else {
        let mut all_psqt = vec![0i32; threat_psqt_count + psqt_weights.len()];
        *pos += read_leb128_i32(data, *pos, &mut all_psqt)?;
        threat_psqt_weights.copy_from_slice(&all_psqt[..threat_psqt_count]);
        psqt_weights.copy_from_slice(&all_psqt[threat_psqt_count..]);
    }

    Ok(FeatureTransformerWeights {
        biases,
        weights,
        psqt_weights,
        threat_weights,
        threat_psqt_weights,
    })
}

fn read_layer_stack(
    data: &[u8],
    pos: &mut usize,
    format: ModelFormat,
) -> Result<LayerStackWeights, NnueError> {
    read_hash(data, pos, ARCHITECTURE_HASH, "layer stack")?;

    // fc_0: biases[32] as i32, weights[32*1024] as i8 (file: output-major)
    let mut fc0_biases = Box::new([0i32; FC0_OUTPUTS]);
    read_i32_slice(data, pos, fc0_biases.as_mut_slice())?;
    let fc0_weight_count = FC0_OUTPUTS * TRANSFORMED_DIMS;
    let mut fc0_weights_raw = vec![0i8; fc0_weight_count];
    read_i8_slice(data, pos, &mut fc0_weights_raw)?;
    let fc0_weights = transpose_weights(&fc0_weights_raw, FC0_OUTPUTS, TRANSFORMED_DIMS);

    // fc_1: biases[32] as i32, weights[32*64] as i8 (file: output-major, padded input)
    let mut fc1_biases = Box::new([0i32; L3_BIG]);
    read_i32_slice(data, pos, fc1_biases.as_mut_slice())?;
    let fc1_weight_count = L3_BIG * FC1_INPUTS_PADDED;
    let mut fc1_weights_raw = vec![0i8; fc1_weight_count];
    read_i8_slice(data, pos, &mut fc1_weights_raw)?;
    let fc1_weights = transpose_weights(&fc1_weights_raw, L3_BIG, FC1_INPUTS_PADDED);

    // fc_2: one output, 128 inputs in current models or 32 in legacy models.
    let mut fc2_biases = Box::new([0i32; 1]);
    read_i32_slice(data, pos, fc2_biases.as_mut_slice())?;
    let fc2_weight_count = format.fc2_inputs();
    let mut fc2_weights = vec![0i8; fc2_weight_count].into_boxed_slice();
    read_i8_slice(data, pos, &mut fc2_weights)?;

    Ok(LayerStackWeights {
        fc0_biases,
        fc0_weights,
        fc1_biases,
        fc1_weights,
        fc2_biases,
        fc2_weights,
    })
}

const fn ensure_remaining(data: &[u8], pos: usize, need: usize) -> Result<(), NnueError> {
    if pos > data.len() || need > data.len() - pos {
        return Err(NnueError::UnexpectedEof { offset: pos });
    }
    Ok(())
}

fn read_hash(
    data: &[u8],
    pos: &mut usize,
    expected: u32,
    section: &'static str,
) -> Result<(), NnueError> {
    let got = read_u32(data, pos)?;
    if got != expected {
        return Err(NnueError::InvalidHash {
            section,
            expected,
            got,
        });
    }
    Ok(())
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, NnueError> {
    ensure_remaining(data, *pos, 4)?;
    let val = u32::from_le_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
    *pos += 4;
    Ok(val)
}

fn read_string(data: &[u8], pos: &mut usize, len: usize) -> Result<String, NnueError> {
    ensure_remaining(data, *pos, len)?;
    let s = String::from_utf8_lossy(&data[*pos..*pos + len]).into_owned();
    *pos += len;
    Ok(s)
}

fn read_i8_slice(data: &[u8], pos: &mut usize, output: &mut [i8]) -> Result<(), NnueError> {
    let count = output.len();
    ensure_remaining(data, *pos, count)?;
    for (i, byte) in data[*pos..*pos + count].iter().enumerate() {
        output[i] = *byte as i8;
    }
    *pos += count;
    Ok(())
}

fn read_i32_slice(data: &[u8], pos: &mut usize, output: &mut [i32]) -> Result<(), NnueError> {
    let byte_count = output.len() * 4;
    ensure_remaining(data, *pos, byte_count)?;
    for (i, chunk) in data[*pos..*pos + byte_count].chunks_exact(4).enumerate() {
        output[i] = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    *pos += byte_count;
    Ok(())
}

/// Decode LEB128-compressed signed 16-bit integers.
///
/// Format: magic string `COMPRESSED_LEB128` (17 bytes), `u32` `byte_count`, then LEB128 data.
pub fn read_leb128_i16(data: &[u8], start: usize, output: &mut [i16]) -> Result<usize, NnueError> {
    let mut pos = start;

    ensure_remaining(data, pos, LEB128_MAGIC.len())?;
    if &data[pos..pos + LEB128_MAGIC.len()] != LEB128_MAGIC.as_slice() {
        return Err(NnueError::InvalidLeb128Magic { offset: pos });
    }
    pos += LEB128_MAGIC.len();

    ensure_remaining(data, pos, 4)?;
    let byte_count =
        u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;

    ensure_remaining(data, pos, byte_count)?;
    let leb_data = &data[pos..pos + byte_count];
    pos += byte_count;

    let mut cursor = std::io::Cursor::new(leb_data);
    let mut decoded = 0;
    while (cursor.position() as usize) < leb_data.len() && decoded < output.len() {
        output[decoded] = decode_signed_leb128_i16(&mut cursor)?;
        decoded += 1;
    }

    if decoded != output.len() {
        return Err(NnueError::Leb128CountMismatch {
            decoded,
            expected: output.len(),
        });
    }
    if cursor.position() as usize != leb_data.len() {
        return Err(NnueError::TrailingData {
            offset: start + LEB128_MAGIC.len() + 4 + cursor.position() as usize,
        });
    }

    Ok(pos - start)
}

/// Decode LEB128-compressed signed 32-bit integers.
pub fn read_leb128_i32(data: &[u8], start: usize, output: &mut [i32]) -> Result<usize, NnueError> {
    let mut pos = start;

    ensure_remaining(data, pos, LEB128_MAGIC.len())?;
    if &data[pos..pos + LEB128_MAGIC.len()] != LEB128_MAGIC.as_slice() {
        return Err(NnueError::InvalidLeb128Magic { offset: pos });
    }
    pos += LEB128_MAGIC.len();

    ensure_remaining(data, pos, 4)?;
    let byte_count =
        u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;

    ensure_remaining(data, pos, byte_count)?;
    let leb_data = &data[pos..pos + byte_count];
    pos += byte_count;

    let mut cursor = std::io::Cursor::new(leb_data);
    let mut decoded = 0;
    while (cursor.position() as usize) < leb_data.len() && decoded < output.len() {
        output[decoded] = decode_signed_leb128_i32(&mut cursor)?;
        decoded += 1;
    }

    if decoded != output.len() {
        return Err(NnueError::Leb128CountMismatch {
            decoded,
            expected: output.len(),
        });
    }
    if cursor.position() as usize != leb_data.len() {
        return Err(NnueError::TrailingData {
            offset: start + LEB128_MAGIC.len() + 4 + cursor.position() as usize,
        });
    }

    Ok(pos - start)
}

fn decode_signed_leb128_i16(reader: &mut impl Read) -> Result<i16, NnueError> {
    i16::try_from(decode_signed_leb128(reader, 16)?)
        .map_err(|_| NnueError::InvalidLeb128 { bits: 16 })
}

fn decode_signed_leb128_i32(reader: &mut impl Read) -> Result<i32, NnueError> {
    i32::try_from(decode_signed_leb128(reader, 32)?)
        .map_err(|_| NnueError::InvalidLeb128 { bits: 32 })
}

fn decode_signed_leb128(reader: &mut impl Read, bits: u32) -> Result<i64, NnueError> {
    let mut result = 0i64;
    let mut shift: u32 = 0;
    let mut byte_buf = [0u8; 1];
    while shift < bits {
        reader.read_exact(&mut byte_buf)?;
        let byte = byte_buf[0];
        result |= i64::from(byte & 0x7f) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            if byte & 0x40 != 0 {
                result |= !0i64 << shift;
            }
            return Ok(result);
        }
    }
    Err(NnueError::InvalidLeb128 { bits })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_rejects_unknown_version_and_mismatched_hashes() {
        assert!(matches!(
            NnueModel::decode(&0u32.to_le_bytes()),
            Err(NnueError::InvalidVersion { got: 0 })
        ));
        for (version, format) in [
            (VERSION, ModelFormat::Current),
            (LEGACY_VERSION, ModelFormat::Legacy),
        ] {
            let mut data = version.to_le_bytes().to_vec();
            data.extend_from_slice(&0u32.to_le_bytes());
            assert!(matches!(
                NnueModel::decode(&data),
                Err(NnueError::InvalidHash {
                    section: "network",
                    ..
                })
            ));
            data[4..8].copy_from_slice(&(format.feature_hash() ^ ARCHITECTURE_HASH).to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes()); // empty description
            data.extend_from_slice(&0u32.to_le_bytes()); // invalid feature hash
            assert!(matches!(
                NnueModel::decode(&data),
                Err(NnueError::InvalidHash {
                    section: "feature transformer",
                    ..
                })
            ));
            assert!(matches!(
                read_layer_stack(&0u32.to_le_bytes(), &mut 0, format),
                Err(NnueError::InvalidHash {
                    section: "layer stack",
                    ..
                })
            ));
        }
    }

    #[test]
    fn test_model_layer_layouts_are_distinct() {
        for format in [ModelFormat::Current, ModelFormat::Legacy] {
            let mut data = ARCHITECTURE_HASH.to_le_bytes().to_vec();
            let parameters = FC0_OUTPUTS * 4
                + FC0_OUTPUTS * TRANSFORMED_DIMS
                + L3_BIG * 4
                + L3_BIG * FC1_INPUTS_PADDED
                + 4
                + format.fc2_inputs();
            data.resize(data.len() + parameters, 0);
            let mut pos = 0;
            let layer = read_layer_stack(&data, &mut pos, format).expect("valid layer");
            assert_eq!(pos, data.len());
            assert_eq!(layer.fc2_weights.len(), format.fc2_inputs());
            assert!(read_layer_stack(&data[..data.len() - 1], &mut 0, format).is_err());
        }
    }

    #[test]
    fn test_leb128_rejects_overlong_and_out_of_range_values() {
        for bytes in [&[0x80u8, 0x80, 0x80][..], &[0xff, 0xff, 0x03][..]] {
            assert!(matches!(
                decode_signed_leb128_i16(&mut std::io::Cursor::new(bytes)),
                Err(NnueError::InvalidLeb128 { bits: 16 })
            ));
        }
        for bytes in [
            &[0x80u8, 0x80, 0x80, 0x80, 0x80][..],
            &[0xff, 0xff, 0xff, 0xff, 0x0f][..],
        ] {
            assert!(matches!(
                decode_signed_leb128_i32(&mut std::io::Cursor::new(bytes)),
                Err(NnueError::InvalidLeb128 { bits: 32 })
            ));
        }
    }

    #[test]
    fn test_leb128_rejects_unconsumed_block_bytes() {
        let mut block = LEB128_MAGIC.to_vec();
        block.extend_from_slice(&2u32.to_le_bytes());
        block.extend_from_slice(&[0, 0]);
        assert!(matches!(
            read_leb128_i16(&block, 0, &mut [0]),
            Err(NnueError::TrailingData { .. })
        ));
        assert!(matches!(
            read_leb128_i32(&block, 0, &mut [0]),
            Err(NnueError::TrailingData { .. })
        ));
        assert!(matches!(
            ensure_remaining(&[], usize::MAX, 1),
            Err(NnueError::UnexpectedEof { .. })
        ));
    }

    #[test]
    fn test_leb128_i16_roundtrip() {
        let values: Vec<i16> = vec![0, 1, -1, 127, -128, 255, -256, i16::MAX, i16::MIN];
        for &val in &values {
            let mut encoded = Vec::new();
            encode_signed_leb128_i16(val, &mut encoded);
            let mut cursor = std::io::Cursor::new(encoded.as_slice());
            let decoded = decode_signed_leb128_i16(&mut cursor).expect("decode failed");
            assert_eq!(val, decoded, "roundtrip failed for {val}");
        }
    }

    #[test]
    fn test_leb128_i32_roundtrip() {
        let values: Vec<i32> = vec![0, 1, -1, 127, -128, 32767, -32768, i32::MAX, i32::MIN];
        for &val in &values {
            let mut encoded = Vec::new();
            encode_signed_leb128_i32(val, &mut encoded);
            let mut cursor = std::io::Cursor::new(encoded.as_slice());
            let decoded = decode_signed_leb128_i32(&mut cursor).expect("decode failed");
            assert_eq!(val, decoded, "roundtrip failed for {val}");
        }
    }

    #[test]
    fn test_read_leb128_i16_block() {
        let values: [i16; 4] = [100, -50, 0, 300];
        let mut leb_bytes = Vec::new();
        for &v in &values {
            encode_signed_leb128_i16(v, &mut leb_bytes);
        }

        let mut block = Vec::new();
        block.extend_from_slice(LEB128_MAGIC);
        block.extend_from_slice(&(leb_bytes.len() as u32).to_le_bytes());
        block.extend_from_slice(&leb_bytes);

        let mut output = [0i16; 4];
        let consumed = read_leb128_i16(&block, 0, &mut output).expect("read failed");
        assert_eq!(consumed, block.len());
        assert_eq!(output, values);
    }

    #[test]
    fn test_read_leb128_i32_block() {
        let values: [i32; 3] = [100_000, -50_000, 0];
        let mut leb_bytes = Vec::new();
        for &v in &values {
            encode_signed_leb128_i32(v, &mut leb_bytes);
        }

        let mut block = Vec::new();
        block.extend_from_slice(LEB128_MAGIC);
        block.extend_from_slice(&(leb_bytes.len() as u32).to_le_bytes());
        block.extend_from_slice(&leb_bytes);

        let mut output = [0i32; 3];
        let consumed = read_leb128_i32(&block, 0, &mut output).expect("read failed");
        assert_eq!(consumed, block.len());
        assert_eq!(output, values);
    }

    fn encode_signed_leb128_i16(mut val: i16, out: &mut Vec<u8>) {
        loop {
            let mut byte = (val & 0x7f) as u8;
            val >>= 7;
            let more = !((val == 0 && byte & 0x40 == 0) || (val == -1 && byte & 0x40 != 0));
            if more {
                byte |= 0x80;
            }
            out.push(byte);
            if !more {
                break;
            }
        }
    }

    fn encode_signed_leb128_i32(mut val: i32, out: &mut Vec<u8>) {
        loop {
            let mut byte = (val & 0x7f) as u8;
            val >>= 7;
            let more = !((val == 0 && byte & 0x40 == 0) || (val == -1 && byte & 0x40 != 0));
            if more {
                byte |= 0x80;
            }
            out.push(byte);
            if !more {
                break;
            }
        }
    }
}
