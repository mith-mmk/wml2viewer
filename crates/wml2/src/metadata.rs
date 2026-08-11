//! Metadata value types shared by image decoders and encoders.

use crate::error::{ImgError, ImgErrorKind};
#[cfg(feature = "exif")]
use crate::tiff::header::TiffHeaders;
#[cfg(feature = "exif")]
use crate::tiff::header::exif_to_bytes;
use std::collections::HashMap;

#[cfg(feature = "c2pa")]
pub mod c2pa;
#[cfg(feature = "exif")]
pub mod exif;

/// A map of metadata keys to typed values.
pub type Metadata = HashMap<String, DataMap>;

/// A typed metadata value.
#[derive(Debug, Clone, PartialEq)]
pub enum DataMap {
    UInt(u64),
    SInt(i64),
    Float(f64),
    UIntAllay(Vec<u64>),
    SIntAllay(Vec<i64>),
    FloatAllay(Vec<f64>),
    Raw(Vec<u8>),
    Ascii(String),
    JSON(String),
    I18NString(String),
    SJISString(Vec<u8>),
    #[cfg(feature = "exif")]
    Exif(TiffHeaders),
    ICCProfile(Vec<u8>),
    None,
}

impl DataMap {
    /// Converts the metadata value to a display-oriented string.
    pub fn to_string(&self) -> String {
        match self {
            DataMap::UInt(d) => d.to_string(),
            DataMap::SInt(d) => d.to_string(),
            DataMap::Float(d) => d.to_string(),
            DataMap::UIntAllay(d) => {
                format!("{:?}", d)
            }
            DataMap::SIntAllay(d) => {
                format!("{:?}", d)
            }
            DataMap::FloatAllay(d) => {
                format!("{:?}", d)
            }
            DataMap::Raw(d) => {
                format!("{:?}", d)
            }
            DataMap::Ascii(d) => d.to_string(),
            DataMap::JSON(d) => json_pretty(d),
            DataMap::I18NString(d) => d.to_string(),
            DataMap::SJISString(d) => {
                format!("{:?}", d)
            }
            #[cfg(feature = "exif")]
            DataMap::Exif(header) => header.to_string(),
            DataMap::ICCProfile(iccprofile) => {
                format!("{:?}", iccprofile)
            }
            DataMap::None => "none".to_owned(),
        }
    }
}

/// Formats a JSON string using two-space indentation.
///
/// The formatter is intentionally small and only changes whitespace outside
/// string literals. It is suitable for metadata display paths where the source
/// JSON is already produced by the decoder.
pub fn json_pretty(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len() + value.len() / 4);
    let mut indent = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut index = 0usize;

    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                out.push(ch);
            }
            '{' | '[' => {
                out.push(ch);
                let close = if ch == '{' { '}' } else { ']' };
                if next_non_ws(&chars, index + 1) != Some(close) {
                    indent += 1;
                    push_json_newline(&mut out, indent);
                }
            }
            '}' | ']' => {
                let open = if ch == '}' { '{' } else { '[' };
                if previous_non_ws(&chars, index) != Some(open) {
                    indent = indent.saturating_sub(1);
                    push_json_newline(&mut out, indent);
                }
                out.push(ch);
            }
            ',' => {
                out.push(',');
                push_json_newline(&mut out, indent);
            }
            ':' => {
                out.push_str(": ");
            }
            ch if ch.is_whitespace() => {}
            _ => out.push(ch),
        }
        index += 1;
    }

    out
}

fn push_json_newline(out: &mut String, indent: usize) {
    out.push('\n');
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn next_non_ws(chars: &[char], mut index: usize) -> Option<char> {
    while index < chars.len() {
        let ch = chars[index];
        if !ch.is_whitespace() {
            return Some(ch);
        }
        index += 1;
    }
    None
}

fn previous_non_ws(chars: &[char], index: usize) -> Option<char> {
    let mut index = index.checked_sub(1)?;
    loop {
        let ch = chars[index];
        if !ch.is_whitespace() {
            return Some(ch);
        }
        if index == 0 {
            break;
        }
        index -= 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_pretty_formats_like_json_stringify_indent_two() {
        let json = r#"{"a":1,"b":[true,{"c":"d,e"}],"empty":[]}"#;

        assert_eq!(
            json_pretty(json),
            "{\n  \"a\": 1,\n  \"b\": [\n    true,\n    {\n      \"c\": \"d,e\"\n    }\n  ],\n  \"empty\": []\n}"
        );
    }
}

/// Extracts a serialized EXIF payload from metadata when present.
///
/// This accepts decoded TIFF-style metadata (`"Tiff headers"`), generic EXIF
/// metadata (`"EXIF"`), or an already serialized fallback (`"EXIF Raw"`).
pub fn get_exif(
    metadata: Option<&Metadata>,
) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    let Some(metadata) = metadata else {
        return Ok(None);
    };

    #[cfg(feature = "exif")]
    match metadata.get("EXIF") {
        Some(DataMap::Exif(headers)) => return exif_to_bytes(headers).map(Some),
        Some(DataMap::Raw(bytes)) => return Ok(Some(bytes.clone())),
        Some(_) => {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                "EXIF metadata is not serializable".to_string(),
            )));
        }
        None => {}
    }

    #[cfg(feature = "exif")]
    match metadata.get("Tiff headers") {
        Some(DataMap::Exif(headers)) => return exif_to_bytes(headers).map(Some),
        Some(_) => {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                "Tiff headers metadata is not serializable".to_string(),
            )));
        }
        None => {}
    }

    match metadata.get("EXIF Raw") {
        Some(DataMap::Raw(bytes)) => Ok(Some(bytes.clone())),
        Some(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "EXIF Raw metadata is not raw bytes".to_string(),
        ))),
        None => Ok(None),
    }
}

fn find_exif_option<'a>(options: Option<&'a Metadata>) -> Option<&'a DataMap> {
    let options = options?;
    options
        .get("exif")
        .or_else(|| options.get("exif "))
        .or_else(|| {
            options
                .iter()
                .find(|(key, _)| key.trim().eq_ignore_ascii_case("exif"))
                .map(|(_, value)| value)
        })
}

/// Resolves an EXIF payload from encoder options.
///
/// Supported option forms:
/// - `{"exif": Raw(bytes)}`: use serialized EXIF/TIFF bytes directly
/// - `{"exif": Exif(headers)}`: serialize [`TiffHeaders`] on demand
/// - `{"exif": Ascii("copy")}`: copy EXIF from source metadata
pub fn get_exif_option(
    options: Option<&Metadata>,
    metadata: Option<&Metadata>,
) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    let Some(value) = find_exif_option(options) else {
        return Ok(None);
    };

    match value {
        DataMap::Raw(bytes) => Ok(Some(bytes.clone())),
        #[cfg(feature = "exif")]
        DataMap::Exif(headers) => exif_to_bytes(headers).map(Some),
        DataMap::Ascii(value) if value.trim().eq_ignore_ascii_case("copy") => get_exif(metadata),
        DataMap::Ascii(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "exif option must be Raw(bytes), Exif(headers), or Ascii(\"copy\")".to_string(),
        ))),
        _ => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "exif option must be Raw(bytes), Exif(headers), or Ascii(\"copy\")".to_string(),
        ))),
    }
}
