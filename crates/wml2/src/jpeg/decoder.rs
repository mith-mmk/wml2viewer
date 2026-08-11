//! JPEG decoder implementation.

/*
 * jpeg/decoder.rs  Mith@mmk (C) 2022
 * use MIT License
 */
type Error = Box<dyn std::error::Error>;
use crate::draw::*;
use crate::error::*;
use crate::jpeg::header::*;
use crate::jpeg::progressive::decode_progressive;
use crate::jpeg::util::make_metadata;
use crate::jpeg::util::print_header;
use crate::jpeg::warning::*;
use crate::warning::*;
use bin_rs::reader::BinaryReader;
#[cfg(feature = "idct_slower")]
use std::f32::consts::PI;

#[cfg(any(
    all(feature = "idct_llm", feature = "idct_aan"),
    all(feature = "idct_llm", feature = "idct_slower"),
    all(feature = "idct_aan", feature = "idct_slower"),
))]
compile_error!("Choose only one JPEG IDCT feature: idct_llm, idct_aan, or idct_slower");

#[cfg(not(any(feature = "idct_llm", feature = "idct_aan", feature = "idct_slower")))]
compile_error!("Enable one JPEG IDCT feature: idct_llm, idct_aan, or idct_slower");

#[cfg(any(
    all(feature = "idct_llm", feature = "idct_aan"),
    all(feature = "idct_llm", feature = "idct_slower"),
    all(feature = "idct_aan", feature = "idct_slower"),
    not(any(feature = "idct_llm", feature = "idct_aan", feature = "idct_slower"))
))]
pub(crate) fn idct(_: &[i32]) -> Vec<u8> {
    unreachable!("JPEG IDCT feature selection is invalid")
}

pub(crate) struct BitReader<'decode, B> {
    pub reader: &'decode mut B,
    pub(crate) bptr: usize,
    prev_rst: usize,
    pub(crate) b: u8,
    rst: bool,
}

#[allow(unused)]
impl<'decode, B: BinaryReader> BitReader<'decode, B> {
    pub fn new(reader: &'decode mut B) -> Self {
        let bptr: usize = 0;
        let b: u8 = 0;
        Self {
            reader,
            bptr,
            b,
            rst: false,
            prev_rst: 7,
        }
    }

    pub fn rst(&mut self) -> Result<bool, Error> {
        Ok(self.rst)
    }

    pub fn next_marker(&mut self) -> Result<u8, Error> {
        let buf = self.reader.read_bytes_no_move(2)?;
        if buf[0] != 0xff {
            let s = format!(
                "Nothing marker but {:02x} offset:{:08x}",
                buf[0],
                self.reader.offset()?
            );
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::DecodeError,
                s.to_string(),
            )));
        }
        self.reader.read_byte()?;
        loop {
            let b = self.reader.read_byte()?;
            if b != 0xff {
                self.b = 0;
                self.bptr = 0;
                return Ok(b);
            }
        }
    }

    #[inline]
    pub fn next_byte(&mut self) -> Result<u8, Error> {
        let mut b = self.reader.read_byte()?;
        if b == 0xff {
            let mut marker = self.reader.read_byte()?;
            while marker == 0xff {
                marker = self.reader.read_byte()?;
            }
            match marker {
                0x00 => {
                    b = 0xff;
                }
                0xd0..=0xd7 => {
                    // RST
                    let rst_no = (b & 0x7) as usize;
                    if rst_no != (self.prev_rst + 1) % 8 {
                        return Err(Box::new(ImgError::new_const(
                            ImgErrorKind::DecodeError,
                            "No Interval RST".to_string(),
                        )));
                    }
                    self.prev_rst = rst_no;
                    self.rst = true;
                    return self.next_byte();
                }
                0xd9 => {
                    // EOI
                    let boxstr = format!(
                        "FF after 00 or RST {:02x} offset {:08x}",
                        marker,
                        self.reader.offset()?
                    );
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        boxstr,
                    )));
                }
                _ => {
                    let boxstr = format!(
                        "FF after 00 or RST {:02x} offset {:08x}",
                        marker,
                        self.reader.offset()?
                    );
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        boxstr,
                    )));
                }
            }
        }
        Ok(b)
    }

    #[inline]
    pub fn get_bit(&mut self) -> Result<usize, Error> {
        if self.bptr == 0 {
            self.b = self.next_byte()?;
            self.bptr = 8;
        }
        self.bptr -= 1;
        let r = (self.b >> self.bptr) as usize & 0x1;
        Ok(r)
    }

    #[inline]
    pub fn get_bits(&mut self, mut bits: usize) -> Result<i32, Error> {
        if bits == 0 {
            return Ok(0);
        }
        if self.bptr == 0 {
            self.b = self.next_byte()?;
            self.bptr = 8;
        }
        let mut v = 0_i32;

        loop {
            if bits > self.bptr {
                v = (v << self.bptr) | (self.b as i32 & ((1 << self.bptr) - 1));
                bits -= self.bptr;
                self.b = self.next_byte()?;
                self.bptr = 8;
            } else {
                self.bptr -= bits;
                v = (v << bits) | (self.b as i32 >> self.bptr) & ((1 << bits) - 1);
                break;
            }
        }
        Ok(v)
    }

    pub fn reset(&mut self) {
        self.bptr = 0;
        self.b = 0;
    }
}

#[inline]
pub(crate) fn huffman_read<B: BinaryReader>(
    bit_reader: &mut BitReader<B>,
    table: &HuffmanDecodeTable,
) -> Result<u32, Error> {
    let mut d = 0;
    for l in 0..16 {
        d = (d << 1) | bit_reader.get_bit()?;
        if table.max[l] >= d as i32 {
            let p = d - table.min[l] as usize + table.pos[l];
            return Ok(table.val[p] as u32);
        }
    }
    let boxstr = format!(
        "huffman_read is overflow val{} offset {:08x}",
        d,
        bit_reader.reader.offset()?
    );
    Err(Box::new(ImgError::new_const(
        ImgErrorKind::OutboundIndex,
        boxstr,
    )))
}

#[derive(std::cmp::PartialEq, Debug)]
pub(crate) struct HuffmanDecodeTable {
    pub(crate) pos: Vec<usize>,
    pub(crate) val: Vec<usize>,
    pub(crate) min: Vec<i32>,
    pub(crate) max: Vec<i32>,
}

#[inline]
pub(crate) fn dc_read<B: BinaryReader>(
    bitread: &mut BitReader<B>,
    dc_decode: &HuffmanDecodeTable,
    pred: i32,
) -> Result<i32, Error> {
    let ssss = huffman_read(bitread, dc_decode)?;
    let v = bitread.get_bits(ssss as usize)?;
    let diff = extend(v, ssss as usize);
    let dc = diff + pred;
    Ok(dc)
}

#[inline] // for base line huffman
pub(crate) fn ac_read<B: BinaryReader>(
    bitread: &mut BitReader<B>,
    ac_decode: &HuffmanDecodeTable,
) -> Result<Vec<i32>, Error> {
    let mut zigzag: usize = 1;
    let mut zz = [0_i32; 64];
    loop {
        // F2.2.2
        let ac = huffman_read(bitread, ac_decode)?;

        let ssss = ac & 0xf;
        let rrrr = ac >> 4;
        if ssss == 0 {
            if ac == 0x00 {
                //EOB
                return Ok(zz.to_vec());
            }
            if rrrr == 15 {
                //ZRL
                zigzag += 16;
                continue;
            }
            return Ok(zz.to_vec()); // N/A
        } else {
            zigzag += rrrr as usize;
            let v = bitread.get_bits(ssss as usize)?;
            let z = extend(v, ssss as usize);
            if zigzag <= 63 {
                zz[zigzag] = z;
            }
        }
        if zigzag >= 63 {
            return Ok(zz.to_vec());
        }
        zigzag += 1;
    }
}

#[inline]
pub(crate) fn baseline_read<B: BinaryReader>(
    bitread: &mut BitReader<B>,
    dc_decode: &HuffmanDecodeTable,
    ac_decode: &HuffmanDecodeTable,
    pred: i32,
) -> Result<(Vec<i32>, i32), Error> {
    let dc = dc_read(bitread, dc_decode, pred)?;
    let mut zz = ac_read(bitread, ac_decode)?;
    zz[0] = dc;
    Ok((zz, dc))
}

#[inline]
pub(crate) fn extend(v: i32, t: usize) -> i32 {
    let mut v = v;
    if t == 0 {
        return v;
    }
    let mut vt = 1 << (t - 1);

    if v < vt {
        vt = (-1 << t) + 1;
        v += vt;
    }
    v
}

#[cfg(all(
    feature = "idct_slower",
    not(any(feature = "idct_llm", feature = "idct_aan"))
))]
pub(crate) fn idct(f: &[i32]) -> Vec<u8> {
    let vals: Vec<u8> = (0..64)
        .map(|i| {
            let (x, y) = ((i % 8) as f32, (i / 8) as f32);
            // IDCT from CCITT Rec. T.81 (1992 E) p.27 A3.3
            let mut val: f32 = 0.0;
            for u in 0..8 {
                let cu = if u == 0 { 1.0 / 2.0_f32.sqrt() } else { 1.0 };
                for v in 0..8 {
                    let cv = if v == 0 {
                        1.0_f32 / 2.0_f32.sqrt()
                    } else {
                        1.0
                    };
                    val += cu
                        * cv
                        * (f[v * 8 + u] as f32)
                        * ((2.0 * x + 1.0) * u as f32 * PI / 16.0_f32).cos()
                        * ((2.0 * y + 1.0) * v as f32 * PI / 16.0_f32).cos();
                }
            }
            val = val / 4.0;

            // level shift from CCITT Rec. T.81 (1992 E) p.26 A3.1
            let v = val.round() as i32 + 128;
            v.clamp(0, 255) as u8
        })
        .collect();
    vals
}

#[cfg(all(
    feature = "idct_llm",
    not(any(feature = "idct_aan", feature = "idct_slower"))
))]
pub(crate) fn idct(f: &[i32]) -> Vec<u8> {
    let m1 = 0.5411961; // α ∁Ecos(3π/8)
    let m2 = 1.306_563; // β ∁Ecos(3π/8)
    let m3 = 1.414_213_5; // γ v2
    let m4 = 0.831_469_6; // η cos(3π/16)
    let m5 = 0.555_570_24; // θ sin(3π/16)
    let m6 = 0.98078528; // δ cos(π/16)
    let m7 = 0.195_090_32; // ε sin(π/16)
    let m0 = 0.125; // ∁E/4 * ∁E/4

    let mut ff = [0_f32; 64];
    for j in 0..8 {
        let i = j * 8;
        let f0 = f[i] as f32; // X0
        let f1 = f[1 + i] as f32; // X1
        let f2 = f[2 + i] as f32; // X2
        let f3 = f[3 + i] as f32; // X3
        let f4 = f[4 + i] as f32; // X4
        let f6 = f[6 + i] as f32; // X5
        let f5 = f[5 + i] as f32; // X6
        let f7 = f[7 + i] as f32; // X7

        // implement batterfly mutilply

        // even part
        // part 2

        let y0 = f0 + f4;
        let y1 = f0 - f4;
        let y2 = m1 * f2 - m2 * f6;
        let y3 = m1 * f6 + m2 * f2;

        // part3

        let x0 = y0 + y3;
        let x1 = y1 + y2;
        let x2 = y1 - y2;
        let x3 = y0 - y3;

        // odds part

        // part 1
        let z4 = f1 - f7;
        let z5 = f3 * m3;
        let z6 = f5 * m3;
        let z7 = f1 + f7;

        // part 2
        let y4 = z4 + z6;
        let y5 = z7 - z5;
        let y6 = z4 - z6;
        let y7 = z7 + z5;

        // part 3
        let x4 = y4 * m4 - y7 * m5;
        let x5 = y5 * m6 - y6 * m7;
        let x6 = y6 * m6 + y5 * m7;
        let x7 = y7 * m4 + y4 * m5;

        // last part  multiply ∁E / 4 after parts

        ff[i] = x0 + x7; // x0
        ff[7 + i] = x0 - x7; // x1
        ff[1 + i] = x1 + x6; // x2
        ff[6 + i] = x1 - x6; // x3
        ff[2 + i] = x2 + x5; // x4
        ff[5 + i] = x2 - x5; // x5
        ff[3 + i] = x3 + x4; // x6
        ff[4 + i] = x3 - x4; // x7
    }
    for i in 0..8 {
        let f0 = ff[i];
        let f1 = ff[8 + i];
        let f2 = ff[2 * 8 + i];
        let f3 = ff[3 * 8 + i];
        let f4 = ff[4 * 8 + i];
        let f5 = ff[5 * 8 + i];
        let f6 = ff[6 * 8 + i];
        let f7 = ff[7 * 8 + i];

        // odds part
        // part 1

        // even part
        // part 2

        let y0 = f0 + f4;
        let y1 = f0 - f4;
        let y2 = m1 * f2 - m2 * f6;
        let y3 = m1 * f6 + m2 * f2;

        // part3

        let x0 = y0 + y3;
        let x1 = y1 + y2;
        let x2 = y1 - y2;
        let x3 = y0 - y3;

        // odds part
        // part 1
        let z4 = f1 - f7;
        let z5 = f3 * m3;
        let z6 = f5 * m3;
        let z7 = f1 + f7;

        // part 2
        let y4 = z4 + z6;
        let y5 = z7 - z5;
        let y6 = z4 - z6;
        let y7 = z7 + z5;

        let x4 = y4 * m4 - y7 * m5;
        let x5 = y5 * m6 - y6 * m7;
        let x6 = y6 * m6 + y5 * m7;
        let x7 = y7 * m4 + y4 * m5;

        ff[i] = (x0 + x7) * m0;
        ff[7 * 8 + i] = (x0 - x7) * m0;
        ff[8 + i] = (x1 + x6) * m0;
        ff[6 * 8 + i] = (x1 - x6) * m0;
        ff[2 * 8 + i] = (x2 + x5) * m0;
        ff[5 * 8 + i] = (x2 - x5) * m0;
        ff[3 * 8 + i] = (x3 + x4) * m0;
        ff[4 * 8 + i] = (x3 - x4) * m0;
    }

    let val = ff
        .iter()
        .map(|i| ((*i + 128.5) as i32).clamp(0, 255) as u8)
        .collect();
    val
}

#[inline]
// AAN algorythm
#[cfg(all(
    feature = "idct_aan",
    not(any(feature = "idct_llm", feature = "idct_slower"))
))]
pub(crate) fn idct(f: &[i32]) -> Vec<u8> {
    let mut _f = [0_f32; 64];
    let mut vals = [0_u8; 64];
    let m0 = 1.847759;
    let m1 = 1.4142135;
    let m3 = 1.4142135;
    let m5 = 0.76536685;
    let m2 = m0 - m5;
    let m4 = m0 + m5;

    let s0 = 0.35355338;
    let s1 = 0.49039263;
    let s2 = 0.46193975;
    let s3 = 0.4157348;
    let s4 = 0.35355338;
    let s5 = 0.2777851;
    let s6 = 0.19134171;
    let s7 = 0.09754512;

    for i in 0..8 {
        let g0 = f[0 * 8 + i] as f32 * s0;
        let g1 = f[4 * 8 + i] as f32 * s4;
        let g2 = f[2 * 8 + i] as f32 * s2;
        let g3 = f[6 * 8 + i] as f32 * s6;
        let g4 = f[5 * 8 + i] as f32 * s5;
        let g5 = f[1 * 8 + i] as f32 * s1;
        let g6 = f[7 * 8 + i] as f32 * s7;
        let g7 = f[3 * 8 + i] as f32 * s3;

        let f0 = g0;
        let f1 = g1;
        let f2 = g2;
        let f3 = g3;
        let f4 = g4 - g7;
        let f5 = g5 + g6;
        let f6 = g5 - g6;
        let f7 = g4 + g7;

        let e0 = f0;
        let e1 = f1;
        let e2 = f2 - f3;
        let e3 = f2 + f3;
        let e4 = f4;
        let e5 = f5 - f7;
        let e6 = f6;
        let e7 = f5 + f7;
        let e8 = f4 + f6;

        let d0 = e0;
        let d1 = e1;
        let d2 = e2 * m1;
        let d3 = e3;
        let d4 = e4 * m2;
        let d5 = e5 * m3;
        let d6 = e6 * m4;
        let d7 = e7;
        let d8 = e8 * m5;

        let c0 = d0 + d1;
        let c1 = d0 - d1;
        let c2 = d2 - d3;
        let c3 = d3;
        let c4 = d4 + d8;
        let c5 = d5 + d7;
        let c6 = d6 - d8;
        let c7 = d7;
        let c8 = c5 - c6;

        let b0 = c0 + c3;
        let b1 = c1 + c2;
        let b2 = c1 - c2;
        let b3 = c0 - c3;
        let b4 = c4 - c8;
        let b5 = c8;
        let b6 = c6 - c7;
        let b7 = c7;

        _f[0 * 8 + i] = b0 + b7;
        _f[1 * 8 + i] = b1 + b6;
        _f[2 * 8 + i] = b2 + b5;
        _f[3 * 8 + i] = b3 + b4;
        _f[4 * 8 + i] = b3 - b4;
        _f[5 * 8 + i] = b2 - b5;
        _f[6 * 8 + i] = b1 - b6;
        _f[7 * 8 + i] = b0 - b7;
    }

    for i in 0..8 {
        let g0 = _f[i * 8 + 0] as f32 * s0;
        let g1 = _f[i * 8 + 4] as f32 * s4;
        let g2 = _f[i * 8 + 2] as f32 * s2;
        let g3 = _f[i * 8 + 6] as f32 * s6;
        let g4 = _f[i * 8 + 5] as f32 * s5;
        let g5 = _f[i * 8 + 1] as f32 * s1;
        let g6 = _f[i * 8 + 7] as f32 * s7;
        let g7 = _f[i * 8 + 3] as f32 * s3;

        let f0 = g0;
        let f1 = g1;
        let f2 = g2;
        let f3 = g3;
        let f4 = g4 - g7;
        let f5 = g5 + g6;
        let f6 = g5 - g6;
        let f7 = g4 + g7;

        let e0 = f0;
        let e1 = f1;
        let e2 = f2 - f3;
        let e3 = f2 + f3;
        let e4 = f4;
        let e5 = f5 - f7;
        let e6 = f6;
        let e7 = f5 + f7;
        let e8 = f4 + f6;

        let d0 = e0;
        let d1 = e1;
        let d2 = e2 * m1;
        let d3 = e3;
        let d4 = e4 * m2;
        let d5 = e5 * m3;
        let d6 = e6 * m4;
        let d7 = e7;
        let d8 = e8 * m5;

        let c0 = d0 + d1;
        let c1 = d0 - d1;
        let c2 = d2 - d3;
        let c3 = d3;
        let c4 = d4 + d8;
        let c5 = d5 + d7;
        let c6 = d6 - d8;
        let c7 = d7;
        let c8 = c5 - c6;

        let b0 = c0 + c3;
        let b1 = c1 + c2;
        let b2 = c1 - c2;
        let b3 = c0 - c3;
        let b4 = c4 - c8;
        let b5 = c8;
        let b6 = c6 - c7;
        let b7 = c7;

        vals[i * 8 + 0] = ((b0 + b7 + 128.5) as i32).clamp(0, 255) as u8;
        vals[i * 8 + 1] = ((b1 + b6 + 128.5) as i32).clamp(0, 255) as u8;
        vals[i * 8 + 2] = ((b2 + b5 + 128.5) as i32).clamp(0, 255) as u8;
        vals[i * 8 + 3] = ((b3 + b4 + 128.5) as i32).clamp(0, 255) as u8;
        vals[i * 8 + 4] = ((b3 - b4 + 128.5) as i32).clamp(0, 255) as u8;
        vals[i * 8 + 5] = ((b2 - b5 + 128.5) as i32).clamp(0, 255) as u8;
        vals[i * 8 + 6] = ((b1 - b6 + 128.5) as i32).clamp(0, 255) as u8;
        vals[i * 8 + 7] = ((b0 - b7 + 128.5) as i32).clamp(0, 255) as u8;
    }
    vals.to_vec()
}

pub(crate) fn convert_rgb(
    plane: usize,
    mcu_units: &Vec<Vec<u8>>,
    component: &Vec<Component>,
    color_space: String,
    (h_max, v_max): (usize, usize),
) -> Vec<u8> {
    // g / ga
    if plane == 3 {
        if &color_space == "RGB" {
            rgb_to_rgb(mcu_units, component, (h_max, v_max))
        }
        // RGB
        else {
            yuv_to_rgb(mcu_units, component, (h_max, v_max))
        }
    } else if plane == 4 {
        // hasBug
        if &color_space == "YCcK" {
            ycck_to_rgb(mcu_units, component, (h_max, v_max))
        }
        // YCcK Spec Unknown
        else if &color_space == "CMYK" {
            cmyk_to_rgb(mcu_units, component, (h_max, v_max))
        } else {
            yuv_to_rgb(mcu_units, component, (h_max, v_max))
        }
    } else {
        y_to_rgb(mcu_units, component)
    }
}

// Glayscale
pub(crate) fn y_to_rgb(yuv: &Vec<Vec<u8>>, hv_maps: &Vec<Component>) -> Vec<u8> {
    let mut buffer: Vec<u8> = (0..hv_maps[0].h * hv_maps[0].v * 64 * 4)
        .map(|_| 0)
        .collect();
    for v in 0..hv_maps[0].v {
        for h in 0..hv_maps[0].h {
            let gray = &yuv[v * hv_maps[0].h + h];
            for y in 0..8 {
                let offset = (y + v * 8) * hv_maps[0].h * 8 * 4;
                for x in 0..8 {
                    let xx = (x + h * 8) * 4;
                    let cy = gray[y * 8 + x];
                    buffer[xx + offset] = cy; // R
                    buffer[xx + offset + 1] = cy; // G
                    buffer[xx + offset + 2] = cy; // B
                    buffer[xx + offset + 3] = 0xff; // A
                }
            }
        }
    }
    buffer
}

pub(crate) fn yuv_to_rgb(
    yuv: &Vec<Vec<u8>>,
    hv_maps: &Vec<Component>,
    (h_max, v_max): (usize, usize),
) -> Vec<u8> {
    let mut buffer: Vec<u8> = (0..h_max * v_max * 64 * 4).map(|_| 0).collect();
    let y_map = 0;
    let u_map = y_map + hv_maps[0].h * hv_maps[0].v;
    let v_map = u_map + hv_maps[1].h * hv_maps[1].v;

    let uy = v_max / hv_maps[1].v;
    let vy = v_max / hv_maps[2].v;
    let ux = h_max / hv_maps[1].h;
    let vx = h_max / hv_maps[2].h;

    for v in 0..v_max {
        let mut u_map_cur = u_map + v / v_max;
        let mut v_map_cur = v_map + v / v_max;

        for h in 0..h_max {
            let gray = &yuv[v * h_max + h];
            u_map_cur += h / h_max;
            v_map_cur += h / h_max;

            for y in 0..8 {
                let offset = ((y + v * 8) * (8 * h_max)) * 4;
                for x in 0..8 {
                    let xx = (x + h * 8) * 4;
                    let shift = 4090;
                    let cy = gray[y * 8 + x] as i32;
                    let cb = yuv[u_map_cur][(((y + v * 8) / uy % 8) * 8) + ((x + h * 8) / ux) % 8]
                        as i32;
                    let cr = yuv[v_map_cur][(((y + v * 8) / vy % 8) * 8) + ((x + h * 8) / vx) % 8]
                        as i32;

                    let crr = (1.402 * shift as f32) as i32;
                    let cbg = (-0.34414 * shift as f32) as i32;
                    let crg = (-0.71414 * shift as f32) as i32;
                    let cbb = (1.772 * shift as f32) as i32;

                    let red = cy + (crr * (cr - 128)) / shift;
                    let green = cy + (cbg * (cb - 128) + crg * (cr - 128)) / shift;
                    let blue = cy + (cbb * (cb - 128)) / shift;

                    let red = red.clamp(0, 255) as u8;
                    let green = green.clamp(0, 255) as u8;
                    let blue = blue.clamp(0, 255) as u8;

                    buffer[xx + offset] = red; //R
                    buffer[xx + offset + 1] = green; //G
                    buffer[xx + offset + 2] = blue; //B
                    buffer[xx + offset + 3] = 0xff; //A
                }
            }
        }
    }

    buffer
}

pub(crate) fn rgb_to_rgb(
    yuv: &Vec<Vec<u8>>,
    hv_maps: &Vec<Component>,
    (h_max, v_max): (usize, usize),
) -> Vec<u8> {
    let mut buffer: Vec<u8> = (0..h_max * v_max * 64 * 4).map(|_| 0).collect();
    let r_map = 0;
    let g_map = r_map + hv_maps[0].h * hv_maps[0].v;
    let b_map = g_map + hv_maps[1].h * hv_maps[1].v;

    let ry = v_max / hv_maps[0].v;
    let gy = v_max / hv_maps[1].v;
    let by = v_max / hv_maps[2].v;
    let rx = h_max / hv_maps[0].h;
    let gx = h_max / hv_maps[1].h;
    let bx = h_max / hv_maps[2].h;

    for v in 0..v_max {
        let mut r_map_cur = r_map + v / v_max;
        let mut g_map_cur = g_map + v / v_max;
        let mut b_map_cur = b_map + v / v_max;

        for h in 0..h_max {
            r_map_cur += h / h_max;
            g_map_cur += h / h_max;
            b_map_cur += h / h_max;

            for y in 0..8 {
                let offset = ((y + v * 8) * (8 * h_max)) * 4;
                for x in 0..8 {
                    let xx = (x + h * 8) * 4;
                    let red = yuv[r_map_cur][(((y + v * 8) / ry % 8) * 8) + ((x + h * 8) / rx) % 8];
                    let green =
                        yuv[g_map_cur][(((y + v * 8) / gy % 8) * 8) + ((x + h * 8) / gx) % 8];
                    let blue =
                        yuv[b_map_cur][(((y + v * 8) / by % 8) * 8) + ((x + h * 8) / bx) % 8];

                    buffer[xx + offset] = red; //R
                    buffer[xx + offset + 1] = green; //G
                    buffer[xx + offset + 2] = blue; //B
                    buffer[xx + offset + 3] = 0xff; //A
                }
            }
        }
    }

    buffer
}

pub(crate) fn ycck_to_rgb(
    yuv: &Vec<Vec<u8>>,
    hv_maps: &Vec<Component>,
    (h_max, v_max): (usize, usize),
) -> Vec<u8> {
    let mut buffer: Vec<u8> = (0..h_max * v_max * 64 * 4).map(|_| 0).collect();
    let y_map = 0;
    let c1_map = y_map + hv_maps[0].h * hv_maps[0].v;
    let c2_map = c1_map + hv_maps[1].h * hv_maps[1].v;
    let k_map = c2_map + hv_maps[2].h * hv_maps[2].v;

    let _yy = v_max / hv_maps[0].v;
    let c1y = v_max / hv_maps[1].v;
    let c2y = v_max / hv_maps[2].v;
    let _ky = v_max / hv_maps[3].v;

    let _yx = h_max / hv_maps[0].h;
    let c1x = h_max / hv_maps[1].h;
    let c2x = h_max / hv_maps[2].h;
    let _kx = h_max / hv_maps[3].h;

    for v in 0..v_max {
        let y_map_cur = y_map + v / v_max;
        let c1_map_cur = c1_map + v / v_max;
        let c2_map_cur = c2_map + v / v_max;
        let k_map_cur = k_map + v / v_max;

        for h in 0..h_max {
            let y_map_cur = y_map_cur + h / h_max;
            let c1_map_cur = c1_map_cur + h / h_max;
            let c2_map_cur = c2_map_cur + h / h_max;
            let k_map_cur = k_map_cur + h / h_max;

            for y in 0..8 {
                let offset = ((y + v * 8) * (8 * h_max)) * 4;
                for x in 0..8 {
                    let xx = (x + h * 8) * 4;
                    let yin = yuv[y_map_cur]
                        [(((y + v * 8) / _yy % 8) * 8) + ((x + h * 8) / _yx) % 8]
                        as i32;
                    let c1 = yuv[c1_map_cur]
                        [(((y + v * 8) / c1y % 8) * 8) + ((x + h * 8) / c1x) % 8]
                        as i32;
                    let c2 = yuv[c2_map_cur]
                        [(((y + v * 8) / c2y % 8) * 8) + ((x + h * 8) / c2x) % 8]
                        as i32;
                    let key = yuv[k_map_cur]
                        [(((y + v * 8) / _ky % 8) * 8) + ((x + h * 8) / _kx) % 8]
                        as i32;

                    let cy = key - yin;
                    let cb = 255 - c1;
                    let cr = 255 - c2;

                    let shift = 4096;

                    let crr = (1.402 * shift as f32) as i32;
                    let cbg = (-0.34414 * shift as f32) as i32;
                    let crg = (-0.71414 * shift as f32) as i32;
                    let cbb = (1.772 * shift as f32) as i32;

                    let red = cy + (crr * (cr - 128)) / shift;
                    let green = cy + (cbg * (cb - 128) + crg * (cr - 128)) / shift;
                    let blue = cy + (cbb * (cb - 128)) / shift;

                    let red = red.clamp(0, 255) as u8;
                    let green = green.clamp(0, 255) as u8;
                    let blue = blue.clamp(0, 255) as u8;

                    buffer[xx + offset] = red; //R
                    buffer[xx + offset + 1] = green; //G
                    buffer[xx + offset + 2] = blue; //B
                    buffer[xx + offset + 3] = 0xff; //A

                    buffer[xx + offset] = red; //R
                    buffer[xx + offset + 1] = green; //G
                    buffer[xx + offset + 2] = blue; //B
                    buffer[xx + offset + 3] = 0xff; //A
                }
            }
        }
    }

    buffer
}

/* spec known */
pub(crate) fn cmyk_to_rgb(
    yuv: &Vec<Vec<u8>>,
    hv_maps: &Vec<Component>,
    (h_max, v_max): (usize, usize),
) -> Vec<u8> {
    let mut buffer: Vec<u8> = (0..h_max * v_max * 64 * 4).map(|_| 0).collect();
    let c_map = 0;
    let m_map = c_map + hv_maps[0].h * hv_maps[0].v;
    let y_map = m_map + hv_maps[1].h * hv_maps[1].v;
    let k_map = y_map + hv_maps[2].h * hv_maps[2].v;

    let cy = v_max / hv_maps[0].v;
    let my = v_max / hv_maps[1].v;
    let yy = v_max / hv_maps[2].v;
    let ky = v_max / hv_maps[3].v;

    let cx = h_max / hv_maps[0].h;
    let mx = h_max / hv_maps[1].h;
    let yx = h_max / hv_maps[2].h;
    let kx = h_max / hv_maps[3].h;

    for v in 0..v_max {
        let mut c_map_cur = c_map + v / cy;
        let mut m_map_cur = m_map + v / my;
        let mut y_map_cur = y_map + v / yy;
        let mut k_map_cur = k_map + v / ky;

        for h in 0..h_max {
            c_map_cur += h / cx;
            m_map_cur += h / mx;
            y_map_cur += h / yx;
            k_map_cur += h / kx;

            for y in 0..8 {
                let offset = ((y + v * 8) * (8 * h_max)) * 4;
                for x in 0..8 {
                    let xx = (x + h * 8) * 4;
                    let cc = yuv[c_map_cur][(((y + v * 8) / cy % 8) * 8) + ((x + h * 8) / cx) % 8]
                        as i32;
                    let cm = yuv[m_map_cur][(((y + v * 8) / my % 8) * 8) + ((x + h * 8) / mx) % 8]
                        as i32;
                    let cy = yuv[y_map_cur][(((y + v * 8) / yy % 8) * 8) + ((x + h * 8) / yx) % 8]
                        as i32;
                    let ck = yuv[k_map_cur][(((y + v * 8) / ky % 8) * 8) + ((x + h * 8) / kx) % 8]
                        as i32;

                    let red = (ck + cc - 255).clamp(0, 255) as u8;
                    let green = (ck + cm - 255).clamp(0, 255) as u8;
                    let blue = (ck + cy - 255).clamp(0, 255) as u8;

                    buffer[xx + offset] = red; //R
                    buffer[xx + offset + 1] = green; //G
                    buffer[xx + offset + 2] = blue; //B
                    buffer[xx + offset + 3] = 0xff; //A
                }
            }
        }
    }

    buffer
}

pub(crate) fn expand_huffman_table(huffman_table: &HuffmanTable) -> Option<HuffmanDecodeTable> {
    let mut current_max: Vec<i32> = Vec::new();
    let mut current_min: Vec<i32> = Vec::new();

    let mut code: i32 = 0;
    let mut pos: usize = 0;
    for l in 0..16 {
        if huffman_table.len[l] != 0 {
            current_min.push(code);
            if pos >= huffman_table.val.len() {
                break;
            }
            pos += huffman_table.len[l];
            code += huffman_table.len[l] as i32;
            current_max.push(code - 1);
        } else {
            current_min.push(-1);
            current_max.push(-1);
        }
        code <<= 1;
    }

    let val: Vec<usize> = huffman_table.val.to_vec();
    let pos: Vec<usize> = huffman_table.pos.to_vec();

    Some(HuffmanDecodeTable {
        val,
        pos,
        max: current_max,
        min: current_min,
    })
}

pub(crate) fn huffman_extend(
    huffman_tables: &HuffmanTables,
) -> (
    Vec<Option<HuffmanDecodeTable>>,
    Vec<Option<HuffmanDecodeTable>>,
) {
    let mut dc_decode: Vec<Option<HuffmanDecodeTable>> = vec![None, None, None, None];
    let mut ac_decode: Vec<Option<HuffmanDecodeTable>> = vec![None, None, None, None];

    for huffman_table in huffman_tables.dc_tables.iter() {
        if let Some(huffman_table) = huffman_table {
            dc_decode[huffman_table.no] = expand_huffman_table(huffman_table);
        }
    }

    for huffman_table in huffman_tables.ac_tables.iter() {
        if let Some(huffman_table) = huffman_table {
            ac_decode[huffman_table.no] = expand_huffman_table(huffman_table);
        }
    }

    (dc_decode, ac_decode)
}

fn jpeg_illegal_data(message: impl Into<String>) -> Error {
    Box::new(ImgError::new_const(
        ImgErrorKind::IllegalData,
        message.into(),
    ))
}

pub(crate) fn require_scan_header<'a>(
    header: &'a JpegHaeder,
) -> Result<&'a HuffmanScanHeader, Error> {
    header
        .huffman_scan_header
        .as_ref()
        .ok_or_else(|| jpeg_illegal_data("Not undefined Huffman Scan Header"))
}

pub(crate) fn require_frame_header<'a>(header: &'a JpegHaeder) -> Result<&'a FrameHeader, Error> {
    header
        .frame_header
        .as_ref()
        .ok_or_else(|| jpeg_illegal_data("Not undefined Frame Header"))
}

pub(crate) fn require_components<'a>(
    frame_header: &'a FrameHeader,
) -> Result<&'a Vec<Component>, Error> {
    frame_header
        .component
        .as_ref()
        .ok_or_else(|| jpeg_illegal_data("Not undefined Frame Header Component"))
}

pub(crate) fn require_quantization_tables<'a>(
    header: &'a JpegHaeder,
) -> Result<&'a Vec<QuantizationTable>, Error> {
    header
        .quantization_tables
        .as_ref()
        .ok_or_else(|| jpeg_illegal_data("Not undefined Quantization Tables"))
}

pub(crate) struct BaselineScanSlot<'a> {
    pub(crate) dc: &'a HuffmanDecodeTable,
    pub(crate) ac: &'a HuffmanDecodeTable,
    pub(crate) component_index: usize,
    pub(crate) quant_index: usize,
}

pub(crate) struct ProgressiveScanSlot<'a> {
    pub(crate) dc: Option<&'a HuffmanDecodeTable>,
    pub(crate) ac: Option<&'a HuffmanDecodeTable>,
    pub(crate) component_index: usize,
    pub(crate) quant_index: usize,
    pub(crate) in_scan: bool,
    pub(crate) is_first: bool,
}

fn require_huffman_decode_table<'a>(
    tables: &'a [Option<HuffmanDecodeTable>],
    index: usize,
    kind: &str,
) -> Result<&'a HuffmanDecodeTable, Error> {
    tables
        .get(index)
        .and_then(Option::as_ref)
        .ok_or_else(|| jpeg_illegal_data(format!("missing {kind} Huffman table {}", index)))
}

fn validate_quantization_table(
    quantization_tables: &[QuantizationTable],
    index: usize,
) -> Result<(), Error> {
    if quantization_tables.get(index).is_none() {
        return Err(jpeg_illegal_data(format!(
            "missing quantization table {}",
            index
        )));
    }
    Ok(())
}

pub(crate) fn build_baseline_scan_slots<'a>(
    scan: &[(usize, usize, usize, usize, bool, bool)],
    quantization_tables: &[QuantizationTable],
    dc_decode: &'a [Option<HuffmanDecodeTable>],
    ac_decode: &'a [Option<HuffmanDecodeTable>],
) -> Result<Vec<BaselineScanSlot<'a>>, Error> {
    let mut slots = Vec::with_capacity(scan.len());
    for (dc_current, ac_current, component_index, tq, in_scan, _) in scan {
        if !in_scan {
            return Err(jpeg_illegal_data(
                "baseline scan is missing one or more frame components",
            ));
        }
        validate_quantization_table(quantization_tables, *tq)?;
        slots.push(BaselineScanSlot {
            dc: require_huffman_decode_table(dc_decode, *dc_current, "DC")?,
            ac: require_huffman_decode_table(ac_decode, *ac_current, "AC")?,
            component_index: *component_index,
            quant_index: *tq,
        });
    }
    Ok(slots)
}

pub(crate) fn build_progressive_scan_slots<'a>(
    scan: &[(usize, usize, usize, usize, bool, bool)],
    quantization_tables: &[QuantizationTable],
    dc_decode: &'a [Option<HuffmanDecodeTable>],
    ac_decode: &'a [Option<HuffmanDecodeTable>],
    ss: usize,
    se: usize,
) -> Result<Vec<ProgressiveScanSlot<'a>>, Error> {
    let mut slots = Vec::with_capacity(scan.len());
    for (dc_current, ac_current, component_index, tq, in_scan, is_first) in scan {
        validate_quantization_table(quantization_tables, *tq)?;
        let dc = if *in_scan && ss == 0 {
            Some(require_huffman_decode_table(dc_decode, *dc_current, "DC")?)
        } else {
            None
        };
        let ac = if *in_scan && se > 0 {
            Some(require_huffman_decode_table(ac_decode, *ac_current, "AC")?)
        } else {
            None
        };
        slots.push(ProgressiveScanSlot {
            dc,
            ac,
            component_index: *component_index,
            quant_index: *tq,
            in_scan: *in_scan,
            is_first: *is_first,
        });
    }
    Ok(slots)
}

#[cfg(feature = "multithread")]
#[derive(std::cmp::PartialEq)]
enum ThreadCommand {
    Stop,
    Run,
}

pub(crate) fn calc_mcu(component: &Vec<Component>) -> (usize, usize, usize, usize, usize) {
    let mut h_max = 1;
    let mut v_max = 1;
    let mut dy = 8;
    let mut dx = 8;
    let mut size = 0;
    for i in 0..component.len() {
        size += component[i].h * component[i].v;
        dx = usize::max(component[i].h * 8, dx);
        dy = usize::max(component[i].v * 8, dy);
        h_max = usize::max(component[i].h, h_max);
        v_max = usize::max(component[i].v, v_max);
    }

    (size, h_max, v_max, dx, dy)
}

pub(crate) fn calc_scan(
    component: &Vec<Component>,
    huffman_scan_header: &HuffmanScanHeader,
) -> Vec<(usize, usize, usize, usize, bool, bool)> {
    let mut scan: Vec<(usize, usize, usize, usize, bool, bool)> = Vec::new();

    let mut j = 0;
    for i in 0..component.len() {
        let tq = component[i].tq;
        if j < huffman_scan_header.ns && huffman_scan_header.csn[j] == component[i].c {
            scan.push((
                huffman_scan_header.tdcn[j],
                huffman_scan_header.tacn[j],
                i,
                tq,
                true,
                true,
            ));
            for _ in 1..component[i].h * component[i].v {
                scan.push((
                    huffman_scan_header.tdcn[j],
                    huffman_scan_header.tacn[j],
                    i,
                    tq,
                    true,
                    false,
                ));
            }
            j += 1;
        } else {
            scan.push((0, 0, i, tq, false, true));
            for _ in 1..component[i].h * component[i].v {
                scan.push((0, 0, i, tq, false, false));
            }
        }
    }
    scan
}

#[cfg(feature = "multithread")]
pub(crate) fn decode_baseline<'decode, B: BinaryReader>(
    reader: &mut B,
    header: &JpegHaeder,
    option: &mut DecodeOptions,
    mut warnings: Option<ImgWarnings>,
) -> Result<Option<ImgWarnings>, Error> {
    let width = header.width;
    let height = header.height;
    let huffman_scan_header = require_scan_header(header)?;
    let fh = require_frame_header(header)?.clone();
    let color_space = fh.color_space.to_string();
    let component = require_components(&fh)?.clone();
    let plane = fh.plane;
    // decode
    option.drawer.init(width, height, InitOptions::new())?;

    let quantization_tables = require_quantization_tables(header)?.clone();
    let (dc_decode, ac_decode) = huffman_extend(&header.huffman_tables);

    let mut bitread = BitReader::new(reader);
    let (mcu_size, h_max, v_max, dx, dy) = calc_mcu(&component);
    let scan = calc_scan(&component, huffman_scan_header);
    let scan_slots =
        build_baseline_scan_slots(&scan, &quantization_tables, &dc_decode, &ac_decode)?;

    let mut preds: Vec<i32> = (0..component.len()).map(|_| 0).collect();

    let mcu_y_max = (height + dy - 1) / dy;
    let mcu_x_max = (width + dx - 1) / dx;

    let mut mcu_interval = if header.interval > 0 {
        header.interval as isize
    } else {
        -1
    };

    let (tx1, rx1) = std::sync::mpsc::channel();
    let (tx2, rx2) = std::sync::mpsc::channel();
    let (tx3, rx3) = std::sync::mpsc::channel();
    let (tx4, rx4) = std::sync::mpsc::channel();

    let sq = &super::util::ZIG_ZAG_SEQUENCE;

    std::thread::spawn(move || {
        loop {
            let (com, zz, mcu_x, mcu_y, tq) =
                rx1.recv().unwrap_or((ThreadCommand::Stop, vec![], 0, 0, 0));
            if com == ThreadCommand::Stop {
                let _ = tx2.send((com, zz, mcu_x, mcu_y));
                break;
            }
            let q = quantization_tables[tq].q.clone();
            let zz: Vec<i32> = (0..64).map(|i| zz[sq[i]] * q[sq[i]] as i32).collect();
            let _ = tx2.send((com, zz, mcu_x, mcu_y));
        }
    });

    std::thread::spawn(move || {
        loop {
            let (com, zz, mcu_x, mcu_y) = rx2.recv().unwrap_or((ThreadCommand::Stop, vec![], 0, 0));
            if com == ThreadCommand::Stop {
                let _ = tx3.send((com, vec![], mcu_x, mcu_y));
                break;
            }
            let ff = idct(&zz);
            let _ = tx3.send((com, ff, mcu_x, mcu_y));
        }
    });

    std::thread::spawn(move || {
        loop {
            let mut mcu_units: Vec<Vec<u8>> = Vec::new();
            let mut com = ThreadCommand::Run;
            let mut mcu_x = 0;
            let mut mcu_y = 0;
            for _ in 0..mcu_size {
                let (_com, ff, _mcu_x, _mcu_y) =
                    rx3.recv().unwrap_or((ThreadCommand::Stop, vec![], 0, 0));
                mcu_units.push(ff);
                com = _com;
                mcu_x = _mcu_x;
                mcu_y = _mcu_y
            }
            if com == ThreadCommand::Stop {
                let _ = tx4.send((com, vec![], mcu_x, mcu_y));
                break;
            }
            let data = convert_rgb(
                plane,
                &mcu_units,
                &component,
                color_space.to_string(),
                (h_max, v_max),
            );

            let _ = tx4.send((com, data, mcu_x, mcu_y));
        }
    });

    for mcu_y in 0..mcu_y_max {
        for mcu_x in 0..mcu_x_max {
            for scannumber in 0..mcu_size {
                let slot = &scan_slots[scannumber];
                let ret =
                    baseline_read(&mut bitread, slot.dc, slot.ac, preds[slot.component_index]);
                let (zz, pred);
                match ret {
                    Ok((_zz, _pred)) => {
                        zz = _zz;
                        pred = _pred;
                    }
                    Err(..) => {
                        warnings = ImgWarnings::add(
                            warnings,
                            Box::new(JpegWarning::new_const(
                                JpegWarningKind::DataCorruption,
                                "baseline".to_string(),
                            )),
                        );
                        return Ok(warnings);
                    }
                }
                preds[slot.component_index] = pred;
                let _ = tx1.send((ThreadCommand::Run, zz, mcu_x, mcu_y, slot.quant_index));
            }
            if header.interval > 0 {
                mcu_interval -= 1;
                if mcu_interval == 0 && mcu_x < mcu_x_max && mcu_y < mcu_y_max - 1 {
                    if bitread.rst()? {
                        mcu_interval = header.interval as isize;
                        for i in 0..preds.len() {
                            preds[i] = 0;
                        }
                    } else {
                        // Reset Interval
                        let r = bitread.next_marker()?;
                        if (0xd0..=0xd7).contains(&r) {
                            mcu_interval = header.interval as isize;
                            for i in 0..preds.len() {
                                preds[i] = 0;
                            }
                        } else if r == 0xd9 {
                            // EOI
                            option.drawer.terminate(None)?;
                            warnings = ImgWarnings::add(
                                warnings,
                                Box::new(JpegWarning::new_const(
                                    JpegWarningKind::IlligalRSTMaker,
                                    "Unexcept EOI,Is this image corruption?".to_string(),
                                )),
                            );
                            let _ = tx1.send((ThreadCommand::Stop, vec![], 0, 0, 0));
                            return Ok(warnings);
                        }
                    }
                } else if bitread.rst()? {
                    warnings = ImgWarnings::add(
                        warnings,
                        Box::new(JpegWarning::new_const(
                            JpegWarningKind::IlligalRSTMaker,
                            "Unexcept RST marker location,Is this image corruption?".to_string(),
                        )),
                    );
                    mcu_interval = header.interval as isize;
                    for i in 0..preds.len() {
                        preds[i] = 0;
                    }
                }
            }
        }
    }
    let _ = tx1.send((ThreadCommand::Stop, vec![], 0, 0, 0));
    // Only implement RGB

    loop {
        let (com, data, mcu_x, mcu_y) = rx4.recv().unwrap_or((ThreadCommand::Stop, vec![], 0, 0));
        if com == ThreadCommand::Stop {
            break;
        }
        option
            .drawer
            .draw(mcu_x * dx, mcu_y * dy, dx, dy, &data, None)?;
    }

    let b = bitread.next_marker();
    match b {
        Ok(marker) => {
            match marker {
                0xd9 => {
                    // EOI
                    option.drawer.terminate(None)?;
                    return Ok(warnings);
                }
                0xdd => {
                    option.drawer.terminate(None)?;
                    warnings = ImgWarnings::add(
                        warnings,
                        Box::new(JpegWarning::new_const(
                            JpegWarningKind::UnexpectMarker,
                            "DNL,No Support Multi scan/frame".to_string(),
                        )),
                    );
                    return Ok(warnings);
                }
                _ => {
                    option.drawer.terminate(None)?;
                    warnings = ImgWarnings::add(
                        warnings,
                        Box::new(JpegWarning::new_const(
                            JpegWarningKind::UnexpectMarker,
                            "No Support Multi scan/frame".to_string(),
                        )),
                    );
                    return Ok(warnings);
                }
            }
        }
        Err(s) => {
            let s = format!("found {:?}", s);
            warnings = ImgWarnings::add(
                warnings,
                Box::new(JpegWarning::new_const(
                    JpegWarningKind::UnexpectMarker,
                    s.to_string(),
                )),
            );
        }
    }
    option.drawer.terminate(None)?;
    Ok(warnings)
}

#[cfg(not(feature = "multithread"))]
pub(crate) fn decode_baseline<'decode, B: BinaryReader>(
    reader: &mut B,
    header: &JpegHaeder,
    option: &mut DecodeOptions,
    mut warnings: Option<ImgWarnings>,
) -> Result<Option<ImgWarnings>, Error> {
    let width = header.width;
    let height = header.height;
    let huffman_scan_header = require_scan_header(header)?;
    let fh = require_frame_header(header)?;
    let color_space = fh.color_space.to_string();
    let component = require_components(fh)?;
    let plane = fh.plane;
    // decode
    option.drawer.init(width, height, InitOptions::new())?;

    let quantization_tables = require_quantization_tables(header)?;
    let (dc_decode, ac_decode) = huffman_extend(&header.huffman_tables);

    let mut bitread = BitReader::new(reader);
    let (mcu_size, h_max, v_max, dx, dy) = calc_mcu(&component);
    let scan = calc_scan(&component, &huffman_scan_header);
    let scan_slots = build_baseline_scan_slots(&scan, quantization_tables, &dc_decode, &ac_decode)?;

    let mut preds: Vec<i32> = (0..component.len()).map(|_| 0).collect();

    let mcu_y_max = (height + dy - 1) / dy;
    let mcu_x_max = (width + dx - 1) / dx;

    let mut mcu_interval = if header.interval > 0 {
        header.interval as isize
    } else {
        -1
    };

    for mcu_y in 0..mcu_y_max {
        for mcu_x in 0..mcu_x_max {
            let mut mcu_units: Vec<Vec<u8>> = Vec::new();

            for scannumber in 0..mcu_size {
                let slot = &scan_slots[scannumber];
                let ret =
                    baseline_read(&mut bitread, slot.dc, slot.ac, preds[slot.component_index]);
                let (zz, pred);
                match ret {
                    Ok((_zz, _pred)) => {
                        zz = _zz;
                        pred = _pred;
                    }
                    Err(..) => {
                        warnings = ImgWarnings::add(
                            warnings,
                            Box::new(JpegWarning::new_const(
                                JpegWarningKind::DataCorruption,
                                "baseline".to_string(),
                            )),
                        );
                        return Ok(warnings);
                    }
                }
                preds[slot.component_index] = pred;

                let sq = &super::util::ZIG_ZAG_SEQUENCE;
                let q = quantization_tables[slot.quant_index].q.clone();
                let zz: Vec<i32> = (0..64).map(|i| zz[sq[i]] * q[sq[i]] as i32).collect();
                let ff = idct(&zz);
                mcu_units.push(ff);
            }

            // Only implement RGB
            let data = convert_rgb(
                plane,
                &mcu_units,
                &component,
                color_space.to_string(),
                (h_max, v_max),
            );

            option
                .drawer
                .draw(mcu_x * dx, mcu_y * dy, dx, dy, &data, None)?;

            if header.interval > 0 {
                mcu_interval = mcu_interval - 1;
                if mcu_interval == 0 && mcu_x < mcu_x_max && mcu_y < mcu_y_max - 1 {
                    if bitread.rst()? == true {
                        mcu_interval = header.interval as isize;
                        for i in 0..preds.len() {
                            preds[i] = 0;
                        }
                    } else {
                        // Reset Interval
                        let r = bitread.next_marker()?;
                        if r >= 0xd0 && r <= 0xd7 {
                            mcu_interval = header.interval as isize;
                            for i in 0..preds.len() {
                                preds[i] = 0;
                            }
                        } else if r == 0xd9 {
                            // EOI
                            option.drawer.terminate(None)?;
                            warnings = ImgWarnings::add(
                                warnings,
                                Box::new(JpegWarning::new_const(
                                    JpegWarningKind::IlligalRSTMaker,
                                    "Unexcept EOI,Is this image corruption?".to_string(),
                                )),
                            );
                            return Ok(warnings);
                        }
                    }
                } else if bitread.rst()? == true {
                    warnings = ImgWarnings::add(
                        warnings,
                        Box::new(JpegWarning::new_const(
                            JpegWarningKind::IlligalRSTMaker,
                            "Unexcept RST marker location,Is this image corruption?".to_string(),
                        )),
                    );
                    mcu_interval = header.interval as isize;
                    for i in 0..preds.len() {
                        preds[i] = 0;
                    }
                    //                 return Ok(Warning);
                }
            }
        }
    }

    let b = bitread.next_marker();
    match b {
        Ok(marker) => {
            match marker {
                0xd9 => {
                    // EOI
                    option.drawer.terminate(None)?;
                    return Ok(warnings);
                }
                0xdd => {
                    option.drawer.terminate(None)?;
                    warnings = ImgWarnings::add(
                        warnings,
                        Box::new(JpegWarning::new_const(
                            JpegWarningKind::UnexpectMarker,
                            "DNL,No Support Multi scan/frame".to_string(),
                        )),
                    );
                    return Ok(warnings);
                }
                _ => {
                    option.drawer.terminate(None)?;
                    warnings = ImgWarnings::add(
                        warnings,
                        Box::new(JpegWarning::new_const(
                            JpegWarningKind::UnexpectMarker,
                            "No Support Multi scan/frame".to_string(),
                        )),
                    );
                    return Ok(warnings);
                }
            }
        }
        Err(s) => {
            let s = format!("found {:?}", s);
            warnings = ImgWarnings::add(
                warnings,
                Box::new(JpegWarning::new_const(
                    JpegWarningKind::UnexpectMarker,
                    s.to_string(),
                )),
            );
        }
    }
    option.drawer.terminate(None)?;
    Ok(warnings)
}

pub fn decode<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let mut warnings: Option<ImgWarnings> = None;
    // Make Huffman Table
    // Scan Header
    let mut header = JpegHaeder::new(reader, 0)?;

    if option.debug_flag > 0 {
        let boxstr = print_header(&header, option.debug_flag);
        option.drawer.verbose(&boxstr, None)?;
    }

    if header.is_hierachical {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Hierachical is not support".to_string(),
        )));
    }

    if header.frame_header.is_none() {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Not undefined Frame Header".to_string(),
        )));
    }

    let fh = require_frame_header(&header)?;
    let plane = fh.plane;
    if plane == 0 || plane > 4 {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Not support planes".to_string(),
        )));
    }

    if fh.component.is_none() {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Not undefined Frame Header Component".to_string(),
        )));
    }

    if header.quantization_tables.is_none() {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Not undefined Quantization Tables".to_string(),
        )));
    }

    if !fh.is_huffman {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "This decoder suport huffman only".to_string(),
        )));
    }

    //    if fh.is_progressive == true {
    //        return Err(Box::new(ImgError::new_const(ImgErrorKind::DecodeError,"This Decoder is not support progressive".to_string())));
    //    }

    if fh.is_differential {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "This Decoder not support differential".to_string(),
        )));
    }

    if plane == 4 {
        warnings = ImgWarnings::add(
            warnings,
            Box::new(JpegWarning::new_const(
                JpegWarningKind::UnknowFormat,
                "Plane 4 color translation rule is known".to_string(),
            )),
        )
    }

    let map = make_metadata(&header);
    for (key, value) in &map {
        option.drawer.set_metadata(key, value.clone())?;
    }

    if fh.is_progressive {
        decode_progressive(reader, &mut header, option, warnings)
    } else {
        decode_baseline(reader, &header, option, warnings)
    }
}
