use std::io::Read;
use std::path::Path;

use thiserror::Error;

pub const VERSION: u32 = 0x7AF3_2F20;
pub const TRANSFORMED_DIMS: usize = 1024;
pub const PSQ_DIMS: usize = 16_536;
pub const THREAT_DIMS: usize = 45_547;
pub const PSQT_BUCKETS: usize = 16;
pub const LAYER_STACKS: usize = 16;
pub const L2_BIG: usize = 31;
pub const L3_BIG: usize = 32;
pub const WEIGHT_SCALE_BITS: u32 = 6;
pub const OUTPUT_SCALE: i32 = 16;
#[allow(dead_code)]
pub const PS_NB: usize = 689;
#[allow(dead_code)]
pub const ATTACK_BUCKET_NB: usize = 4;
#[allow(dead_code)]
pub const KING_BUCKET_NB: usize = 6;

const FC0_OUTPUTS: usize = L2_BIG + 1; // 32
const FC1_INPUTS_PADDED: usize = 64;
const FC2_INPUTS: usize = 32;

const LEB128_MAGIC: &[u8; 17] = b"COMPRESSED_LEB128";

#[derive(Debug, Error)]
pub enum NnueError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid NNUE version: expected 0x{:08X}, got 0x{got:08X}", VERSION)]
    InvalidVersion { got: u32 },
    #[error("zstd decompression failed: {0}")]
    Zstd(std::io::Error),
    #[error("unexpected end of data at offset {offset}")]
    UnexpectedEof { offset: usize },
    #[error("invalid LEB128 magic at offset {offset}")]
    InvalidLeb128Magic { offset: usize },
    #[error("LEB128 decode produced {decoded} values, expected {expected}")]
    Leb128CountMismatch { decoded: usize, expected: usize },
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
    pub description: String,
    pub ft: FeatureTransformerWeights,
    pub layer_stacks: Vec<LayerStackWeights>,
}

impl NnueModel {
    pub fn load(path: &Path) -> Result<Self, NnueError> {
        let compressed = std::fs::read(path)?;
        let decompressed =
            zstd::stream::decode_all(std::io::Cursor::new(&compressed)).map_err(NnueError::Zstd)?;

        let data = &decompressed;
        let mut pos = 0;

        let version = read_u32(data, &mut pos)?;
        if version != VERSION {
            return Err(NnueError::InvalidVersion { got: version });
        }
        let _hash = read_u32(data, &mut pos)?;
        let desc_size = read_u32(data, &mut pos)? as usize;
        let description = read_string(data, &mut pos, desc_size)?;

        let ft = read_feature_transformer(data, &mut pos)?;
        let mut layer_stacks = Vec::with_capacity(LAYER_STACKS);
        for _ in 0..LAYER_STACKS {
            layer_stacks.push(read_layer_stack(data, &mut pos)?);
        }

        Ok(Self {
            description,
            ft,
            layer_stacks,
        })
    }
}

fn read_feature_transformer(
    data: &[u8],
    pos: &mut usize,
) -> Result<FeatureTransformerWeights, NnueError> {
    let _ft_hash = read_u32(data, pos)?;

    let mut biases = Box::new([0i16; TRANSFORMED_DIMS]);
    let consumed = read_leb128_i16(data, *pos, biases.as_mut_slice())?;
    *pos += consumed;

    let threat_weight_count = THREAT_DIMS * TRANSFORMED_DIMS;
    let mut threat_weights = vec![0i8; threat_weight_count].into_boxed_slice();
    read_i8_slice(data, pos, &mut threat_weights)?;

    let psq_weight_count = PSQ_DIMS * TRANSFORMED_DIMS;
    let mut weights = vec![0i8; psq_weight_count].into_boxed_slice();
    read_i8_slice(data, pos, &mut weights)?;

    let total_psqt = (THREAT_DIMS + PSQ_DIMS) * PSQT_BUCKETS;
    let mut all_psqt = vec![0i32; total_psqt];
    let consumed = read_leb128_i32(data, *pos, &mut all_psqt)?;
    *pos += consumed;

    let threat_psqt_count = THREAT_DIMS * PSQT_BUCKETS;
    let threat_psqt_weights = all_psqt[..threat_psqt_count].to_vec().into_boxed_slice();
    let psqt_weights = all_psqt[threat_psqt_count..].to_vec().into_boxed_slice();

    Ok(FeatureTransformerWeights {
        biases,
        weights,
        psqt_weights,
        threat_weights,
        threat_psqt_weights,
    })
}

fn read_layer_stack(data: &[u8], pos: &mut usize) -> Result<LayerStackWeights, NnueError> {
    let _arch_hash = read_u32(data, pos)?;

    // fc_0: biases[32] as i32, weights[32*1024] as i8
    let mut fc0_biases = Box::new([0i32; FC0_OUTPUTS]);
    read_i32_slice(data, pos, fc0_biases.as_mut_slice())?;
    let fc0_weight_count = FC0_OUTPUTS * TRANSFORMED_DIMS;
    let mut fc0_weights = vec![0i8; fc0_weight_count].into_boxed_slice();
    read_i8_slice(data, pos, &mut fc0_weights)?;

    // fc_1: biases[32] as i32, weights[32*64] as i8 (input padded from 62 to 64)
    let mut fc1_biases = Box::new([0i32; L3_BIG]);
    read_i32_slice(data, pos, fc1_biases.as_mut_slice())?;
    let fc1_weight_count = L3_BIG * FC1_INPUTS_PADDED;
    let mut fc1_weights = vec![0i8; fc1_weight_count].into_boxed_slice();
    read_i8_slice(data, pos, &mut fc1_weights)?;

    // fc_2: biases[1] as i32, weights[1*32] as i8
    let mut fc2_biases = Box::new([0i32; 1]);
    read_i32_slice(data, pos, fc2_biases.as_mut_slice())?;
    let fc2_weight_count = FC2_INPUTS;
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
    if pos + need > data.len() {
        return Err(NnueError::UnexpectedEof { offset: pos });
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

    Ok(pos - start)
}

fn decode_signed_leb128_i16(reader: &mut impl Read) -> Result<i16, NnueError> {
    let mut result: i16 = 0;
    let mut shift: u32 = 0;
    let mut byte_buf = [0u8; 1];
    loop {
        reader.read_exact(&mut byte_buf)?;
        let byte = byte_buf[0];
        result |= (i16::from(byte & 0x7f)) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            if shift < 16 && (byte & 0x40) != 0 {
                result |= !0i16 << shift;
            }
            break;
        }
    }
    Ok(result)
}

fn decode_signed_leb128_i32(reader: &mut impl Read) -> Result<i32, NnueError> {
    let mut result: i32 = 0;
    let mut shift: u32 = 0;
    let mut byte_buf = [0u8; 1];
    loop {
        reader.read_exact(&mut byte_buf)?;
        let byte = byte_buf[0];
        result |= (i32::from(byte & 0x7f)) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            if shift < 32 && (byte & 0x40) != 0 {
                result |= !0i32 << shift;
            }
            break;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

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
