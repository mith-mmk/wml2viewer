use crate::CoreErrorKind;
use crate::archive::{
    ArchiveLimits, VirtualContainer, VirtualSourceFormat, normalize_relative_entry_path,
};
use crate::reading::SourceId;
use oxiarc_archive::{LzhCompressionLevel, LzhWriter};
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

fn lha_bytes(name: &str, payload: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    {
        let mut writer = LzhWriter::new(&mut output);
        writer.set_compression(LzhCompressionLevel::Store);
        writer.add_file(name, payload).unwrap();
        writer.finish().unwrap();
    }
    output
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
