//! BMP header parsing and constants.

/*
 *  bmp/header.rs (C) 2022 Mith@mmk
 *
 *
 */

type Error = Box<dyn std::error::Error>;

use crate::error::ImgError;
use crate::error::ImgErrorKind;
use bin_rs::reader::BinaryReader;
use std::io::SeekFrom;

#[allow(unused)]
#[derive(Debug)]
pub struct BitmapHeader {
    pub filesize: usize,
    pub image_offset: usize,
    pub width: i32,
    pub height: i32,
    pub bit_count: usize,
    pub compression: Option<Compressions>,
    pub color_table: Option<Vec<ColorTable>>,
    pub bitmap_file_header: BitmapFileHeader,
    pub bitmap_info: BitmapInfo,
    pub read_size: u32,
}

#[derive(Debug)]
pub struct BitmapFileHeader {
    pub bf_type: u16,
    pub bf_size: u32,
    pub bf_reserved1: u16,
    pub bf_reserved2: u16,
    pub bf_offbits: u32,
}

#[allow(unused)]
#[derive(Debug)]
pub struct BitmapWindowsInfo {
    pub bi_size: u32,
    pub bi_width: u32,
    pub bi_height: u32,
    pub bi_plane: u16,
    pub bi_bit_count: u16,
    pub bi_compression: u32,
    pub bi_size_image: u32,
    pub bi_xpels_per_meter: u32,
    pub bi_ypels_per_meter: u32,
    pub bi_clr_used: u32,
    pub bi_clr_importation: u32,
    pub b_v4_header: Option<BitmapInfoV4>,
    pub b_v5_header: Option<BitmapInfoV5>,
}

#[derive(Debug)]
pub struct BitmapCore {
    pub bc_size: u32,
    pub bc_width: u16,
    pub bc_height: u16,
    pub bc_plane: u16,
    pub bc_bit_count: u16,
}

#[derive(Debug)]
pub struct CieXYZ {
    pub ciexyz_x: u32,
    pub ciexyz_y: u32,
    pub ciexyz_z: u32,
}

#[derive(Debug)]
pub struct CieXYZTriple {
    pub ciexyz_red: CieXYZ,
    pub ciexyz_green: CieXYZ,
    pub ciexyz_blue: CieXYZ,
}

#[derive(Debug)]
pub struct BitmapInfoV4 {
    pub b_v4_red_mask: u32,
    pub b_v4_green_mask: u32,
    pub b_v4_blue_mask: u32,
    pub b_v4_alpha_mask: u32,
    pub b_v4_cstype: u32,
    pub b_v4_endpoints: Option<CieXYZTriple>,
    pub b_v4_gamma_red: u32,
    pub b_v4_gamma_green: u32,
    pub b_v4_gamma_blue: u32,
}

#[derive(Debug)]
pub struct BitmapInfoV5 {
    pub b_v5_intent: u32,
    pub b_v5_profile_data: u32,
    pub b_v5_profile_size: u32,
    pub b_v5_reserved: u32,
}

#[derive(Debug)]
pub enum BitmapInfo {
    Windows(BitmapWindowsInfo),
    Os2(BitmapCore),
}

#[derive(Debug, Clone, Copy)]
pub struct ColorTable {
    pub blue: u8,
    pub green: u8,
    pub red: u8,
    pub reserved: u8,
}

#[derive(Debug)]
pub enum Compressions {
    BiRGB = 0,
    BiRLE8 = 1,
    BiRLE4 = 2,
    BiBitFileds = 3,
    BiJpeg = 4,
    BiPng = 5,
}

impl BitmapHeader {
    pub fn new<B: BinaryReader>(reader: &mut B, _opt: usize) -> Result<Self, Error> {
        let mut read_size;
        let b = reader.read_byte()?;
        let m = reader.read_byte()?;

        if b != b'B' || m != b'M' {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::UnknownFormat,
                "Not Bitmap".to_string(),
            )));
        }

        let bitmap_file_header = BitmapFileHeader {
            bf_type: (b as u16) | (m as u16) << 8,
            bf_size: reader.read_u32_le()?,
            bf_reserved1: reader.read_u16_le()?,
            bf_reserved2: reader.read_u16_le()?,
            bf_offbits: reader.read_u32_le()?,
        };

        read_size = 14;
        let bi_size = reader.read_u32_le()?;
        read_size += 4;
        let width;
        let height;
        let bit_per_count;
        let bitmap_info;
        let compression;
        let mut clut_size;
        let clut_offset;
        match bi_size {
            12 | 40 | 52 | 56 | 108 | 124 => {}
            _ => {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::DecodeError,
                    "Broken Header Bitmap".to_string(),
                )));
            }
        }

        if bi_size == 12 {
            let os2header = BitmapCore {
                bc_size: bi_size,
                bc_width: reader.read_u16_le()?,
                bc_height: reader.read_u16_le()?,
                bc_plane: reader.read_u16_le()?,
                bc_bit_count: reader.read_u16_le()?,
            };

            read_size += 12 - 4;
            width = os2header.bc_width as i32;
            height = os2header.bc_height as i32;
            compression = Some(Compressions::BiRGB);
            bit_per_count = os2header.bc_bit_count as usize;
            clut_size = 0;
            clut_offset = 12 + 14;
            bitmap_info = BitmapInfo::Os2(os2header);
        } else {
            let mut info_header = BitmapWindowsInfo {
                bi_size,
                bi_width: reader.read_u32_le()?,
                bi_height: reader.read_u32_le()?,
                bi_plane: reader.read_u16_le()?,
                bi_bit_count: reader.read_u16_le()?,
                bi_compression: reader.read_u32_le()?,
                bi_size_image: reader.read_u32_le()?,
                bi_xpels_per_meter: reader.read_u32_le()?,
                bi_ypels_per_meter: reader.read_u32_le()?,
                bi_clr_used: reader.read_u32_le()?,
                bi_clr_importation: reader.read_u32_le()?, //40
                b_v4_header: None,
                b_v5_header: None,
            };

            read_size += 40 - 4;
            bit_per_count = info_header.bi_bit_count as usize;
            clut_size = if bit_per_count <= 8 {
                info_header.bi_clr_used as usize
            } else {
                0
            };
            let clut_bytes = u32::try_from(clut_size)
                .ok()
                .and_then(|size| size.checked_mul(4))
                .ok_or_else(|| {
                    Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        "Broken palette Bitmap".to_string(),
                    )) as Error
                })?;
            let header_size = bitmap_file_header
                .bf_offbits
                .checked_sub(14)
                .and_then(|offset| offset.checked_sub(clut_bytes))
                .ok_or_else(|| {
                    Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        "Broken header Bitmap".to_string(),
                    )) as Error
                })?; // issue image header size 40 but required 56
            clut_offset = info_header.bi_size as u64 + 14; // same too
            width = info_header.bi_width as i32;
            height = info_header.bi_height as i32;

            let b_v4_header = if header_size >= 52 {
                // V4
                let b_v4_red_mask = reader.read_u32_le()?;
                let b_v4_green_mask = reader.read_u32_le()?;
                let b_v4_blue_mask = reader.read_u32_le()?;
                let mut b_v4_alpha_mask = 0;
                let mut b_v4_cstype = 0;
                let mut b_v4_endpoints: Option<CieXYZTriple> = None;
                let mut b_v4_gamma_red = 0;
                let mut b_v4_gamma_green = 0;
                let mut b_v4_gamma_blue = 0;
                read_size += 12;

                if header_size >= 56 {
                    b_v4_alpha_mask = reader.read_u32_le()?;
                    read_size += 4;
                }
                if header_size >= 60 {
                    b_v4_cstype = reader.read_u32_le()?;
                    read_size += 4;
                }
                if header_size >= 96 {
                    b_v4_endpoints = Some(CieXYZTriple {
                        ciexyz_red: CieXYZ {
                            ciexyz_x: reader.read_u32_le()?,
                            ciexyz_y: reader.read_u32_le()?,
                            ciexyz_z: reader.read_u32_le()?,
                        },
                        ciexyz_green: CieXYZ {
                            ciexyz_x: reader.read_u32_le()?,
                            ciexyz_y: reader.read_u32_le()?,
                            ciexyz_z: reader.read_u32_le()?,
                        },
                        ciexyz_blue: CieXYZ {
                            ciexyz_x: reader.read_u32_le()?,
                            ciexyz_y: reader.read_u32_le()?,
                            ciexyz_z: reader.read_u32_le()?,
                        },
                    });
                    read_size += 36;
                }
                if header_size >= 108 {
                    b_v4_gamma_red = reader.read_u32_le()?;
                    b_v4_gamma_green = reader.read_u32_le()?;
                    b_v4_gamma_blue = reader.read_u32_le()?; //108
                    read_size += 12;
                }

                Some(BitmapInfoV4 {
                    b_v4_red_mask,
                    b_v4_green_mask,
                    b_v4_blue_mask,
                    b_v4_alpha_mask,
                    b_v4_cstype,
                    b_v4_endpoints,
                    b_v4_gamma_red,
                    b_v4_gamma_green,
                    b_v4_gamma_blue,
                })
            } else {
                None
            };

            let b_v5_header = if header_size >= 112 {
                let b_v5_intent = reader.read_u32_le()?; // 112
                let (b_v5_profile_data, b_v5_profile_size) = if header_size >= 120 {
                    (reader.read_u32_le()?, reader.read_u32_le()?)
                } else {
                    (0, 0)
                }; //120
                let b_v5_reserved = if header_size >= 124 {
                    reader.read_u32_le()?
                } else {
                    0
                };
                Some(BitmapInfoV5 {
                    b_v5_intent,
                    b_v5_profile_data,
                    b_v5_profile_size,
                    b_v5_reserved,
                })
            } else {
                None
            };

            info_header.b_v4_header = b_v4_header;
            info_header.b_v5_header = b_v5_header;

            compression = match info_header.bi_compression {
                0 => Some(Compressions::BiRGB),
                1 => Some(Compressions::BiRLE8),
                2 => Some(Compressions::BiRLE4),
                3 => Some(Compressions::BiBitFileds),
                4 => Some(Compressions::BiJpeg),
                5 => Some(Compressions::BiPng),
                _ => None,
            };

            bitmap_info = BitmapInfo::Windows(info_header);
        }
        if clut_size == 0 && bit_per_count <= 8 && bit_per_count > 0 {
            clut_size = 1 << bit_per_count;
        }

        if clut_size > 65536 {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::UnsupportedFeature,
                "Clut Size too large".to_string(),
            )));
        }

        let mut color_table: Vec<ColorTable> = Vec::with_capacity(clut_size);

        if clut_size > 0 {
            if clut_offset >= bitmap_file_header.bf_offbits as u64 {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::DecodeError,
                    "Broken pallet Bitmap".to_string(),
                )));
            }
            reader.seek(SeekFrom::Start(clut_offset))?;
            for _ in 0..clut_size {
                match bitmap_info {
                    BitmapInfo::Windows(..) => {
                        color_table.push(ColorTable {
                            blue: reader.read_byte()?,
                            green: reader.read_byte()?,
                            red: reader.read_byte()?,
                            reserved: reader.read_byte()?,
                        });
                        read_size += 4;
                    }
                    BitmapInfo::Os2(..) => {
                        color_table.push(ColorTable {
                            blue: reader.read_byte()?,
                            green: reader.read_byte()?,
                            red: reader.read_byte()?,
                            reserved: 0,
                        });
                        read_size += 3;
                    }
                }
            }
        }

        let color_table = if !color_table.is_empty() {
            Some(color_table)
        } else {
            None
        };

        Ok(BitmapHeader {
            filesize: bitmap_file_header.bf_size as usize,
            image_offset: bitmap_file_header.bf_offbits as usize,
            width,
            height,
            bit_count: bit_per_count,
            compression,
            color_table,
            bitmap_file_header,
            bitmap_info,
            read_size,
        })
    }
}
