//! CCITT fax decoder tables and helpers.

// need rust 1.60 +
mod black;
mod white;
type Error = Box<dyn std::error::Error>;
use crate::error::ImgError;
use crate::error::ImgErrorKind;
pub use black::black_tree;
pub use white::white_tree;

const EOL: i32 = -2;

const WHITE: u8 = 0;
/*
const UNDEF:u8 = 1;
const TERMINATE:u8 = 2;
*/
const BLACK: u8 = 0xff;

#[derive(PartialEq, Debug)]
pub enum Encoder {
    HuffmanRLE,
    G31d,
    G32d,
    G4,
}

#[derive(Debug)]
pub struct HuffmanTree {
    pub working_bits: usize,
    pub max_bits: usize,
    pub append_bits: usize,
    pub matrix: Vec<(i32, i32)>,
    pub append: Vec<(i32, i32)>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Mode {
    Pass,
    Horiz,
    V,
    Vr(usize),
    Vl(usize),
    Ext2D(usize),
    EOL,
    None,
}

pub struct BitReader {
    pub buffer: Vec<u8>,
    ptr: usize,
    left_bits: usize,
    last_byte: u32,
    is_lsb: bool,
    warning: bool,
}

impl BitReader {
    pub fn new(data: &[u8], is_lsb: bool) -> Self {
        Self {
            buffer: data.to_vec(),
            last_byte: 0,
            ptr: 0,
            left_bits: 0,
            is_lsb,
            warning: false,
        }
    }

    fn look_bits(&mut self, size: usize) -> Result<usize, Error> {
        if self.is_lsb {
            return self.look_bits_lsb(size);
        } else {
            return self.look_bits_msb(size);
        }
    }

    fn look_bits_msb(&mut self, size: usize) -> Result<usize, Error> {
        while self.left_bits <= 24 {
            if self.ptr >= self.buffer.len() {
                if self.left_bits < size {
                    self.warning = true;
                    if size >= 12 {
                        return Ok(0x1); // send EOL
                    } else {
                        return Ok(0x0);
                    }
                }
                break;
            }

            self.last_byte = (self.last_byte << 8) | (self.buffer[self.ptr] as u32);
            self.ptr += 1;
            self.left_bits += 8;
        }
        if self.left_bits < size {
            self.warning = true;
            if size >= 12 {
                return Ok(0x1);
            } else {
                return Ok(0x0);
            }
        }

        let bits = (self.last_byte >> (self.left_bits - size)) & ((1 << size) - 1);

        Ok(bits as usize)
    }

    fn skip_bits(&mut self, size: usize) {
        if self.left_bits >= size {
            self.left_bits -= size;
        } else {
            let r = self.look_bits(size);
            if r.is_ok() {
                self.left_bits = self.left_bits.saturating_sub(size);
            } else {
                self.left_bits = 0;
            }
        }
    }

    fn look_bits_lsb(&mut self, size: usize) -> Result<usize, Error> {
        while self.left_bits <= 24 {
            if self.ptr >= self.buffer.len() {
                if self.left_bits < size {
                    self.warning = true;
                    if size >= 12 {
                        return Ok(0x1); // send EOL
                    } else {
                        return Ok(0x0);
                    }
                }
                break;
            }
            self.last_byte = (self.last_byte << 8) | (self.buffer[self.ptr].reverse_bits() as u32);
            self.ptr += 1;
            self.left_bits += 8;
        }
        if self.left_bits < size {
            self.warning = true;
            if size >= 12 {
                return Ok(0x1);
            } else {
                return Ok(0x0);
            }
        }

        let bits = (self.last_byte >> (self.left_bits - size)) & ((1 << size) - 1);

        Ok(bits as usize)
    }

    fn get_bits(&mut self, size: usize) -> Result<usize, Error> {
        let bits = self.look_bits(size);
        self.skip_bits(size);
        bits
    }

    fn value(&mut self, tree: &HuffmanTree) -> Result<i32, Error> {
        loop {
            let pos = self.look_bits(tree.working_bits)?;
            let (mut bits, mut val) = tree.matrix[pos];
            if bits == -1 {
                let pos = self.look_bits(tree.max_bits)?;
                // need rust 1.60
                (bits, val) = tree.append[pos];
                if bits == -1 {
                    // fill bits can be arbitrarily long, so handle them iteratively.
                    self.skip_bits(1);
                    if self.warning {
                        return Ok(EOL);
                    }
                    continue;
                }
            }
            self.skip_bits(bits as usize);
            return Ok(val);
        }
    }

    fn run_len(&mut self, tree: &HuffmanTree) -> Result<i32, Error> {
        let mut run_len = self.value(tree)?;
        if run_len == EOL {
            return Ok(EOL);
        }
        let mut tolal_run = run_len;
        while run_len >= 64 {
            run_len = self.value(tree)?;
            tolal_run += run_len;
        }
        Ok(tolal_run)
    }

    fn mode(&mut self) -> Result<Mode, Error> {
        let array: [(usize, Mode); 128] = [
            // 0000000
            (12, Mode::None),
            // 0000001
            (10, Mode::Ext2D(0)),
            // 0000010
            (7, Mode::Vl(3)),
            // 0000011
            (7, Mode::Vr(3)),
            // 000010x
            (6, Mode::Vl(2)),
            (6, Mode::Vl(2)),
            // 000011x
            (6, Mode::Vr(2)),
            (6, Mode::Vr(2)),
            // 0001xxx
            (4, Mode::Pass),
            (4, Mode::Pass),
            (4, Mode::Pass),
            (4, Mode::Pass),
            (4, Mode::Pass),
            (4, Mode::Pass),
            (4, Mode::Pass),
            (4, Mode::Pass),
            // 001xxxx
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            (3, Mode::Horiz),
            // 010xxxx
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            (3, Mode::Vl(1)),
            // 011xxxx 16
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            (3, Mode::Vr(1)),
            // 1xxxxxx 64
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
            (1, Mode::V),
        ];
        if self.look_bits(12)? == 1 {
            self.skip_bits(12);
            return Ok(Mode::EOL);
        }
        let (bits, mode) = array[self.look_bits(7)?].clone();

        if bits <= 7 {
            self.skip_bits(bits);
            return Ok(mode);
        }

        if bits == 10 {
            self.skip_bits(10);
            let n = self.look_bits(3)?;
            self.skip_bits(3);
            return Ok(Mode::Ext2D(n));
        }
        self.skip_bits(bits);
        Ok(Mode::None)
    }

    // skip next byte
    fn flush(&mut self) {
        self.left_bits -= self.left_bits % 8;
    }
}

// Tiff independ
pub fn decoder(
    buf: &[u8],
    width: usize,
    height: usize,
    encoding: Encoder,
    is_lsb: bool,
) -> Result<(Vec<u8>, bool), Error> {
    let mut data = Vec::with_capacity(width * height);
    let white = white_tree();
    let black = black_tree();

    let mut reader = BitReader::new(buf, is_lsb);

    let mut code1;

    // seek first EOL
    if encoding == Encoder::G31d || encoding == Encoder::G32d {
        loop {
            code1 = reader.look_bits(12)?;
            if code1 != 0 {
                break;
            };
            reader.skip_bits(1);
        }
        if code1 == 1 {
            reader.skip_bits(12);
        }
    }

    let mut is2d = encoding == Encoder::G4;

    let mut y = 0;

    if encoding == Encoder::G32d && reader.get_bits(1)? == 0 {
        is2d = true
    }
    // slow code
    /*
    let mut ref_codes = vec![WHITE;width];
    ref_codes.push(TERMINATE);
    let mut cur_codes = Vec::with_capacity(width+1);
    for _ in 0..width {cur_codes.push(UNDEF);}
    cur_codes.push(TERMINATE);
    */
    // fast code
    let mut codes = vec![0, 0, width];

    loop {
        let mut a0 = 0;
        let mut eol = false;

        // test code
        let pre_codes = codes;
        codes = Vec::with_capacity(width + 2);
        codes.push(0);
        codes.push(0);
        let mut codes_ptr = 1;

        while a0 < width && !eol {
            let mode = if is2d { reader.mode()? } else { Mode::Horiz };
            match mode {
                Mode::Horiz => {
                    let (mut len1, mut len2);
                    if codes.len() & 0x1 == 0 {
                        len1 = reader.run_len(&white)?;
                        len2 = reader.run_len(&black)?;
                    } else {
                        len1 = reader.run_len(&black)?;
                        len2 = reader.run_len(&white)?;
                    }
                    if len1 == EOL {
                        // EOL
                        eol = true;
                        len1 = 0;
                    }
                    if len2 == EOL {
                        // EOL
                        eol = true;
                        len2 = 0;
                    }

                    a0 += len1 as usize;
                    codes.push(a0);
                    a0 += len2 as usize;
                    codes.push(a0);
                }
                Mode::Pass => {
                    codes_ptr += 1;
                    while codes_ptr < pre_codes.len() && a0 >= pre_codes[codes_ptr] {
                        codes_ptr += 2;
                    }
                    codes_ptr += 1;
                    let b2 = pre_codes.get(codes_ptr).copied().unwrap_or_else(|| {
                        reader.warning = true;
                        width
                    });
                    a0 = b2;
                }
                Mode::V => {
                    codes_ptr += 1;

                    while codes_ptr < pre_codes.len() && a0 >= pre_codes[codes_ptr] {
                        codes_ptr += 2;
                    }

                    let a1 = pre_codes.get(codes_ptr).copied().unwrap_or_else(|| {
                        reader.warning = true;
                        width
                    });
                    a0 = a1;

                    codes.push(a0);
                }
                Mode::Vr(n) => {
                    codes_ptr += 1;

                    while codes_ptr < pre_codes.len() && a0 >= pre_codes[codes_ptr] {
                        codes_ptr += 2;
                    }

                    let b1 = pre_codes.get(codes_ptr).copied().unwrap_or_else(|| {
                        reader.warning = true;
                        width
                    });

                    let a1 = (b1 + n).min(width);
                    a0 = a1;
                    codes.push(a0);
                }
                Mode::Vl(n) => {
                    codes_ptr += 1;

                    while codes_ptr < pre_codes.len() && a0 >= pre_codes[codes_ptr] {
                        codes_ptr += 2;
                    }
                    let b1 = pre_codes.get(codes_ptr).copied().unwrap_or_else(|| {
                        reader.warning = true;
                        width
                    });
                    let a1 = b1.saturating_sub(n);
                    a0 = a1;
                    codes.push(a0);

                    codes_ptr = codes_ptr.saturating_sub(2);
                }
                Mode::Ext2D(n) => {
                    let message = format!("not support 2D Ext({}) for CCITT decoder", n);
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        message,
                    )));
                }
                Mode::None => {
                    // fill?
                    reader.get_bits(1)?;
                    // error
                }
                Mode::EOL => {
                    eol = true;
                    break;
                }
            }
        }
        if encoding == Encoder::G4 {
            // G4 Tiff encoding does have eight EOLs at the end.
            if eol {
                // EOBF uncheck
                break;
            }
            /*
                if reader.look_bits(12)? == 1 {  // EOL?
                    reader.skip_bits(12);
                } else {
                    warning!("unexcept eol");
                }
                reader.flush();
            */
        }

        codes.push(width);

        let mut color = WHITE;

        for i in 2..codes.len() {
            for _ in codes[i - 1]..codes[i] {
                data.push(color);
            }
            color ^= BLACK;
        }
        // slow code
        /*
                for i in 0..width {
                    if cur_codes[i] == BLACK {
                        data.push(BLACK);
                    } else {
                        data.push(0);
                    }
                }
                ref_codes = cur_codes.clone();
                cur_codes = vec![UNDEF;width];
                cur_codes.push(TERMINATE);
        */
        y += 1;

        if y >= height {
            break;
        } // G3 Tiff encoding does not have EOL at the end.

        if (encoding == Encoder::G31d || encoding == Encoder::G32d) && !eol {
            loop {
                if reader.look_bits(12)? == 1 {
                    // EOL?
                    reader.skip_bits(12);
                    break;
                }
                reader.skip_bits(1); // fill
            }
        }

        if encoding == Encoder::G32d {
            let v = reader.get_bits(1)?;
            is2d = v == 0;
        }

        if encoding == Encoder::HuffmanRLE {
            reader.flush();
        }
    }

    Ok((data, reader.warning))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_handles_long_fill_without_recursing() {
        let mut reader = BitReader::new(&vec![0; 4096], false);
        let value = reader.value(&white_tree()).unwrap();

        assert_eq!(value, EOL);
        assert!(reader.warning);
    }
}
