use crate::CoreErrorKind;
use crate::archive::{
    ArchiveLimits, VirtualContainer, VirtualSourceFormat, normalize_relative_entry_path,
};
use crate::image::{DecodeRequest, decode};
use crate::reading::SourceId;
use oxiarc_archive::{LzhCompressionLevel, LzhHeader, LzhMethod, LzhWriter};
use oxiarc_lzhuf::encode_lzh;
use std::io::{Cursor, Write};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn zip_bytes(name: &str, payload: &[u8]) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut output);
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();
    }
    output.into_inner()
}

fn lha_bytes_with_options(
    name: &str,
    payload: &[u8],
    header_level: u8,
    compression: LzhCompressionLevel,
) -> Vec<u8> {
    let mut output = Vec::new();
    {
        let mut writer = LzhWriter::new(&mut output).with_header_level(header_level);
        writer.set_compression(compression);
        writer.add_file(name, payload).unwrap();
        writer.finish().unwrap();
    }
    output
}

fn lha_bytes(name: &str, payload: &[u8]) -> Vec<u8> {
    lha_bytes_with_options(name, payload, 2, LzhCompressionLevel::Store)
}

fn lha_crc16(payload: &[u8]) -> u16 {
    payload.iter().fold(0_u16, |mut crc, &byte| {
        crc ^= byte as u16;
        for _ in 0..8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ 0xa001
            };
        }
        crc
    })
}

fn raw_lha_bytes(name: &str, payload: &[u8], header_level: u8, method: LzhMethod) -> Vec<u8> {
    let compressed = encode_lzh(payload, method).unwrap();
    let mut output = Vec::new();
    {
        let mut writer = LzhWriter::new(&mut output).with_header_level(header_level);
        writer
            .add_file_raw(
                name,
                method,
                lha_crc16(payload),
                payload.len() as u64,
                &compressed,
                0,
                None,
            )
            .unwrap();
        writer.finish().unwrap();
    }
    output
}

/// Builds an original, uncompressed 640 x 400, 16-colour MAG image.
///
/// Every pixel uses palette index zero. Keeping this fixture generator here
/// makes the archive/decode regression hermetic and avoids redistributing
/// user-provided or otherwise copyrighted image data.
fn generated_mag_640x400() -> Vec<u8> {
    const WIDTH: u16 = 640;
    const HEIGHT: u16 = 400;
    const MAG_HEADER_BYTES: u32 = 32;
    const PALETTE_BYTES: u32 = 16 * 3;
    const FLAG_A_BYTES: u32 = 4_000;
    const PIXEL_BYTES: u32 = 128_000;

    let flag_a_offset = MAG_HEADER_BYTES + PALETTE_BYTES;
    let pixel_offset = flag_a_offset + FLAG_A_BYTES;
    let mut bytes = Vec::with_capacity(31 + pixel_offset as usize + PIXEL_BYTES as usize);

    bytes.extend_from_slice(b"MAKI02  ");
    bytes.extend_from_slice(b"TEST");
    bytes.extend_from_slice(&[0; 18]);
    bytes.push(0x1a);

    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&(WIDTH - 1).to_le_bytes());
    bytes.extend_from_slice(&(HEIGHT - 1).to_le_bytes());
    bytes.extend_from_slice(&flag_a_offset.to_le_bytes());
    bytes.extend_from_slice(&pixel_offset.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&pixel_offset.to_le_bytes());
    bytes.extend_from_slice(&PIXEL_BYTES.to_le_bytes());

    // MAG stores each palette colour as G, R, B.
    bytes.extend_from_slice(&[0x20, 0x10, 0x30]);
    bytes.extend_from_slice(&[0; (15 * 3) as usize]);
    bytes.extend_from_slice(&[0; FLAG_A_BYTES as usize]);
    bytes.extend_from_slice(&[0; PIXEL_BYTES as usize]);
    bytes
}

#[test]
fn zip_lists_and_reads_from_bytes() {
    let source = SourceId(41);
    let container = VirtualContainer::open(
        VirtualSourceFormat::Zip,
        source,
        zip_bytes("pages/001.png", b"png"),
        ArchiveLimits::default(),
    )
    .unwrap();
    assert_eq!(container.entries()[0].name, "pages/001.png");
    assert_eq!(container.entries()[0].source_id, source);
    assert_eq!(container.read_entry(0).unwrap(), b"png");
}

#[test]
fn lha_lists_and_reads_from_bytes() {
    let container = VirtualContainer::open(
        VirtualSourceFormat::Lha,
        SourceId(42),
        lha_bytes("pages/001.png", b"lha"),
        ArchiveLimits::default(),
    )
    .unwrap();
    assert_eq!(container.entries()[0].name, "pages/001.png");
    assert_eq!(container.read_entry(0).unwrap(), b"lha");
}

#[test]
fn lha_header_levels_and_supported_compression_methods_round_trip() {
    let payload = b"standard LHA wire format regression".repeat(64);
    for header_level in 0..=3 {
        for compression in [LzhCompressionLevel::Store, LzhCompressionLevel::Lh5] {
            let container = VirtualContainer::open(
                VirtualSourceFormat::Lha,
                SourceId(42),
                lha_bytes_with_options("pages/001.bin", &payload, header_level, compression),
                ArchiveLimits::default(),
            )
            .unwrap_or_else(|error| {
                panic!("header level {header_level}, {compression:?}: {error}")
            });
            assert_eq!(container.read_entry(0).unwrap(), payload);
        }
    }
}

#[test]
fn lha_lh4_through_lh7_materialize_across_extended_header_levels() {
    let payload = b"retro image archive compression method regression".repeat(256);
    for header_level in 1..=3 {
        for method in [
            LzhMethod::Lh4,
            LzhMethod::Lh5,
            LzhMethod::Lh6,
            LzhMethod::Lh7,
        ] {
            let container = VirtualContainer::open(
                VirtualSourceFormat::Lha,
                SourceId(42),
                raw_lha_bytes("pages/001.bin", &payload, header_level, method),
                ArchiveLimits::default(),
            )
            .unwrap_or_else(|error| panic!("header level {header_level}, {method:?}: {error}"));
            assert_eq!(container.read_entry(0).unwrap(), payload);
        }
    }
}

#[test]
fn level_one_lh5_mag_materializes_and_decodes_to_rgba() {
    let mag = generated_mag_640x400();
    let container = VirtualContainer::open(
        VirtualSourceFormat::Lha,
        SourceId(42),
        lha_bytes_with_options("generated.mag", &mag, 1, LzhCompressionLevel::Lh5),
        ArchiveLimits::default(),
    )
    .unwrap();

    assert_eq!(container.entries().len(), 1);
    assert_eq!(container.entries()[0].name, "generated.mag");
    let materialized = container.read_entry(0).unwrap();
    assert_eq!(materialized, mag);

    let decoded = decode(DecodeRequest {
        bytes: &materialized,
        format_hint: Some("image/x-mag"),
    })
    .unwrap();
    assert_eq!(decoded.poster.width(), 640);
    assert_eq!(decoded.poster.height(), 400);
    assert_eq!(&decoded.poster.pixels()[..4], &[0x10, 0x20, 0x30, 0xff]);
    assert_eq!(decoded.poster.pixels().len(), 640 * 400 * 4);
}

#[test]
fn lha_crc_mismatch_is_rejected_during_materialization() {
    let mut bytes =
        lha_bytes_with_options("page.bin", b"crc protected", 1, LzhCompressionLevel::Store);
    let header = LzhHeader::read(&mut Cursor::new(&bytes), 0)
        .unwrap()
        .unwrap();
    bytes[header.data_offset as usize] ^= 0xff;

    let container = VirtualContainer::open(
        VirtualSourceFormat::Lha,
        SourceId(42),
        bytes,
        ArchiveLimits::default(),
    )
    .unwrap();
    assert_eq!(
        container.read_entry(0).unwrap_err().kind(),
        CoreErrorKind::Archive
    );
}

#[test]
fn truncated_lh5_is_rejected_without_unwinding() {
    let mut bytes = lha_bytes_with_options(
        "page.bin",
        &b"truncated compressed data".repeat(128),
        1,
        LzhCompressionLevel::Lh5,
    );
    bytes.truncate(bytes.len() - 3);

    let result = std::panic::catch_unwind(|| {
        VirtualContainer::open(
            VirtualSourceFormat::Lha,
            SourceId(42),
            bytes,
            ArchiveLimits::default(),
        )
        .and_then(|container| container.read_entry(0))
    });
    assert!(result.is_ok());
    assert!(result.unwrap().is_err());
}

#[test]
fn lha_declared_size_respects_the_configured_entry_limit() {
    let limits = ArchiveLimits {
        maximum_entry_bytes: 3,
        ..ArchiveLimits::default()
    };
    let error = VirtualContainer::open(
        VirtualSourceFormat::Lha,
        SourceId(42),
        lha_bytes_with_options("page.bin", b"four", 1, LzhCompressionLevel::Lh5),
        limits,
    )
    .unwrap_err();
    assert_eq!(error.kind(), CoreErrorKind::Limit);
}

#[test]
fn listed_file_materializes_only_valid_relative_entries() {
    let bytes = b"#!WMLViewer2 ListedFile\n# note\npages/001.png\n".to_vec();
    let container = VirtualContainer::open(
        VirtualSourceFormat::ListedFile,
        SourceId(43),
        bytes,
        ArchiveLimits::default(),
    )
    .unwrap();
    let materialized = container
        .materialize_entry(0, |path| {
            assert_eq!(path, "pages/001.png");
            Ok(b"listed".to_vec())
        })
        .unwrap();
    assert_eq!(materialized, b"listed");
}

#[test]
fn materialized_entry_cannot_exceed_the_configured_byte_limit() {
    let limits = ArchiveLimits {
        maximum_entry_bytes: 3,
        ..ArchiveLimits::default()
    };
    let container = VirtualContainer::open(
        VirtualSourceFormat::ListedFile,
        SourceId(44),
        b"#!WMLViewer2 ListedFile\npages/001.png\n".to_vec(),
        limits,
    )
    .unwrap();
    let error = container
        .materialize_entry(0, |_| Ok(b"four".to_vec()))
        .unwrap_err();
    assert_eq!(error.kind(), CoreErrorKind::Limit);
}

#[test]
fn traversal_is_rejected_for_every_virtual_source() {
    assert_eq!(
        normalize_relative_entry_path("../secret.png")
            .unwrap_err()
            .kind(),
        CoreErrorKind::Archive
    );
    assert!(
        VirtualContainer::open(
            VirtualSourceFormat::Zip,
            SourceId(1),
            zip_bytes("../secret.png", b"secret"),
            ArchiveLimits::default(),
        )
        .is_err()
    );
    assert!(
        VirtualContainer::open(
            VirtualSourceFormat::Lha,
            SourceId(1),
            lha_bytes("../secret.png", b"secret"),
            ArchiveLimits::default(),
        )
        .is_err()
    );
    assert!(
        VirtualContainer::open(
            VirtualSourceFormat::ListedFile,
            SourceId(1),
            b"#!WMLViewer2 ListedFile\n../secret.png\n".to_vec(),
            ArchiveLimits::default(),
        )
        .is_err()
    );
}

#[test]
fn malformed_lha_returns_an_error_without_unwinding() {
    let result = VirtualContainer::open(
        VirtualSourceFormat::Lha,
        SourceId(1),
        b"not an lha archive".to_vec(),
        ArchiveLimits::default(),
    );
    assert!(result.is_err());
}
