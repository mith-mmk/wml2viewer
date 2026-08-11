//! PNG chunk and header parsing types.

use crate::color::RGBA;
use crate::error::*;
use bin_rs::io::*;
use bin_rs::reader::BinaryReader;
type Error = Box<dyn std::error::Error>;

pub(crate) const SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
pub(crate) const IMAGE_HEADER: [u8; 4] = [b'I', b'H', b'D', b'R'];
const PALLET: [u8; 4] = [b'P', b'L', b'T', b'E'];
pub(crate) const IMAGE_DATA: [u8; 4] = [b'I', b'D', b'A', b'T'];
pub(crate) const IMAGE_END: [u8; 4] = [b'I', b'E', b'N', b'D'];
const TRANNCEPEARENCY: [u8; 4] = [b't', b'R', b'N', b'S'];

const GAMMA: [u8; 4] = [b'g', b'A', b'M', b'A'];
/*
const COLOR_HMR:[u8;4] = [b'c',b'H',b'R',b'M'];
*/
const SRGB: [u8; 4] = [b's', b'R', b'G', b'B'];
const ICC_PROFILE: [u8; 4] = [b'i', b'C', b'C', b'P'];
pub(crate) const EXIF_PROFILE: [u8; 4] = [b'e', b'X', b'I', b'f'];
pub(crate) const C2PA_CHUNK: [u8; 4] = *b"caBX";

pub(crate) const TEXTDATA: [u8; 4] = [b't', b'E', b'X', b't'];
pub(crate) const COMPRESSED_TEXTUAL_DATA: [u8; 4] = [b'z', b'T', b'X', b't'];
pub(crate) const I18N_TEXT: [u8; 4] = [b'i', b'T', b'X', b't'];
pub(crate) const BACKGROUND_COLOR: [u8; 4] = [b'b', b'K', b'G', b'D'];
/* no use
const PHYSICAL_PIXEL_DIMENSION:[u8;4] = [b'p',b'H',b'Y',b's'];
const SIGNIFICANT_BITS:[u8;4] = [b's',b'B',b'I',b'T'];
const STANDARD_PALLET:[u8;4] = [b's',b'P',b'L',b'T'];
const PALLTE_HISTGRAM:[u8;4] = [b'h',b'I',b'S',b'T'];
*/
pub(crate) const MODIFIED_TIME: [u8; 4] = [b't', b'I', b'M', b'E'];

/*
// no impl
const FRACTALIMAGE:[u8;4] = [b'f',b'R',b'A',b'c'];
const GIFGRAPHICEXTENTION:[u8;4] = [b'g',b'I',b'F',b'g'];
const GIFTEXTEXTENTION:[u8;4] = [b'g',b'I',b'F',b't'];
const GIFEXTENTION:[u8;4] = [b'g',b'I',b'F',b'x'];
const IMAGEOFFSETS:[u8;4] = [b'o',b'F',b'F',b's'];
const PIXELCALC:[u8;4] = [b'p',b'C',b'A',b'l'];
const SCAL:[u8;4] = [b's',b'C',b'A',b'L'];
*/

// APNG

pub(crate) const ANIMATION_CONTROLE: [u8; 4] = [b'a', b'c', b'T', b'L'];
pub(crate) const FRAME_CONTROLE: [u8; 4] = [b'f', b'c', b'T', b'L'];
pub(crate) const FRAME_DATA: [u8; 4] = [b'f', b'd', b'A', b'T'];

fn chunk_size_error(name: &str, length: u32, expected: &str) -> Error {
    Box::new(ImgError::new_const(
        ImgErrorKind::IllegalData,
        format!("Illegal size {}: {} (expected {})", name, length, expected),
    ))
}

pub(crate) fn to_string(
    text: &[u8],
    compressed: bool,
    maximum_output: Option<usize>,
) -> (String, String) {
    let mut split = 0;
    let keyword = read_ascii_string(text, 0, text.len());
    for i in 0..text.len() {
        if text[i] == 0 {
            split = i + 1;
            break;
        }
    }
    let string = if compressed {
        let compressed = text.get(split.saturating_add(1)..).unwrap_or_default();
        let decoded = if let Some(maximum_output) = maximum_output {
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(compressed, maximum_output)
        } else {
            miniz_oxide::inflate::decompress_to_vec_zlib(compressed)
        };
        if let Ok(decode) = decoded {
            decode
        } else {
            b"".to_vec()
        }
    } else {
        text[split..].to_vec()
    };
    let string = read_ascii_string(&string, 0, string.len());
    (keyword, string)
}

#[derive(Debug, Clone)]
pub enum BacgroundColor {
    Index(u8),
    Grayscale(u16),
    TrueColor((u16, u16, u16)),
}

#[derive(Debug, Clone)]
pub struct FrameControl {
    pub sequence_number: u32,
    pub width: u32,
    pub height: u32,
    pub x_offset: u32,
    pub y_offset: u32,
    pub delay_num: u16,
    pub delay_den: u16,
    pub dispose_op: u8,
    pub blend_op: u8,
}

#[derive(Debug, Clone)]
pub struct PngHeader {
    pub width: u32,
    pub height: u32,
    pub bitpersample: u8,
    pub color_type: u8,
    pub compression: u8,
    pub filter_method: u8,
    pub interace_method: u8,
    pub crc: u32,
    pub image_lenghth: u32,
    pub pallete: Option<Vec<RGBA>>,
    pub gamma: Option<u32>,
    pub transparency: Option<Vec<u8>>,
    pub srgb: Option<u8>,
    pub iccprofile: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
    #[cfg(feature = "c2pa")]
    pub c2pa: Option<Vec<u8>>,
    pub background_color: Option<BacgroundColor>,
    pub sbit: Option<Vec<u8>>,
    pub text: Vec<(String, String)>,
    pub modified_time: Option<String>,
    pub is_apng: bool,
    pub num_frames: u32,
    pub num_plays: u32,
    pub frame_controls: Vec<FrameControl>,
}

impl PngHeader {
    pub fn new<B: BinaryReader>(
        reader: &mut B,
        _opt: usize,
        maximum_metadata_output: Option<usize>,
    ) -> Result<Self, Error> {
        let signature = reader.read_bytes_as_vec(8)?;
        if signature != SIGNATURE {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::NoSupportFormat,
                "not PNG".to_string(),
            )));
        }

        let mut header = Self {
            width: 0,
            height: 0,
            bitpersample: 32,
            color_type: 2,
            compression: 1,
            filter_method: 0,
            interace_method: 0,
            crc: 0,
            image_lenghth: 0,
            pallete: None,
            gamma: None,
            transparency: None,
            srgb: None,
            iccprofile: None,
            exif: None,
            #[cfg(feature = "c2pa")]
            c2pa: None,
            background_color: None,
            sbit: None,
            text: Vec::new(),
            modified_time: None,
            is_apng: false,
            num_frames: 0,
            num_plays: 0,
            frame_controls: vec![],
        };

        let mut pallete_size = 0;

        loop {
            let buf = reader.read_bytes_no_move(8)?;
            let length = read_u32_be(&buf, 0);
            let chunck = &buf[4..];
            if chunck == IMAGE_DATA {
                header.image_lenghth = length;
                break;
            } else if chunck == IMAGE_HEADER {
                if length != 13 {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::IllegalData,
                        "Illegal size IHDR".to_string(),
                    )));
                }
                reader.skip_ptr(8)?;
                header.width = reader.read_u32_be()?;
                header.height = reader.read_u32_be()?;
                header.bitpersample = reader.read_byte()?;
                header.color_type = reader.read_byte()?;

                match header.color_type {
                    0 => {
                        if header.bitpersample != 1
                            && header.bitpersample != 2
                            && header.bitpersample != 4
                            && header.bitpersample != 8
                            && header.bitpersample != 16
                        {
                            let string = format!(
                                "Glayscale must be bitpersample 1,2,4,8,16 but {}",
                                header.bitpersample
                            );
                            return Err(Box::new(ImgError::new_const(
                                ImgErrorKind::IllegalData,
                                string,
                            )));
                        }
                    }
                    2 => {
                        if header.bitpersample != 8 && header.bitpersample != 16 {
                            let string = format!(
                                "True colors must be bitpersample 8,16 but {}",
                                header.bitpersample
                            );
                            return Err(Box::new(ImgError::new_const(
                                ImgErrorKind::IllegalData,
                                string,
                            )));
                        }
                    }
                    3 => {
                        if header.bitpersample != 1
                            && header.bitpersample != 2
                            && header.bitpersample != 4
                            && header.bitpersample != 8
                        {
                            let string = format!(
                                "Index colors must be bitpersample 8,16 but {}",
                                header.bitpersample
                            );
                            return Err(Box::new(ImgError::new_const(
                                ImgErrorKind::IllegalData,
                                string,
                            )));
                        }
                    }
                    4 => {
                        if header.bitpersample != 8 && header.bitpersample != 16 {
                            let string = format!(
                                "Glayscale with alpha must be bitpersample 8,16 but {}",
                                header.bitpersample
                            );
                            return Err(Box::new(ImgError::new_const(
                                ImgErrorKind::IllegalData,
                                string,
                            )));
                        }
                    }
                    6 => {
                        if header.bitpersample != 8 && header.bitpersample != 16 {
                            let string = format!(
                                "True colors with alpha must be bitpersample 8,16 but {}",
                                header.bitpersample
                            );
                            return Err(Box::new(ImgError::new_const(
                                ImgErrorKind::IllegalData,
                                string,
                            )));
                        }
                    }
                    _ => {
                        let string = format!("Color type {} is unknown", header.color_type);
                        return Err(Box::new(ImgError::new_const(
                            ImgErrorKind::IllegalData,
                            string,
                        )));
                    }
                }

                header.compression = reader.read_byte()?;
                header.filter_method = reader.read_byte()?;
                header.interace_method = reader.read_byte()?;
                let _crc = reader.read_u32_be()?;
            } else if chunck == PALLET {
                reader.skip_ptr(8)?;
                if length % 3 != 0 {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::IllegalData,
                        "Illegal size PLTE".to_string(),
                    )));
                }
                pallete_size = length as usize / 3;
                let mut pallet: Vec<RGBA> = Vec::with_capacity(pallete_size);
                for _ in 0..pallete_size {
                    let color = RGBA {
                        red: reader.read_byte()?,
                        green: reader.read_byte()?,
                        blue: reader.read_byte()?,
                        alpha: 0xff,
                    };
                    pallet.push(color);
                }
                header.pallete = Some(pallet);
                let _crc = reader.read_u32_be()?;
            } else if chunck == TRANNCEPEARENCY {
                reader.skip_ptr(8)?;
                let transparency = reader.read_bytes_as_vec(length as usize)?;
                match header.color_type {
                    3 => {
                        if header.pallete.is_none() {
                            return Err(Box::new(ImgError::new_const(
                                ImgErrorKind::IllegalData,
                                "Illegal format tRNS before PLTE".to_string(),
                            )));
                        } else if length as usize > pallete_size {
                            return Err(Box::new(ImgError::new_const(
                                ImgErrorKind::IllegalData,
                                "Illegal format tRNS too big,PLTE size".to_string(),
                            )));
                        }
                        if let Some(pallete) = &mut header.pallete {
                            for i in 0..transparency.len() {
                                pallete[i].alpha = transparency[i];
                            }
                        }
                    }
                    0 => {
                        if length != 2 {
                            return Err(chunk_size_error("tRNS", length, "2"));
                        }
                    }
                    2 => {
                        if length != 6 {
                            return Err(chunk_size_error("tRNS", length, "6"));
                        }
                    }
                    _ => {
                        return Err(Box::new(ImgError::new_const(
                            ImgErrorKind::IllegalData,
                            "Illegal format tRNS for this color type".to_string(),
                        )));
                    }
                }
                header.transparency = Some(transparency);
                let _crc = reader.read_u32_be()?;
            } else if chunck == GAMMA {
                reader.skip_ptr(8)?;
                if length != 4 {
                    return Err(chunk_size_error("gAMA", length, "4"));
                }
                let gamma = reader.read_u32_be()?;
                header.gamma = Some(gamma);
                let _crc = reader.read_u32_be()?;
            } else if chunck == TEXTDATA || chunck == I18N_TEXT {
                reader.skip_ptr(8)?;
                let text = reader.read_bytes_as_vec(length as usize)?;
                header
                    .text
                    .push(to_string(&text, false, maximum_metadata_output));
                let _crc = reader.read_u32_be()?;
            } else if chunck == COMPRESSED_TEXTUAL_DATA {
                reader.skip_ptr(8)?;
                let text = reader.read_bytes_as_vec(length as usize)?;
                header
                    .text
                    .push(to_string(&text, true, maximum_metadata_output));
                let _crc = reader.read_u32_be()?;
            } else if chunck == BACKGROUND_COLOR {
                reader.skip_ptr(8)?;
                let buffer = reader.read_bytes_as_vec(length as usize)?;
                match header.color_type {
                    3 => {
                        if length != 1 {
                            return Err(chunk_size_error("bKGD", length, "1"));
                        }
                        header.background_color = Some(BacgroundColor::Index(buffer[0]));
                    }
                    0 | 4 => {
                        if length != 2 {
                            return Err(chunk_size_error("bKGD", length, "2"));
                        }
                        let color = read_u16_be(&buffer, 0);
                        header.background_color = Some(BacgroundColor::Grayscale(color));
                    }
                    2 | 6 => {
                        if length != 6 {
                            return Err(chunk_size_error("bKGD", length, "6"));
                        }
                        let red = read_u16_be(&buffer, 0);
                        let green = read_u16_be(&buffer, 2);
                        let blue = read_u16_be(&buffer, 4);
                        header.background_color =
                            Some(BacgroundColor::TrueColor((red, green, blue)));
                    }
                    _ => {}
                }
                let _crc = reader.read_u32_be()?;
            } else if chunck == MODIFIED_TIME {
                // no impl...
                reader.skip_ptr(8)?;
                if length != 7 {
                    return Err(chunk_size_error("tIME", length, "7"));
                }
                let year = reader.read_u16_be()?;
                let month = reader.read_byte()?;
                let day = reader.read_byte()?;
                let hour = reader.read_byte()?;
                let miniute = reader.read_byte()?;
                let second = reader.read_byte()?;
                let date = format!("{}-{}-{} {}:{}:{}", year, month, day, hour, miniute, second);
                header.modified_time = Some(date);
                let _crc = reader.read_u32_be()?;
            } else if chunck == ANIMATION_CONTROLE {
                reader.skip_ptr(8)?;
                if length != 8 {
                    return Err(chunk_size_error("acTL", length, "8"));
                }
                header.is_apng = true;
                header.num_frames = reader.read_u32_be()?;
                header.num_plays = reader.read_u32_be()?;
                let _crc = reader.read_u32_be()?;
            } else if chunck == FRAME_CONTROLE {
                // noimpl
                reader.skip_ptr(8)?;
                if length != 26 {
                    return Err(chunk_size_error("fcTL", length, "26"));
                }
                let frame_control = FrameControl {
                    sequence_number: reader.read_u32_be()?,
                    width: reader.read_u32_be()?,
                    height: reader.read_u32_be()?,
                    x_offset: reader.read_u32_be()?,
                    y_offset: reader.read_u32_be()?,
                    delay_num: reader.read_u16_be()?,
                    delay_den: reader.read_u16_be()?,
                    dispose_op: reader.read_byte()?,
                    blend_op: reader.read_byte()?,
                };
                header.frame_controls.push(frame_control);
                let _crc = reader.read_u32_be()?;
            } else if chunck == SRGB {
                // noimpl
                reader.skip_ptr(8)?;
                if length != 1 {
                    return Err(chunk_size_error("sRGB", length, "1"));
                }
                let srgb = reader.read_byte()?;
                header.srgb = Some(srgb);
                let _crc = reader.read_u32_be()?;
            } else if chunck == ICC_PROFILE {
                // noimpl
                reader.skip_ptr(8)?;
                let icc_profile = reader.read_bytes_as_vec(length as usize)?;
                header.iccprofile = Some(icc_profile);
                let _crc = reader.read_u32_be()?;
            } else if chunck == EXIF_PROFILE {
                reader.skip_ptr(8)?;
                let exif = reader.read_bytes_as_vec(length as usize)?;
                header.exif = Some(exif);
                let _crc = reader.read_u32_be()?;
            } else if chunck == C2PA_CHUNK {
                reader.skip_ptr(8)?;
                #[cfg(feature = "c2pa")]
                {
                    let mut c2pa = reader.read_bytes_as_vec(length as usize)?;
                    header.c2pa.get_or_insert_with(Vec::new).append(&mut c2pa);
                }
                #[cfg(not(feature = "c2pa"))]
                {
                    reader.skip_ptr(length as usize)?;
                }
                let _crc = reader.read_u32_be()?;
            } else {
                // no impl...
                reader.skip_ptr(8)?;
                reader.skip_ptr(length as usize)?;
                let _crc = reader.read_u32_be()?;
            }
        }
        Ok(header)
    }
}
