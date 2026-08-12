//! MAG header parsing.

use bin_rs::reader::BinaryReader;
type Error = Box<dyn std::error::Error>;

pub struct MAGFileHeader {
    header: [u8; 8],
    machine: [u8; 4],
    user: [u8; 18],
    comment: Option<Vec<u8>>,
}

// typedef struct { uchar mgHeader; uchar mgMachine; uchar mgSystem; uchar mgScreenmode;
//  ushort mgStartX, mgStartY, mgEndX, mgEndY;
//  ulong mgFlagA_offset, mgFlagB_offset, mgFlagBSize, mgPixelD_offset, mgPixelDSize; } MAGH;
pub struct MAGHeader {
    pub header: u8,
    pub machine: u8,
    pub system: u8,
    pub screenmode: u8,
    pub start_x: u16,
    pub start_y: u16,
    pub end_x: u16,
    pub end_y: u16,
    pub flag_a_offset: u32,
    pub flag_b_offset: u32,
    pub flag_b_size: u32,
    pub pixel_d_offset: u32,
    pub pixel_d_size: u32,
    //
    pub palette: Vec<(u8, u8, u8)>,
}

impl MAGFileHeader {
    pub fn new<B: BinaryReader>(reader: &mut B) -> Result<Self, Error> {
        let mut header = [0; 8];
        reader.read_exact(&mut header)?;
        let mut machine = [0; 4];
        reader.read_exact(&mut machine)?;
        let mut user = [0; 18];
        reader.read_exact(&mut user)?;
        // Scan comment Area until 0x1A
        let mut comment = Vec::new();
        loop {
            let b = reader.read_byte()?;
            if b == 0x1A {
                break;
            }
            comment.push(b);
        }
        // The comment is encoded in Shift-JIS, but we will just treat it as ASCII for simplicity.
        Ok(Self {
            header,
            machine,
            user,
            comment: Some(comment),
        })
    }

    pub fn get_header(&self) -> String {
        let mut s = String::new();
        for &b in &self.header {
            if b == 0 {
                break;
            }
            s.push(b as char);
        }
        s
    }

    pub fn get_machine(&self) -> String {
        let mut s = String::new();
        for &b in &self.machine {
            if b == 0 {
                break;
            }
            s.push(b as char);
        }
        s
    }
    // The user field is a null-terminated string, encode as Shift-JIS, but we will just treat it as ASCII for simplicity.
    pub fn get_user(&self) -> Vec<u8> {
        let mut s = Vec::new();
        for &b in &self.user {
            if b == 0 {
                break;
            }
            s.push(b);
        }
        s
    }

    pub fn get_comment(&self) -> Option<Vec<u8>> {
        self.comment.clone()
    }
}

impl MAGHeader {
    pub fn new<B: BinaryReader>(reader: &mut B) -> Result<Self, Error> {
        let mut own = Self {
            header: reader.read_byte()?,
            machine: reader.read_byte()?,
            system: reader.read_byte()?,
            screenmode: reader.read_byte()?,
            start_x: reader.read_u16_le()?,
            start_y: reader.read_u16_le()?,
            end_x: reader.read_u16_le()?,
            end_y: reader.read_u16_le()?,
            flag_a_offset: reader.read_u32_le()?,
            flag_b_offset: reader.read_u32_le()?,
            flag_b_size: reader.read_u32_le()?,
            pixel_d_offset: reader.read_u32_le()?,
            pixel_d_size: reader.read_u32_le()?,
            palette: Vec::new(),
        };
        let num_colors = if own.screenmode & 0x80 != 0 { 256 } else { 16 };
        for _ in 0..num_colors {
            let g = reader.read_byte().unwrap_or(0);
            let r = reader.read_byte().unwrap_or(0);
            let b = reader.read_byte().unwrap_or(0);
            own.palette.push((r, g, b));
        }
        Ok(own)
    }

    pub fn get_number_of_colors(&self) -> usize {
        let bpp = if self.screenmode & 0x80 != 0 { 8 } else { 4 };
        1 << bpp
    }
}
