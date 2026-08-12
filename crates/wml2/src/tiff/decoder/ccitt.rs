//! CCITT-compressed TIFF decoding helpers.

// Tiff depend
type Error = Box<dyn std::error::Error>;
use crate::decoder::ccitt::{Encoder, decoder};
use crate::error::*;
use crate::tiff::decoder::Tiff;
use crate::tiff::header::Compression;

pub fn decode(buf: &[u8], header: &Tiff) -> Result<(Vec<u8>, bool), Error> {
    let t4_options = header.t4_options;
    let t6_options = header.t6_options;

    let encoding = match header.compression {
        Compression::CCITTHuffmanRLE => {
            // no test
            Encoder::HuffmanRLE
        }
        Compression::CCITTGroup3Fax => {
            if t4_options & 0x01 == 0 {
                Encoder::G31d //1D
            } else {
                Encoder::G32d
            }
        }
        Compression::CCITTGroup4Fax => Encoder::G4,
        _ => {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::DecodeError,
                "This encoding is not CCITT".to_string(),
            )));
        }
    };

    if t4_options & 0x2 > 0 || t6_options & 0x2 > 0 {
        //        encoding = 0;   // UNCOMPRESSED
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Uncompress mode is not support".to_string(),
        )));
    }

    //    let photometric_interpretation = header.photometric_interpretation.clone();

    let width = header.width as usize;
    let height = header.height as usize;
    let is_lsb = header.fill_order == 2;

    decoder(buf, width, height, encoding, is_lsb)
}
