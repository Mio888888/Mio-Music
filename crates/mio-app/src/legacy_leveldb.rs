//! A small read-only LevelDB reader for the original's `localStorage` on Windows.
//!
//! WebView2 (Chromium) keeps `localStorage` in a LevelDB folder, `Local Storage/leveldb`. Only
//! what the import needs is implemented: the write-ahead `.log` files and the `.ldb`/`.sst`
//! tables (raw or Snappy blocks), merged by sequence number so the newest value of a key wins and
//! deleted keys disappear. Nothing is ever written; callers hand in a copy of the folder.
use std::{collections::HashMap, fs, path::Path};

const BLOCK_SIZE: usize = 32768;
const TABLE_MAGIC: u64 = 0xdb47_7524_8b80_fb57;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

fn varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        let byte = *data.get(*pos)?;
        *pos += 1;
        result |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

fn take<'a>(data: &'a [u8], pos: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = pos.checked_add(len)?;
    let slice = data.get(*pos..end)?;
    *pos = end;
    Some(slice)
}

/// Raw (block) Snappy decompression.
fn snappy(input: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0;
    let expected = usize::try_from(varint(input, &mut pos)?).ok()?;
    if expected > 64 * 1024 * 1024 {
        return None;
    }
    let mut out = Vec::with_capacity(expected);
    while pos < input.len() {
        let tag = input[pos];
        pos += 1;
        match tag & 3 {
            0 => {
                let mut len = usize::from(tag >> 2);
                if len >= 60 {
                    let extra = len - 59;
                    let bytes = take(input, &mut pos, extra)?;
                    len = bytes
                        .iter()
                        .enumerate()
                        .fold(0usize, |n, (i, b)| n | (usize::from(*b) << (8 * i)));
                }
                out.extend_from_slice(take(input, &mut pos, len.checked_add(1)?)?);
            }
            kind => {
                let (len, offset) = match kind {
                    1 => {
                        let byte = usize::from(*input.get(pos)?);
                        pos += 1;
                        (
                            4 + usize::from((tag >> 2) & 7),
                            (usize::from(tag >> 5) << 8) | byte,
                        )
                    }
                    2 => {
                        let bytes = take(input, &mut pos, 2)?;
                        (
                            1 + usize::from(tag >> 2),
                            usize::from(u16::from_le_bytes([bytes[0], bytes[1]])),
                        )
                    }
                    _ => {
                        let bytes = take(input, &mut pos, 4)?;
                        (
                            1 + usize::from(tag >> 2),
                            usize::try_from(u32::from_le_bytes([
                                bytes[0], bytes[1], bytes[2], bytes[3],
                            ]))
                            .ok()?,
                        )
                    }
                };
                if offset == 0 || offset > out.len() || out.len() + len > expected {
                    return None;
                }
                // Copies may overlap their own output, so go byte by byte.
                for _ in 0..len {
                    let byte = out[out.len() - offset];
                    out.push(byte);
                }
            }
        }
    }
    (out.len() == expected).then_some(out)
}

/// One key of the merged database: its newest sequence number and value (`None` = deleted).
type Merged = HashMap<Vec<u8>, (u64, Option<Vec<u8>>)>;

fn offer(merged: &mut Merged, key: &[u8], sequence: u64, value: Option<Vec<u8>>) {
    match merged.get(key) {
        Some((existing, _)) if *existing >= sequence => {}
        _ => {
            merged.insert(key.to_vec(), (sequence, value));
        }
    }
}

/// A write batch of the log: `sequence(8) count(4)` then puts (`1 key value`) and deletes
/// (`0 key`).
fn read_batch(batch: &[u8], merged: &mut Merged) -> Option<()> {
    let base = u64::from_le_bytes(batch.get(0..8)?.try_into().ok()?);
    let count = u32::from_le_bytes(batch.get(8..12)?.try_into().ok()?);
    let mut pos = 12;
    for index in 0..u64::from(count) {
        let kind = *batch.get(pos)?;
        pos += 1;
        let key_len = usize::try_from(varint(batch, &mut pos)?).ok()?;
        let key = take(batch, &mut pos, key_len)?;
        let value = if kind == 1 {
            let value_len = usize::try_from(varint(batch, &mut pos)?).ok()?;
            Some(take(batch, &mut pos, value_len)?.to_vec())
        } else {
            None
        };
        offer(merged, key, base.wrapping_add(index), value);
    }
    Some(())
}

fn read_log(data: &[u8], merged: &mut Merged) {
    let mut record = Vec::new();
    for block in data.chunks(BLOCK_SIZE) {
        let mut pos = 0;
        while pos + 7 <= block.len() {
            let length = usize::from(u16::from_le_bytes([block[pos + 4], block[pos + 5]]));
            let kind = block[pos + 6];
            pos += 7;
            let Some(payload) = block.get(pos..pos + length) else {
                break;
            };
            pos += length;
            match kind {
                1 => {
                    let _ = read_batch(payload, merged);
                }
                2 => {
                    record.clear();
                    record.extend_from_slice(payload);
                }
                3 => record.extend_from_slice(payload),
                4 => {
                    record.extend_from_slice(payload);
                    let _ = read_batch(&record, merged);
                    record.clear();
                }
                // Padding at the end of a block.
                _ => {}
            }
        }
    }
}

/// The contents of the block at `offset`/`size` (after its 5-byte trailer is checked for the
/// compression type only; checksums are not verified, the data is only read back).
fn block(data: &[u8], offset: usize, size: usize) -> Option<Vec<u8>> {
    let raw = data.get(offset..offset.checked_add(size)?)?;
    match *data.get(offset + size)? {
        0 => Some(raw.to_vec()),
        1 => snappy(raw),
        _ => None,
    }
}

/// Entries `(key, value)` of a table block, undoing the key prefix compression.
fn block_entries(contents: &[u8]) -> Option<Vec<(Vec<u8>, Vec<u8>)>> {
    let restarts = usize::try_from(u32::from_le_bytes(
        contents
            .get(contents.len().checked_sub(4)?..)?
            .try_into()
            .ok()?,
    ))
    .ok()?;
    let end = contents.len().checked_sub(4 + 4 * restarts)?;
    let mut pos = 0;
    let mut key: Vec<u8> = Vec::new();
    let mut entries = Vec::new();
    while pos < end {
        let shared = usize::try_from(varint(contents, &mut pos)?).ok()?;
        let unshared = usize::try_from(varint(contents, &mut pos)?).ok()?;
        let value_len = usize::try_from(varint(contents, &mut pos)?).ok()?;
        if shared > key.len() {
            return None;
        }
        key.truncate(shared);
        key.extend_from_slice(take(contents, &mut pos, unshared)?);
        let value = take(contents, &mut pos, value_len)?.to_vec();
        entries.push((key.clone(), value));
    }
    Some(entries)
}

fn read_table(data: &[u8], merged: &mut Merged) {
    let Some(footer) = data
        .len()
        .checked_sub(48)
        .and_then(|start| data.get(start..))
    else {
        return;
    };
    if u64::from_le_bytes(footer[40..48].try_into().unwrap_or_default()) != TABLE_MAGIC {
        return;
    }
    let mut pos = 0;
    // The metaindex handle comes first; the index handle is the second one.
    let handle = |pos: &mut usize| -> Option<(usize, usize)> {
        let offset = usize::try_from(varint(footer, pos)?).ok()?;
        let size = usize::try_from(varint(footer, pos)?).ok()?;
        Some((offset, size))
    };
    if handle(&mut pos).is_none() {
        return;
    }
    let Some((index_offset, index_size)) = handle(&mut pos) else {
        return;
    };
    let Some(index) = block(data, index_offset, index_size).and_then(|b| block_entries(&b)) else {
        return;
    };
    for (_, value) in index {
        let mut at = 0;
        let (Some(offset), Some(size)) = (varint(&value, &mut at), varint(&value, &mut at)) else {
            continue;
        };
        let (Ok(offset), Ok(size)) = (usize::try_from(offset), usize::try_from(size)) else {
            continue;
        };
        let Some(entries) = block(data, offset, size).and_then(|b| block_entries(&b)) else {
            continue;
        };
        for (internal, value) in entries {
            // The last 8 bytes are `sequence << 8 | kind` (kind 0 = deletion).
            let Some(split) = internal.len().checked_sub(8) else {
                continue;
            };
            let tag = u64::from_le_bytes(internal[split..].try_into().unwrap_or_default());
            let value = (tag & 0xff == 1).then_some(value);
            offer(merged, &internal[..split], tag >> 8, value);
        }
    }
}

fn read_limited(path: &Path) -> Option<Vec<u8>> {
    let metadata = fs::metadata(path).ok()?;
    (metadata.is_file() && metadata.len() <= MAX_FILE_BYTES)
        .then(|| fs::read(path).ok())
        .flatten()
}

/// Every live key/value pair of the LevelDB folder `dir`.
pub fn read_dir(dir: &Path) -> HashMap<Vec<u8>, Vec<u8>> {
    let mut merged = Merged::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return HashMap::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        match extension {
            "ldb" | "sst" => {
                if let Some(data) = read_limited(&path) {
                    read_table(&data, &mut merged);
                }
            }
            "log" => {
                if let Some(data) = read_limited(&path) {
                    read_log(&data, &mut merged);
                }
            }
            _ => {}
        }
    }
    merged
        .into_iter()
        .filter_map(|(key, (_, value))| value.map(|value| (key, value)))
        .collect()
}

/// Chromium's `localStorage` strings: a leading `0` marks UTF-16LE text, `1` Latin-1.
fn decode_string(bytes: &[u8]) -> Option<String> {
    let (&kind, rest) = bytes.split_first()?;
    match kind {
        1 => Some(rest.iter().map(|&b| char::from(b)).collect()),
        0 if rest.len() % 2 == 0 => {
            let units: Vec<u16> = rest
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16(&units).ok()
        }
        _ => None,
    }
}

/// `localStorage` items per origin (`http://tauri.localhost`, …) of a Chromium LevelDB folder.
pub fn read_local_storage(dir: &Path) -> HashMap<String, HashMap<String, String>> {
    let mut origins: HashMap<String, HashMap<String, String>> = HashMap::new();
    for (key, value) in read_dir(dir) {
        // `_<origin>\0<encoded key>`; the other keys are metadata.
        let Some(rest) = key.strip_prefix(b"_") else {
            continue;
        };
        let Some(split) = rest.iter().position(|&b| b == 0) else {
            continue;
        };
        let Ok(origin) = std::str::from_utf8(&rest[..split]) else {
            continue;
        };
        let (Some(name), Some(text)) = (decode_string(&rest[split + 1..]), decode_string(&value))
        else {
            continue;
        };
        origins
            .entry(origin.to_owned())
            .or_default()
            .insert(name, text);
    }
    origins
}
