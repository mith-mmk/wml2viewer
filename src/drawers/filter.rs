use crate::canvas::*;
use crate::grayscale::to_grayscale;
use std::io::Error;

enum FilterType {
    Copy,
    Median,
    Erode,
    Dilate,
    Sharpness,
    Blur,
    Average,
    Smooth,
    Sharpen,
    Shadow,
    Canny,
    Edges,
    EdgeX,
    EdgeY,
    Gaussian,
    Laplacian,
    Laplacian8,
    Emboss,
    Outline,
    Grayscale,
    Unknown,
}

impl From<&str> for FilterType {
    fn from(s: &str) -> Self {
        match s {
            "copy" => FilterType::Copy,
            "median" => FilterType::Median,
            "erode" => FilterType::Erode,
            "dilate" => FilterType::Dilate,
            "sharpness" => FilterType::Sharpness,
            "blur" => FilterType::Blur,
            "average" => FilterType::Average,
            "smooth" => FilterType::Smooth,
            "sharpen" => FilterType::Sharpen,
            "shadow" => FilterType::Shadow,
            "canny" => FilterType::Canny,
            "edges" => FilterType::Edges,
            "edgeX" => FilterType::EdgeX,
            "edgeY" => FilterType::EdgeY,
            "gaussian" => FilterType::Gaussian,
            "laplacian" => FilterType::Laplacian,
            "laplacian8" => FilterType::Laplacian8,
            "emboss" => FilterType::Emboss,
            "outline" => FilterType::Outline,
            "grayscale" => FilterType::Grayscale,
            _ => FilterType::Unknown,
        }
    }
}


pub struct Kernel {
    pub width: usize,
    pub height: usize,
    pub matrix: Vec<Vec<f32>>,
}

impl Kernel {
    pub fn new(matrix: [[f32; 3]; 3]) -> Self {
        let matrix = vec![matrix[0].to_vec(), matrix[1].to_vec(), matrix[2].to_vec()];
        Self {
            width: 3,
            height: 3,
            matrix,
        }
    }
}

#[inline]
fn rgb_to_yuv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32;
    let g = g as f32;
    let b = b as f32;
    let y = 0.29900 * r + 0.58700 * g + 0.11400 * b;
    let u = -0.16874 * r - 0.33126 * g + 0.50000 * b;
    let v = 0.50000 * r - 0.41869 * g - 0.081 * b;
    (y, u, v)
}

#[inline]
fn rgb_to_y(r: u8, g: u8, b: u8) -> f32 {
    let r = r as f32;
    let g = g as f32;
    let b = b as f32;
    
    0.29900 * r + 0.58700 * g + 0.11400 * b
}

#[inline]
fn yuv_to_rgb(y: f32, u: f32, v: f32) -> (u8, u8, u8) {
    let crr = 1.402;
    let cbg = -0.34414;
    let crg = -0.71414;
    let cbb = 1.772;

    let r = (y + (crr * v)) as i32;
    let g = (y + (cbg * u) + crg * v) as i32;
    let b = (y + (cbb * u)) as i32;

    let r = r.clamp(0, 255) as u8;
    let g = g.clamp(0, 255) as u8;
    let b = b.clamp(0, 255) as u8;

    (r, g, b)
}

pub fn lum_filter(src: &dyn Screen, dest: &mut dyn Screen, kernel: &Kernel) {
    if dest.width() == 0 || dest.height() == 0 {
        dest.reinit(src.width(), src.height());
    }
    let dest_height = dest.height() as usize;
    let dest_width = dest.width() as usize;

    let mut coeff = 0.0;
    let matrix = &kernel.matrix;
    let u0 = (kernel.height + 1) as i32;
    let v0 = (kernel.width + 1) as i32;
    for u in 0..kernel.height {
        for v in 0..kernel.width {
            coeff += matrix[v][u];
        }
    }
    let src_buffer = src.buffer();
    let dest_buffer = dest.buffer_mut();

    for y in 0..src.height() as usize {
        let offset = y * src.width() as usize * 4;
        if y >= dest_height {
            break;
        }
        for x in 0..src.width() as usize {
            if x >= dest_width {
                break;
            }
            let r = src_buffer[offset + x * 4];
            let g = src_buffer[offset + x * 4 + 1];
            let b = src_buffer[offset + x * 4 + 2];
            let a = src_buffer[offset + x * 4 + 3];
            let mut l = 0.0;
            for u in 0..kernel.height {
                let uu = (y as i32 + u as i32 - u0).clamp(0, src.height() as i32 - 1) as usize
                    * src.width() as usize
                    * 4;
                for v in 0..kernel.width {
                    let vv =
                        (x as i32 + v as i32 - v0).clamp(0, src.width() as i32 - 1) as usize * 4;
                    let r = src_buffer[uu + vv];
                    let g = src_buffer[uu + vv + 1];
                    let b = src_buffer[uu + vv + 2];
                    let ln = rgb_to_y(r, g, b);
                    l += ln * matrix[v][u];
                }
            }
            let l = l / coeff;
            let (_, u, v) = rgb_to_yuv(r, g, b);
            let (r, g, b) = yuv_to_rgb(l, u, v);

            dest_buffer[offset + x * 4] = r;
            dest_buffer[offset + x * 4 + 1] = g;
            dest_buffer[offset + x * 4 + 2] = b;
            dest_buffer[offset + x * 4 + 3] = a;
        }
    }
}

pub fn grayscale(src: &dyn Screen, dest: &mut dyn Screen) {
    if dest.width() == 0 || dest.height() == 0 {
        dest.reinit(src.width(), src.height());
    }
    let dest_height = dest.height() as usize;
    let dest_width = dest.width() as usize;

    let src_buffer = src.buffer();
    let dest_buffer = dest.buffer_mut();

    for y in 0..src.height() as usize {
        let offset = y * src.width() as usize * 4;
        if y >= dest_height {
            break;
        }
        for x in 0..src.width() as usize {
            if x >= dest_width {
                break;
            }
            let r = src_buffer[offset + x * 4];
            let g = src_buffer[offset + x * 4 + 1];
            let b = src_buffer[offset + x * 4 + 2];
            let a = src_buffer[offset + x * 4 + 3];
            let (l, _, _) = rgb_to_yuv(r, g, b);
            let l = (l.round() as i16).clamp(0, 255) as u8;

            dest_buffer[offset + x * 4] = l;
            dest_buffer[offset + x * 4 + 1] = l;
            dest_buffer[offset + x * 4 + 2] = l;
            dest_buffer[offset + x * 4 + 3] = a;
        }
    }
}

pub fn rgb_filter(src: &dyn Screen, dest: &mut dyn Screen, kernel: &Kernel) {
    if dest.width() == 0 || dest.height() == 0 {
        dest.reinit(src.width(), src.height());
    }
    let dest_height = dest.height() as usize;
    let dest_width = dest.width() as usize;

    let mut coeff = 0.0;
    let matrix = &kernel.matrix;
    let u0 = (kernel.height + 1) as i32;
    let v0 = (kernel.width + 1) as i32;
    for u in 0..kernel.height {
        for v in 0..kernel.width {
            coeff += matrix[v][u];
        }
    }
    let src_buffer = src.buffer();
    let dest_buffer = dest.buffer_mut();

    for y in 0..src.height() as usize {
        let offset = y * src.width() as usize * 4;
        if y >= dest_height {
            break;
        }
        for x in 0..src.width() as usize {
            if x >= dest_width {
                break;
            }
            let mut r = 0.0;
            let mut g = 0.0;
            let mut b = 0.0;
            let a = src_buffer[offset + x * 4 + 3];
            for u in 0..kernel.height {
                let uu = (y as i32 + u as i32 - u0).clamp(0, src.height() as i32 - 1) as usize
                    * src.width() as usize
                    * 4;
                for v in 0..kernel.width {
                    let vv =
                        (x as i32 + v as i32 - v0).clamp(0, src.width() as i32 - 1) as usize * 4;
                    r += src_buffer[uu + vv] as f32 * matrix[v][u];
                    g += src_buffer[uu + vv + 1] as f32 * matrix[v][u];
                    b += src_buffer[uu + vv + 2] as f32 * matrix[v][u];
                }
            }
            let r = ((r / coeff).round() as i32).clamp(0, 255) as u8;
            let g = ((g / coeff).round() as i32).clamp(0, 255) as u8;
            let b = ((b / coeff).round() as i32).clamp(0, 255) as u8;

            dest_buffer[offset + x * 4] = r;
            dest_buffer[offset + x * 4 + 1] = g;
            dest_buffer[offset + x * 4 + 2] = b;
            dest_buffer[offset + x * 4 + 3] = a;
        }
    }
}

pub fn sharpness(src: &dyn Screen, dest: &mut dyn Screen) {
    let matrix = [[-1.0, -1.0, -1.0], [-1.0, 10.0, -1.0], [-1.0, -1.0, -1.0]];
    lum_filter(src, dest, &Kernel::new(matrix))
}

pub fn blur(src: &dyn Screen, dest: &mut dyn Screen) {
    let matrix = [[1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 1.0]];
    lum_filter(src, dest, &Kernel::new(matrix))
}

pub fn copy_to(src: &dyn Screen, dest: &mut dyn Screen) {
    if dest.width() == 0 || dest.height() == 0 {
        dest.reinit(src.width(), src.height());
    }

    let dest_height = dest.height() as usize;
    let dest_width = dest.width() as usize;

    let src_buffer = src.buffer();
    let dest_buffer = dest.buffer_mut();

    for y in 0..src.height() as usize {
        let offset = y * src.width() as usize * 4;
        if y >= dest_height {
            break;
        }
        for x in 0..src.width() as usize {
            if x >= dest_width {
                break;
            }
            dest_buffer[offset + x * 4] = src_buffer[offset + x * 4];
            dest_buffer[offset + x * 4 + 1] = src_buffer[offset + x * 4 + 1];
            dest_buffer[offset + x * 4 + 2] = src_buffer[offset + x * 4 + 2];
            dest_buffer[offset + x * 4 + 3] = src_buffer[offset + x * 4 + 3];
        }
    }
}

pub fn combine(src1 : &dyn Screen, src2: &dyn Screen, dest: &mut dyn Screen) {
    // √(src1^2 + src2^2) arctan (src2/src1)
    if dest.width() == 0 || dest.height() == 0 {
        dest.reinit(src1.width(), src1.height());
    }
    let dest_height = dest.height() as usize;
    let dest_width = dest.width() as usize;
    let src1_buffer = src1.buffer();
    let src2_buffer = src2.buffer();
    let dest_buffer = dest.buffer_mut();
    for y in 0..src1.height() as usize {
        let offset = y * src1.width() as usize * 4;
        if y >= dest_height {
            break;
        }
        for x in 0..src1.width() as usize {
            if x >= dest_width {
                break;
            }
            let r1 = src1_buffer[offset + x * 4] as f32;
            let g1 = src1_buffer[offset + x * 4 + 1] as f32;
            let b1 = src1_buffer[offset + x * 4 + 2] as f32;

            let r2 = src2_buffer[offset + x * 4] as f32;
            let g2 = src2_buffer[offset + x * 4 + 1] as f32;
            let b2 = src2_buffer[offset + x * 4 + 2] as f32;

            let r = ((r1 * r1 + r2 * r2).sqrt().round() as i32).clamp(0, 255) as u8;
            let g = ((g1 * g1 + g2 * g2).sqrt().round() as i32).clamp(0, 255) as u8;
            let b = ((b1 * b1 + b2 * b2).sqrt().round() as i32).clamp(0, 255) as u8;
            let a = src1_buffer[offset + x * 4 + 3];

            dest_buffer[offset + x * 4] = r;
            dest_buffer[offset + x * 4 + 1] = g;
            dest_buffer[offset + x * 4 + 2] = b;
            dest_buffer[offset + x * 4 + 3] = a;
        }
    }
}

pub fn ranking(src: &dyn Screen, dest: &mut dyn Screen, rank: usize) {
    if dest.width() == 0 || dest.height() == 0 {
        dest.reinit(src.width(), src.height());
    }

    let dest_height = dest.height() as usize;
    let dest_width = dest.width() as usize;

    let src_buffer = src.buffer();
    let dest_buffer = dest.buffer_mut();

    for y in 0..src.height() as usize {
        let offset = y * src.width() as usize * 4;
        if y >= dest_height {
            break;
        }
        for x in 0..src.width() as usize {
            if x >= dest_width {
                break;
            }
            let a = src_buffer[offset + x * 4 + 3];
            let mut l = [(0.0, 0, 0, 0); 9];
            for u in 0..3 {
                let uu = (y as i32 + u - 1).clamp(0, src.height() as i32 - 1) as usize
                    * src.width() as usize
                    * 4;
                for v in 0..3 {
                    let vv = (x as i32 + v - 1).clamp(0, src.width() as i32 - 1) as usize * 4;
                    let r = src_buffer[uu + vv];
                    let g = src_buffer[uu + vv + 1];
                    let b = src_buffer[uu + vv + 2];
                    let ln = rgb_to_y(r, g, b);
                    l[u as usize * 3 + v as usize] = (ln, r, g, b);
                }
            }
            let mut l = l.to_vec();
            l.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let l = l[rank];

            dest_buffer[offset + x * 4] = l.1;
            dest_buffer[offset + x * 4 + 1] = l.2;
            dest_buffer[offset + x * 4 + 2] = l.3;
            dest_buffer[offset + x * 4 + 3] = a;
        }
    }
}


pub fn filter(src: &dyn Screen, dest: &mut dyn Screen, filter_name: &str) -> Result<(), Error> {
    let filter_type = FilterType::from(filter_name);
    _filter(src, dest, filter_type)    
}


fn _filter(src: &dyn Screen, dest: &mut dyn Screen, filter_type: FilterType) -> Result<(), Error> {
    match filter_type {
        FilterType::Copy => copy_to(src, dest),
        FilterType::Median => ranking(src, dest, 4),
        FilterType::Erode => ranking(src, dest, 0),
        FilterType::Dilate => ranking(src, dest, 8),
        FilterType::Sharpness => sharpness(src, dest),
        FilterType::Blur => blur(src, dest),
        FilterType::Average => {
            let matrix = [[1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 1.0]];
            rgb_filter(src, dest, &Kernel::new(matrix));
        }

        FilterType::Smooth => {
            let matrix = [[1.0, 1.0, 1.0], [1.0, 4.0, 1.0], [1.0, 1.0, 1.0]];
            lum_filter(src, dest, &Kernel::new(matrix));
        }
        FilterType::Sharpen => {
            let matrix = [[-1.0, -1.0, -1.0], [-1.0, 12.0, -1.0], [-1.0, -1.0, -1.0]];
            lum_filter(src, dest, &Kernel::new(matrix));
        }
        FilterType::Shadow => {
            let matrix = [[1.0, 2.0, 1.0], [0.0, 1.0, 0.0], [-1.0, -2.0, -1.0]];
            lum_filter(src, dest, &Kernel::new(matrix));
        }
        FilterType::Canny => {
            let matrix_a = [[-1.0, -2.0, -1.0], [0.0, 0.0, 0.0], [1.0, 2.0, 1.0]];
            let matrix_b = [[1.0, 0.0, -1.0], [2.0, 0.0, -2.0], [1.0, 0.0, -1.0]];
            let mut tmp = Canvas::new(src.width(), src.height());
            lum_filter(src, &mut tmp, &Kernel::new(matrix_a));
            lum_filter(&tmp as &dyn Screen, dest, &Kernel::new(matrix_b));
        }
        FilterType::Edges => {
            let matrix_a = [[1.0, 2.0, 1.0], [0.0, 0.0, 0.0], [-1.0, -2.0, -1.0]];
            let matrix_b = [[1.0, 0.0, -1.0], [2.0, 0.0, -2.0], [1.0, 0.0, -1.0]];
            // ガウシアン
            let mut tmp_gaussian = Canvas::new(src.width(), src.height());
            lum_filter(src, &mut tmp_gaussian, &Kernel::new([[1.0, 2.0, 1.0], [2.0, 4.0, 2.0], [1.0, 2.0, 1.0]]));
            let mut tmp_a = Canvas::new(src.width(), src.height());
            let mut tmp_b = Canvas::new(src.width(), src.height());
            // SobelX
            lum_filter(&tmp_gaussian as &dyn Screen, &mut tmp_a, &Kernel::new(matrix_a));
            // SobelY
            lum_filter(&tmp_gaussian as &dyn Screen, &mut tmp_b, &Kernel::new(matrix_b));    
            // combine
            combine(&tmp_a as &dyn Screen, &tmp_b as &dyn Screen, dest);
        }
        FilterType::EdgeX => { // Sobel X
            let matrix = [[-1.0, 0.0, 1.0], [-2.0, 0.0, 2.0], [-1.0, 0.0, 1.0]];
            lum_filter(src, dest, &Kernel::new(matrix));
        }
        FilterType::EdgeY => { // Sobel Y
            let matrix = [[-1.0, -2.0, -1.0], [0.0, 0.0, 0.0], [1.0, 2.0, 1.0]];
            lum_filter(src, dest, &Kernel::new(matrix));
        }
        FilterType::Gaussian => {
            let matrix = [[1.0, 2.0, 1.0], [2.0, 4.0, 2.0], [1.0, 2.0, 1.0]];
            lum_filter(src, dest, &Kernel::new(matrix));
        }
        FilterType::Laplacian => {
            let matrix = [[0.0, 1.0, 0.0], [1.0, -4.0, 1.0], [0.0, 1.0, 0.0]];
            lum_filter(src, dest, &Kernel::new(matrix));
        }
        FilterType::Laplacian8 => {
            let matrix = [[1.0, 1.0, 1.0], [1.0, -8.0, 1.0], [1.0, 1.0, 1.0]];
            lum_filter(src, dest, &Kernel::new(matrix));
        }
        FilterType::Emboss => {
            let matrix = [[-2.0, -1.0, 0.0], [-1.0, 1.0, 1.0], [0.0, 1.0, 2.0]];
            lum_filter(src, dest, &Kernel::new(matrix));
        }
        FilterType::Outline => {
            let matrix = [[-1.0, -1.0, -1.0], [-1.0, 8.0, -1.0], [-1.0, -1.0, -1.0]];
            lum_filter(src, dest, &Kernel::new(matrix));
        }
        FilterType::Grayscale => {
            to_grayscale(src, dest, 0);
        }
        FilterType::Unknown => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unknown filter",
            ))
        }
    }
    Ok(())
}
/*

平均 (Box blur)	[[1,1,1],[1,1,1],[1,1,1]] / 9	均等平滑化
Gaussian (σ≈1)	[[1,2,1],[2,4,2],[1,2,1]] / 16	ノイズ除去＋自然なぼかし
Sharpen	[[0,-1,0],[-1,5,-1],[0,-1,0]]	輪郭強調
強Sharpen	[[-1,-1,-1],[-1,9,-1],[-1,-1,-1]]	より強めのシャープ
Edge (Sobel X)	[[-1,0,1],[-2,0,2],[-1,0,1]]	垂直エッジ検出
Edge (Sobel Y)	[[-1,-2,-1],[0,0,0],[1,2,1]]	水平エッジ検出
Edge (Laplacian)	[[0,1,0],[1,-4,1],[0,1,0]]	方向性を持たない輪郭検出
Emboss	[[-2,-1,0],[-1,1,1],[0,1,2]]	浮き出し効果
Outline	[[-1,-1,-1],[-1,8,-1],[-1,-1,-1]]	輪郭のみ抽出
 */

 pub fn filters(src: &dyn Screen, dest: &mut dyn Screen, filter_names: Vec<String>) -> Result<(), Error> {
    let mut intermediate_src = Canvas::new(src.width(), src.height());
    let mut intermediate_dest = Canvas::new(src.width(), src.height());
    copy_to(src, &mut intermediate_src);

    for (i, filter_name) in filter_names.iter().enumerate() {
        if i % 2 == 0 {
            filter(&intermediate_src, &mut intermediate_dest, filter_name)?;
        } else {
            filter(&intermediate_dest, &mut intermediate_src, filter_name)?;
        }
    }

    if filter_names.len() % 2 == 0 {
        copy_to(&intermediate_src, dest);
    } else {
        copy_to(&intermediate_dest, dest);
    }

    Ok(())
}
