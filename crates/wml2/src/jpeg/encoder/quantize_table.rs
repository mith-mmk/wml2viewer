//! Quantization tables for JPEG encoding.

const FDCT_DIV: f32 = 8.0;

pub(crate) const LUMA_QTABLE: [u8; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];

pub(crate) const CHROMA_QTABLE: [u8; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];

pub fn create_qt(q: usize) -> (Vec<f32>, Vec<f32>) {
    let mut lq = Vec::with_capacity(64);
    let mut cq = Vec::with_capacity(64);
    let scale = if q < 50 {
        q as f32 / 5000.0
    } else {
        200.0 - 2.0 * q as f32
    };

    for i in 0..64 {
        let lv = (scale * LUMA_QTABLE[i] as f32 + 50.0) / 100.0 / FDCT_DIV;
        let cv = (scale * CHROMA_QTABLE[i] as f32 + 50.0) / 100.0 / FDCT_DIV;
        lq.push(lv);
        cq.push(cv);
    }

    (lq, cq)
}

pub(crate) fn scaled_quant_tables(quality: usize) -> ([u8; 64], [u8; 64]) {
    let quality = quality.clamp(1, 100);
    let scale = if quality < 50 {
        5000 / quality
    } else {
        200 - quality * 2
    };

    let mut luma = [0u8; 64];
    let mut chroma = [0u8; 64];

    for i in 0..64 {
        let lv = ((LUMA_QTABLE[i] as usize * scale + 50) / 100).clamp(1, 255);
        let cv = ((CHROMA_QTABLE[i] as usize * scale + 50) / 100).clamp(1, 255);
        luma[i] = lv as u8;
        chroma[i] = cv as u8;
    }

    (luma, chroma)
}
