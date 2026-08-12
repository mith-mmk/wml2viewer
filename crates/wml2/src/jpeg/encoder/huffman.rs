//! Huffman table construction for JPEG encoding.

use super::bitwriter::BitWriter;

type Error = Box<dyn std::error::Error>;

const EOB: u8 = 0x00;
const ZRL: u8 = 0xf0;

const BITS_DC_LUMA: [usize; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const BITS_AC_LUMA: [usize; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 125];
const BITS_DC_CHROMA: [usize; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const BITS_AC_CHROMA: [usize; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 119];

const VAL_DC: [usize; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

const VAL_AC_LUMA: [usize; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7,
    0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5,
    0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
    0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];

const VAL_AC_CHROMA: [usize; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
    0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xA1, 0xB1, 0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0,
    0x15, 0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34, 0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19, 0x1A, 0x26,
    0x27, 0x28, 0x29, 0x2A, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
    0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3,
    0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA,
    0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];

#[derive(std::cmp::PartialEq, Debug)]
pub(crate) struct HuffmanWriteTable {
    pub val: Vec<(usize, usize)>,
}

pub(crate) struct HuffmanWriteTables {
    pub lum_dc: HuffmanWriteTable,
    pub lum_ac: HuffmanWriteTable,
    pub chrom_dc: HuffmanWriteTable,
    pub chrom_ac: HuffmanWriteTable,
}

#[derive(std::cmp::PartialEq, Debug)]
struct HuffmanTable {
    pub ac: bool,
    pub len: Vec<usize>,
    pub val: Vec<usize>,
}

fn default_huffman_tables() -> [HuffmanTable; 4] {
    [
        HuffmanTable {
            ac: false,
            len: BITS_DC_LUMA.to_vec(),
            val: VAL_DC.to_vec(),
        },
        HuffmanTable {
            ac: true,
            len: BITS_AC_LUMA.to_vec(),
            val: VAL_AC_LUMA.to_vec(),
        },
        HuffmanTable {
            ac: false,
            len: BITS_DC_CHROMA.to_vec(),
            val: VAL_DC.to_vec(),
        },
        HuffmanTable {
            ac: true,
            len: BITS_AC_CHROMA.to_vec(),
            val: VAL_AC_CHROMA.to_vec(),
        },
    ]
}

fn expand_table(huffman_table: &HuffmanTable) -> HuffmanWriteTable {
    let max_value = if huffman_table.ac { 255 } else { 15 };
    let mut table = vec![(17, 0); max_value + 1];

    let mut prev_pos = 0;
    let mut code = 0;
    for l in 0..16 {
        let pos = prev_pos + huffman_table.len[l];
        for i in prev_pos..pos {
            table[huffman_table.val[i]] = (l + 1, code);
            code += 1;
        }
        code <<= 1;
        prev_pos = pos;
    }

    HuffmanWriteTable { val: table }
}

pub(crate) fn default_huffman_writer() -> HuffmanWriteTables {
    let [lum_dc, lum_ac, chrom_dc, chrom_ac] =
        default_huffman_tables().map(|table| expand_table(&table));
    HuffmanWriteTables {
        lum_dc,
        lum_ac,
        chrom_dc,
        chrom_ac,
    }
}

pub(crate) fn shrink(v: i32) -> (usize, u16) {
    if v == 0 {
        return (0, 0);
    }

    let abs = v.unsigned_abs() as usize;
    let size = usize::BITS as usize - abs.leading_zeros() as usize;
    let bits = if v > 0 {
        v as u16
    } else {
        ((1_i32 << size) - 1 + v) as u16
    };

    (size, bits)
}

pub(crate) fn huffman_write(
    bit_writer: &mut BitWriter,
    val: u8,
    table: &HuffmanWriteTable,
) -> Result<(), Error> {
    let val = val as usize;
    if val >= table.val.len() {
        let boxstr = format!("huffman_write is overflow val{}", val);
        return Err(Box::new(std::io::Error::other(boxstr)));
    }
    let (bits, i) = table.val[val];
    bit_writer.write_bits(i as u16, bits)?;
    Ok(())
}

pub(crate) fn encode_block(
    bit_writer: &mut BitWriter,
    block: &[i32; 64],
    pred: &mut i32,
    dc_table: &HuffmanWriteTable,
    ac_table: &HuffmanWriteTable,
) -> Result<(), Error> {
    let diff = block[0] - *pred;
    *pred = block[0];
    let (size, bits) = shrink(diff);
    huffman_write(bit_writer, size as u8, dc_table)?;
    bit_writer.write_bits(bits, size)?;

    let mut zero_run = 0usize;
    for &coeff in block.iter().skip(1) {
        if coeff == 0 {
            zero_run += 1;
            continue;
        }

        while zero_run >= 16 {
            huffman_write(bit_writer, ZRL, ac_table)?;
            zero_run -= 16;
        }

        let (size, bits) = shrink(coeff);
        let symbol = ((zero_run as u8) << 4) | (size as u8 & 0x0f);
        huffman_write(bit_writer, symbol, ac_table)?;
        bit_writer.write_bits(bits, size)?;
        zero_run = 0;
    }

    if zero_run > 0 {
        huffman_write(bit_writer, EOB, ac_table)?;
    }

    Ok(())
}

pub(crate) fn write_dht(buf: &mut Vec<u8>) {
    write_marker(buf, 0xc4);
    write_u16_be(buf, 0x01a2);

    write_huffman_segment(buf, 0x00, &BITS_DC_LUMA, &VAL_DC);
    write_huffman_segment(buf, 0x10, &BITS_AC_LUMA, &VAL_AC_LUMA);
    write_huffman_segment(buf, 0x01, &BITS_DC_CHROMA, &VAL_DC);
    write_huffman_segment(buf, 0x11, &BITS_AC_CHROMA, &VAL_AC_CHROMA);
}

fn write_huffman_segment(buf: &mut Vec<u8>, table_id: u8, bits: &[usize; 16], values: &[usize]) {
    buf.push(table_id);
    for &len in bits {
        buf.push(len as u8);
    }
    for &value in values {
        buf.push(value as u8);
    }
}

fn write_marker(buf: &mut Vec<u8>, marker: u8) {
    buf.push(0xff);
    buf.push(marker);
}

fn write_u16_be(buf: &mut Vec<u8>, value: u16) {
    buf.push((value >> 8) as u8);
    buf.push((value & 0xff) as u8);
}
