//! C2PA manifest-store extraction helpers.
//!
//! This module extracts embedded C2PA/JUMBF bytes and exposes a JSON summary.
//! It intentionally does not perform certificate, signature, or assertion
//! validation; callers can pass the raw manifest store to a validator.

use super::DataMap;
use std::collections::HashMap;

pub const C2PA_JSON_KEY: &str = "C2PA";
pub const C2PA_RAW_KEY: &str = "C2PA Raw";
pub const PNG_CHUNK_TYPE: [u8; 4] = *b"caBX";
pub const JPEG_APP11_MARKER: u8 = 0xeb;

const C2PA_MANIFEST_STORE_UUID: [u8; 16] = [
    0x63, 0x32, 0x70, 0x61, 0x00, 0x11, 0x00, 0x10, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];

/// Parsed top-level C2PA manifest-store view.
#[derive(Debug, Clone, PartialEq)]
pub struct C2paManifestStore {
    pub source: String,
    pub payload: Vec<u8>,
    pub boxes: Vec<JumbfBox>,
}

/// JUMBF/BMFF box summary used by [`C2paManifestStore::to_json`].
#[derive(Debug, Clone, PartialEq)]
pub struct JumbfBox {
    pub box_type: String,
    pub offset: usize,
    pub size: usize,
    pub header_size: usize,
    pub uuid: Option<String>,
    pub label: Option<String>,
    pub text: Option<String>,
    pub cbor: Option<String>,
    pub children: Vec<JumbfBox>,
}

impl C2paManifestStore {
    pub fn new(source: impl Into<String>, payload: &[u8]) -> Self {
        Self {
            source: source.into(),
            payload: payload.to_vec(),
            boxes: parse_jumbf_boxes(payload),
        }
    }

    /// Serializes a compact JSON view including the raw manifest store as base64.
    pub fn to_json(&self) -> String {
        let mut json = String::new();
        json.push('{');
        push_json_pair(&mut json, "format", "c2pa");
        json.push(',');
        push_json_pair(&mut json, "source", &self.source);
        json.push(',');
        push_json_usize(&mut json, "manifest_store_length", self.payload.len());
        json.push(',');
        push_json_pair(
            &mut json,
            "manifest_store_base64",
            &base64_encode(&self.payload),
        );
        json.push(',');
        json.push_str("\"jumbf_boxes\":[");
        for (index, jumbf_box) in self.boxes.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            push_box_json(&mut json, jumbf_box);
        }
        json.push(']');
        json.push('}');
        json
    }
}

/// Inserts C2PA JSON and raw payload entries into a metadata map.
pub fn insert_metadata(map: &mut HashMap<String, DataMap>, source: &str, payload: &[u8]) {
    let manifest = C2paManifestStore::new(source, payload);
    map.insert(C2PA_JSON_KEY.to_string(), DataMap::JSON(manifest.to_json()));
    map.insert(C2PA_RAW_KEY.to_string(), DataMap::Raw(payload.to_vec()));
}

/// Builds a JSON string directly from an embedded C2PA manifest store.
pub fn manifest_store_json(source: &str, payload: &[u8]) -> String {
    C2paManifestStore::new(source, payload).to_json()
}

/// Builds a compact human-readable C2PA summary from [`C2paManifestStore::to_json`].
///
/// This intentionally ignores byte payloads, base64 data, hashes, signatures,
/// and validation material. It keeps the fields that are useful for quick
/// inspection in metadata dumps.
pub fn c2pa_to_text(json: &str) -> String {
    let mut claim_names = Vec::new();
    let mut search_from = 0usize;
    while let Some(key_pos) = find_json_key(json, "claim_generator_info", search_from) {
        search_from = key_pos + 1;
        let Some(object_start) = json[key_pos..].find('{').map(|pos| key_pos + pos) else {
            continue;
        };
        let Some(object_end) = find_matching_json(json, object_start, '{', '}') else {
            continue;
        };
        if let Some(name) = read_string_key(&json[object_start..=object_end], "name", 0) {
            push_unique(&mut claim_names, name);
        }
    }

    let mut actions = Vec::new();
    search_from = 0;
    while let Some(key_pos) = find_json_key(json, "actions", search_from) {
        search_from = key_pos + 1;
        let Some(array_start) = json[key_pos..].find('[').map(|pos| key_pos + pos) else {
            continue;
        };
        let Some(array_end) = find_matching_json(json, array_start, '[', ']') else {
            continue;
        };
        collect_action_summaries(&json[array_start + 1..array_end], &mut actions);
    }

    let mut out = String::new();
    if !claim_names.is_empty() {
        out.push_str("claim_generator_info.name:\n");
        for name in claim_names {
            out.push_str("- ");
            out.push_str(&name);
            out.push('\n');
        }
    }
    if !actions.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("actions:\n");
        for action in actions {
            out.push_str(&action);
        }
    }

    if out.is_empty() {
        "C2PA: no displayable claim/action metadata".to_string()
    } else {
        out.trim_end().to_string()
    }
}

/// Extracts the C2PA JUMBF bytes from one JPEG APP11 payload when recognizable.
pub fn jpeg_app11_payload_to_manifest_store(payload: &[u8]) -> Option<Vec<u8>> {
    if let Some(start) = find_jumbf_start(payload) {
        return Some(payload[start..].to_vec());
    }

    if starts_with_c2pa_identifier(payload) {
        let mut start = 4;
        if payload.get(start).copied() == Some(0) {
            start += 1;
        }
        if let Some(nested_start) = find_jumbf_start(&payload[start..]) {
            return Some(payload[start + nested_start..].to_vec());
        }
        return Some(payload[start..].to_vec());
    }

    if contains_c2pa_uuid(payload) {
        return Some(payload.to_vec());
    }

    None
}

/// Concatenates recognized C2PA APP11 fragments in encounter order.
pub fn jpeg_app11_payloads_to_manifest_store<'a, I>(payloads: I) -> Option<Vec<u8>>
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let mut payload = Vec::new();
    for part in payloads {
        if let Some(mut fragment) = jpeg_app11_payload_to_manifest_store(part) {
            payload.append(&mut fragment);
        }
    }
    (!payload.is_empty()).then_some(payload)
}

/// Returns true if the payload looks like a C2PA manifest store.
pub fn is_c2pa_manifest_store(payload: &[u8]) -> bool {
    contains_c2pa_uuid(payload) || payload.windows(4).any(|window| window == b"c2pa")
}

pub fn parse_jumbf_boxes(data: &[u8]) -> Vec<JumbfBox> {
    parse_jumbf_boxes_at(data, 0)
}

fn parse_jumbf_boxes_at(data: &[u8], base_offset: usize) -> Vec<JumbfBox> {
    let mut boxes = Vec::new();
    let mut offset = 0usize;
    while offset + 8 <= data.len() {
        let Some((box_size, header_size)) = read_box_size(&data[offset..]) else {
            break;
        };
        if box_size < header_size {
            break;
        }
        let end = if box_size == 0 {
            data.len()
        } else {
            let Some(end) = offset.checked_add(box_size) else {
                break;
            };
            if end > data.len() {
                break;
            }
            end
        };

        let box_type = String::from_utf8_lossy(&data[offset + 4..offset + 8]).to_string();
        let content = &data[offset + header_size..end];
        let mut jumbf_box = JumbfBox {
            box_type: box_type.clone(),
            offset: base_offset + offset,
            size: end - offset,
            header_size,
            uuid: None,
            label: None,
            text: None,
            cbor: None,
            children: Vec::new(),
        };

        if box_type == "jumd" {
            parse_description_box(content, &mut jumbf_box);
        } else if box_type == "jumb" {
            jumbf_box.children = parse_jumbf_boxes_at(content, base_offset + offset + header_size);
        } else if box_type == "json" {
            if let Ok(text) = std::str::from_utf8(content) {
                jumbf_box.text = Some(text.trim_end_matches('\0').to_string());
            }
        } else if box_type == "cbor" {
            jumbf_box.cbor = cbor_to_json(content);
        } else if box_type == "uuid" && content.len() >= 16 {
            jumbf_box.uuid = Some(uuid_to_string(&content[..16]));
        }

        boxes.push(jumbf_box);
        offset = end;
    }
    boxes
}

fn parse_description_box(content: &[u8], jumbf_box: &mut JumbfBox) {
    if content.len() < 17 {
        return;
    }
    jumbf_box.uuid = Some(uuid_to_string(&content[..16]));
    let toggles = content[16];
    let label_present = toggles & 0x02 != 0;
    if label_present {
        let label_start = 17;
        if let Some(label_end) = content[label_start..].iter().position(|byte| *byte == 0) {
            let label = &content[label_start..label_start + label_end];
            jumbf_box.label = Some(String::from_utf8_lossy(label).to_string());
        }
    }
}

fn read_box_size(data: &[u8]) -> Option<(usize, usize)> {
    if data.len() < 8 {
        return None;
    }
    let lbox = u32::from_be_bytes(data[..4].try_into().ok()?);
    if lbox == 1 {
        if data.len() < 16 {
            return None;
        }
        let xlbox = u64::from_be_bytes(data[8..16].try_into().ok()?);
        usize::try_from(xlbox).ok().map(|size| (size, 16))
    } else {
        Some((lbox as usize, 8))
    }
}

fn find_jumbf_start(data: &[u8]) -> Option<usize> {
    for offset in 0..data.len().saturating_sub(7) {
        if &data[offset + 4..offset + 8] != b"jumb" {
            continue;
        }
        let Some((size, header_size)) = read_box_size(&data[offset..]) else {
            continue;
        };
        if header_size <= size && (size == 0 || offset + size <= data.len()) {
            return Some(offset);
        }
    }
    None
}

fn contains_c2pa_uuid(data: &[u8]) -> bool {
    data.windows(C2PA_MANIFEST_STORE_UUID.len())
        .any(|window| window == C2PA_MANIFEST_STORE_UUID)
}

fn starts_with_c2pa_identifier(data: &[u8]) -> bool {
    data.starts_with(b"C2PA")
}

fn collect_action_summaries(json: &str, actions: &mut Vec<String>) {
    let mut offset = 0usize;
    while let Some(relative_start) = json[offset..].find('{') {
        let object_start = offset + relative_start;
        let Some(object_end) = find_matching_json(json, object_start, '{', '}') else {
            break;
        };
        let object = &json[object_start..=object_end];
        if let Some(action) = read_string_key(object, "action", 0) {
            let mut summary = String::new();
            summary.push_str("- ");
            summary.push_str(&action);
            summary.push('\n');
            if let Some(when) = read_object_key(object, "when", 0) {
                if let Some(value) = read_string_key(when, "value", 0) {
                    push_summary_field(&mut summary, "when", &value);
                }
            }
            if let Some(software_agent) = read_object_key(object, "softwareAgent", 0) {
                if let Some(name) = read_string_key(software_agent, "name", 0) {
                    push_summary_field(&mut summary, "softwareAgent.name", &name);
                }
                if let Some(version) = read_string_key(software_agent, "version", 0) {
                    push_summary_field(&mut summary, "softwareAgent.version", &version);
                }
            }
            for key in ["description", "digitalSourceType"] {
                if let Some(value) = read_string_key(object, key, 0) {
                    push_summary_field(&mut summary, key, &value);
                }
            }
            push_unique(actions, summary);
        }
        offset = object_end + 1;
    }
}

fn push_summary_field(summary: &mut String, key: &str, value: &str) {
    summary.push_str("  ");
    summary.push_str(key);
    summary.push_str(": ");
    summary.push_str(value);
    summary.push('\n');
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn find_json_key(json: &str, key: &str, from: usize) -> Option<usize> {
    let needle = format!("\"{}\"", key);
    json.get(from..)?.find(&needle).map(|pos| from + pos)
}

fn read_string_key(json: &str, key: &str, from: usize) -> Option<String> {
    let key_pos = find_json_key(json, key, from)?;
    let colon = json.get(key_pos..)?.find(':').map(|pos| key_pos + pos)?;
    let mut value_start = colon + 1;
    while let Some(ch) = json.get(value_start..)?.chars().next() {
        if !ch.is_whitespace() {
            break;
        }
        value_start += ch.len_utf8();
    }
    read_json_string_at(json, value_start).map(|(value, _)| value)
}

fn read_object_key<'a>(json: &'a str, key: &str, from: usize) -> Option<&'a str> {
    let key_pos = find_json_key(json, key, from)?;
    let colon = json.get(key_pos..)?.find(':').map(|pos| key_pos + pos)?;
    let mut value_start = colon + 1;
    while let Some(ch) = json.get(value_start..)?.chars().next() {
        if !ch.is_whitespace() {
            break;
        }
        value_start += ch.len_utf8();
    }
    if json.get(value_start..)?.chars().next()? != '{' {
        return None;
    }
    let value_end = find_matching_json(json, value_start, '{', '}')?;
    json.get(value_start..=value_end)
}

fn read_json_string_at(json: &str, start: usize) -> Option<(String, usize)> {
    if json.as_bytes().get(start).copied()? != b'"' {
        return None;
    }
    let mut value = String::new();
    let mut index = start + 1;
    while index < json.len() {
        let ch = json.get(index..)?.chars().next()?;
        index += ch.len_utf8();
        match ch {
            '"' => return Some((value, index)),
            '\\' => {
                let escaped = json.get(index..)?.chars().next()?;
                index += escaped.len_utf8();
                match escaped {
                    '"' => value.push('"'),
                    '\\' => value.push('\\'),
                    '/' => value.push('/'),
                    'b' => value.push('\u{08}'),
                    'f' => value.push('\u{0c}'),
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    'u' => {
                        let hex = json.get(index..index + 4)?;
                        let code = u16::from_str_radix(hex, 16).ok()?;
                        value.push(char::from_u32(u32::from(code))?);
                        index += 4;
                    }
                    _ => return None,
                }
            }
            _ => value.push(ch),
        }
    }
    None
}

fn find_matching_json(json: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut index = start;
    while index < json.len() {
        let ch = json.get(index..)?.chars().next()?;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += ch.len_utf8();
            continue;
        }

        if ch == '"' {
            in_string = true;
        } else if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index);
            }
        }
        index += ch.len_utf8();
    }
    None
}

fn cbor_to_json(data: &[u8]) -> Option<String> {
    let mut decoder = CborJsonDecoder::new(data);
    decoder.value().ok()
}

struct CborJsonDecoder<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> CborJsonDecoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn value(&mut self) -> Result<String, ()> {
        let byte = self.read_u8()?;
        let major = byte >> 5;
        let additional = byte & 0x1f;
        match major {
            0 => Ok(self.argument(additional)?.to_string()),
            1 => {
                let value = self.argument(additional)?;
                let signed = -1i128 - i128::from(value);
                Ok(signed.to_string())
            }
            2 => self.byte_string(additional),
            3 => self.text_string(additional),
            4 => self.array(additional),
            5 => self.map(additional),
            6 => self.tag(additional),
            7 => self.simple(additional),
            _ => Err(()),
        }
    }

    fn byte_string(&mut self, additional: u8) -> Result<String, ()> {
        if additional == 31 {
            let mut data = Vec::new();
            loop {
                if self.peek_u8()? == 0xff {
                    self.offset += 1;
                    break;
                }
                let byte = self.read_u8()?;
                if byte >> 5 != 2 {
                    return Err(());
                }
                let len = usize::try_from(self.argument(byte & 0x1f)?).map_err(|_| ())?;
                data.extend_from_slice(self.read_bytes(len)?);
            }
            return Ok(byte_string_json(&data));
        }

        let len = usize::try_from(self.argument(additional)?).map_err(|_| ())?;
        Ok(byte_string_json(self.read_bytes(len)?))
    }

    fn text_string(&mut self, additional: u8) -> Result<String, ()> {
        if additional == 31 {
            let mut text = String::new();
            loop {
                if self.peek_u8()? == 0xff {
                    self.offset += 1;
                    break;
                }
                let byte = self.read_u8()?;
                if byte >> 5 != 3 {
                    return Err(());
                }
                let len = usize::try_from(self.argument(byte & 0x1f)?).map_err(|_| ())?;
                let part = std::str::from_utf8(self.read_bytes(len)?).map_err(|_| ())?;
                text.push_str(part);
            }
            return Ok(format!("\"{}\"", json_escape(&text)));
        }

        let len = usize::try_from(self.argument(additional)?).map_err(|_| ())?;
        let text = std::str::from_utf8(self.read_bytes(len)?).map_err(|_| ())?;
        Ok(format!("\"{}\"", json_escape(text)))
    }

    fn array(&mut self, additional: u8) -> Result<String, ()> {
        let mut json = String::new();
        json.push('[');
        if additional == 31 {
            let mut index = 0usize;
            loop {
                if self.peek_u8()? == 0xff {
                    self.offset += 1;
                    break;
                }
                if index > 0 {
                    json.push(',');
                }
                json.push_str(&self.value()?);
                index += 1;
            }
        } else {
            let len = usize::try_from(self.argument(additional)?).map_err(|_| ())?;
            for index in 0..len {
                if index > 0 {
                    json.push(',');
                }
                json.push_str(&self.value()?);
            }
        }
        json.push(']');
        Ok(json)
    }

    fn map(&mut self, additional: u8) -> Result<String, ()> {
        let mut json = String::new();
        json.push('{');
        if additional == 31 {
            let mut index = 0usize;
            loop {
                if self.peek_u8()? == 0xff {
                    self.offset += 1;
                    break;
                }
                if index > 0 {
                    json.push(',');
                }
                self.map_entry(&mut json)?;
                index += 1;
            }
        } else {
            let len = usize::try_from(self.argument(additional)?).map_err(|_| ())?;
            for index in 0..len {
                if index > 0 {
                    json.push(',');
                }
                self.map_entry(&mut json)?;
            }
        }
        json.push('}');
        Ok(json)
    }

    fn map_entry(&mut self, json: &mut String) -> Result<(), ()> {
        let key = self.value()?;
        if key.starts_with('"') {
            json.push_str(&key);
        } else {
            json.push('"');
            json.push_str(&json_escape(&key));
            json.push('"');
        }
        json.push(':');
        json.push_str(&self.value()?);
        Ok(())
    }

    fn tag(&mut self, additional: u8) -> Result<String, ()> {
        let tag = self.argument(additional)?;
        let value = self.value()?;
        Ok(format!("{{\"tag\":{},\"value\":{}}}", tag, value))
    }

    fn simple(&mut self, additional: u8) -> Result<String, ()> {
        match additional {
            20 => Ok("false".to_string()),
            21 => Ok("true".to_string()),
            22 | 23 => Ok("null".to_string()),
            24 => Ok(format!("{{\"simple\":{}}}", self.read_u8()?)),
            25 => {
                let value = self.read_u16()?;
                Ok(format!("{{\"float16_bits\":{}}}", value))
            }
            26 => {
                let value = f32::from_bits(self.read_u32()?);
                Ok(format!("{}", value))
            }
            27 => {
                let value = f64::from_bits(self.read_u64()?);
                Ok(format!("{}", value))
            }
            31 => Err(()),
            value => Ok(format!("{{\"simple\":{}}}", value)),
        }
    }

    fn argument(&mut self, additional: u8) -> Result<u64, ()> {
        match additional {
            value @ 0..=23 => Ok(u64::from(value)),
            24 => Ok(u64::from(self.read_u8()?)),
            25 => Ok(u64::from(self.read_u16()?)),
            26 => Ok(u64::from(self.read_u32()?)),
            27 => self.read_u64(),
            _ => Err(()),
        }
    }

    fn peek_u8(&self) -> Result<u8, ()> {
        self.data.get(self.offset).copied().ok_or(())
    }

    fn read_u8(&mut self) -> Result<u8, ()> {
        let byte = self.peek_u8()?;
        self.offset += 1;
        Ok(byte)
    }

    fn read_u16(&mut self) -> Result<u16, ()> {
        let bytes: [u8; 2] = self.read_bytes(2)?.try_into().map_err(|_| ())?;
        Ok(u16::from_be_bytes(bytes))
    }

    fn read_u32(&mut self) -> Result<u32, ()> {
        let bytes: [u8; 4] = self.read_bytes(4)?.try_into().map_err(|_| ())?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn read_u64(&mut self) -> Result<u64, ()> {
        let bytes: [u8; 8] = self.read_bytes(8)?.try_into().map_err(|_| ())?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], ()> {
        let end = self.offset.checked_add(len).ok_or(())?;
        let bytes = self.data.get(self.offset..end).ok_or(())?;
        self.offset = end;
        Ok(bytes)
    }
}

fn byte_string_json(data: &[u8]) -> String {
    let mut json = String::new();
    json.push('{');
    push_json_pair(&mut json, "type", "bytes");
    json.push(',');
    push_json_usize(&mut json, "length", data.len());
    json.push(',');
    push_json_pair(&mut json, "base64", &base64_encode(data));
    if let Ok(text) = std::str::from_utf8(data) {
        if text
            .chars()
            .all(|ch| ch == '\n' || ch == '\r' || ch == '\t' || ch >= ' ')
        {
            json.push(',');
            push_json_pair(&mut json, "text", text);
        }
    }
    json.push('}');
    json
}

fn push_box_json(json: &mut String, jumbf_box: &JumbfBox) {
    json.push('{');
    push_json_pair(json, "type", &jumbf_box.box_type);
    json.push(',');
    push_json_usize(json, "offset", jumbf_box.offset);
    json.push(',');
    push_json_usize(json, "size", jumbf_box.size);
    json.push(',');
    push_json_usize(json, "header_size", jumbf_box.header_size);
    if let Some(uuid) = &jumbf_box.uuid {
        json.push(',');
        push_json_pair(json, "uuid", uuid);
    }
    if let Some(label) = &jumbf_box.label {
        json.push(',');
        push_json_pair(json, "label", label);
    }
    if let Some(text) = &jumbf_box.text {
        json.push(',');
        push_json_pair(json, "text", text);
    }
    if let Some(cbor) = &jumbf_box.cbor {
        json.push(',');
        json.push_str("\"cbor\":");
        json.push_str(cbor);
    }
    if !jumbf_box.children.is_empty() {
        json.push(',');
        json.push_str("\"children\":[");
        for (index, child) in jumbf_box.children.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            push_box_json(json, child);
        }
        json.push(']');
    }
    json.push('}');
}

fn push_json_pair(json: &mut String, key: &str, value: &str) {
    json.push('"');
    json.push_str(key);
    json.push_str("\":\"");
    json.push_str(&json_escape(value));
    json.push('"');
}

fn push_json_usize(json: &mut String, key: &str, value: usize) {
    json.push('"');
    json.push_str(key);
    json.push_str("\":");
    json.push_str(&value.to_string());
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch < ' ' => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn uuid_to_string(bytes: &[u8]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_bytes(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut data = Vec::with_capacity(8 + payload.len());
        data.extend_from_slice(&((8 + payload.len()) as u32).to_be_bytes());
        data.extend_from_slice(box_type);
        data.extend_from_slice(payload);
        data
    }

    fn c2pa_jumbf() -> Vec<u8> {
        let mut jumd = Vec::new();
        jumd.extend_from_slice(&C2PA_MANIFEST_STORE_UUID);
        jumd.push(0x03);
        jumd.extend_from_slice(b"c2pa\0");
        let jumd = box_bytes(b"jumd", &jumd);
        let json = box_bytes(b"json", br#"{"claim":"test"}"#);

        let mut payload = Vec::new();
        payload.extend_from_slice(&jumd);
        payload.extend_from_slice(&json);
        box_bytes(b"jumb", &payload)
    }

    #[test]
    fn jumbf_json_contains_label_uuid_and_raw_payload() {
        let payload = c2pa_jumbf();
        let manifest = C2paManifestStore::new("png-caBX", &payload);
        let json = manifest.to_json();

        assert!(json.contains("\"format\":\"c2pa\""));
        assert!(json.contains("\"source\":\"png-caBX\""));
        assert!(json.contains("63327061-0011-0010-8000-00aa00389b71"));
        assert!(json.contains("\"label\":\"c2pa\""));
        assert!(json.contains("\"manifest_store_base64\""));
    }

    #[test]
    fn cbor_box_json_contains_decoded_claim_content() {
        let cbor = hex_bytes(
            "a267616374696f6e7381a266616374696f6e6c633270612e63726561746564647768656e74323032362d30342d32355430303a30303a30305a64686173685820000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        );
        let cbor = box_bytes(b"cbor", &cbor);

        let mut jumd = Vec::new();
        jumd.extend_from_slice(&[
            0x63, 0x62, 0x6f, 0x72, 0x00, 0x11, 0x00, 0x10, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38,
            0x9b, 0x71,
        ]);
        jumd.push(0x03);
        jumd.extend_from_slice(b"c2pa.actions.v2\0");
        let jumd = box_bytes(b"jumd", &jumd);

        let mut payload = Vec::new();
        payload.extend_from_slice(&jumd);
        payload.extend_from_slice(&cbor);
        let manifest = C2paManifestStore::new("test", &box_bytes(b"jumb", &payload));
        let json = manifest.to_json();

        assert!(json.contains("\"cbor\""));
        assert!(json.contains("\"actions\""));
        assert!(json.contains("\"c2pa.created\""));
        assert!(json.contains("\"hash\":{\"type\":\"bytes\",\"length\":32"));
    }

    #[test]
    fn c2pa_to_text_keeps_claim_name_and_actions_only() {
        let json = r#"{"manifest_store_base64":"AAAA","jumbf_boxes":[{"cbor":{"claim_generator_info":{"name":"OpenAI Media Service API","icon":{"hash":{"type":"bytes","base64":"AAAA"}}},"actions":[{"action":"c2pa.created","when":{"tag":0,"value":"2026-04-25T00:00:00Z"},"softwareAgent":{"name":"gpt-image","version":"2.0"},"digitalSourceType":"http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia","hash":{"type":"bytes","base64":"BBBB"}},{"action":"c2pa.converted"}]}},{"cbor":{"tag":18,"value":[{}]}}]}"#;

        let text = c2pa_to_text(json);

        assert!(text.contains("claim_generator_info.name:"));
        assert!(text.contains("OpenAI Media Service API"));
        assert!(text.contains("- c2pa.created"));
        assert!(text.contains("when: 2026-04-25T00:00:00Z"));
        assert!(text.contains("softwareAgent.name: gpt-image"));
        assert!(text.contains("softwareAgent.version: 2.0"));
        assert!(text.contains("digitalSourceType: http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia"));
        assert!(text.contains("- c2pa.converted"));
        assert!(!text.contains("base64"));
        assert!(!text.contains("signature"));
    }

    #[test]
    fn jpeg_app11_extracts_direct_jumbf_payload() {
        let payload = c2pa_jumbf();
        assert_eq!(
            jpeg_app11_payload_to_manifest_store(&payload),
            Some(payload.clone())
        );

        let mut wrapped = b"C2PA\0".to_vec();
        wrapped.extend_from_slice(&payload);
        assert_eq!(
            jpeg_app11_payload_to_manifest_store(&wrapped),
            Some(payload)
        );
    }

    fn hex_bytes(hex: &str) -> Vec<u8> {
        hex.as_bytes()
            .chunks(2)
            .map(|chunk| {
                let text = std::str::from_utf8(chunk).unwrap();
                u8::from_str_radix(text, 16).unwrap()
            })
            .collect()
    }
}
