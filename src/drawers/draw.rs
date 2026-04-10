//! blend screen and screen.
/*
 * draw.rs  Mith@mmk (C) 2022
 *
 */

use crate::canvas::*;
use crate::utils::calc_alphablend;

pub fn draw_over_screen(src: &dyn Screen, dest: &mut dyn Screen, dx: i32, dy: i32) {
    let width = src.width();
    let height = src.height();

    let dest_width = dest.width();
    let dest_height = dest.height();
    if dx + (width as i32) < 0 {
        return;
    }
    if dy + (height as i32) < 0 {
        return;
    }
    if dx >= dest_width as i32 {
        return;
    }
    if dy >= dest_height as i32 {
        return;
    }

    let srcbuf = &src.buffer();
    let destbuf = &mut dest.buffer_mut(); // move ownership dest -> destbuf because use mut

    let sx = if dx < 0 { 0 } else { dx as u32 };
    let sy = if dy < 0 { 0 } else { dy as u32 };
    let ex = if dx + (width as i32) >= dest_width as i32 {
        dest_width
    } else {
        (dx + (width as i32)) as u32
    };
    let ey = if dy + (height as i32) >= dest_height as i32 {
        dest_height
    } else {
        (dy + (height as i32)) as u32
    };

    for y in sy..ey {
        let dest_offset = (y * dest_width * 4) as usize;
        let src_offset = ((y as i32 - dy) as u32 * width * 4) as usize;
        for x in sx..ex {
            let dest_pos = dest_offset + x as usize * 4;
            let src_pos = src_offset + (x as i32 - dx) as usize * 4;

            destbuf[dest_pos] = srcbuf[src_pos];
            destbuf[dest_pos + 1] = srcbuf[src_pos + 1];
            destbuf[dest_pos + 2] = srcbuf[src_pos + 2];
            destbuf[dest_pos + 3] = 0xff; // alpha is must 0xff
        }
    }
}

pub fn draw_over_screen_with_alpha(src: &dyn Screen, dest: &mut dyn Screen, dx: i32, dy: i32) {
    let width = src.width();
    let height = src.height();

    let dest_width = dest.width();
    let dest_height = dest.height();
    if dx + (width as i32) < 0 {
        return;
    }
    if dy + (height as i32) < 0 {
        return;
    }
    if dx >= dest_width as i32 {
        return;
    }
    if dy >= dest_height as i32 {
        return;
    }

    let srcbuf = &src.buffer();
    let destbuf = &mut dest.buffer_mut(); // move ownership dest -> destbuf because use mut

    let sx = if dx < 0 { 0 } else { dx as u32 };
    let sy = if dy < 0 { 0 } else { dy as u32 };
    let ex = if dx + (width as i32) >= dest_width as i32 {
        dest_width
    } else {
        (dx + (width as i32)) as u32
    };
    let ey = if dy + (height as i32) >= dest_height as i32 {
        dest_height
    } else {
        (dy + (height as i32)) as u32
    };
    let global_alpha = if let Some(ga) = src.alpha() {
        ga as f32 / 255.0
    } else {
        1.0
    };

    for y in sy..ey {
        let dest_offset = (y * dest_width * 4) as usize;
        let src_offset = ((y as i32 - dy) as u32 * width * 4) as usize;
        for x in sx..ex {
            let dest_pos = dest_offset + x as usize * 4;
            let src_pos = src_offset + (x as i32 - dx) as usize * 4;
            let alpha = (srcbuf[src_pos + 3] as f32 / 255.0) * global_alpha;
            if alpha == 0.0 {
                continue;
            }

            destbuf[dest_pos] = calc_alphablend(srcbuf[src_pos], destbuf[dest_pos], alpha);
            destbuf[dest_pos + 1] =
                calc_alphablend(srcbuf[src_pos + 1], destbuf[dest_pos + 1], alpha);
            destbuf[dest_pos + 2] =
                calc_alphablend(srcbuf[src_pos + 2], destbuf[dest_pos + 2], alpha);
            destbuf[dest_pos + 3] = 0xff; // alpha is must 0xff
        }
    }
}
