    use super::{ZipCacheReader, set_zip_workaround_options, zip_prefers_low_io};
    use crate::options::ZipWorkaroundOptions;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("wml2viewer-{name}-{unique}.bin"))
    }

    #[test]
    fn zip_cache_reader_supports_seek_and_read() {
        let path = temp_path("zip-cache");
        let mut file = File::create(&path).unwrap();
        for index in 0..(1024 * 32) {
            let value = (index % 251) as u8;
            file.write_all(&[value]).unwrap();
        }
        drop(file);

        let file = File::open(&path).unwrap();
        let mut reader = ZipCacheReader::new(file).unwrap();
        let mut buf = [0u8; 128];

        reader.seek(SeekFrom::Start(4093)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(buf[0], (4093 % 251) as u8);
        assert_eq!(buf[127], ((4093 + 127) % 251) as u8);

        reader.seek(SeekFrom::Start(32)).unwrap();
        reader.read_exact(&mut buf[..8]).unwrap();
        assert_eq!(&buf[..8], &[32, 33, 34, 35, 36, 37, 38, 39]);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn zip_prefers_low_io_even_when_local_cache_is_enabled() {
        let path = temp_path("zip-low-io");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"dummy").unwrap();
        drop(file);

        let original = ZipWorkaroundOptions::default();
        set_zip_workaround_options(ZipWorkaroundOptions {
            threshold_mb: 0,
            local_cache: true,
        });

        assert!(zip_prefers_low_io(&path));

        set_zip_workaround_options(original);
        let _ = std::fs::remove_file(path);
    }

