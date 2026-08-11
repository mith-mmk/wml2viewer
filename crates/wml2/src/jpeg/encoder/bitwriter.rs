//! Bitstream writer for JPEG encoding.

type Error = Box<dyn std::error::Error>;

pub(crate) struct BitWriter {
    pub buf: Vec<u8>,
    pub(crate) bptr: usize,
    pub(crate) b: u8,
}

impl BitWriter {
    pub fn new() -> Self {
        Self {
            buf: vec![],
            bptr: 0,
            b: 0,
        }
    }

    pub fn write_bits(&mut self, bits: u16, len: usize) -> Result<(), Error> {
        if len == 0 {
            return Ok(());
        }
        if len + self.bptr >= 8 {
            let mut len = len;
            let shift = 8 - self.bptr;

            self.b = ((self.b as u32) << shift & 0xff) as u8 | (bits >> (len - shift) & 0xff) as u8;
            if self.b == 0xff {
                self.buf.push(0xff);
                self.buf.push(0x00);
            } else {
                self.buf.push(self.b);
            }
            len -= shift;
            while len >= 8 {
                len -= 8;
                let b = ((bits >> len) & 0xff) as u8;
                if b == 0xff {
                    self.buf.push(0xff);
                    self.buf.push(0x00);
                } else {
                    self.buf.push(b);
                }
            }
            self.b = (bits & ((1 << len) - 1)) as u8;
            self.bptr = len;
        } else {
            self.b = self.b << len | bits as u8;
            self.bptr += len;
        }

        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        if self.bptr > 0 {
            let b = self.b << (8 - self.bptr);
            self.buf.push(b);
            self.bptr = 0;
        }
        self.b = 0;
        Ok(())
    }
}
