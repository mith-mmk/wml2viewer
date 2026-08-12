//! Forward DCT routines for JPEG encoding.

#[cfg(feature = "fdct_slower")]
pub(crate) fn fdct_block(input: &[f32; 64]) -> [f32; 64] {
    let mut out = [0.0_f32; 64];
    let pi = std::f32::consts::PI;

    for v in 0..8 {
        for u in 0..8 {
            let mut sum = 0.0;
            for y in 0..8 {
                for x in 0..8 {
                    let sample = input[y * 8 + x];
                    let cos_x = (((2 * x + 1) as f32) * u as f32 * pi / 16.0).cos();
                    let cos_y = (((2 * y + 1) as f32) * v as f32 * pi / 16.0).cos();
                    sum += sample * cos_x * cos_y;
                }
            }
            let cu = if u == 0 { 1.0 / 2.0_f32.sqrt() } else { 1.0 };
            let cv = if v == 0 { 1.0 / 2.0_f32.sqrt() } else { 1.0 };
            out[v * 8 + u] = 0.25 * cu * cv * sum;
        }
    }

    out
}

#[cfg(not(feature = "fdct_slower"))]
pub(crate) fn fdct_block(f: &[f32; 64]) -> [f32; 64] {
    let m0 = 0.7071067811865475;
    let m1 = 1.3870398453221475;
    let m2 = 1.3065629648763766;
    let m3 = 1.1758756024193588;
    let m5 = 0.7856949583871023;
    let m6 = 0.5411961001461971;
    let m7 = 0.2758993792829431;
    let mut zz = [0_f32; 64];

    for j in 0..8 {
        let i = j * 8;
        let f0 = f[i];
        let f1 = f[i + 1];
        let f2 = f[i + 2];
        let f3 = f[i + 3];
        let f4 = f[i + 4];
        let f5 = f[i + 5];
        let f6 = f[i + 6];
        let f7 = f[i + 7];

        let a0 = f0 + f7;
        let a7 = f0 - f7;
        let a1 = f1 + f6;
        let a6 = f1 - f6;
        let a2 = f2 + f5;
        let a5 = f2 - f5;
        let a3 = f3 + f4;
        let a4 = f3 - f4;

        let c0 = a0 + a3;
        let c3 = a0 - a3;
        let c1 = a1 + a2;
        let c2 = a1 - a2;

        zz[i + 0] = c0 + c1;
        zz[i + 4] = c0 - c1;
        zz[i + 2] = c2 * m6 + c3 * m2;
        zz[i + 6] = c3 * m6 - c2 * m2;

        let c3 = a4 * m3 + a7 * m5;
        let c0 = a7 * m3 - a4 * m5;
        let c2 = a5 * m1 + a6 * m7;
        let c1 = a6 * m1 - a5 * m7;

        zz[i + 5] = c3 - c1;
        zz[i + 3] = c0 - c2;

        let d0 = (c0 + c2) * m0;
        let d3 = (c3 + c1) * m0;

        zz[i + 1] = d0 + d3;
        zz[i + 7] = d0 - d3;
    }

    for i in 0..8 {
        let f0 = zz[i + 0 * 8];
        let f1 = zz[i + 1 * 8];
        let f2 = zz[i + 2 * 8];
        let f3 = zz[i + 3 * 8];
        let f4 = zz[i + 4 * 8];
        let f5 = zz[i + 5 * 8];
        let f6 = zz[i + 6 * 8];
        let f7 = zz[i + 7 * 8];

        let a0 = f0 + f7;
        let a7 = f0 - f7;
        let a1 = f1 + f6;
        let a6 = f1 - f6;
        let a2 = f2 + f5;
        let a5 = f2 - f5;
        let a3 = f3 + f4;
        let a4 = f3 - f4;

        let c0 = a0 + a3;
        let c3 = a0 - a3;
        let c1 = a1 + a2;
        let c2 = a1 - a2;

        zz[i + 0 * 8] = c0 + c1;
        zz[i + 4 * 8] = c0 - c1;
        zz[i + 2 * 8] = c2 * m6 + c3 * m2;
        zz[i + 6 * 8] = c3 * m6 - c2 * m2;

        let c3 = a4 * m3 + a7 * m5;
        let c0 = a7 * m3 - a4 * m5;
        let c2 = a5 * m1 + a6 * m7;
        let c1 = a6 * m1 - a5 * m7;

        zz[i + 5 * 8] = c3 - c1;
        zz[i + 3 * 8] = c0 - c2;

        let d0 = (c0 + c2) * m0;
        let d3 = (c3 + c1) * m0;

        zz[i + 1 * 8] = d0 + d3;
        zz[i + 7 * 8] = d0 - d3;

        for j in 0..8 {
            zz[i + j * 8] *= 0.125;
        }
    }

    zz
}

#[cfg(test)]
mod tests {
    use super::fdct_block;

    #[cfg(feature = "fdct_slower")]
    fn fdct_reference(input: &[f32; 64]) -> [f32; 64] {
        let mut out = [0.0_f32; 64];
        let pi = std::f32::consts::PI;

        for v in 0..8 {
            for u in 0..8 {
                let mut sum = 0.0;
                for y in 0..8 {
                    for x in 0..8 {
                        let sample = input[y * 8 + x];
                        let cos_x = (((2 * x + 1) as f32) * u as f32 * pi / 16.0).cos();
                        let cos_y = (((2 * y + 1) as f32) * v as f32 * pi / 16.0).cos();
                        sum += sample * cos_x * cos_y;
                    }
                }
                let cu = if u == 0 { 1.0 / 2.0_f32.sqrt() } else { 1.0 };
                let cv = if v == 0 { 1.0 / 2.0_f32.sqrt() } else { 1.0 };
                out[v * 8 + u] = 0.25 * cu * cv * sum;
            }
        }

        out
    }

    #[cfg(feature = "fdct_slower")]
    fn assert_close(lhs: &[f32; 64], rhs: &[f32; 64], epsilon: f32) {
        for i in 0..64 {
            let diff = (lhs[i] - rhs[i]).abs();
            assert!(
                diff <= epsilon,
                "coefficient {} differs: lhs={} rhs={} diff={}",
                i,
                lhs[i],
                rhs[i],
                diff
            );
        }
    }

    fn gradient_block() -> [f32; 64] {
        let mut block = [0.0_f32; 64];
        for y in 0..8 {
            for x in 0..8 {
                block[y * 8 + x] = (x as f32 * 7.0) + (y as f32 * 13.0) - 64.0;
            }
        }
        block
    }

    fn signed_pattern_block() -> [f32; 64] {
        [
            -76.0, -73.0, -67.0, -62.0, -58.0, -67.0, -64.0, -55.0, -65.0, -69.0, -73.0, -38.0,
            -19.0, -43.0, -59.0, -56.0, -66.0, -69.0, -60.0, -15.0, 16.0, -24.0, -62.0, -55.0,
            -65.0, -70.0, -57.0, -6.0, 26.0, -22.0, -58.0, -59.0, -61.0, -67.0, -60.0, -24.0, -2.0,
            -40.0, -60.0, -58.0, -49.0, -63.0, -68.0, -58.0, -51.0, -60.0, -70.0, -53.0, -43.0,
            -57.0, -64.0, -69.0, -73.0, -67.0, -63.0, -45.0, -41.0, -49.0, -59.0, -60.0, -63.0,
            -52.0, -50.0, -34.0,
        ]
    }

    #[test]
    fn fdct_produces_finite_values_for_level_shifted_gradient() {
        let out = fdct_block(&gradient_block());
        assert!(out.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn fdct_produces_finite_values_for_signed_pattern() {
        let out = fdct_block(&signed_pattern_block());
        assert!(out.iter().all(|value| value.is_finite()));
    }

    #[cfg(feature = "fdct_slower")]
    #[test]
    fn slower_fdct_matches_reference_for_level_shifted_gradient() {
        let block = gradient_block();
        let reference = fdct_reference(&block);
        let actual = fdct_block(&block);
        assert_close(&reference, &actual, 0.001);
    }

    #[cfg(feature = "fdct_slower")]
    #[test]
    fn slower_fdct_matches_reference_for_signed_pattern() {
        let block = signed_pattern_block();
        let reference = fdct_reference(&block);
        let actual = fdct_block(&block);
        assert_close(&reference, &actual, 0.001);
    }
}
