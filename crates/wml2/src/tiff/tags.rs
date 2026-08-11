//! TIFF tag identifiers and helpers.

/*
 * tiff/tags.rs  Mith@mmk (C) 2022
 * use MIT License
 */

use super::header::DataPack;
use super::util::convert;
use crate::metadata::DataMap;
use bin_rs::io::read_string;

pub fn gps_mapper(tag: u16, data: &DataPack, length: usize) -> (String, DataMap) {
    let tagname;
    let s;
    match tag {
        0x0000 => {
            tagname = "GPSVersionID";
            s = convert(data, length);
        }
        0x0001 => {
            tagname = "GPSLatitudeRef";
            s = convert(data, length);
        }

        0x0002 => {
            tagname = "GPSLatitude";
            s = convert(data, length);
        }
        0x0003 => {
            tagname = "GPSLongitudeRef";
            s = convert(data, length);
        }
        0x0004 => {
            tagname = "GPSLongitude";
            s = convert(data, length);
        }
        0x0005 => {
            tagname = "GPSAltitudeRef";
            s = convert(data, length);
        }

        0x0006 => {
            tagname = "GPSAltitude";
            s = convert(data, length);
        }
        0x0007 => {
            tagname = "GPSTimeStamp";
            s = convert(data, length);
        }
        0x0008 => {
            tagname = "GPSSatellites";
            s = convert(data, length);
        }
        0x0009 => {
            tagname = "GPSStatus";
            s = convert(data, length);
        }
        0x000a => {
            tagname = "GPSMeasureMode";
            s = convert(data, length);
        }
        0x000b => {
            tagname = "GPSDOP";
            s = convert(data, length);
        }
        0x000c => {
            tagname = "GPSSpeedRef";
            s = convert(data, length);
        }
        0x000d => {
            tagname = "GPSSpeed";
            s = convert(data, length);
        }
        0x000e => {
            tagname = "GPSTrackRef";
            s = convert(data, length);
        }
        0x000f => {
            tagname = "GPSTrack";
            s = convert(data, length);
        }
        0x0010 => {
            tagname = "GPSImgDirectionRef";
            s = convert(data, length);
        }
        0x0011 => {
            tagname = "GPSImgDirection";
            s = convert(data, length);
        }
        0x0012 => {
            tagname = "GPSMapDatum";
            s = convert(data, length);
        }
        0x0013 => {
            tagname = "GPSDestLatitudeRef";
            s = convert(data, length);
        }
        0x0014 => {
            tagname = "GPSDestLatitude";
            s = convert(data, length);
        }
        0x0015 => {
            tagname = "GPSDestLongitudeRef";
            s = convert(data, length);
        }
        0x0016 => {
            tagname = "GPSDestLongitude";
            s = convert(data, length);
        }
        0x0017 => {
            tagname = "GPSDestBearingRef";
            s = convert(data, length);
        }
        0x0018 => {
            tagname = "GPSDestBearing";
            s = convert(data, length);
        }
        0x0019 => {
            tagname = "GPSDestDistanceRef";
            s = convert(data, length);
        }
        0x001a => {
            tagname = "GPSDestDistance";
            s = convert(data, length);
        }
        0x001b => {
            tagname = "GPSProcessingMethod";
            s = convert(data, length);
        }
        0x001c => {
            tagname = "GPSAreaInformation";
            s = convert(data, length);
        }
        0x001d => {
            tagname = "GPSDateStamp";
            s = convert(data, length);
        }
        0x001e => {
            tagname = "GPSDifferential";
            s = convert(data, length);
        }
        0x001f => {
            tagname = "GPSHPositioningError";
            s = convert(data, length);
        }
        _ => {
            tagname = "UnKnown";
            s = convert(data, length);
        }
    }
    (tagname.to_string(), s)
}

pub fn tag_mapper(tag: u16, data: &DataPack, length: usize) -> (String, DataMap) {
    let tagname;
    let s;
    match tag {
        0x0001 => {
            tagname = "InteropIndex";
            s = convert(data, length);
        }
        0x0002 => {
            tagname = "InteropVersion";
            s = convert(data, length);
        }
        0x000b => {
            tagname = "ProcessingSoftware";
            s = convert(data, length);
        }
        0x00fe => {
            tagname = "SubfileType";
            s = convert(data, length);
        }
        0x00ff => {
            tagname = "OldSubfileType";
            s = convert(data, length);
        }
        0x0100 => {
            tagname = "ImageWidth";
            s = convert(data, length);
        }
        0x0101 => {
            tagname = "ImageHeight";
            s = convert(data, length);
        }
        0x0102 => {
            tagname = "BitsPerSample";
            s = convert(data, length);
        }
        0x0103 => {
            tagname = "Compression";
            s = convert(data, length);
        }
        0x0106 => {
            tagname = "PhotometricInterpretation";
            s = convert(data, length);
        }
        0x0107 => {
            tagname = "Thresholding";
            s = convert(data, length);
        }
        0x0108 => {
            tagname = "CellWidth";
            s = convert(data, length);
        }
        0x0109 => {
            tagname = "CellLength";
            s = convert(data, length);
        }
        0x010a => {
            tagname = "FillOrder";
            s = convert(data, length);
        }
        0x010d => {
            tagname = "DocumentName";
            s = convert(data, length);
        }
        0x010e => {
            tagname = "ImageDescription";
            s = convert(data, length);
        }
        0x010f => {
            tagname = "Make";
            s = convert(data, length);
        }
        0x0110 => {
            tagname = "Model";
            s = convert(data, length);
        }
        0x0111 => {
            tagname = "StripOffsets";
            s = convert(data, length);
        }
        0x0112 => {
            tagname = "Orientation";
            s = convert(data, length);
        }
        0x0115 => {
            tagname = "SamplesPerPixel";
            s = convert(data, length);
        }
        0x0116 => {
            tagname = "RowsPerStrip";
            s = convert(data, length);
        }
        0x0117 => {
            tagname = "StripByteCounts";
            s = convert(data, length);
        }
        0x0118 => {
            tagname = "MinSampleValue";
            s = convert(data, length);
        }
        0x0119 => {
            tagname = "MaxSampleValue";
            s = convert(data, length);
        }
        0x011a => {
            tagname = "XResolution";
            s = convert(data, length);
        }
        0x011b => {
            tagname = "YResolution";
            s = convert(data, length);
        }
        0x011c => {
            tagname = "PlanarConfiguration";
            s = convert(data, length);
        }
        0x011d => {
            tagname = "PageName";
            s = convert(data, length);
        }
        0x011e => {
            tagname = "XPosition";
            s = convert(data, length);
        }
        0x011f => {
            tagname = "YPosition";
            s = convert(data, length);
        }
        0x0120 => {
            tagname = "FreeOffsets";
            s = convert(data, length);
        }
        0x0121 => {
            tagname = "FreeByteCounts";
            s = convert(data, length);
        }
        0x0122 => {
            tagname = "GrayResponseUnit";
            s = convert(data, length);
        }
        0x0123 => {
            tagname = "GrayResponseCurve";
            s = convert(data, length);
        }
        0x0124 => {
            tagname = "T4Options";
            s = convert(data, length);
        }
        0x0125 => {
            tagname = "T6Options";
            s = convert(data, length);
        }
        0x0128 => {
            tagname = "ResolutionUnit";
            s = convert(data, length);
        }
        0x0129 => {
            tagname = "PageNumber";
            s = convert(data, length);
        }
        0x012c => {
            tagname = "ColorResponseUnit";
            s = convert(data, length);
        }
        0x012d => {
            tagname = "TransferFunction";
            s = convert(data, length);
        }
        0x0131 => {
            tagname = "Software";
            s = convert(data, length);
        }
        0x0132 => {
            tagname = "ModifyDate";
            s = convert(data, length);
        }
        0x013b => {
            tagname = "Artist";
            s = convert(data, length);
        }
        0x013c => {
            tagname = "HostComputer";
            s = convert(data, length);
        }
        0x013d => {
            tagname = "Predictor";
            s = convert(data, length);
        }
        0x013e => {
            tagname = "WhitePoint";
            s = convert(data, length);
        }
        0x013f => {
            tagname = "PrimaryChromaticities";
            s = convert(data, length);
        }
        0x0140 => {
            tagname = "ColorMap";
            s = convert(data, length);
        }
        0x0141 => {
            tagname = "HalftoneHints";
            s = convert(data, length);
        }
        0x0142 => {
            tagname = "TileWidth";
            s = convert(data, length);
        }
        0x0143 => {
            tagname = "TileLength";
            s = convert(data, length);
        }
        0x0144 => {
            tagname = "TileOffsets";
            s = convert(data, length);
        }
        0x0145 => {
            tagname = "TileByteCounts";
            s = convert(data, length);
        }
        0x0146 => {
            tagname = "BadFaxLines";
            s = convert(data, length);
        }
        0x0147 => {
            tagname = "CleanFaxData";
            s = convert(data, length);
        }
        0x0148 => {
            tagname = "ConsecutiveBadFaxLines";
            s = convert(data, length);
        }
        0x014a => {
            tagname = "SubIFD";
            s = convert(data, length);
        }
        0x014c => {
            tagname = "InkSet";
            s = convert(data, length);
        }
        0x014d => {
            tagname = "InkNames";
            s = convert(data, length);
        }
        0x014e => {
            tagname = "NumberofInks";
            s = convert(data, length);
        }
        0x0150 => {
            tagname = "DotRange";
            s = convert(data, length);
        }
        0x0151 => {
            tagname = "TargetPrinter";
            s = convert(data, length);
        }
        0x0152 => {
            tagname = "ExtraSamples";
            s = convert(data, length);
        }
        0x0153 => {
            tagname = "SampleFormat";
            s = convert(data, length);
        }
        0x0154 => {
            tagname = "SMinSampleValue";
            s = convert(data, length);
        }
        0x0155 => {
            tagname = "SMaxSampleValue";
            s = convert(data, length);
        }
        0x0156 => {
            tagname = "TransferRange";
            s = convert(data, length);
        }
        0x0157 => {
            tagname = "ClipPath";
            s = convert(data, length);
        }
        0x0158 => {
            tagname = "XClipPathUnits";
            s = convert(data, length);
        }
        0x0159 => {
            tagname = "YClipPathUnits";
            s = convert(data, length);
        }
        0x015a => {
            tagname = "Indexed";
            s = convert(data, length);
        }
        0x015b => {
            tagname = "JPEGTables";
            s = convert(data, length);
        }
        0x015f => {
            tagname = "OPIProxy";
            s = convert(data, length);
        }
        0x0190 => {
            tagname = "GlobalParametersIFD";
            s = convert(data, length);
        }
        0x0191 => {
            tagname = "ProfileType";
            s = convert(data, length);
        }
        0x0192 => {
            tagname = "FaxProfile";
            s = convert(data, length);
        }
        0x0193 => {
            tagname = "CodingMethods";
            s = convert(data, length);
        }
        0x0194 => {
            tagname = "VersionYear";
            s = convert(data, length);
        }
        0x0195 => {
            tagname = "ModeNumber";
            s = convert(data, length);
        }
        0x01b1 => {
            tagname = "Decode";
            s = convert(data, length);
        }
        0x01b2 => {
            tagname = "DefaultImageColor";
            s = convert(data, length);
        }
        0x01b3 => {
            tagname = "T82Options";
            s = convert(data, length);
        }
        0x01b5 => {
            tagname = "JPEGTables";
            s = convert(data, length);
        }
        0x0200 => {
            tagname = "JPEGProc";
            s = convert(data, length);
        }
        0x0201 => {
            tagname = "ThumbnailOffset";
            s = convert(data, length);
        }
        0x0202 => {
            tagname = "ThumbnailLength";
            s = convert(data, length);
        }
        0x0203 => {
            tagname = "JPEGRestartInterval";
            s = convert(data, length);
        }
        0x0205 => {
            tagname = "JPEGLosslessPredictors";
            s = convert(data, length);
        }
        0x0206 => {
            tagname = "JPEGPointTransforms";
            s = convert(data, length);
        }
        0x0207 => {
            tagname = "JPEGQTables";
            s = convert(data, length);
        }
        0x0208 => {
            tagname = "JPEGDCTables";
            s = convert(data, length);
        }
        0x0209 => {
            tagname = "JPEGACTables";
            s = convert(data, length);
        }
        0x0211 => {
            tagname = "YCbCrCoefficients";
            s = convert(data, length);
        }
        0x0212 => {
            tagname = "YCbCrSubSampling";
            s = convert(data, length);
        }
        0x0213 => {
            tagname = "YCbCrPositioning";
            s = convert(data, length);
        }
        0x0214 => {
            tagname = "ReferenceBlackWhite";
            s = convert(data, length);
        }
        0x022f => {
            tagname = "StripRowCounts";
            s = convert(data, length);
        }
        0x02bc => {
            tagname = "ApplicationNotes";
            s = convert(data, length);
        }
        0x03e7 => {
            tagname = "USPTOMiscellaneous";
            s = convert(data, length);
        }
        0x1000 => {
            tagname = "RelatedImageFileFormat";
            s = convert(data, length);
        }
        0x1001 => {
            tagname = "RelatedImageWidth";
            s = convert(data, length);
        }
        0x1002 => {
            tagname = "RelatedImageHeight";
            s = convert(data, length);
        }
        0x4746 => {
            tagname = "Rating";
            s = convert(data, length);
        }
        0x4747 => {
            tagname = "XP_DIP_XML";
            s = convert(data, length);
        }
        0x4748 => {
            tagname = "StitchInfo";
            s = convert(data, length);
        }
        0x4749 => {
            tagname = "RatingPercent";
            s = convert(data, length);
        }
        0x7000 => {
            tagname = "SonyRawFileType";
            s = convert(data, length);
        }
        0x7010 => {
            tagname = "SonyToneCurve";
            s = convert(data, length);
        }
        0x7031 => {
            tagname = "VignettingCorrection";
            s = convert(data, length);
        }
        0x7032 => {
            tagname = "VignettingCorrParams";
            s = convert(data, length);
        }
        0x7034 => {
            tagname = "ChromaticAberrationCorrection";
            s = convert(data, length);
        }
        0x7035 => {
            tagname = "ChromaticAberrationCorrParams";
            s = convert(data, length);
        }
        0x7036 => {
            tagname = "DistortionCorrection";
            s = convert(data, length);
        }
        0x7037 => {
            tagname = "DistortionCorrParams";
            s = convert(data, length);
        }
        0x74c7 => {
            tagname = "SonyCropTopLeft";
            s = convert(data, length);
        }
        0x74c8 => {
            tagname = "SonyCropSize";
            s = convert(data, length);
        }
        0x800d => {
            tagname = "ImageID";
            s = convert(data, length);
        }
        0x80a3 => {
            tagname = "WangTag1";
            s = convert(data, length);
        }
        0x80a4 => {
            tagname = "WangAnnotation";
            s = convert(data, length);
        }
        0x80a5 => {
            tagname = "WangTag3";
            s = convert(data, length);
        }
        0x80a6 => {
            tagname = "WangTag4";
            s = convert(data, length);
        }
        0x80b9 => {
            tagname = "ImageReferencePoints";
            s = convert(data, length);
        }
        0x80ba => {
            tagname = "RegionXformTackPoint";
            s = convert(data, length);
        }
        0x80bb => {
            tagname = "WarpQuadrilateral";
            s = convert(data, length);
        }
        0x80bc => {
            tagname = "AffineTransformMat";
            s = convert(data, length);
        }
        0x80e3 => {
            tagname = "Matteing";
            s = convert(data, length);
        }
        0x80e4 => {
            tagname = "DataType";
            s = convert(data, length);
        }
        0x80e5 => {
            tagname = "ImageDepth";
            s = convert(data, length);
        }
        0x80e6 => {
            tagname = "TileDepth";
            s = convert(data, length);
        }
        0x8214 => {
            tagname = "ImageFullWidth";
            s = convert(data, length);
        }
        0x8215 => {
            tagname = "ImageFullHeight";
            s = convert(data, length);
        }
        0x8216 => {
            tagname = "TextureFormat";
            s = convert(data, length);
        }
        0x8217 => {
            tagname = "WrapModes";
            s = convert(data, length);
        }
        0x8218 => {
            tagname = "FovCot";
            s = convert(data, length);
        }
        0x8219 => {
            tagname = "MatrixWorldToScreen";
            s = convert(data, length);
        }
        0x821a => {
            tagname = "MatrixWorldToCamera";
            s = convert(data, length);
        }
        0x827d => {
            tagname = "Model2";
            s = convert(data, length);
        }
        0x828d => {
            tagname = "CFARepeatPatternDim";
            s = convert(data, length);
        }
        0x828e => {
            tagname = "CFAPattern2";
            s = convert(data, length);
        }
        0x828f => {
            tagname = "BatteryLevel";
            s = convert(data, length);
        }
        0x8290 => {
            tagname = "KodakIFD";
            s = convert(data, length);
        }
        0x8298 => {
            tagname = "Copyright";
            s = convert(data, length);
        }
        0x829a => {
            tagname = "ExposureTime";
            s = convert(data, length);
        }
        0x829d => {
            tagname = "FNumber";
            s = convert(data, length);
        }
        0x82a5 => {
            tagname = "MDFileTag";
            s = convert(data, length);
        }
        0x82a6 => {
            tagname = "MDScalePixel";
            s = convert(data, length);
        }
        0x82a7 => {
            tagname = "MDColorTable";
            s = convert(data, length);
        }
        0x82a8 => {
            tagname = "MDLabName";
            s = convert(data, length);
        }
        0x82a9 => {
            tagname = "MDSampleInfo";
            s = convert(data, length);
        }
        0x82aa => {
            tagname = "MDPrepDate";
            s = convert(data, length);
        }
        0x82ab => {
            tagname = "MDPrepTime";
            s = convert(data, length);
        }
        0x82ac => {
            tagname = "MDFileUnits";
            s = convert(data, length);
        }
        0x830e => {
            tagname = "PixelScale";
            s = convert(data, length);
        }
        0x8335 => {
            tagname = "AdventScale";
            s = convert(data, length);
        }
        0x8336 => {
            tagname = "AdventRevision";
            s = convert(data, length);
        }
        0x835c => {
            tagname = "UIC1Tag";
            s = convert(data, length);
        }
        0x835d => {
            tagname = "UIC2Tag";
            s = convert(data, length);
        }
        0x835e => {
            tagname = "UIC3Tag";
            s = convert(data, length);
        }
        0x835f => {
            tagname = "UIC4Tag";
            s = convert(data, length);
        }
        0x83bb => {
            tagname = "IPTC-NAA";
            s = convert(data, length);
        }
        0x847e => {
            tagname = "IntergraphPacketData";
            s = convert(data, length);
        }
        0x847f => {
            tagname = "IntergraphFlagRegisters";
            s = convert(data, length);
        }
        0x8480 => {
            tagname = "IntergraphMatrix";
            s = convert(data, length);
        }
        0x8481 => {
            tagname = "INGRReserved";
            s = convert(data, length);
        }
        0x8482 => {
            tagname = "ModelTiePoint";
            s = convert(data, length);
        }
        0x84e0 => {
            tagname = "Site";
            s = convert(data, length);
        }
        0x84e1 => {
            tagname = "ColorSequence";
            s = convert(data, length);
        }
        0x84e2 => {
            tagname = "IT8Header";
            s = convert(data, length);
        }
        0x84e3 => {
            tagname = "RasterPadding";
            s = convert(data, length);
        }
        0x84e4 => {
            tagname = "BitsPerRunLength";
            s = convert(data, length);
        }
        0x84e5 => {
            tagname = "BitsPerExtendedRunLength";
            s = convert(data, length);
        }
        0x84e6 => {
            tagname = "ColorTable";
            s = convert(data, length);
        }
        0x84e7 => {
            tagname = "ImageColorIndicator";
            s = convert(data, length);
        }
        0x84e8 => {
            tagname = "BackgroundColorIndicator";
            s = convert(data, length);
        }
        0x84e9 => {
            tagname = "ImageColorValue";
            s = convert(data, length);
        }
        0x84ea => {
            tagname = "BackgroundColorValue";
            s = convert(data, length);
        }
        0x84eb => {
            tagname = "PixelIntensityRange";
            s = convert(data, length);
        }
        0x84ec => {
            tagname = "TransparencyIndicator";
            s = convert(data, length);
        }
        0x84ed => {
            tagname = "ColorCharacterization";
            s = convert(data, length);
        }
        0x84ee => {
            tagname = "HCUsage";
            s = convert(data, length);
        }
        0x84ef => {
            tagname = "TrapIndicator";
            s = convert(data, length);
        }
        0x84f0 => {
            tagname = "CMYKEquivalent";
            s = convert(data, length);
        }
        0x8546 => {
            tagname = "SEMInfo";
            s = convert(data, length);
        }
        0x8568 => {
            tagname = "AFCP_IPTC";
            s = convert(data, length);
        }
        0x85b8 => {
            tagname = "PixelMagicJBIGOptions";
            s = convert(data, length);
        }
        0x85d7 => {
            tagname = "JPLCartoIFD";
            s = convert(data, length);
        }
        0x85d8 => {
            tagname = "ModelTransform";
            s = convert(data, length);
        }
        0x8602 => {
            tagname = "WB_GRGBLevels";
            s = convert(data, length);
        }
        0x8606 => {
            tagname = "LeafData";
            s = convert(data, length);
        }
        0x8649 => {
            tagname = "PhotoshopSettings";
            s = convert(data, length);
        }
        0x8769 => {
            tagname = "ExifOffset";
            s = convert(data, length);
        }
        0x8773 => {
            tagname = "ICC_Profile";
            if let DataPack::Undef(data) = data {
                s = DataMap::ICCProfile(data.to_vec())
            } else {
                s = convert(data, length);
            }
        }
        0x877f => {
            tagname = "TIFF_FXExtensions";
            s = convert(data, length);
        }
        0x8780 => {
            tagname = "MultiProfiles";
            s = convert(data, length);
        }
        0x8781 => {
            tagname = "SharedData";
            s = convert(data, length);
        }
        0x8782 => {
            tagname = "T88Options";
            s = convert(data, length);
        }
        0x87ac => {
            tagname = "ImageLayer";
            s = convert(data, length);
        }
        0x87af => {
            tagname = "GeoTiffDirectory";
            s = convert(data, length);
        }
        0x87b0 => {
            tagname = "GeoTiffDoubleParams";
            s = convert(data, length);
        }
        0x87b1 => {
            tagname = "GeoTiffAsciiParams";
            s = convert(data, length);
        }
        0x87be => {
            tagname = "JBIGOptions";
            s = convert(data, length);
        }
        0x8822 => {
            tagname = "ExposureProgram";
            s = convert(data, length);
        }
        0x8824 => {
            tagname = "SpectralSensitivity";
            s = convert(data, length);
        }
        0x8825 => {
            tagname = "GPSInfo";
            s = convert(data, length);
        }
        0x8827 => {
            tagname = "ISO";
            s = convert(data, length);
        }
        0x8828 => {
            tagname = "Opto-ElectricConvFactor";
            s = convert(data, length);
        }
        0x8829 => {
            tagname = "Interlace";
            s = convert(data, length);
        }
        0x882a => {
            tagname = "TimeZoneOffset";
            s = convert(data, length);
        }
        0x882b => {
            tagname = "SelfTimerMode";
            s = convert(data, length);
        }
        0x8830 => {
            tagname = "SensitivityType";
            s = convert(data, length);
        }
        0x8831 => {
            tagname = "StandardOutputSensitivity";
            s = convert(data, length);
        }
        0x8832 => {
            tagname = "RecommendedExposureIndex";
            s = convert(data, length);
        }
        0x8833 => {
            tagname = "ISOSpeed";
            s = convert(data, length);
        }
        0x8834 => {
            tagname = "ISOSpeedLatitudeyyy";
            s = convert(data, length);
        }
        0x8835 => {
            tagname = "ISOSpeedLatitudezzz";
            s = convert(data, length);
        }
        0x885c => {
            tagname = "FaxRecvParams";
            s = convert(data, length);
        }
        0x885d => {
            tagname = "FaxSubAddress";
            s = convert(data, length);
        }
        0x885e => {
            tagname = "FaxRecvTime";
            s = convert(data, length);
        }
        0x8871 => {
            tagname = "FedexEDR";
            s = convert(data, length);
        }
        0x888a => {
            tagname = "LeafSubIFD";
            s = convert(data, length);
        }
        0x9000 => {
            tagname = "ExifVersion";
            s = convert(data, length);
        }
        0x9003 => {
            tagname = "DateTimeOriginal";
            s = convert(data, length);
        }
        0x9004 => {
            tagname = "CreateDate";
            s = convert(data, length);
        }
        0x9009 => {
            tagname = "GooglePlusUploadCode";
            s = convert(data, length);
        }
        0x9010 => {
            tagname = "OffsetTime";
            s = convert(data, length);
        }
        0x9011 => {
            tagname = "OffsetTimeOriginal";
            s = convert(data, length);
        }
        0x9012 => {
            tagname = "OffsetTimeDigitized";
            s = convert(data, length);
        }
        0x9101 => {
            tagname = "ComponentsConfiguration";
            s = convert(data, length);
        }
        0x9102 => {
            tagname = "CompressedBitsPerPixel";
            s = convert(data, length);
        }
        0x9201 => {
            tagname = "ShutterSpeedValue";
            s = convert(data, length);
        }
        0x9202 => {
            tagname = "ApertureValue";
            s = convert(data, length);
        }
        0x9203 => {
            tagname = "BrightnessValue";
            s = convert(data, length);
        }
        0x9204 => {
            tagname = "ExposureCompensation";
            s = convert(data, length);
        }
        0x9205 => {
            tagname = "MaxApertureValue";
            s = convert(data, length);
        }
        0x9206 => {
            tagname = "SubjectDistance";
            s = convert(data, length);
        }
        0x9207 => {
            tagname = "MeteringMode";
            s = convert(data, length);
        }
        0x9208 => {
            tagname = "LightSource";
            s = convert(data, length);
        }
        0x9209 => {
            tagname = "Flash";
            s = convert(data, length);
        }
        0x920a => {
            tagname = "FocalLength";
            s = convert(data, length);
        }
        0x920b => {
            tagname = "FlashEnergy";
            s = convert(data, length);
        }
        0x920c => {
            tagname = "SpatialFrequencyResponse";
            s = convert(data, length);
        }
        0x920d => {
            tagname = "Noise";
            s = convert(data, length);
        }
        0x920e => {
            tagname = "FocalPlaneXResolution";
            s = convert(data, length);
        }
        0x920f => {
            tagname = "FocalPlaneYResolution";
            s = convert(data, length);
        }
        0x9210 => {
            tagname = "FocalPlaneResolutionUnit";
            s = convert(data, length);
        }
        0x9211 => {
            tagname = "ImageNumber";
            s = convert(data, length);
        }
        0x9212 => {
            tagname = "SecurityClassification";
            s = convert(data, length);
        }
        0x9213 => {
            tagname = "ImageHistory";
            s = convert(data, length);
        }
        0x9214 => {
            tagname = "SubjectArea";
            s = convert(data, length);
        }
        0x9215 => {
            tagname = "ExposureIndex";
            s = convert(data, length);
        }
        0x9216 => {
            tagname = "TIFF-EPStandardID";
            s = convert(data, length);
        }
        0x9217 => {
            tagname = "SensingMethod";
            s = convert(data, length);
        }
        0x923a => {
            tagname = "CIP3DataFile";
            s = convert(data, length);
        }
        0x923b => {
            tagname = "CIP3Sheet";
            s = convert(data, length);
        }
        0x923c => {
            tagname = "CIP3Side";
            s = convert(data, length);
        }
        0x923f => {
            tagname = "StoNits";
            s = convert(data, length);
        }
        0x927c => {
            tagname = "MakerNoteApple";
            match data {
                DataPack::Undef(d) => {
                    s = DataMap::Ascii(read_string(d, 0, d.len()));
                }
                _ => {
                    s = convert(data, length);
                }
            }
        }
        0x9286 => {
            tagname = "UserComment";
            s = convert(data, length);
        }
        0x9290 => {
            tagname = "SubSecTime";
            s = convert(data, length);
        }
        0x9291 => {
            tagname = "SubSecTimeOriginal";
            s = convert(data, length);
        }
        0x9292 => {
            tagname = "SubSecTimeDigitized";
            s = convert(data, length);
        }
        0x932f => {
            tagname = "MSDocumentText";
            s = convert(data, length);
        }
        0x9330 => {
            tagname = "MSPropertySetStorage";
            s = convert(data, length);
        }
        0x9331 => {
            tagname = "MSDocumentTextPosition";
            s = convert(data, length);
        }
        0x935c => {
            tagname = "ImageSourceData";
            s = convert(data, length);
        }
        0x9400 => {
            tagname = "AmbientTemperature";
            s = convert(data, length);
        }
        0x9401 => {
            tagname = "Humidity";
            s = convert(data, length);
        }
        0x9402 => {
            tagname = "Pressure";
            s = convert(data, length);
        }
        0x9403 => {
            tagname = "WaterDepth";
            s = convert(data, length);
        }
        0x9404 => {
            tagname = "Acceleration";
            s = convert(data, length);
        }
        0x9405 => {
            tagname = "CameraElevationAngle";
            s = convert(data, length);
        }
        0x9c9b => {
            tagname = "XPTitle";
            s = convert(data, length);
        }
        0x9c9c => {
            tagname = "XPComment";
            s = convert(data, length);
        }
        0x9c9d => {
            tagname = "XPAuthor";
            s = convert(data, length);
        }
        0x9c9e => {
            tagname = "XPKeywords";
            s = convert(data, length);
        }
        0x9c9f => {
            tagname = "XPSubject";
            s = convert(data, length);
        }
        0xa000 => {
            tagname = "FlashpixVersion";
            s = convert(data, length);
        }
        0xa001 => {
            tagname = "ColorSpace";
            s = convert(data, length);
        }
        0xa002 => {
            tagname = "ExifImageWidth";
            s = convert(data, length);
        }
        0xa003 => {
            tagname = "ExifImageHeight";
            s = convert(data, length);
        }
        0xa004 => {
            tagname = "RelatedSoundFile";
            s = convert(data, length);
        }
        0xa005 => {
            tagname = "InteropOffset";
            s = convert(data, length);
        }
        0xa010 => {
            tagname = "SamsungRawPointersOffset";
            s = convert(data, length);
        }
        0xa011 => {
            tagname = "SamsungRawPointersLength";
            s = convert(data, length);
        }
        0xa101 => {
            tagname = "SamsungRawByteOrder";
            s = convert(data, length);
        }
        0xa102 => {
            tagname = "SamsungRawUnknown?";
            s = convert(data, length);
        }
        0xa20b => {
            tagname = "FlashEnergy";
            s = convert(data, length);
        }
        0xa20c => {
            tagname = "SpatialFrequencyResponse";
            s = convert(data, length);
        }
        0xa20d => {
            tagname = "Noise";
            s = convert(data, length);
        }
        0xa20e => {
            tagname = "FocalPlaneXResolution";
            s = convert(data, length);
        }
        0xa20f => {
            tagname = "FocalPlaneYResolution";
            s = convert(data, length);
        }
        0xa210 => {
            tagname = "FocalPlaneResolutionUnit";
            s = convert(data, length);
        }
        0xa211 => {
            tagname = "ImageNumber";
            s = convert(data, length);
        }
        0xa212 => {
            tagname = "SecurityClassification";
            s = convert(data, length);
        }
        0xa213 => {
            tagname = "ImageHistory";
            s = convert(data, length);
        }
        0xa214 => {
            tagname = "SubjectLocation";
            s = convert(data, length);
        }
        0xa215 => {
            tagname = "ExposureIndex";
            s = convert(data, length);
        }
        0xa216 => {
            tagname = "TIFF-EPStandardID";
            s = convert(data, length);
        }
        0xa217 => {
            tagname = "SensingMethod";
            s = convert(data, length);
        }
        0xa300 => {
            tagname = "FileSource";
            s = convert(data, length);
        }
        0xa301 => {
            tagname = "SceneType";
            s = convert(data, length);
        }
        0xa302 => {
            tagname = "CFAPattern";
            s = convert(data, length);
        }
        0xa401 => {
            tagname = "CustomRendered";
            s = convert(data, length);
        }
        0xa402 => {
            tagname = "ExposureMode";
            s = convert(data, length);
        }
        0xa403 => {
            tagname = "WhiteBalance";
            s = convert(data, length);
        }
        0xa404 => {
            tagname = "DigitalZoomRatio";
            s = convert(data, length);
        }
        0xa405 => {
            tagname = "FocalLengthIn35mmFormat";
            s = convert(data, length);
        }
        0xa406 => {
            tagname = "SceneCaptureType";
            s = convert(data, length);
        }
        0xa407 => {
            tagname = "GainControl";
            s = convert(data, length);
        }
        0xa408 => {
            tagname = "Contrast";
            s = convert(data, length);
        }
        0xa409 => {
            tagname = "Saturation";
            s = convert(data, length);
        }
        0xa40a => {
            tagname = "Sharpness";
            s = convert(data, length);
        }
        0xa40b => {
            tagname = "DeviceSettingDescription";
            s = convert(data, length);
        }
        0xa40c => {
            tagname = "SubjectDistanceRange";
            s = convert(data, length);
        }
        0xa420 => {
            tagname = "ImageUniqueID";
            s = convert(data, length);
        }
        0xa430 => {
            tagname = "OwnerName";
            s = convert(data, length);
        }
        0xa431 => {
            tagname = "SerialNumber";
            s = convert(data, length);
        }
        0xa432 => {
            tagname = "LensInfo";
            s = convert(data, length);
        }
        0xa433 => {
            tagname = "LensMake";
            s = convert(data, length);
        }
        0xa434 => {
            tagname = "LensModel";
            s = convert(data, length);
        }
        0xa435 => {
            tagname = "LensSerialNumber";
            s = convert(data, length);
        }
        0xa460 => {
            tagname = "CompositeImage";
            s = convert(data, length);
        }
        0xa461 => {
            tagname = "CompositeImageCount";
            s = convert(data, length);
        }
        0xa462 => {
            tagname = "CompositeImageExposureTimes";
            s = convert(data, length);
        }
        0xa480 => {
            tagname = "GDALMetadata";
            s = convert(data, length);
        }
        0xa481 => {
            tagname = "GDALNoData";
            s = convert(data, length);
        }
        0xa500 => {
            tagname = "Gamma";
            s = convert(data, length);
        }
        0xafc0 => {
            tagname = "ExpandSoftware";
            s = convert(data, length);
        }
        0xafc1 => {
            tagname = "ExpandLens";
            s = convert(data, length);
        }
        0xafc2 => {
            tagname = "ExpandFilm";
            s = convert(data, length);
        }
        0xafc3 => {
            tagname = "ExpandFilterLens";
            s = convert(data, length);
        }
        0xafc4 => {
            tagname = "ExpandScanner";
            s = convert(data, length);
        }
        0xafc5 => {
            tagname = "ExpandFlashLamp";
            s = convert(data, length);
        }
        0xb4c3 => {
            tagname = "HasselbladRawImage";
            s = convert(data, length);
        }
        0xbc01 => {
            tagname = "PixelFormat";
            s = convert(data, length);
        }
        0xbc02 => {
            tagname = "Transformation";
            s = convert(data, length);
        }
        0xbc03 => {
            tagname = "Uncompressed";
            s = convert(data, length);
        }
        0xbc04 => {
            tagname = "ImageType";
            s = convert(data, length);
        }
        0xbc80 => {
            tagname = "ImageWidth";
            s = convert(data, length);
        }
        0xbc81 => {
            tagname = "ImageHeight";
            s = convert(data, length);
        }
        0xbc82 => {
            tagname = "WidthResolution";
            s = convert(data, length);
        }
        0xbc83 => {
            tagname = "HeightResolution";
            s = convert(data, length);
        }
        0xbcc0 => {
            tagname = "ImageOffset";
            s = convert(data, length);
        }
        0xbcc1 => {
            tagname = "ImageByteCount";
            s = convert(data, length);
        }
        0xbcc2 => {
            tagname = "AlphaOffset";
            s = convert(data, length);
        }
        0xbcc3 => {
            tagname = "AlphaByteCount";
            s = convert(data, length);
        }
        0xbcc4 => {
            tagname = "ImageDataDiscard";
            s = convert(data, length);
        }
        0xbcc5 => {
            tagname = "AlphaDataDiscard";
            s = convert(data, length);
        }
        0xc427 => {
            tagname = "OceScanjobDesc";
            s = convert(data, length);
        }
        0xc428 => {
            tagname = "OceApplicationSelector";
            s = convert(data, length);
        }
        0xc429 => {
            tagname = "OceIDNumber";
            s = convert(data, length);
        }
        0xc42a => {
            tagname = "OceImageLogic";
            s = convert(data, length);
        }
        0xc44f => {
            tagname = "Annotations";
            s = convert(data, length);
        }
        0xc4a5 => {
            tagname = "PrintIM";
            s = convert(data, length);
        }
        0xc51b => {
            tagname = "HasselbladExif";
            s = convert(data, length);
        }
        0xc573 => {
            tagname = "OriginalFileName";
            s = convert(data, length);
        }
        0xc580 => {
            tagname = "USPTOOriginalContentType";
            s = convert(data, length);
        }
        0xc5e0 => {
            tagname = "CR2CFAPattern";
            s = convert(data, length);
        }
        0xc612 => {
            tagname = "DNGVersion";
            s = convert(data, length);
        }
        0xc613 => {
            tagname = "DNGBackwardVersion";
            s = convert(data, length);
        }
        0xc614 => {
            tagname = "UniqueCameraModel";
            s = convert(data, length);
        }
        0xc615 => {
            tagname = "LocalizedCameraModel";
            s = convert(data, length);
        }
        0xc616 => {
            tagname = "CFAPlaneColor";
            s = convert(data, length);
        }
        0xc617 => {
            tagname = "CFALayout";
            s = convert(data, length);
        }
        0xc618 => {
            tagname = "LinearizationTable";
            s = convert(data, length);
        }
        0xc619 => {
            tagname = "BlackLevelRepeatDim";
            s = convert(data, length);
        }
        0xc61a => {
            tagname = "BlackLevel";
            s = convert(data, length);
        }
        0xc61b => {
            tagname = "BlackLevelDeltaH";
            s = convert(data, length);
        }
        0xc61c => {
            tagname = "BlackLevelDeltaV";
            s = convert(data, length);
        }
        0xc61d => {
            tagname = "WhiteLevel";
            s = convert(data, length);
        }
        0xc61e => {
            tagname = "DefaultScale";
            s = convert(data, length);
        }
        0xc61f => {
            tagname = "DefaultCropOrigin";
            s = convert(data, length);
        }
        0xc620 => {
            tagname = "DefaultCropSize";
            s = convert(data, length);
        }
        0xc621 => {
            tagname = "ColorMatrix1";
            s = convert(data, length);
        }
        0xc622 => {
            tagname = "ColorMatrix2";
            s = convert(data, length);
        }
        0xc623 => {
            tagname = "CameraCalibration1";
            s = convert(data, length);
        }
        0xc624 => {
            tagname = "CameraCalibration2";
            s = convert(data, length);
        }
        0xc625 => {
            tagname = "ReductionMatrix1";
            s = convert(data, length);
        }
        0xc626 => {
            tagname = "ReductionMatrix2";
            s = convert(data, length);
        }
        0xc627 => {
            tagname = "AnalogBalance";
            s = convert(data, length);
        }
        0xc628 => {
            tagname = "AsShotNeutral";
            s = convert(data, length);
        }
        0xc629 => {
            tagname = "AsShotWhiteXY";
            s = convert(data, length);
        }
        0xc62a => {
            tagname = "BaselineExposure";
            s = convert(data, length);
        }
        0xc62b => {
            tagname = "BaselineNoise";
            s = convert(data, length);
        }
        0xc62c => {
            tagname = "BaselineSharpness";
            s = convert(data, length);
        }
        0xc62d => {
            tagname = "BayerGreenSplit";
            s = convert(data, length);
        }
        0xc62e => {
            tagname = "LinearResponseLimit";
            s = convert(data, length);
        }
        0xc62f => {
            tagname = "CameraSerialNumber";
            s = convert(data, length);
        }
        0xc630 => {
            tagname = "DNGLensInfo";
            s = convert(data, length);
        }
        0xc631 => {
            tagname = "ChromaBlurRadius";
            s = convert(data, length);
        }
        0xc632 => {
            tagname = "AntiAliasStrength";
            s = convert(data, length);
        }
        0xc633 => {
            tagname = "ShadowScale";
            s = convert(data, length);
        }
        0xc634 => {
            tagname = "SR2Private";
            s = convert(data, length);
        }
        0xc635 => {
            tagname = "MakerNoteSafety";
            s = convert(data, length);
        }
        0xc640 => {
            tagname = "RawImageSegmentation";
            s = convert(data, length);
        }
        0xc65a => {
            tagname = "CalibrationIlluminant1";
            s = convert(data, length);
        }
        0xc65b => {
            tagname = "CalibrationIlluminant2";
            s = convert(data, length);
        }
        0xc65c => {
            tagname = "BestQualityScale";
            s = convert(data, length);
        }
        0xc65d => {
            tagname = "RawDataUniqueID";
            s = convert(data, length);
        }
        0xc660 => {
            tagname = "AliasLayerMetadata";
            s = convert(data, length);
        }
        0xc68b => {
            tagname = "OriginalRawFileName";
            s = convert(data, length);
        }
        0xc68c => {
            tagname = "OriginalRawFileData";
            s = convert(data, length);
        }
        0xc68d => {
            tagname = "ActiveArea";
            s = convert(data, length);
        }
        0xc68e => {
            tagname = "MaskedAreas";
            s = convert(data, length);
        }
        0xc68f => {
            tagname = "AsShotICCProfile";
            s = convert(data, length);
        }
        0xc690 => {
            tagname = "AsShotPreProfileMatrix";
            s = convert(data, length);
        }
        0xc691 => {
            tagname = "CurrentICCProfile";
            s = convert(data, length);
        }
        0xc692 => {
            tagname = "CurrentPreProfileMatrix";
            s = convert(data, length);
        }
        0xc6bf => {
            tagname = "ColorimetricReference";
            s = convert(data, length);
        }
        0xc6c5 => {
            tagname = "SRawType";
            s = convert(data, length);
        }
        0xc6d2 => {
            tagname = "PanasonicTitle";
            s = convert(data, length);
        }
        0xc6d3 => {
            tagname = "PanasonicTitle2";
            s = convert(data, length);
        }
        0xc6f3 => {
            tagname = "CameraCalibrationSig";
            s = convert(data, length);
        }
        0xc6f4 => {
            tagname = "ProfileCalibrationSig";
            s = convert(data, length);
        }
        0xc6f5 => {
            tagname = "ProfileIFD";
            s = convert(data, length);
        }
        0xc6f6 => {
            tagname = "AsShotProfileName";
            s = convert(data, length);
        }
        0xc6f7 => {
            tagname = "NoiseReductionApplied";
            s = convert(data, length);
        }
        0xc6f8 => {
            tagname = "ProfileName";
            s = convert(data, length);
        }
        0xc6f9 => {
            tagname = "ProfileHueSatMapDims";
            s = convert(data, length);
        }
        0xc6fa => {
            tagname = "ProfileHueSatMapData1";
            s = convert(data, length);
        }
        0xc6fb => {
            tagname = "ProfileHueSatMapData2";
            s = convert(data, length);
        }
        0xc6fc => {
            tagname = "ProfileToneCurve";
            s = convert(data, length);
        }
        0xc6fd => {
            tagname = "ProfileEmbedPolicy";
            s = convert(data, length);
        }
        0xc6fe => {
            tagname = "ProfileCopyright";
            s = convert(data, length);
        }
        0xc714 => {
            tagname = "ForwardMatrix1";
            s = convert(data, length);
        }
        0xc715 => {
            tagname = "ForwardMatrix2";
            s = convert(data, length);
        }
        0xc716 => {
            tagname = "PreviewApplicationName";
            s = convert(data, length);
        }
        0xc717 => {
            tagname = "PreviewApplicationVersion";
            s = convert(data, length);
        }
        0xc718 => {
            tagname = "PreviewSettingsName";
            s = convert(data, length);
        }
        0xc719 => {
            tagname = "PreviewSettingsDigest";
            s = convert(data, length);
        }
        0xc71a => {
            tagname = "PreviewColorSpace";
            s = convert(data, length);
        }
        0xc71b => {
            tagname = "PreviewDateTime";
            s = convert(data, length);
        }
        0xc71c => {
            tagname = "RawImageDigest";
            s = convert(data, length);
        }
        0xc71d => {
            tagname = "OriginalRawFileDigest";
            s = convert(data, length);
        }
        0xc71e => {
            tagname = "SubTileBlockSize";
            s = convert(data, length);
        }
        0xc71f => {
            tagname = "RowInterleaveFactor";
            s = convert(data, length);
        }
        0xc725 => {
            tagname = "ProfileLookTableDims";
            s = convert(data, length);
        }
        0xc726 => {
            tagname = "ProfileLookTableData";
            s = convert(data, length);
        }
        0xc740 => {
            tagname = "OpcodeList1";
            s = convert(data, length);
        }
        0xc741 => {
            tagname = "OpcodeList2";
            s = convert(data, length);
        }
        0xc74e => {
            tagname = "OpcodeList3";
            s = convert(data, length);
        }
        0xc761 => {
            tagname = "NoiseProfile";
            s = convert(data, length);
        }
        0xc763 => {
            tagname = "TimeCodes";
            s = convert(data, length);
        }
        0xc764 => {
            tagname = "FrameRate";
            s = convert(data, length);
        }
        0xc772 => {
            tagname = "TStop";
            s = convert(data, length);
        }
        0xc789 => {
            tagname = "ReelName";
            s = convert(data, length);
        }
        0xc791 => {
            tagname = "OriginalDefaultFinalSize";
            s = convert(data, length);
        }
        0xc792 => {
            tagname = "OriginalBestQualitySize";
            s = convert(data, length);
        }
        0xc793 => {
            tagname = "OriginalDefaultCropSize";
            s = convert(data, length);
        }
        0xc7a1 => {
            tagname = "CameraLabel";
            s = convert(data, length);
        }
        0xc7a3 => {
            tagname = "ProfileHueSatMapEncoding";
            s = convert(data, length);
        }
        0xc7a4 => {
            tagname = "ProfileLookTableEncoding";
            s = convert(data, length);
        }
        0xc7a5 => {
            tagname = "BaselineExposureOffset";
            s = convert(data, length);
        }
        0xc7a6 => {
            tagname = "DefaultBlackRender";
            s = convert(data, length);
        }
        0xc7a7 => {
            tagname = "NewRawImageDigest";
            s = convert(data, length);
        }
        0xc7a8 => {
            tagname = "RawToPreviewGain";
            s = convert(data, length);
        }
        0xc7aa => {
            tagname = "CacheVersion";
            s = convert(data, length);
        }
        0xc7b5 => {
            tagname = "DefaultUserCrop";
            s = convert(data, length);
        }
        0xc7d5 => {
            tagname = "NikonNEFInfo";
            s = convert(data, length);
        }
        0xc7e9 => {
            tagname = "DepthFormat";
            s = convert(data, length);
        }
        0xc7ea => {
            tagname = "DepthNear";
            s = convert(data, length);
        }
        0xc7eb => {
            tagname = "DepthFar";
            s = convert(data, length);
        }
        0xc7ec => {
            tagname = "DepthUnits";
            s = convert(data, length);
        }
        0xc7ed => {
            tagname = "DepthMeasureType";
            s = convert(data, length);
        }
        0xc7ee => {
            tagname = "EnhanceParams";
            s = convert(data, length);
        }
        0xcd2d => {
            tagname = "ProfileGainTableMap";
            s = convert(data, length);
        }
        0xcd2e => {
            tagname = "SemanticName";
            s = convert(data, length);
        }
        0xcd30 => {
            tagname = "SemanticInstanceIFD";
            s = convert(data, length);
        }
        0xcd31 => {
            tagname = "CalibrationIlluminant3";
            s = convert(data, length);
        }
        0xcd32 => {
            tagname = "CameraCalibration3";
            s = convert(data, length);
        }
        0xcd33 => {
            tagname = "ColorMatrix3";
            s = convert(data, length);
        }
        0xcd34 => {
            tagname = "ForwardMatrix3";
            s = convert(data, length);
        }
        0xcd35 => {
            tagname = "IlluminantData1";
            s = convert(data, length);
        }
        0xcd36 => {
            tagname = "IlluminantData2";
            s = convert(data, length);
        }
        0xcd37 => {
            tagname = "IlluminantData3";
            s = convert(data, length);
        }
        0xcd38 => {
            tagname = "MaskSubArea";
            s = convert(data, length);
        }
        0xcd39 => {
            tagname = "ProfileHueSatMapData3";
            s = convert(data, length);
        }
        0xcd3a => {
            tagname = "ReductionMatrix3";
            s = convert(data, length);
        }
        0xcd3b => {
            tagname = "RGBTables";
            s = convert(data, length);
        }
        0xea1c => {
            tagname = "Padding";
            s = convert(data, length);
        }
        0xea1d => {
            tagname = "OffsetSchema";
            s = convert(data, length);
        }
        0xfde8 => {
            tagname = "OwnerName";
            s = convert(data, length);
        }
        0xfde9 => {
            tagname = "SerialNumber";
            s = convert(data, length);
        }
        0xfdea => {
            tagname = "Lens";
            s = convert(data, length);
        }
        0xfe00 => {
            tagname = "KDC_IFD";
            s = convert(data, length);
        }
        0xfe4c => {
            tagname = "RawFile";
            s = convert(data, length);
        }
        0xfe4d => {
            tagname = "Converter";
            s = convert(data, length);
        }
        0xfe4e => {
            tagname = "WhiteBalance";
            s = convert(data, length);
        }
        0xfe51 => {
            tagname = "Exposure";
            s = convert(data, length);
        }
        0xfe52 => {
            tagname = "Shadows";
            s = convert(data, length);
        }
        0xfe53 => {
            tagname = "Brightness";
            s = convert(data, length);
        }
        0xfe54 => {
            tagname = "Contrast";
            s = convert(data, length);
        }
        0xfe55 => {
            tagname = "Saturation";
            s = convert(data, length);
        }
        0xfe56 => {
            tagname = "Sharpness";
            s = convert(data, length);
        }
        0xfe57 => {
            tagname = "Smoothness";
            s = convert(data, length);
        }
        0xfe58 => {
            tagname = "MoireFilter";
            s = convert(data, length);
        }
        _ => {
            tagname = "Unknown";
            s = convert(data, length);
        }
    }
    (tagname.to_string(), s)
}
