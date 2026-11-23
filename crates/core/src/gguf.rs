use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufMeta {
    pub architecture: Option<String>,
    pub context_length: Option<u32>,
}

/// Read just enough GGUF metadata to name a family and a context window.
/// A broken or partial file returns `None` so the model can still be listed.
pub fn read_gguf_meta(path: &Path) -> Option<GgufMeta> {
    let mut file = File::open(path).ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;
    parse_gguf_meta(&data)
}

pub fn parse_gguf_meta(data: &[u8]) -> Option<GgufMeta> {
    if data.len() < 24 {
        return None;
    }
    let magic = u32::from_le_bytes(data[0..4].try_into().ok()?);
    if magic != 0x4655_4747 {
        return None;
    }
    let version = u32::from_le_bytes(data[4..8].try_into().ok()?);
    if version < 2 {
        return None;
    }
    let kv_count = u64::from_le_bytes(data[16..24].try_into().ok()?);
    let mut cursor = 24usize;
    let mut architecture = None;
    let mut context_length = None;
    for _ in 0..kv_count {
        let key = read_string(data, &mut cursor)?;
        let value_type = read_u32(data, &mut cursor)?;
        if key == "general.architecture" && value_type == 8 {
            architecture = Some(read_string(data, &mut cursor)?);
        } else if key.ends_with(".context_length") && (value_type == 4 || value_type == 10) {
            context_length = Some(read_uint(data, &mut cursor, value_type)?);
        } else {
            skip_value(data, &mut cursor, value_type)?;
        }
        if architecture.is_some() && context_length.is_some() {
            break;
        }
    }
    Some(GgufMeta {
        architecture,
        context_length,
    })
}

fn read_u32(data: &[u8], cursor: &mut usize) -> Option<u32> {
    let end = cursor.checked_add(4)?;
    let bytes: [u8; 4] = data.get(*cursor..end)?.try_into().ok()?;
    *cursor = end;
    Some(u32::from_le_bytes(bytes))
}

fn read_u64(data: &[u8], cursor: &mut usize) -> Option<u64> {
    let end = cursor.checked_add(8)?;
    let bytes: [u8; 8] = data.get(*cursor..end)?.try_into().ok()?;
    *cursor = end;
    Some(u64::from_le_bytes(bytes))
}

fn read_string(data: &[u8], cursor: &mut usize) -> Option<String> {
    let len = read_u64(data, cursor)? as usize;
    let end = cursor.checked_add(len)?;
    let bytes = data.get(*cursor..end)?;
    *cursor = end;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn read_uint(data: &[u8], cursor: &mut usize, value_type: u32) -> Option<u32> {
    match value_type {
        4 => read_u32(data, cursor),
        10 => {
            let value = read_u64(data, cursor)?;
            Some(u32::try_from(value).unwrap_or(u32::MAX))
        }
        _ => None,
    }
}

fn skip_value(data: &[u8], cursor: &mut usize, value_type: u32) -> Option<()> {
    match value_type {
        0 | 1 | 7 => advance(data, cursor, 1),
        2 | 3 => advance(data, cursor, 2),
        4 | 5 | 6 => advance(data, cursor, 4),
        10 | 11 | 12 => advance(data, cursor, 8),
        8 => {
            let len = read_u64(data, cursor)? as usize;
            advance(data, cursor, len)
        }
        9 => {
            let element = read_u32(data, cursor)?;
            let len = read_u64(data, cursor)?;
            for _ in 0..len {
                skip_value(data, cursor, element)?;
            }
            Some(())
        }
        _ => None,
    }
}

fn advance(data: &[u8], cursor: &mut usize, len: usize) -> Option<()> {
    let end = cursor.checked_add(len)?;
    if end > data.len() {
        return None;
    }
    *cursor = end;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_str(buf: &mut Vec<u8>, value: &str) {
        buf.extend_from_slice(&(value.len() as u64).to_le_bytes());
        buf.extend_from_slice(value.as_bytes());
    }

    #[test]
    fn reads_architecture_and_context() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0x4655_4747u32.to_le_bytes());
        buf.extend_from_slice(&3u32.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());
        buf.extend_from_slice(&2u64.to_le_bytes());
        push_str(&mut buf, "general.architecture");
        buf.extend_from_slice(&8u32.to_le_bytes());
        push_str(&mut buf, "qwen3");
        push_str(&mut buf, "qwen3.context_length");
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(&32_768u32.to_le_bytes());

        let meta = parse_gguf_meta(&buf).unwrap();
        assert_eq!(meta.architecture.as_deref(), Some("qwen3"));
        assert_eq!(meta.context_length, Some(32_768));
    }

    #[test]
    fn rejects_a_non_gguf_file() {
        assert!(parse_gguf_meta(b"not a model").is_none());
    }
}
