//! `wml2` draw-side adapter for the standalone `webp-rust` decoder.

type Error = Box<dyn std::error::Error>;

use crate::color::RGBA;
use crate::draw::{
    DecodeOptions, DrawOptions, ImageRect, InitOptions, NextBlend, NextDispose, NextOption,
    NextOptions, TerminateOptions, VerboseOptions, check_declared_animation_budget,
};
use crate::error::{ImgError, ImgErrorKind};
use crate::warning::ImgWarnings;
use bin_rs::io::read_u32_le;
use bin_rs::reader::{BinaryReader, BytesReader};

pub mod alpha {
    pub use webp_codec::decoder::alpha::*;
}

pub mod animation {
    pub use webp_codec::decoder::animation::*;
}

pub mod header {
    pub use webp_codec::decoder::header::*;
}

pub mod lossless {
    pub use webp_codec::decoder::lossless::*;
}

pub mod lossy {
    pub use webp_codec::decoder::lossy::*;
}

pub mod quant {
    pub use webp_codec::decoder::quant::*;
}

pub mod tree {
    pub use webp_codec::decoder::tree::*;
}

pub mod vp8 {
    pub use webp_codec::decoder::vp8::*;
}

pub mod vp8i {
    pub use webp_codec::decoder::vp8i::*;
}

pub use webp_codec::decoder::{
    AlphaHeader, AnimationHeader, ChunkHeader, DecodedAnimation, DecodedAnimationFrame,
    DecodedImage, DecodedYuvImage, DecoderError, LosslessInfo, LossyHeader, MacroBlockData,
    MacroBlockDataFrame, MacroBlockHeaders, ParsedAnimationFrame, ParsedAnimationWebp, ParsedWebp,
    Vp8xHeader, WebpFeatures, WebpFormat, apply_alpha_plane, decode_alpha_plane,
    decode_animation_webp, decode_lossless_vp8l_to_rgba, decode_lossless_webp_to_rgba,
    decode_lossy_vp8_to_rgba, decode_lossy_vp8_to_yuv, decode_lossy_webp_to_rgba,
    decode_lossy_webp_to_yuv, get_features, parse_animation_webp, parse_lossy_headers,
    parse_macroblock_data, parse_macroblock_headers, parse_still_webp,
};

fn compat_rgba(color: &webp_codec::compat::RGBA) -> RGBA {
    RGBA {
        red: color.red,
        green: color.green,
        blue: color.blue,
        alpha: color.alpha,
    }
}

fn map_error(error: DecoderError) -> Error {
    let kind = match error {
        DecoderError::InvalidParam(_) => ImgErrorKind::InvalidParameter,
        DecoderError::NotEnoughData(_) => ImgErrorKind::UnexpectedEof,
        DecoderError::Bitstream(_) => ImgErrorKind::IllegalData,
        DecoderError::Unsupported(_) => ImgErrorKind::UnsupportedFeature,
    };
    Box::new(ImgError::new_const(kind, error.to_string()))
}

fn read_container<B: BinaryReader>(reader: &mut B) -> Result<Vec<u8>, Error> {
    let header = reader.read_bytes_no_move(12)?;
    if header.len() < 12 || &header[0..4] != b"RIFF" || &header[8..12] != b"WEBP" {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::IllegalData,
            "not a WebP RIFF container".to_string(),
        )));
    }

    let riff_size = read_u32_le(&header, 4) as usize;
    let total_size = riff_size + 8;
    if total_size < 12 {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::IllegalData,
            "invalid WebP container length".to_string(),
        )));
    }

    Ok(reader.read_bytes_as_vec(total_size)?)
}

fn compat_next_options(next: webp_codec::compat::NextOptions) -> NextOptions {
    NextOptions {
        flag: match next.flag {
            webp_codec::compat::NextOption::Continue => NextOption::Continue,
            webp_codec::compat::NextOption::Next => NextOption::Next,
            webp_codec::compat::NextOption::Dispose => NextOption::Dispose,
            webp_codec::compat::NextOption::ClearAbort => NextOption::ClearAbort,
            webp_codec::compat::NextOption::Terminate => NextOption::Terminate,
        },
        await_time: next.await_time,
        image_rect: next.image_rect.map(|rect| ImageRect {
            start_x: rect.start_x,
            start_y: rect.start_y,
            width: rect.width,
            height: rect.height,
        }),
        dispose_option: next.dispose_option.map(|dispose| match dispose {
            webp_codec::compat::NextDispose::None => NextDispose::None,
            webp_codec::compat::NextDispose::Override => NextDispose::Override,
            webp_codec::compat::NextDispose::Background => NextDispose::Background,
            webp_codec::compat::NextDispose::Previous => NextDispose::Previous,
        }),
        blend: next.blend.map(|blend| match blend {
            webp_codec::compat::NextBlend::Source => NextBlend::Source,
            webp_codec::compat::NextBlend::Override => NextBlend::Override,
        }),
    }
}

struct DrawerAdapter<'a> {
    drawer: &'a mut dyn crate::draw::DrawCallback,
}

impl webp_codec::compat::DrawCallback for DrawerAdapter<'_> {
    fn init(
        &mut self,
        width: usize,
        height: usize,
        option: Option<webp_codec::compat::InitOptions>,
    ) -> Result<Option<webp_codec::compat::CallbackResponse>, Error> {
        let option = option.map(|option| InitOptions {
            loop_count: option.loop_count,
            background: option.background.as_ref().map(compat_rgba),
            animation: option.animation,
        });
        self.drawer.init(width, height, option).map(|response| {
            response.map(|response| {
                if response.response == crate::draw::ResponseCommand::Abort {
                    webp_codec::compat::CallbackResponse::abort()
                } else {
                    webp_codec::compat::CallbackResponse::cont()
                }
            })
        })
    }

    fn draw(
        &mut self,
        start_x: usize,
        start_y: usize,
        width: usize,
        height: usize,
        data: &[u8],
        _option: Option<webp_codec::compat::DrawOptions>,
    ) -> Result<Option<webp_codec::compat::CallbackResponse>, Error> {
        self.drawer
            .draw(start_x, start_y, width, height, data, None::<DrawOptions>)
            .map(|response| {
                response.map(|response| {
                    if response.response == crate::draw::ResponseCommand::Abort {
                        webp_codec::compat::CallbackResponse::abort()
                    } else {
                        webp_codec::compat::CallbackResponse::cont()
                    }
                })
            })
    }

    fn terminate(
        &mut self,
        _term: Option<webp_codec::compat::TerminateOptions>,
    ) -> Result<Option<webp_codec::compat::CallbackResponse>, Error> {
        Ok(Some(webp_codec::compat::CallbackResponse::cont()))
    }

    fn next(
        &mut self,
        next: Option<webp_codec::compat::NextOptions>,
    ) -> Result<Option<webp_codec::compat::CallbackResponse>, Error> {
        self.drawer
            .next(next.map(compat_next_options))
            .map(|response| {
                response.map(|response| {
                    if response.response == crate::draw::ResponseCommand::Abort {
                        webp_codec::compat::CallbackResponse::abort()
                    } else {
                        webp_codec::compat::CallbackResponse::cont()
                    }
                })
            })
    }

    fn verbose(
        &mut self,
        verbose: &str,
        _option: Option<webp_codec::compat::VerboseOptions>,
    ) -> Result<Option<webp_codec::compat::CallbackResponse>, Error> {
        self.drawer
            .verbose(verbose, None::<VerboseOptions>)
            .map(|response| {
                response.map(|response| {
                    if response.response == crate::draw::ResponseCommand::Abort {
                        webp_codec::compat::CallbackResponse::abort()
                    } else {
                        webp_codec::compat::CallbackResponse::cont()
                    }
                })
            })
    }

    fn set_metadata(
        &mut self,
        _key: &str,
        _value: webp_codec::compat::DataMap,
    ) -> Result<Option<webp_codec::compat::CallbackResponse>, Error> {
        Ok(Some(webp_codec::compat::CallbackResponse::cont()))
    }
}

pub fn decode<B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let data = read_container(reader)?;
    option.drawer.check_decode_control()?;
    let features = get_features(&data).map_err(map_error)?;
    if features.has_animation {
        let parsed = parse_animation_webp(&data).map_err(map_error)?;
        check_declared_animation_budget(
            option.drawer,
            parsed.features.width,
            parsed.features.height,
            parsed
                .frames
                .iter()
                .map(|frame| (frame.width, frame.height)),
        )?;
    }
    let (metadata, warnings) = crate::webp::utils::make_metadata(&data).map_err(map_error)?;

    let mut compat_reader = BytesReader::from(data);
    let mut adapter = DrawerAdapter {
        drawer: option.drawer,
    };
    let mut compat_option = webp_codec::compat::DecodeOptions::new(&mut adapter);
    compat_option.debug_flag = option.debug_flag;
    webp_codec::compat::decode(&mut compat_reader, &mut compat_option)?;
    option.drawer.check_decode_control()?;

    for (key, value) in &metadata {
        option.drawer.set_metadata(key, value.clone())?;
    }
    option.drawer.terminate(None::<TerminateOptions>)?;

    Ok(warnings)
}
