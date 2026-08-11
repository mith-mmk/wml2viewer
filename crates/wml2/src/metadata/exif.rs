//! Convenience helpers for TIFF-style EXIF metadata.

use crate::tiff::header::{DataPack, Rational, TiffHeader, TiffHeaders, exif_to_bytes, read_tags};
use bin_rs::reader::BytesReader;

type Error = Box<dyn std::error::Error>;

/// EXIF/TIFF IFD selector used by metadata edit helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExifIfd {
    /// The primary TIFF IFD.
    Primary,
    /// The EXIF sub-IFD referenced by tag `0x8769`.
    Exif,
    /// The GPS sub-IFD referenced by tag `0x8825`.
    Gps,
}

/// Parsed GPS coordinate metadata.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsCoordinate {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
}

/// Parses a TIFF byte stream used by EXIF APP1/eXIf payloads.
pub fn parse_exif(data: &[u8]) -> Result<TiffHeaders, Error> {
    let mut reader = BytesReader::new(data);
    read_tags(&mut reader)
}

/// Serializes a parsed EXIF tree into a TIFF byte stream.
pub fn exif_to_vec(headers: &TiffHeaders) -> Result<Vec<u8>, Error> {
    exif_to_bytes(headers)
}

/// Returns all tags in the selected IFD.
pub fn tags(headers: &TiffHeaders, ifd: ExifIfd) -> Option<&[TiffHeader]> {
    match ifd {
        ExifIfd::Primary => Some(&headers.headers),
        ExifIfd::Exif => headers.exif.as_deref(),
        ExifIfd::Gps => headers.gps.as_deref(),
    }
}

fn tags_mut(headers: &mut TiffHeaders, ifd: ExifIfd) -> &mut Vec<TiffHeader> {
    match ifd {
        ExifIfd::Primary => &mut headers.headers,
        ExifIfd::Exif => headers.exif.get_or_insert_with(Vec::new),
        ExifIfd::Gps => headers.gps.get_or_insert_with(Vec::new),
    }
}

/// Finds a tag in the selected IFD.
pub fn get_tag(headers: &TiffHeaders, ifd: ExifIfd, tagid: usize) -> Option<&TiffHeader> {
    tags(headers, ifd)?.iter().find(|tag| tag.tagid == tagid)
}

/// Finds a mutable tag in the selected IFD, creating the IFD when needed.
pub fn get_tag_mut(
    headers: &mut TiffHeaders,
    ifd: ExifIfd,
    tagid: usize,
) -> Option<&mut TiffHeader> {
    tags_mut(headers, ifd)
        .iter_mut()
        .find(|tag| tag.tagid == tagid)
}

/// Inserts or replaces one tag in the selected IFD.
pub fn upsert_tag(headers: &mut TiffHeaders, ifd: ExifIfd, tagid: usize, data: DataPack) {
    let length = inferred_length(&data);
    let tags = tags_mut(headers, ifd);
    if let Some(tag) = tags.iter_mut().find(|tag| tag.tagid == tagid) {
        tag.data = data;
        tag.length = length;
        return;
    }

    tags.push(TiffHeader {
        tagid,
        data,
        length,
    });
    tags.sort_by_key(|tag| tag.tagid);
}

/// Removes one tag from the selected IFD and returns whether anything changed.
pub fn remove_tag(headers: &mut TiffHeaders, ifd: ExifIfd, tagid: usize) -> bool {
    let tags = tags_mut(headers, ifd);
    let old_len = tags.len();
    tags.retain(|tag| tag.tagid != tagid);
    old_len != tags.len()
}

/// Sets an ASCII tag.
pub fn set_ascii(headers: &mut TiffHeaders, ifd: ExifIfd, tagid: usize, value: impl Into<String>) {
    upsert_tag(headers, ifd, tagid, DataPack::Ascii(value.into()));
}

/// Sets a BYTE tag.
pub fn set_bytes(headers: &mut TiffHeaders, ifd: ExifIfd, tagid: usize, value: Vec<u8>) {
    upsert_tag(headers, ifd, tagid, DataPack::Bytes(value));
}

/// Sets a SHORT tag.
pub fn set_short(headers: &mut TiffHeaders, ifd: ExifIfd, tagid: usize, value: Vec<u16>) {
    upsert_tag(headers, ifd, tagid, DataPack::Short(value));
}

/// Sets a LONG tag.
pub fn set_long(headers: &mut TiffHeaders, ifd: ExifIfd, tagid: usize, value: Vec<u32>) {
    upsert_tag(headers, ifd, tagid, DataPack::Long(value));
}

/// Sets a RATIONAL tag.
pub fn set_rational(headers: &mut TiffHeaders, ifd: ExifIfd, tagid: usize, value: Vec<Rational>) {
    upsert_tag(headers, ifd, tagid, DataPack::Rational(value));
}

/// Extracts decimal GPS latitude/longitude from EXIF GPS tags.
pub fn gps_coordinate(headers: &TiffHeaders) -> Option<GpsCoordinate> {
    let gps = headers.gps.as_ref()?;
    let latitude_ref = ascii_tag_value(gps, 0x0001)?;
    let latitude = rational_tag_value(gps, 0x0002)?;
    let longitude_ref = ascii_tag_value(gps, 0x0003)?;
    let longitude = rational_tag_value(gps, 0x0004)?;

    let mut latitude = dms_to_decimal(latitude)?;
    if latitude_ref
        .trim_end_matches('\0')
        .eq_ignore_ascii_case("S")
    {
        latitude = -latitude;
    }

    let mut longitude = dms_to_decimal(longitude)?;
    if longitude_ref
        .trim_end_matches('\0')
        .eq_ignore_ascii_case("W")
    {
        longitude = -longitude;
    }

    let altitude = rational_tag_value(gps, 0x0006)
        .and_then(|values| values.first())
        .map(|r| {
            let mut value = rational_to_f64(r);
            if byte_tag_value(gps, 0x0005).and_then(|values| values.first().copied()) == Some(1) {
                value = -value;
            }
            value
        });

    Some(GpsCoordinate {
        latitude,
        longitude,
        altitude,
    })
}

/// Replaces the standard GPS coordinate tags using decimal degrees.
pub fn set_gps_coordinate(
    headers: &mut TiffHeaders,
    latitude: f64,
    longitude: f64,
    altitude: Option<f64>,
) {
    upsert_tag(
        headers,
        ExifIfd::Gps,
        0x0000,
        DataPack::Bytes(vec![2, 3, 0, 0]),
    );
    set_ascii(
        headers,
        ExifIfd::Gps,
        0x0001,
        if latitude < 0.0 { "S" } else { "N" },
    );
    set_rational(headers, ExifIfd::Gps, 0x0002, decimal_to_dms(latitude));
    set_ascii(
        headers,
        ExifIfd::Gps,
        0x0003,
        if longitude < 0.0 { "W" } else { "E" },
    );
    set_rational(headers, ExifIfd::Gps, 0x0004, decimal_to_dms(longitude));

    if let Some(altitude) = altitude {
        set_bytes(
            headers,
            ExifIfd::Gps,
            0x0005,
            vec![if altitude < 0.0 { 1 } else { 0 }],
        );
        set_rational(
            headers,
            ExifIfd::Gps,
            0x0006,
            vec![decimal_to_rational(altitude.abs(), 1000)],
        );
    } else {
        remove_tag(headers, ExifIfd::Gps, 0x0005);
        remove_tag(headers, ExifIfd::Gps, 0x0006);
    }
}

fn inferred_length(data: &DataPack) -> usize {
    match data {
        DataPack::Bytes(data) => data.len(),
        DataPack::Ascii(data) => {
            if data.as_bytes().last().copied() == Some(0) {
                data.len()
            } else {
                data.len() + 1
            }
        }
        DataPack::SByte(data) => data.len(),
        DataPack::Short(data) => data.len(),
        DataPack::Long(data) => data.len(),
        DataPack::Rational(data) => data.len(),
        DataPack::SRational(data) => data.len(),
        DataPack::Float(data) => data.len(),
        DataPack::Double(data) => data.len(),
        DataPack::SShort(data) => data.len(),
        DataPack::SLong(data) => data.len(),
        DataPack::Unkown(data) => data.len(),
        DataPack::Undef(data) => data.len(),
    }
}

fn ascii_tag_value(tags: &[TiffHeader], tagid: usize) -> Option<&str> {
    match &tags.iter().find(|tag| tag.tagid == tagid)?.data {
        DataPack::Ascii(value) => Some(value.as_str()),
        _ => None,
    }
}

fn byte_tag_value(tags: &[TiffHeader], tagid: usize) -> Option<&[u8]> {
    match &tags.iter().find(|tag| tag.tagid == tagid)?.data {
        DataPack::Bytes(value) => Some(value),
        DataPack::Undef(value) => Some(value),
        _ => None,
    }
}

fn rational_tag_value(tags: &[TiffHeader], tagid: usize) -> Option<&[Rational]> {
    match &tags.iter().find(|tag| tag.tagid == tagid)?.data {
        DataPack::Rational(value) => Some(value),
        _ => None,
    }
}

fn rational_to_f64(value: &Rational) -> f64 {
    if value.d == 0 {
        0.0
    } else {
        value.n as f64 / value.d as f64
    }
}

fn dms_to_decimal(values: &[Rational]) -> Option<f64> {
    if values.len() < 3 {
        return None;
    }
    Some(
        rational_to_f64(&values[0])
            + rational_to_f64(&values[1]) / 60.0
            + rational_to_f64(&values[2]) / 3600.0,
    )
}

fn decimal_to_dms(value: f64) -> Vec<Rational> {
    let absolute = value.abs();
    let degrees = absolute.floor();
    let minutes_full = (absolute - degrees) * 60.0;
    let minutes = minutes_full.floor();
    let seconds = (minutes_full - minutes) * 60.0;

    vec![
        Rational {
            n: degrees as u32,
            d: 1,
        },
        Rational {
            n: minutes as u32,
            d: 1,
        },
        decimal_to_rational(seconds, 1_000_000),
    ]
}

fn decimal_to_rational(value: f64, denominator: u32) -> Rational {
    let numerator = (value * denominator as f64).round();
    Rational {
        n: numerator.clamp(0.0, u32::MAX as f64) as u32,
        d: denominator,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bin_rs::Endian;

    #[test]
    fn gps_coordinate_roundtrips_decimal_values() {
        let mut headers = TiffHeaders::empty(Endian::LittleEndian);
        set_gps_coordinate(&mut headers, 35.658581, 139.745433, Some(42.25));

        let gps = gps_coordinate(&headers).unwrap();
        assert!((gps.latitude - 35.658581).abs() < 0.000001);
        assert!((gps.longitude - 139.745433).abs() < 0.000001);
        assert!((gps.altitude.unwrap() - 42.25).abs() < 0.001);
    }

    #[test]
    fn tag_edit_helpers_sort_and_replace_tags() {
        let mut headers = TiffHeaders::empty(Endian::LittleEndian);
        set_ascii(&mut headers, ExifIfd::Primary, 0x0110, "model");
        set_ascii(&mut headers, ExifIfd::Primary, 0x010f, "make");
        set_ascii(&mut headers, ExifIfd::Primary, 0x0110, "model2");

        assert_eq!(headers.headers[0].tagid, 0x010f);
        assert_eq!(headers.headers[1].tagid, 0x0110);
        assert_eq!(
            get_tag(&headers, ExifIfd::Primary, 0x0110).map(|tag| &tag.data),
            Some(&DataPack::Ascii("model2".to_string()))
        );
        assert!(remove_tag(&mut headers, ExifIfd::Primary, 0x010f));
        assert!(get_tag(&headers, ExifIfd::Primary, 0x010f).is_none());
    }
}
