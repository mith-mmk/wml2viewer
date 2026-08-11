//! PNG utility helpers.

use crate::metadata::DataMap;
use crate::tiff::header::read_tags;
use bin_rs::reader::BytesReader;
use std::collections::HashMap;

pub(crate) fn paeth_dec(d: u8, a: i32, b: i32, c: i32) -> u8 {
    let pa = (b - c).abs();
    let pb = (a - c).abs();
    let pc = (b + a - c - c).abs();
    let d = d as i32;
    if pa <= pb && pa <= pc {
        ((d + a) & 0xff) as u8
    } else if pb <= pc {
        ((d + b) & 0xff) as u8
    } else {
        ((d + c) & 0xff) as u8
    }
}

pub(crate) fn paeth_enc(d: u8, a: i32, b: i32, c: i32) -> u8 {
    let pa = (b - c).abs();
    let pb = (a - c).abs();
    let pc = (b + a - c - c).abs();
    let d = d as i32;
    if pa <= pb && pa <= pc {
        ((d - a) & 0xff) as u8
    } else if pb <= pc {
        ((d - b) & 0xff) as u8
    } else {
        ((d - c) & 0xff) as u8
    }
}

pub struct CRC32 {
    crc_table: [u32; 256],
}

impl CRC32 {
    pub fn new() -> Self {
        let mut crc_table = [0_u32; 256];
        for n in 0..256 {
            let mut c = n as u32;
            for _ in 0..8 {
                if c & 0x01 == 0x01 {
                    c = 0xedb8_8320_u32 ^ (c >> 1);
                } else {
                    c >>= 1;
                }
                crc_table[n] = c;
            }
        }
        Self { crc_table }
    }

    fn update_crc32(&self, crc: u32, buf: &[u8]) -> u32 {
        let crc_table = self.crc_table;

        let mut c = crc;
        for data in buf.iter() {
            c = crc_table[((c ^ *data as u32) & 0xff) as usize] ^ (c >> 8);
        }
        c
    }

    pub fn crc32(&self, buf: &[u8]) -> u32 {
        self.update_crc32(0xffff_ffff, buf) ^ 0xffff_ffff
    }
}

/*
fn crc_test() {
    let crc32 = wml2::png::utils::CRC32::new();
    let buf = [73,69,78,68];
    let result = crc32.crc32(&buf);
    assert_eq!(result,0xAE26082);
}

*/

pub(crate) fn make_metadata(
    header: &super::header::PngHeader,
    maximum_metadata_output: Option<usize>,
) -> HashMap<String, DataMap> {
    let mut map: HashMap<String, DataMap> = HashMap::new();
    map.insert("Format".to_string(), DataMap::Ascii("PNG".to_string()));
    map.insert("width".to_string(), DataMap::UInt(header.width as u64));
    map.insert("height".to_string(), DataMap::UInt(header.height as u64));
    if let Some(gamma) = header.gamma {
        let gamma = gamma as f64 / 100000.0;
        map.insert("gamma".to_string(), DataMap::Float(gamma));
    }
    if let Some(modified_time) = &header.modified_time {
        map.insert(
            "gamma".to_string(),
            DataMap::Ascii(modified_time.to_string()),
        );
    }
    if let Some(srgb) = &header.srgb {
        map.insert("sRGB".to_string(), DataMap::UInt(*srgb as u64));
    }
    for (key, val) in &header.text {
        map.insert(key.to_string(), DataMap::Ascii(val.to_string()));
    }
    if let Some(profile) = &header.iccprofile {
        if let Some((profile_name, icc_profile)) =
            decode_icc_profile(profile, maximum_metadata_output)
        {
            map.insert("ICC Profile name".to_string(), DataMap::Ascii(profile_name));
            map.insert("ICC Profile".to_string(), DataMap::ICCProfile(icc_profile));
        }
    }
    if let Some(exif) = &header.exif {
        let mut reader = BytesReader::new(exif);
        if let Ok(headers) = read_tags(&mut reader) {
            map.insert("EXIF".to_string(), DataMap::Exif(headers));
        }
        map.insert("EXIF Raw".to_string(), DataMap::Raw(exif.clone()));
    }
    #[cfg(feature = "c2pa")]
    {
        if let Some(c2pa) = &header.c2pa {
            crate::metadata::c2pa::insert_metadata(&mut map, "png-caBX", c2pa);
        }
    }
    /*
        pub transparency: Option<Vec<u8>>,
        pub background_color: Option<BacgroundColor>,
        pub sbit: Option<Vec<u8>>,
    */

    map
}

fn decode_icc_profile(profile: &[u8], maximum_output: Option<usize>) -> Option<(String, Vec<u8>)> {
    // iCCP payload layout: profile_name\0 compression_method compressed_profile
    let name_end = profile.iter().position(|b| *b == 0)?;
    let method_pos = name_end + 1;
    let data_pos = name_end + 2;
    if data_pos > profile.len() || method_pos >= profile.len() {
        return None;
    }
    if profile[method_pos] != 0 {
        return None;
    }

    let profile_name = String::from_utf8_lossy(&profile[..name_end]).to_string();
    let icc_profile = if let Some(maximum_output) = maximum_output {
        miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
            &profile[data_pos..],
            maximum_output,
        )
    } else {
        miniz_oxide::inflate::decompress_to_vec_zlib(&profile[data_pos..])
    }
    .ok()?;
    Some((profile_name, icc_profile))
}

#[cfg(test)]
mod tests {
    use super::decode_icc_profile;

    #[test]
    fn decode_icc_profile_rejects_truncated_payloads() {
        assert!(decode_icc_profile(b"iccname", None).is_none());
        assert!(decode_icc_profile(b"iccname\0", None).is_none());
        assert!(decode_icc_profile(b"iccname\0\0", None).is_none());
    }

    #[test]
    fn decode_icc_profile_rejects_nonzero_compression_method() {
        let payload = b"iccname\0\x01dummy";
        assert!(decode_icc_profile(payload, None).is_none());
    }
}
