// Synthetic ICC v2 CMYK -> XYZ mft2 profile. Generated independently of the
// implementation, with analytic linear RGB subtractive products and a D50
// adapted sRGB matrix. Test-only bytes, no external profile rights required.
pub fn cmyk_profile() -> Vec<u8> {
    fn put(b: &mut [u8], at: usize, value: u32) {
        b[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }
    let mut lut = vec![0u8; 52];
    lut[..4].copy_from_slice(b"mft2");
    lut[8] = 4;
    lut[9] = 3;
    lut[10] = 2;
    for at in [12, 28, 44] {
        put(&mut lut, at, 65536);
    }
    lut[48..52].copy_from_slice(&[0, 2, 0, 2]);
    for _ in 0..4 {
        lut.extend_from_slice(&[0, 0, 255, 255]);
    }
    for c in [0.0, 1.0] {
        for m in [0.0, 1.0] {
            for y in [0.0, 1.0] {
                for k in [0.0, 1.0] {
                    let rgb = [
                        (1.0 - c) * (1.0 - k),
                        (1.0 - m) * (1.0 - k),
                        (1.0 - y) * (1.0 - k),
                    ];
                    for row in [
                        [0.4360747, 0.3850649, 0.1430804],
                        [0.2225045, 0.7168786, 0.0606169],
                        [0.0139322, 0.0971045, 0.7141733],
                    ] {
                        let xyz: f64 = (0..3).map(|i| rgb[i] * row[i]).sum();
                        lut.extend_from_slice(&((xyz * 32768.0).round() as u16).to_be_bytes());
                    }
                }
            }
        }
    }
    for _ in 0..3 {
        lut.extend_from_slice(&[0, 0, 255, 255]);
    }
    let mut profile = vec![0u8; 144];
    profile[8..12].copy_from_slice(&[2, 0x40, 0, 0]);
    profile[12..16].copy_from_slice(b"prtr");
    profile[16..20].copy_from_slice(b"CMYK");
    profile[20..24].copy_from_slice(b"XYZ ");
    profile[36..40].copy_from_slice(b"acsp");
    for (at, value) in [(68, 63190), (72, 65536), (76, 54061), (128, 1)] {
        put(&mut profile, at, value);
    }
    profile[132..136].copy_from_slice(b"A2B0");
    put(&mut profile, 136, 144);
    put(&mut profile, 140, lut.len() as u32);
    profile.extend(lut);
    let size = profile.len() as u32;
    put(&mut profile, 0, size);
    profile
}
