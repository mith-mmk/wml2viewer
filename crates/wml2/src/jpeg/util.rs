//! JPEG utility helpers.

/*
 * jpeg/util.rs  Mith@mmk (C) 2022
 * use MIT License
 */

// type Error = Box<dyn std::error::Error>;
use super::header::JpegAppHeaders::*;
use super::header::JpegHaeder;
use crate::jpeg::header::HuffmanTable;
use crate::jpeg::header::HuffmanTables;
use crate::metadata::DataMap;
use std::collections::HashMap;

#[allow(unused)]
pub(crate) static ZIG_ZAG_SEQUENCE: [usize; 64] = [
    0, 1, 5, 6, 14, 15, 27, 28, 2, 4, 7, 13, 16, 26, 29, 42, 3, 8, 12, 17, 25, 30, 41, 43, 9, 11,
    18, 24, 31, 40, 44, 53, 10, 19, 23, 32, 39, 45, 52, 54, 20, 22, 33, 38, 46, 51, 55, 60, 21, 34,
    37, 47, 50, 56, 59, 61, 35, 36, 48, 49, 57, 58, 62, 63,
];

/*
pub(crate) static UN_ZIG_ZAG_SEQUENCE:[usize;64] = [
      0,  1,  8, 16,  9,  2,  3, 10,
      17, 24, 32, 25, 18, 11,  4,  5,
      12, 19, 26, 33, 40, 48, 41, 34,
      27, 20, 13,  6,  7, 14, 21, 28,
      35, 42, 49, 56, 57, 50, 43, 36,
      29, 22, 15, 23, 30, 37, 44, 51,
      58, 59, 52, 45, 38, 31, 39, 46,
      53, 60, 61, 54, 47, 55, 62, 63,
  ];
*/

/*
  option
    0x01 = minimam
  & 0x02 = Huffman Table
  & 0x04 = Huffman Table Extract
  & 0x08 = Quantitation table
  & 0x10 = Exif

*/

pub fn print_header(header: &JpegHaeder, option: usize) -> Box<String> {
    let mut str: String = "JPEG\n".to_string();
    match &header.frame_header {
        Some(fh) => {
            str = str
                + &format!(
                    "SOF\nBaseline {} Progressive {} Huffman {} Diffelensial {} Sequensial {} Lossless {} \n",
                    fh.is_baseline,
                    fh.is_progressive,
                    fh.is_huffman,
                    fh.is_differential,
                    fh.is_sequential,
                    fh.is_lossress
                );

            str = str
                + &format!(
                    "Width {} Height {} {} x {}bit\n",
                    header.width, header.height, fh.plane, fh.bitperpixel
                );
            match &fh.component {
                Some(component) => {
                    str = str + &format!("Nf {}\n", component.len());
                    for c in component.iter() {
                        str = str
                            + &format!("ID {} h{} x v{} Quatize Table #{}\n", c.c, c.h, c.v, c.tq);
                    }
                }
                _ => {}
            }
        }
        _ => {
            str += "SOF is nothing!";
        }
    }

    match &header.huffman_scan_header {
        Some(sos) => {
            str = str + &format!("\nSOS\n {} ", sos.ns);
            for i in 0..sos.ns {
                str = str + &format!("Cs{} Td{} Ta{} ", sos.csn[i], sos.tdcn[i], sos.tacn[i]);
            }
            str = str + &format!("Ss {} Se {} Ah {} Al {}\n", sos.ss, sos.se, sos.ah, sos.al);
        }
        _ => {
            str += "SOS is nothing!";
        }
    }

    match &header.comment {
        Some(comment) => {
            str = str + "\nCOM\n" + comment + "\n";
        }
        _ => {}
    }

    match &header.jpeg_app_headers {
        Some(app_headers) => {
            for app in app_headers.iter() {
                match app {
                    Jfif(jfif) => {
                        let unit = match jfif.resolusion_unit {
                            0 => "",
                            1 => "dpi",
                            2 => "dpi",
                            _ => "N/A",
                        };

                        str = str
                            + &format!(
                                "JFIF Ver{:>03x} Resilution Unit X{}{} Y{}{} Thumnail {} {}\n",
                                jfif.version,
                                jfif.x_resolusion,
                                unit,
                                jfif.y_resolusion,
                                unit,
                                jfif.width,
                                jfif.height
                            );
                    }
                    Exif(exif) => {
                        if option & 0x10 == 0x10 {
                            // Exif tag full display flag
                            str += "Exif\n";
                            str = str + &crate::tiff::util::print_tags(exif);
                        } else {
                            str = str + &format!("Exif has{} tags\n", exif.headers.len());
                        }
                    }
                    Ducky(ducky) => {
                        str = str
                            + &format!(
                                "Ducky .. unknow format {} {} {}\n",
                                ducky.quality, ducky.comment, ducky.copyright
                            );
                    }
                    Adobe(adobe) => {
                        str = str
                            + &format!(
                                "Adobe App14 DCTEncodeVersion:{} Flag1:{} Flag2:{} ColorTransform {} {}\n",
                                adobe.dct_encode_version,
                                adobe.flag1,
                                adobe.flag2,
                                adobe.color_transform,
                                match adobe.color_transform {
                                    1 => {
                                        "YCbCr"
                                    }
                                    2 => {
                                        "YCCK"
                                    }
                                    _ => {
                                        "Unknown"
                                    }
                                }
                            );
                    }
                    ICCProfile(icc_profile) => {
                        str = str
                            + &format!(
                                "ICC Profile {} of {}\n",
                                icc_profile.number, icc_profile.total
                            );
                    }
                    Xmp((tag, string)) => {
                        str = str + tag + ":\n" + string + "\n";
                    }
                    Unknown(app) => {
                        str = str
                            + &format!(
                                "App{} {} {}bytes is unknown\n",
                                app.number, app.tag, app.length
                            );
                    }
                }
            }
        }
        _ => {}
    }

    if header.interval > 0 {
        //DRI
        str = str + &format!("Restart Interval {}\n", header.interval);
    }

    if header.line > 0 {
        //DNL
        str = str + &format!("Define number of lines {}\n", header.line);
    }

    if option & 0x08 == 0x08 {
        // Define Quatization Table Flags
        match &(header.quantization_tables) {
            Some(qts) => {
                str += "DQT\n";
                for qt in qts.iter() {
                    str = str + &format!("Pq{}(bytes) Tq{}\n", qt.presision + 1, qt.no);
                    for (i, q) in qt.q.iter().enumerate() {
                        str = str + &format!("{:3}", q);
                        if i % 8 == 7 { str += "\n" } else { str += "," }
                    }
                }
            }
            _ => {
                str += "DQT in nothing!\n\n";
            }
        }
    }

    if option & 0x02 == 0x02 {
        // Define Huffman Table Flags
        let hts = &header.huffman_tables;
        str += "DHT\n";
        for ht in hts.dc_tables.iter() {
            if let Some(ht) = ht {
                str = str + &format!("DC Table{}\n", ht.no);
                str += "L ";
                for l in ht.len.iter() {
                    str = str + &l.to_string() + " ";
                }
                str += "\n V ";
                for v in ht.val.iter() {
                    str = str + &v.to_string() + " ";
                }
                str += "\n";
            }
        }
        for ht in hts.ac_tables.iter() {
            if let Some(ht) = ht {
                str = str + &format!("AC Table{}\n", ht.no);
                str += "L ";
                for l in ht.len.iter() {
                    str = str + &l.to_string() + " ";
                }
                str += "\n V ";
                for v in ht.val.iter() {
                    str = str + &v.to_string() + " ";
                }
                str += "\n";
            }
        }
    }

    if option & 0x04 == 0x04 {
        // Define Huffman Table Decoded
        str += &print_huffman_tables(&header.huffman_tables);
    }

    let strbox: Box<String> = Box::new(str);

    strbox
}

pub(crate) fn print_huffman_tables(huffman_tables: &HuffmanTables) -> String {
    let hts = huffman_tables;
    let mut str = "".to_string();
    for (i, huffman_table) in hts.dc_tables.iter().enumerate() {
        if let Some(huffman_table) = huffman_table {
            str += &print_huffman(i, huffman_table);
        }
    }
    for (i, huffman_table) in hts.ac_tables.iter().enumerate() {
        if let Some(huffman_table) = huffman_table {
            str += &print_huffman(i, huffman_table);
        }
    }
    str
}

fn print_huffman(i: usize, huffman_table: &HuffmanTable) -> String {
    let mut str = "".to_string();

    if huffman_table.ac {
        str = str + &format!("Huffman Table AC {}\n", i);
    } else {
        str = str + &format!("Huffman Table DC {}\n", i);
    }
    let mut code: i32 = 0;
    let mut pos: usize = 0;
    for l in 0..16 {
        if huffman_table.len[l] != 0 {
            for _ in 0..huffman_table.len[l] {
                if pos >= huffman_table.val.len() {
                    break;
                }
                str = str + &format!("{:>02b}  {:>02x}\n", code, huffman_table.val[pos]);
                pos += 1;
                code += 1;
            }
        }
        code <<= 1;
    }

    str
}

pub fn make_metadata(header: &JpegHaeder) -> HashMap<String, DataMap> {
    let mut map: HashMap<String, DataMap> = HashMap::new();
    map.insert("Format".to_string(), DataMap::Ascii("JPEG".to_string()));
    if let Some(fh) = &header.frame_header {
        if fh.is_baseline {
            map.insert(
                "Jpeg DCT process".to_string(),
                DataMap::Ascii("Baseline".to_string()),
            );
        } else {
            map.insert(
                "Jpeg DCT process".to_string(),
                DataMap::Ascii("Extended".to_string()),
            );
        }
        if fh.is_progressive {
            map.insert(
                "Jpeg frame order".to_string(),
                DataMap::Ascii("Progressive".to_string()),
            );
        } else {
            map.insert(
                "Jpeg frame order".to_string(),
                DataMap::Ascii("Sequential".to_string()),
            );
        }
        if fh.is_huffman {
            map.insert(
                "Jpeg coding".to_string(),
                DataMap::Ascii("Huffman".to_string()),
            );
        } else {
            map.insert(
                "Jpeg coding".to_string(),
                DataMap::Ascii("Arithmetic".to_string()),
            );
        }
        if fh.is_differential {
            map.insert(
                "Jpeg differential".to_string(),
                DataMap::Ascii("Differential".to_string()),
            );
        }
        if fh.is_lossress {
            map.insert(
                "Jpeg DCT process".to_string(),
                DataMap::Ascii("Lossress".to_string()),
            );
        }
        map.insert("width".to_string(), DataMap::UInt(fh.width as u64));
        map.insert("height".to_string(), DataMap::UInt(fh.height as u64));
        map.insert(
            "Bits per pixel".to_string(),
            DataMap::UInt(fh.bitperpixel as u64),
        );
        map.insert(
            "Color Space".to_string(),
            DataMap::Ascii(fh.color_space.to_string()),
        );
    }

    if let Some(comment) = &header.comment {
        map.insert("comment".to_string(), DataMap::Ascii(comment.to_string()));
    }

    if let Some(icc_profile) = &header.icc_profile {
        map.insert(
            "ICC Profile".to_string(),
            DataMap::ICCProfile(icc_profile.to_vec()),
        );
    }
    match &header.jpeg_app_headers {
        Some(app_headers) => {
            #[cfg(feature = "c2pa")]
            {
                let mut c2pa_manifest_store = Vec::new();
                for app in app_headers.iter() {
                    if let Unknown(app) = app {
                        if app.number == 11 {
                            if let Some(mut fragment) =
                                crate::metadata::c2pa::jpeg_app11_payload_to_manifest_store(
                                    &app.raw,
                                )
                            {
                                c2pa_manifest_store.append(&mut fragment);
                            }
                        }
                    }
                }
                if !c2pa_manifest_store.is_empty() {
                    crate::metadata::c2pa::insert_metadata(
                        &mut map,
                        "jpeg-app11",
                        &c2pa_manifest_store,
                    );
                }
            }

            for app in app_headers.iter() {
                match app {
                    Jfif(jfif) => {
                        let unit = match jfif.resolusion_unit {
                            0 => "",
                            1 => "dpi",
                            2 => "dpi",
                            _ => "N/A",
                        };

                        let str = format!(
                            "JFIF Ver{:>03x} Resilution Unit X{}{} Y{}{} Thumnail {} {}\n",
                            jfif.version,
                            jfif.x_resolusion,
                            unit,
                            jfif.y_resolusion,
                            unit,
                            jfif.width,
                            jfif.height
                        );
                        map.insert("JFIF".to_string(), DataMap::Ascii(str));
                    }
                    Exif(exif) => {
                        map.insert("EXIF".to_string(), DataMap::Exif(exif.clone()));
                    }
                    Ducky(ducky) => {
                        let str = format!(
                            "Ducky .. unknow format {} {} {}\n",
                            ducky.quality, ducky.comment, ducky.copyright
                        );
                        map.insert("Ducky".to_string(), DataMap::Ascii(str));
                    }
                    Adobe(adobe) => {
                        let str = format!(
                            "Adobe App14 DCTEncode Version:{} Flag1:{} Flag2:{} ColorTransform {} {}\n",
                            adobe.dct_encode_version,
                            adobe.flag1,
                            adobe.flag2,
                            adobe.color_transform,
                            match adobe.color_transform {
                                1 => {
                                    "YCbCr"
                                }
                                2 => {
                                    "YCCK"
                                }
                                _ => {
                                    "Unknown"
                                }
                            }
                        );
                        map.insert("Adobe".to_string(), DataMap::Ascii(str));
                    }
                    Xmp((tag, string)) => {
                        map.insert(tag.to_string(), DataMap::Ascii(string.to_string()));
                    }

                    Unknown(app) => {
                        let key = format!("App{} {}", app.number, app.tag);
                        map.insert(key, DataMap::Raw(app.raw.clone()));
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    map
}
