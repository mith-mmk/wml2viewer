//! PNG and APNG decoder implementation.

// use crate::color;
use crate::color::RGBA;
use crate::draw::*;
use crate::error::*;
use crate::png::header::*;
use crate::png::utils::make_metadata;
use crate::png::utils::paeth_dec;
use crate::png::warning::PngWarning;
use crate::warning::*;
use bin_rs::reader::BinaryReader;
type Error = Box<dyn std::error::Error>;

const START_X: [usize; 7] = [0, 0, 4, 0, 2, 0, 1];
const START_Y: [usize; 7] = [0, 4, 0, 2, 0, 1, 0];
const STEP_Y: [usize; 7] = [8, 8, 8, 4, 4, 2, 2];
const STEP_X: [usize; 7] = [8, 8, 4, 4, 2, 2, 1];

fn png_error(kind: ImgErrorKind, message: impl Into<String>) -> Error {
    Box::new(ImgError::new_const(kind, message.into()))
}

fn palette_entries(header: &PngHeader) -> Result<&[RGBA], Error> {
    header
        .pallete
        .as_deref()
        .ok_or_else(|| png_error(ImgErrorKind::IllegalData, "Palette data is missing."))
}

fn draw_rect(header: &PngHeader) -> (u32, u32) {
    if header.frame_controls.is_empty() {
        (header.width, header.height)
    } else {
        let last = header.frame_controls.len() - 1;
        (
            header.frame_controls[last].width,
            header.frame_controls[last].height,
        )
    }
}

fn pass_length(length: usize, start: usize, step: usize) -> usize {
    if length <= start {
        0
    } else {
        (length - start).div_ceil(step)
    }
}

fn decoded_scanline_bytes(header: &PngHeader) -> Result<usize, Error> {
    let (width, height) = draw_rect(header);
    let width = width as usize;
    let height = height as usize;
    let channels = match header.color_type {
        0 | 3 => 1_usize,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => {
            return Err(png_error(
                ImgErrorKind::IllegalData,
                "unsupported PNG color type",
            ));
        }
    };
    let bits_per_pixel = channels
        .checked_mul(header.bitpersample as usize)
        .ok_or_else(|| png_error(ImgErrorKind::InvalidParameter, "PNG pixel size overflow"))?;
    let pass_bytes = |pass_width: usize, pass_height: usize| -> Result<usize, Error> {
        if pass_width == 0 || pass_height == 0 {
            return Ok(0);
        }
        let row_bits = pass_width
            .checked_mul(bits_per_pixel)
            .ok_or_else(|| png_error(ImgErrorKind::InvalidParameter, "PNG row size overflow"))?;
        let row_bytes = row_bits
            .checked_add(7)
            .and_then(|bits| bits.checked_div(8))
            .and_then(|bytes| bytes.checked_add(1))
            .ok_or_else(|| png_error(ImgErrorKind::InvalidParameter, "PNG row size overflow"))?;
        row_bytes
            .checked_mul(pass_height)
            .ok_or_else(|| png_error(ImgErrorKind::InvalidParameter, "PNG data size overflow"))
    };

    if header.interace_method == 0 {
        return pass_bytes(width, height);
    }
    let mut total = 0_usize;
    for pass in 0..7 {
        let pass_width = pass_length(width, START_X[pass], STEP_X[pass]);
        let pass_height = pass_length(height, START_Y[pass], STEP_Y[pass]);
        total = total
            .checked_add(pass_bytes(pass_width, pass_height)?)
            .ok_or_else(|| png_error(ImgErrorKind::InvalidParameter, "PNG data size overflow"))?;
    }
    Ok(total)
}

fn decompress_image_data(
    header: &PngHeader,
    compressed: &[u8],
    drawer: &dyn DrawCallback,
) -> Result<Vec<u8>, Error> {
    drawer.check_decode_control()?;
    let expected = decoded_scanline_bytes(header)?;
    check_decoded_output_bytes(drawer, expected)?;
    let decoded = if drawer.decode_limits().is_some() {
        miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(compressed, expected).map_err(
            |error| {
                if error.status == miniz_oxide::inflate::TINFLStatus::HasMoreOutput {
                    drawer.decode_limit_error(
                        DecodeLimitKind::DecodedBytes,
                        expected.saturating_add(1) as u128,
                        expected as u128,
                    )
                } else {
                    png_error(
                        ImgErrorKind::DecodeError,
                        format!("Uncompressed Error {error:?}"),
                    )
                }
            },
        )?
    } else {
        miniz_oxide::inflate::decompress_to_vec_zlib(compressed).map_err(|error| {
            png_error(
                ImgErrorKind::DecodeError,
                format!("Uncompressed Error {error:?}"),
            )
        })?
    };
    drawer.check_decode_control()?;
    if decoded.len() < expected {
        return Err(png_error(
            ImgErrorKind::UnexpectedEof,
            "PNG decompressed data is shorter than its scanlines",
        ));
    }
    Ok(decoded)
}

fn load_grayscale(
    header: &PngHeader,
    buffer: &[u8],
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let is_alpha = if header.color_type == 4 { 1 } else { 0 };
    let (width, height) = draw_rect(header);
    let raw_length = (width * (header.bitpersample as u32 / 8 + is_alpha) + 1) as usize;
    let mut prev_buf: Vec<u8> = Vec::new();

    for y in 0..height as usize {
        let mut ptr = raw_length * y;
        let flag = buffer[ptr];
        let mut outbuf: Vec<u8> = (0..width * 4).map(|_| 0).collect();
        ptr += 1;
        let mut outptr = 0;
        for _ in 0..width as usize {
            let (mut gray, mut alpha) = (0, 0xff);
            match header.bitpersample {
                16 => {
                    gray = buffer[ptr];
                    ptr += 2;
                    if is_alpha == 1 {
                        alpha = buffer[ptr];
                        ptr += 2;
                    } else {
                        alpha = 0xff;
                    }
                }
                8 => {
                    gray = buffer[ptr];
                    ptr += 1;
                    if is_alpha == 1 {
                        alpha = buffer[ptr];
                        ptr += 1;
                    } else {
                        alpha = 0xff;
                    }
                }
                _ => {}
            }
            match flag {
                1 => {
                    // Sub
                    if outptr > 0 {
                        gray = gray.wrapping_add(outbuf[outptr - 4]);
                        alpha = alpha.wrapping_add(outbuf[outptr - 1]);
                    }
                }
                2 => {
                    // Up
                    if !prev_buf.is_empty() {
                        gray = gray.wrapping_add(prev_buf[outptr]);
                        alpha = alpha.wrapping_add(prev_buf[outptr + 3]);
                    }
                }
                3 => {
                    // Avalage
                    let (mut gray_, mut alpha_);
                    if outptr > 0 {
                        gray_ = outbuf[outptr - 4] as u32;
                        alpha_ = outbuf[outptr - 1] as u32;
                    } else {
                        gray_ = 0;
                        alpha_ = 0;
                    }
                    if !prev_buf.is_empty() {
                        gray_ += prev_buf[outptr] as u32;
                        alpha_ += prev_buf[outptr + 3] as u32;
                    } else {
                        gray_ += 0;
                        alpha_ += 0;
                    }
                    gray_ /= 2;
                    alpha_ /= 2;

                    gray = gray.wrapping_add(gray_ as u8);
                    alpha = alpha.wrapping_add(alpha_ as u8);
                }
                4 => {
                    // Pease
                    let (gray_a, alpha_a);
                    if outptr > 0 {
                        gray_a = outbuf[outptr - 4] as i32;
                        alpha_a = outbuf[outptr - 1] as i32;
                    } else {
                        gray_a = 0;
                        alpha_a = 0;
                    }
                    let (gray_b, alpha_b);
                    if !prev_buf.is_empty() {
                        gray_b = prev_buf[outptr] as i32;
                        alpha_b = prev_buf[outptr + 3] as i32;
                    } else {
                        gray_b = 0;
                        alpha_b = 0;
                    }
                    let (gray_c, alpha_c);
                    if !prev_buf.is_empty() && outptr > 0 {
                        gray_c = prev_buf[outptr - 4] as i32;
                        alpha_c = prev_buf[outptr - 1] as i32;
                    } else {
                        gray_c = 0;
                        alpha_c = 0;
                    }

                    gray = paeth_dec(gray, gray_a, gray_b, gray_c);
                    alpha = paeth_dec(alpha, alpha_a, alpha_b, alpha_c);
                }
                _ => {} // None
            }
            outbuf[outptr] = gray;
            outbuf[outptr + 1] = gray;
            outbuf[outptr + 2] = gray;
            if is_alpha == 0 {
                alpha = 0xff;
            }
            outbuf[outptr + 3] = alpha;
            outptr += 4;
        }
        option.drawer.draw(0, y, width as usize, 1, &outbuf, None)?;
        prev_buf = outbuf;
    }
    Ok(None)
}

fn load_grayscale_progressive(
    header: &PngHeader,
    buffer: &[u8],
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let is_alpha = if header.color_type == 6 { 1 } else { 0 };
    let mut prev_buf: Vec<u8> = Vec::new();

    let (width, height) = draw_rect(header);
    let mut ptr = 0;

    for i in 0..7 {
        let sx = START_Y[i];
        let sy = START_X[i];
        let step_x = STEP_X[i];
        let step_y = STEP_Y[i];
        let mut y = sy;
        while y < height as usize {
            let mut outbuf: Vec<u8> = (0..width * 4).map(|_| 0).collect();
            let flag = buffer[ptr];
            ptr += 1;
            let mut outptr = 0;
            let mut x = sx;
            while x < width as usize {
                let (mut gray, mut alpha) = (0, 0xff);
                match header.bitpersample {
                    16 => {
                        gray = buffer[ptr];
                        ptr += 2;
                        if is_alpha == 1 {
                            alpha = buffer[ptr];
                            ptr += 2;
                        } else {
                            alpha = 0xff;
                        }
                    }
                    8 => {
                        gray = buffer[ptr];
                        ptr += 1;
                        if is_alpha == 1 {
                            alpha = buffer[ptr];
                            ptr += 1;
                        } else {
                            alpha = 0xff;
                        }
                    }
                    _ => {}
                }
                match flag {
                    1 => {
                        // Sub
                        if outptr > 0 {
                            gray = gray.wrapping_add(outbuf[outptr - 4]);
                            alpha = alpha.wrapping_add(outbuf[outptr - 1]);
                        }
                    }
                    2 => {
                        // Up
                        if !prev_buf.is_empty() {
                            gray = gray.wrapping_add(prev_buf[outptr]);
                            alpha = alpha.wrapping_add(prev_buf[outptr + 3]);
                        }
                    }
                    3 => {
                        // Avalage
                        let (mut gray_, mut alpha_);
                        if outptr > 0 {
                            gray_ = outbuf[outptr - 4] as u32;
                            alpha_ = outbuf[outptr - 1] as u32;
                        } else {
                            gray_ = 0;
                            alpha_ = 0;
                        }
                        if !prev_buf.is_empty() {
                            gray_ += prev_buf[outptr] as u32;
                            alpha_ += prev_buf[outptr + 3] as u32;
                        } else {
                            gray_ += 0;
                            alpha_ += 0;
                        }
                        gray_ /= 2;
                        alpha_ /= 2;

                        gray = gray.wrapping_add(gray_ as u8);
                        alpha = alpha.wrapping_add(alpha_ as u8);
                    }
                    4 => {
                        // Pease
                        let (gray_a, alpha_a);
                        if outptr > 0 {
                            gray_a = outbuf[outptr - 4] as i32;
                            alpha_a = outbuf[outptr - 1] as i32;
                        } else {
                            gray_a = 0;
                            alpha_a = 0;
                        }
                        let (gray_b, alpha_b);
                        if !prev_buf.is_empty() {
                            gray_b = prev_buf[outptr] as i32;
                            alpha_b = prev_buf[outptr + 3] as i32;
                        } else {
                            gray_b = 0;
                            alpha_b = 0;
                        }
                        let (gray_c, alpha_c);
                        if !prev_buf.is_empty() && outptr > 0 {
                            gray_c = prev_buf[outptr - 4] as i32;
                            alpha_c = prev_buf[outptr - 1] as i32;
                        } else {
                            gray_c = 0;
                            alpha_c = 0;
                        }

                        gray = paeth_dec(gray, gray_a, gray_b, gray_c);
                        alpha = paeth_dec(alpha, alpha_a, alpha_b, alpha_c);
                    }
                    _ => {} // None
                }
                outbuf[outptr] = gray;
                outbuf[outptr + 1] = gray;
                outbuf[outptr + 2] = gray;
                outbuf[outptr + 3] = alpha;
                outptr += 4;
                option.drawer.draw(x, y, 1, 1, &outbuf, None)?;
                x += step_x;
            }
            y += step_y;
            prev_buf = outbuf;
        }
    }
    Ok(None)
}

fn load_truecolor(
    header: &PngHeader,
    buffer: &[u8],
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let is_alpha = if header.color_type == 6 { 1 } else { 0 };
    let (width, height) = draw_rect(header);
    let raw_length = (width * (header.bitpersample as u32 / 8 * (3 + is_alpha)) + 1) as usize;
    let mut prev_buf: Vec<u8> = Vec::new();

    for y in 0..height as usize {
        let mut ptr = raw_length * y;
        let flag = buffer[ptr];
        if option.debug_flag & 0x4 == 0x4 {
            let string = format!("Y:{} filter is {} ", y, flag);
            option.drawer.verbose(&string, None)?;
        }

        let mut outbuf: Vec<u8> = (0..width * 4).map(|_| 0).collect();
        ptr += 1;
        let mut outptr = 0;
        for _ in 0..width as usize {
            let (mut red, mut green, mut blue, mut alpha);
            if header.bitpersample == 16 {
                red = buffer[ptr];
                ptr += 2;
                green = buffer[ptr];
                ptr += 2;
                blue = buffer[ptr];
                ptr += 2;
                if is_alpha == 1 {
                    alpha = buffer[ptr];
                    ptr += 2;
                } else {
                    alpha = 0xff;
                }
            } else {
                red = buffer[ptr];
                ptr += 1;
                green = buffer[ptr];
                ptr += 1;
                blue = buffer[ptr];
                ptr += 1;
                if is_alpha == 1 {
                    alpha = buffer[ptr];
                    ptr += 1;
                } else {
                    alpha = 0xff;
                }
            }
            match flag {
                1 => {
                    // Sub
                    if outptr > 0 {
                        red = red.wrapping_add(outbuf[outptr - 4]);
                        green = green.wrapping_add(outbuf[outptr - 3]);
                        blue = blue.wrapping_add(outbuf[outptr - 2]);
                        alpha = alpha.wrapping_add(outbuf[outptr - 1]);
                    }
                }
                2 => {
                    // Up
                    if !prev_buf.is_empty() {
                        red = red.wrapping_add(prev_buf[outptr]);
                        green = green.wrapping_add(prev_buf[outptr + 1]);
                        blue = blue.wrapping_add(prev_buf[outptr + 2]);
                        alpha = alpha.wrapping_add(prev_buf[outptr + 3]);
                    }
                }
                3 => {
                    // Avalage
                    let (mut red_, mut green_, mut blue_, mut alpha_);
                    if outptr > 0 {
                        red_ = outbuf[outptr - 4] as u32;
                        green_ = outbuf[outptr - 3] as u32;
                        blue_ = outbuf[outptr - 2] as u32;
                        alpha_ = outbuf[outptr - 1] as u32;
                    } else {
                        red_ = 0;
                        green_ = 0;
                        blue_ = 0;
                        alpha_ = 0;
                    }
                    if !prev_buf.is_empty() {
                        red_ += prev_buf[outptr] as u32;
                        green_ += prev_buf[outptr + 1] as u32;
                        blue_ += prev_buf[outptr + 2] as u32;
                        alpha_ += prev_buf[outptr + 3] as u32;
                    } else {
                        red_ += 0;
                        green_ += 0;
                        blue_ += 0;
                        alpha_ += 0;
                    }
                    red_ /= 2;
                    green_ /= 2;
                    blue_ /= 2;
                    alpha_ /= 2;

                    red = red.wrapping_add(red_ as u8);
                    green = green.wrapping_add(green_ as u8);
                    blue = blue.wrapping_add(blue_ as u8);
                    alpha = alpha.wrapping_add(alpha_ as u8);
                }
                4 => {
                    // Pease
                    let (red_a, green_a, blue_a, alpha_a);
                    if outptr > 0 {
                        red_a = outbuf[outptr - 4] as i32;
                        green_a = outbuf[outptr - 3] as i32;
                        blue_a = outbuf[outptr - 2] as i32;
                        alpha_a = outbuf[outptr - 1] as i32;
                    } else {
                        red_a = 0;
                        green_a = 0;
                        blue_a = 0;
                        alpha_a = 0;
                    }
                    let (red_b, green_b, blue_b, alpha_b);
                    if !prev_buf.is_empty() {
                        red_b = prev_buf[outptr] as i32;
                        green_b = prev_buf[outptr + 1] as i32;
                        blue_b = prev_buf[outptr + 2] as i32;
                        alpha_b = prev_buf[outptr + 3] as i32;
                    } else {
                        red_b = 0;
                        green_b = 0;
                        blue_b = 0;
                        alpha_b = 0;
                    }
                    let (red_c, green_c, blue_c, alpha_c);
                    if !prev_buf.is_empty() && outptr > 0 {
                        red_c = prev_buf[outptr - 4] as i32;
                        green_c = prev_buf[outptr - 3] as i32;
                        blue_c = prev_buf[outptr - 2] as i32;
                        alpha_c = prev_buf[outptr - 1] as i32;
                    } else {
                        red_c = 0;
                        green_c = 0;
                        blue_c = 0;
                        alpha_c = 0;
                    }

                    red = paeth_dec(red, red_a, red_b, red_c);
                    green = paeth_dec(green, green_a, green_b, green_c);
                    blue = paeth_dec(blue, blue_a, blue_b, blue_c);
                    alpha = paeth_dec(alpha, alpha_a, alpha_b, alpha_c);
                }
                _ => {} // None
            }
            outbuf[outptr] = red;
            outbuf[outptr + 1] = green;
            outbuf[outptr + 2] = blue;
            if is_alpha == 0 {
                alpha = 0xff;
            }
            outbuf[outptr + 3] = alpha;
            outptr += 4;
        }
        option.drawer.draw(0, y, width as usize, 1, &outbuf, None)?;
        prev_buf = outbuf;
    }
    Ok(None)
}

fn load_truecolor_progressive(
    header: &PngHeader,
    buffer: &[u8],
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let is_alpha = if header.color_type == 6 { 1 } else { 0 };
    let mut prev_buf: Vec<u8> = Vec::new();
    let (width, height) = draw_rect(header);
    let mut ptr = 0;

    for i in 0..7 {
        let sx = START_Y[i];
        let sy = START_X[i];
        let step_x = STEP_X[i];
        let step_y = STEP_Y[i];
        let mut y = sy;
        while y < height as usize {
            let mut outbuf: Vec<u8> = (0..width * 4).map(|_| 0).collect();
            let flag = buffer[ptr];
            ptr += 1;
            let mut outptr = 0;
            let mut x = sx;
            while x < width as usize {
                let (mut red, mut green, mut blue, mut alpha);
                if header.bitpersample == 16 {
                    red = buffer[ptr];
                    ptr += 2;
                    green = buffer[ptr];
                    ptr += 2;
                    blue = buffer[ptr];
                    ptr += 2;
                    if is_alpha == 1 {
                        alpha = buffer[ptr];
                        ptr += 2;
                    } else {
                        alpha = 0xff;
                    }
                } else {
                    red = buffer[ptr];
                    ptr += 1;
                    green = buffer[ptr];
                    ptr += 1;
                    blue = buffer[ptr];
                    ptr += 1;
                    if is_alpha == 1 {
                        alpha = buffer[ptr];
                        ptr += 1;
                    } else {
                        alpha = 0xff;
                    }
                }
                match flag {
                    1 => {
                        // Sub
                        if outptr > 0 {
                            red = red.wrapping_add(outbuf[outptr - 4]);
                            green = green.wrapping_add(outbuf[outptr - 3]);
                            blue = blue.wrapping_add(outbuf[outptr - 2]);
                            alpha = alpha.wrapping_add(outbuf[outptr - 1]);
                        }
                    }
                    2 => {
                        // Up
                        if !prev_buf.is_empty() {
                            red = red.wrapping_add(prev_buf[outptr]);
                            green = green.wrapping_add(prev_buf[outptr + 1]);
                            blue = blue.wrapping_add(prev_buf[outptr + 2]);
                            alpha = alpha.wrapping_add(prev_buf[outptr + 3]);
                        }
                    }
                    3 => {
                        // Avalage
                        let (mut red_, mut green_, mut blue_, mut alpha_);
                        if outptr > 0 {
                            red_ = outbuf[outptr - 4] as u32;
                            green_ = outbuf[outptr - 3] as u32;
                            blue_ = outbuf[outptr - 2] as u32;
                            alpha_ = outbuf[outptr - 1] as u32;
                        } else {
                            red_ = 0;
                            green_ = 0;
                            blue_ = 0;
                            alpha_ = 0;
                        }
                        if !prev_buf.is_empty() {
                            red_ += prev_buf[outptr] as u32;
                            green_ += prev_buf[outptr + 1] as u32;
                            blue_ += prev_buf[outptr + 2] as u32;
                            alpha_ += prev_buf[outptr + 3] as u32;
                        } else {
                            red_ += 0;
                            green_ += 0;
                            blue_ += 0;
                            alpha_ += 0;
                        }
                        red_ /= 2;
                        green_ /= 2;
                        blue_ /= 2;
                        alpha_ /= 2;

                        red = red.wrapping_add(red_ as u8);
                        green = green.wrapping_add(green_ as u8);
                        blue = blue.wrapping_add(blue_ as u8);
                        alpha = alpha.wrapping_add(alpha_ as u8);
                    }
                    4 => {
                        // Pease
                        let (red_a, green_a, blue_a, alpha_a);
                        if outptr > 0 {
                            red_a = outbuf[outptr - 4] as i32;
                            green_a = outbuf[outptr - 3] as i32;
                            blue_a = outbuf[outptr - 2] as i32;
                            alpha_a = outbuf[outptr - 1] as i32;
                        } else {
                            red_a = 0;
                            green_a = 0;
                            blue_a = 0;
                            alpha_a = 0;
                        }
                        let (red_b, green_b, blue_b, alpha_b);
                        if !prev_buf.is_empty() {
                            red_b = prev_buf[outptr] as i32;
                            green_b = prev_buf[outptr + 1] as i32;
                            blue_b = prev_buf[outptr + 2] as i32;
                            alpha_b = prev_buf[outptr + 3] as i32;
                        } else {
                            red_b = 0;
                            green_b = 0;
                            blue_b = 0;
                            alpha_b = 0;
                        }
                        let (red_c, green_c, blue_c, alpha_c);
                        if !prev_buf.is_empty() && outptr > 0 {
                            red_c = prev_buf[outptr - 4] as i32;
                            green_c = prev_buf[outptr - 3] as i32;
                            blue_c = prev_buf[outptr - 2] as i32;
                            alpha_c = prev_buf[outptr - 1] as i32;
                        } else {
                            red_c = 0;
                            green_c = 0;
                            blue_c = 0;
                            alpha_c = 0;
                        }

                        red = paeth_dec(red, red_a, red_b, red_c);
                        green = paeth_dec(green, green_a, green_b, green_c);
                        blue = paeth_dec(blue, blue_a, blue_b, blue_c);
                        alpha = paeth_dec(alpha, alpha_a, alpha_b, alpha_c);
                    }
                    _ => {} // None
                }
                outbuf[outptr] = red;
                outbuf[outptr + 1] = green;
                outbuf[outptr + 2] = blue;
                if is_alpha == 0 {
                    alpha = 0xff;
                }
                outbuf[outptr + 3] = alpha;
                outptr += 4;
                option.drawer.draw(x, y, 1, 1, &outbuf, None)?;
                x += step_x;
            }
            y += step_y;
            prev_buf = outbuf;
        }
    }
    Ok(None)
}

fn check_color(pallet: &[RGBA], color: usize) -> Result<(), Error> {
    let color = color as usize;
    if color >= pallet.len() {
        return Err(png_error(
            ImgErrorKind::IllegalData,
            format!("palette index {} is out of range {}", color, pallet.len()),
        ));
    }
    Ok(())
}

fn load_index_color(
    header: &PngHeader,
    buffer: &[u8],
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let (width, height) = draw_rect(header);
    let pallet = palette_entries(header)?;

    let raw_length = ((width * header.bitpersample as u32 + 7) / 8 + 1) as usize;

    let mut outbuf: Vec<u8> = (0..width * 4).map(|_| 0).collect();

    for y in 0..height as usize {
        let mut ptr = raw_length * y;
        ptr += 1;
        let mut outptr = 0;
        for x in 0..width as usize {
            let mut color = 0;
            match header.bitpersample {
                8 => {
                    color = buffer[ptr];
                    ptr += 1;
                }
                4 => {
                    if x % 2 == 0 {
                        color = buffer[ptr] >> 4;
                    } else {
                        color = buffer[ptr] & 0xf;
                        ptr += 1;
                    }
                }
                2 => {
                    let shift = 6 - (x % 4) * 2;
                    color = (buffer[ptr] >> shift) & 0x3;
                    if shift == 0 {
                        ptr += 1;
                    }
                }
                1 => {
                    let shift = 7 - (x % 8);
                    color = (buffer[ptr] >> shift) & 0x1;
                    if shift == 0 {
                        ptr += 1;
                    }
                }
                _ => {}
            }
            // index color also no use filter
            let color = color as usize;
            check_color(pallet, color)?;
            outbuf[outptr] = pallet[color].red;
            outbuf[outptr + 1] = pallet[color].green;
            outbuf[outptr + 2] = pallet[color].blue;
            outbuf[outptr + 3] = pallet[color].alpha;
            outptr += 4;
        }
        option.drawer.draw(0, y, width as usize, 1, &outbuf, None)?;
    }
    Ok(None)
}

fn load_index_color_progressive(
    header: &PngHeader,
    buffer: &[u8],
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let pallet = palette_entries(header)?;
    let (width, height) = draw_rect(header);

    let mut outbuf: Vec<u8> = (0..width * 4).map(|_| 0).collect();
    let mut ptr = 0;

    for i in 0..7 {
        let sx = START_Y[i];
        let sy = START_X[i];
        let step_x = STEP_X[i];
        let step_y = STEP_Y[i];
        let mut y = sy;
        while y < height as usize {
            ptr += 1;
            let mut outptr = 0;
            let mut x = sx;
            let mut x_ = 0;
            while x < width as usize {
                let mut color = 0;
                match header.bitpersample {
                    8 => {
                        color = buffer[ptr];
                        ptr += 1;
                    }
                    4 => {
                        if x_ % 2 == 0 {
                            color = buffer[ptr] >> 4;
                        } else {
                            color = buffer[ptr] & 0xf;
                            ptr += 1;
                        }
                    }
                    2 => {
                        let shift = 6 - (x % 4) * 2;
                        color = (buffer[ptr] >> shift) & 0x3;
                        if shift == 0 {
                            ptr += 1;
                        }
                    }
                    1 => {
                        let shift = 7 - (x % 8);
                        color = (buffer[ptr] >> shift) & 0x1;
                        if shift == 0 {
                            ptr += 1;
                        }
                    }
                    _ => {}
                }

                // index color also no use filter
                let color = color as usize;
                check_color(pallet, color)?;
                outbuf[outptr] = pallet[color].red;
                outbuf[outptr + 1] = pallet[color].green;
                outbuf[outptr + 2] = pallet[color].blue;
                outbuf[outptr + 3] = pallet[color].alpha;
                outptr += 4;
                option.drawer.draw(x, y, 1, 1, &outbuf, None)?;
                x_ += 1;
                x += step_x;
            }
            y += step_y;
        }
    }
    Ok(None)
}

fn load(
    header: &mut PngHeader,
    buffer: &[u8],
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    match header.color_type {
        0 | 4 => {
            if header.bitpersample >= 8 {
                if header.interace_method == 0 {
                    return load_grayscale(header, buffer, option);
                } else {
                    return load_grayscale_progressive(header, buffer, option);
                }
            } else {
                let color_max = 1 << header.bitpersample;
                let mut pallet: Vec<RGBA> = Vec::new();
                for i in 0..color_max {
                    let gray = (i * 255 / (color_max - 1)) as u8;
                    pallet.push(RGBA {
                        red: gray,
                        green: gray,
                        blue: gray,
                        alpha: 0xff,
                    });
                }
                header.pallete = Some(pallet);
                if header.interace_method == 0 {
                    return load_index_color(header, buffer, option);
                } else {
                    return load_index_color_progressive(header, buffer, option);
                }
            }
        }
        2 | 6 => {
            if header.interace_method == 0 {
                return load_truecolor(header, buffer, option);
            } else {
                return load_truecolor_progressive(header, buffer, option);
            }
        }
        3 => {
            if header.interace_method == 0 {
                return load_index_color(header, buffer, option);
            } else {
                return load_index_color_progressive(header, buffer, option);
            }
        }
        _ => {
            let string = format!("Color type {} is unknown", header.color_type);
            Err(Box::new(ImgError::new_const(
                ImgErrorKind::IllegalData,
                string,
            )))
        }
    }
}

fn next_options(frame_control: &FrameControl) -> NextOptions {
    let flag = NextOption::Continue;
    let image_rect = Some(ImageRect {
        start_x: frame_control.x_offset as i32,
        start_y: frame_control.y_offset as i32,
        width: frame_control.width as usize,
        height: frame_control.height as usize,
    });
    let delay_den = if frame_control.delay_den == 0 {
        100
    } else {
        frame_control.delay_den
    };
    let await_time = ((frame_control.delay_num as f32 / delay_den as f32) * 1000.0) as u64;
    let dispose_option = Some(match frame_control.dispose_op {
        1 => NextDispose::Background,
        2 => NextDispose::Previous,
        _ => NextDispose::None,
    });

    let blend = Some(match frame_control.blend_op {
        1 => NextBlend::Source,
        _ => NextBlend::Override,
    });
    NextOptions {
        flag,
        await_time,
        image_rect,
        dispose_option,
        blend,
    }
}

pub fn decode<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let maximum_metadata_output = decoded_metadata_limit(option.drawer);
    let mut header = PngHeader::new(reader, option.debug_flag, maximum_metadata_output)?;

    let backgroud = if let Some(ref background) = header.background_color {
        let background = match background {
            BacgroundColor::Grayscale(gray) => RGBA {
                red: *gray as u8,
                green: *gray as u8,
                blue: *gray as u8,
                alpha: 0xff,
            },
            BacgroundColor::TrueColor((red, green, blue)) => RGBA {
                red: *red as u8,
                green: *green as u8,
                blue: *blue as u8,
                alpha: 0xff,
            },
            BacgroundColor::Index(index) => {
                let index = *index as usize;
                let pallete = header.pallete.as_deref().ok_or_else(|| {
                    png_error(
                        ImgErrorKind::IllegalData,
                        "background color references missing palette",
                    )
                })?;
                if index >= pallete.len() {
                    return Err(png_error(
                        ImgErrorKind::OutboundIndex,
                        format!("background palette index {} is out of range", index),
                    ));
                }
                let r = pallete[index].red;
                let g = pallete[index].green;
                let b = pallete[index].blue;
                RGBA {
                    red: r,
                    green: g,
                    blue: b,
                    alpha: 0xff,
                }
            }
        };
        Some(background) // RGBA
    } else {
        None
    };

    let opt = if header.is_apng {
        Some(InitOptions {
            loop_count: header.num_plays,
            background: backgroud,
            animation: true,
        })
    } else {
        Some(InitOptions {
            loop_count: 0,
            background: backgroud,
            animation: false,
        })
    };

    if header.is_apng {
        check_declared_animation_budget(
            option.drawer,
            header.width as usize,
            header.height as usize,
            std::iter::repeat_n((0, 0), header.num_frames as usize),
        )?;
    }
    option
        .drawer
        .init(header.width as usize, header.height as usize, opt)?;
    if let Some(first_frame) = header.frame_controls.first() {
        option
            .drawer
            .preflight_frame(first_frame.width as usize, first_frame.height as usize)?;
    }
    if option.debug_flag > 0 {
        let mut s = "PNG\n".to_string();
        let s_ = format!(
            "width {} height {}  {} bits per sample\n",
            header.width, header.height, header.bitpersample
        );
        s += &s_;
        let s_ = match header.color_type {
            0 => "Color type: Glayscale\n",
            2 => "Color type: Truecolor\n",
            3 => "Color type: Index Color\n",
            4 => "Color type: Glayscale with alpha\n",
            6 => "Color type: Truecolor with alpha\n",
            _ => "Color type: unkwon\n",
        };
        s += s_;
        let s_ = format!("Transparency {:?}\n", header.transparency);
        s += &s_;
        let s_ = format!("Backgroud color {:?}\n", header.background_color);
        s += &s_;
        let s_ = format!("Pallet {:?}\n", header.pallete);
        s += &s_;

        let s_ = format!("Modified time {:?}\n", header.modified_time);
        s += &s_;
        for (key, mes) in &header.text {
            let s_ = format!("{} : {}", key, mes);
            s += &s_;
        }
        option.drawer.verbose(&s, None)?;
        if !header.frame_controls.is_empty() {
            let s = format!("{:?}", header.frame_controls[0]);
            option.drawer.verbose(&s, None)?;
        }
    }

    let mut buffer: Vec<u8> = Vec::new();
    let mut idat = true;
    let mut allow_multi_image = false;

    loop {
        let length = reader.read_u32_be()?;
        let ret_chunck = reader.read_bytes_as_vec(4);
        match ret_chunck {
            Ok(chunck) => {
                if chunck == IMAGE_DATA {
                    if option.debug_flag > 1 {
                        let string = format!("read compressed image data {} bytes", length);
                        option.drawer.verbose(&string, None)?;
                    }
                    let mut buf = reader.read_bytes_as_vec(length as usize)?;
                    buffer.append(&mut buf);
                    let _crc = reader.read_u32_be()?;
                } else {
                    if idat {
                        match decompress_image_data(&header, &buffer, option.drawer) {
                            Ok(debuffer) => {
                                load(&mut header.clone(), &debuffer, option)?;
                                if !header.frame_controls.is_empty() {
                                    let frame_control = &header.frame_controls[0];
                                    let next = next_options(frame_control);
                                    let result = option.drawer.next(Some(next))?;
                                    if let Some(response) = result {
                                        if response.response == ResponseCommand::Continue {
                                            allow_multi_image = true;
                                            load(&mut header.clone(), &debuffer, option)?;
                                            // Image = Animation Frame 0
                                        }
                                    }
                                }
                            }
                            Err(error) => return Err(error),
                        }

                        idat = false;
                        buffer = vec![];
                    }
                    if chunck == IMAGE_END {
                        if !buffer.is_empty() {
                            match decompress_image_data(&header, &buffer, option.drawer) {
                                Ok(debuffer) => {
                                    load(&mut header, &debuffer, option)?;
                                }
                                Err(error) => return Err(error),
                            }
                        }
                        break;
                    } else if chunck == TEXTDATA || chunck == I18N_TEXT {
                        let text = reader.read_bytes_as_vec(length as usize)?;
                        let (keyword, string) = to_string(&text, false, maximum_metadata_output);
                        header.text.push((keyword, string));
                        let _crc = reader.read_u32_be()?;
                    } else if chunck == COMPRESSED_TEXTUAL_DATA {
                        let text = reader.read_bytes_as_vec(length as usize)?;
                        let (keyword, string) = to_string(&text, true, maximum_metadata_output);
                        header.text.push((keyword, string));
                        let _crc = reader.read_u32_be()?;
                    } else if chunck == C2PA_CHUNK {
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
                    } else if chunck == ANIMATION_CONTROLE {
                        // noimpl error!
                        reader.skip_ptr(length as usize)?;
                        let _crc = reader.read_u32_be()?;
                    } else if chunck == FRAME_CONTROLE {
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
                        if !buffer.is_empty() && allow_multi_image {
                            match decompress_image_data(&header, &buffer, option.drawer) {
                                Ok(debuffer) => {
                                    load(&mut header, &debuffer, option)?;
                                }
                                Err(error) => return Err(error),
                            }
                        }
                        buffer = vec![];

                        option.drawer.preflight_frame(
                            frame_control.width as usize,
                            frame_control.height as usize,
                        )?;
                        let next = next_options(&frame_control);
                        let result = option.drawer.next(Some(next))?;
                        if let Some(response) = result {
                            if response.response == ResponseCommand::Continue {
                                allow_multi_image = true;
                            }
                            if option.debug_flag > 0 {
                                let str = format!("{:?}", frame_control);
                                option.drawer.verbose(&str, None)?;
                            }
                        }

                        header.frame_controls.push(frame_control);

                        let _crc = reader.read_u32_be()?;
                    } else if chunck == FRAME_DATA {
                        let sequence_number = reader.read_u32_be()?;
                        if option.debug_flag > 0 {
                            let string = format!(
                                "read compressed animation image data:{} {} bytes",
                                sequence_number,
                                length - 4
                            );
                            option.drawer.verbose(&string, None)?;
                        }

                        let mut buf = reader.read_bytes_as_vec(length as usize - 4)?;
                        buffer.append(&mut buf);
                        let _crc = reader.read_u32_be()?;
                    } else {
                        reader.skip_ptr(length as usize)?;
                        let _crc = reader.read_u32_be()?;
                    }
                }
            }
            Err(_) => {
                let warnings = ImgWarnings::add(
                    None,
                    Box::new(PngWarning::new(
                        "Data crruption after image datas".to_string(),
                    )),
                );
                if option.debug_flag > 1 {
                    let string = format!("{:?}", &header);
                    option.drawer.verbose(&string, None)?;
                }
                option.drawer.terminate(None)?;
                return Ok(warnings);
            }
        }
    }
    if option.debug_flag > 1 {
        let string = format!("{:?}", &header);
        option.drawer.verbose(&string, None)?;
    }
    let map = make_metadata(&header, maximum_metadata_output);
    for (key, value) in &map {
        option.drawer.set_metadata(key, value.clone())?;
    }

    option.drawer.terminate(None)?;
    Ok(None)
}
