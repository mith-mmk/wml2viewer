//! Color facts from the exact successful decode, never a second file decode.
use wml2::color_outcome::{
    ApproximationMethod, ApproximationReason, Conversion, NotAppliedReason, OutputColorSpace,
};
use wml2::metadata::DataMap;
use wml2viewer_core::image::DecodeDiagnostics;

pub(super) fn color_info_lines(
    diagnostics: &DecodeDiagnostics,
    output_index: Option<usize>,
) -> Vec<String> {
    let outcome = output_index
        .map(|index| diagnostics.output_color(index))
        .unwrap_or_default();
    let output = match outcome.output {
        OutputColorSpace::Srgb => "sRGB",
        OutputColorSpace::Source => "Source coordinates (not necessarily sRGB)",
        OutputColorSpace::Reconstruction => "Reconstruction coordinates (not necessarily sRGB)",
        OutputColorSpace::DeviceApproximation => "Device approximation (not color-managed sRGB)",
        _ => "Unknown / not reported",
    };
    let conversion = match outcome.conversion {
        Conversion::AppliedToSrgb => "ICC conversion to sRGB completed".into(),
        Conversion::Approximation(reason, method) => format!(
            "CMYK approximation: {}; {}",
            approximation_reason(reason),
            match method {
                ApproximationMethod::AdditiveCmyk => "additive CMYK",
                ApproximationMethod::MultiplicativeCmyk => "multiplicative CMYK",
                _ => "unknown method",
            }
        ),
        Conversion::NotApplied(reason) => format!(
            "Not applied: {}",
            match reason {
                NotAppliedReason::AlreadySrgb => "already sRGB",
                NotAppliedReason::Disabled => "color management disabled",
                NotAppliedReason::Unsupported => "unsupported",
                NotAppliedReason::InvalidProfile => "invalid profile",
                NotAppliedReason::SourcePreserved => "source coordinates preserved",
                NotAppliedReason::ReconstructionPreserved => "reconstruction coordinates preserved",
                _ => "unknown reason",
            }
        ),
        _ => "Unknown / not reported".into(),
    };
    let mut lines = vec![
        format!(
            "Decoded output: {}",
            output_index
                .map(|index| format!("{index} (zero-based)"))
                .unwrap_or_else(|| "Unknown mapping".into())
        ),
        format!("Output color space: {output}"),
        format!("Color management: {conversion}"),
    ];
    let metadata = diagnostics.metadata.as_ref();
    let page_key = output_index.map(|index| format!("TIFF Page {} Source ICC Profile", index + 1));
    let source = metadata.and_then(|m| {
        page_key
            .as_ref()
            .and_then(|key| m.get(key))
            .or_else(|| m.get("JPEG XL Source ICC Profile"))
            .or_else(|| m.get("Source ICC Profile"))
    });
    lines.push(format!("Source ICC: {}", profile_description(source)));
    // Preserve the decoder's label rather than claiming this is the source ICC
    // or proof of an applied transform (JXL can generate this output profile).
    lines.push(format!(
        "ICC Profile (decoder metadata): {}",
        profile_description(metadata.and_then(|m| m.get("ICC Profile")))
    ));
    if let Some(value) = metadata.and_then(|m| m.get("JPEG XL Reconstruction ICC Profile")) {
        lines.push(format!(
            "Reconstruction ICC: {}",
            profile_description(Some(value))
        ));
    }
    lines.push("ICC metadata presence does not establish color conversion.".into());
    lines.push("Decoder observations precede Viewer effects and monitor display.".into());
    lines.push(String::new());
    lines.push("[Decoder warnings]".into());
    if diagnostics.warnings.is_empty() {
        lines.push("None reported (not a validation guarantee)".into());
    } else {
        lines.extend(
            diagnostics
                .warnings
                .iter()
                .map(|warning| format!("- {warning}")),
        );
    }
    lines
}

fn profile_description(value: Option<&DataMap>) -> String {
    match value {
        Some(DataMap::ICCProfile(bytes)) => format!(
            "Present ({} bytes; not validated by this panel)",
            bytes.len()
        ),
        Some(_) => "Unknown (unexpected metadata type)".into(),
        None => "Not reported separately / unknown".into(),
    }
}

fn approximation_reason(reason: ApproximationReason) -> &'static str {
    match reason {
        ApproximationReason::MissingProfile => "missing profile",
        ApproximationReason::InvalidProfile => "invalid profile",
        ApproximationReason::UnsupportedProfile => "unsupported profile",
        ApproximationReason::ColorManagementDisabled => "color management disabled",
        ApproximationReason::TransformFailed => "transform failed",
        ApproximationReason::ProfileIgnored => "profile ignored",
        _ => "unknown reason",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wml2::color_outcome::{ColorOutcome, metadata_key};
    #[test]
    fn profile_and_warning_text_never_establish_conversion() {
        let diagnostics = DecodeDiagnostics {
            metadata: Some(std::collections::HashMap::from([(
                "ICC Profile".into(),
                DataMap::ICCProfile(vec![0; 128]),
            )])),
            warnings: vec!["ICC conversion to sRGB completed".into()],
        };
        let lines = color_info_lines(&diagnostics, Some(0));
        assert_eq!(lines[1], "Output color space: Unknown / not reported");
        assert_eq!(lines[2], "Color management: Unknown / not reported");
        assert!(lines.last().unwrap().starts_with("- ICC"));
    }
    #[test]
    fn other_page_does_not_inherit_success() {
        let diagnostics = DecodeDiagnostics {
            metadata: Some(std::collections::HashMap::from([(
                metadata_key(0),
                ColorOutcome {
                    conversion: Conversion::AppliedToSrgb,
                    output: OutputColorSpace::Srgb,
                }
                .to_metadata(),
            )])),
            warnings: vec![],
        };
        assert_eq!(
            color_info_lines(&diagnostics, Some(0))[2],
            "Color management: ICC conversion to sRGB completed"
        );
        assert_eq!(
            color_info_lines(&diagnostics, Some(1))[2],
            "Color management: Unknown / not reported"
        );
    }
}
