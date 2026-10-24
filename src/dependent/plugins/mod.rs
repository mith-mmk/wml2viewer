mod ffmpeg;
mod susie64;
mod system;

use crate::drawers::image::LoadedImage;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginConfig {
    pub internal_priority: i32,
    /// Preferred decoder per MIME type (or extension alias); absent entries inherit legacy behavior.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub formats: BTreeMap<String, String>,
    pub susie64: PluginProviderConfig,
    pub system: PluginProviderConfig,
    pub ffmpeg: PluginProviderConfig,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            internal_priority: 300,
            formats: BTreeMap::new(),
            susie64: susie64::default_provider(),
            system: system::default_provider(),
            ffmpeg: ffmpeg::default_provider(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginProviderConfig {
    pub enable: bool,
    pub priority: i32,
    pub search_path: Vec<PathBuf>,
    pub modules: Vec<PluginModuleConfig>,
}

impl Default for PluginProviderConfig {
    fn default() -> Self {
        Self {
            enable: false,
            priority: 100,
            search_path: Vec::new(),
            modules: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginModuleConfig {
    pub enable: bool,
    pub path: Option<PathBuf>,
    pub plugin_name: String,
    #[serde(rename = "type")]
    pub plugin_type: String,
    pub ext: Vec<PluginExtensionConfig>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginExtensionConfig {
    pub enable: bool,
    pub mime: Vec<String>,
    pub modules: Vec<PluginCapabilityConfig>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginCapabilityConfig {
    #[serde(rename = "type")]
    pub capability_type: String,
    pub priority: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderKind {
    Internal,
    System,
    Ffmpeg,
    Susie64,
}

#[derive(Clone)]
struct DecodeCandidate {
    provider: ProviderKind,
    module: Option<PluginModuleConfig>,
    score: i32,
    provider_priority: i32,
}

#[derive(Clone)]
struct DecodeInput {
    mime: Option<&'static str>,
}

/// Formats supported by the existing plugin MIME/extension routing.
pub const PLUGIN_FORMATS: &[(&str, &str)] = &[
    ("PNG", "image/png"),
    ("JPEG", "image/jpeg"),
    ("GIF", "image/gif"),
    ("BMP", "image/bmp"),
    ("WebP", "image/webp"),
    ("AVIF", "image/avif"),
    ("JPEG 2000", "image/jp2"),
    ("TIFF", "image/tiff"),
    ("ICO", "image/x-icon"),
    ("MAG", "image/x-mag"),
    ("MAKI", "image/x-maki"),
    ("PI", "image/x-pi"),
    ("PIC", "image/x-pic"),
    ("VSP", "image/x-vsp"),
    ("Photo CD", "image/x-photo-cd"),
    ("HEIF / HEIC", "image/heif"),
    ("JPEG XR", "image/jxr"),
];

fn canonical_format(value: &str) -> Option<&'static str> {
    let value = value.trim().to_ascii_lowercase();
    PLUGIN_FORMATS
        .iter()
        .find(|(_, mime)| *mime == value)
        .map(|(_, mime)| *mime)
        .or_else(|| {
            mime_from_path(&PathBuf::from(format!(
                "image.{}",
                value.trim_start_matches('.')
            )))
        })
}

impl PluginConfig {
    pub fn format_decoder(&self, mime: &str) -> Option<&str> {
        self.formats
            .get(mime)
            .or_else(|| {
                self.formats
                    .iter()
                    .find(|(key, _)| canonical_format(key) == Some(mime))
                    .map(|(_, value)| value)
            })
            .map(String::as_str)
    }

    /// Removing all aliases also makes resetting a hand-edited configuration reliable.
    pub fn set_format_decoder(&mut self, mime: &str, decoder: Option<&str>) {
        self.formats
            .retain(|key, _| canonical_format(key) != Some(mime));
        if let Some(decoder) = decoder {
            self.formats.insert(mime.to_string(), decoder.to_string());
        }
    }
}

fn detected_mime(data: &[u8], hint: Option<&Path>) -> Option<&'static str> {
    use wml2::util::{ImageFormat, format_check};
    // Use the same signature detection as the internal decoder; extension is only a fallback.
    match format_check(data) {
        ImageFormat::Png => Some("image/png"),
        ImageFormat::Jpeg => Some("image/jpeg"),
        ImageFormat::Gif => Some("image/gif"),
        ImageFormat::Bmp => Some("image/bmp"),
        ImageFormat::Webp => Some("image/webp"),
        ImageFormat::Tiff => Some("image/tiff"),
        ImageFormat::Ico => Some("image/x-icon"),
        ImageFormat::Mag => Some("image/x-mag"),
        ImageFormat::Maki => Some("image/x-maki"),
        ImageFormat::Pi => Some("image/x-pi"),
        ImageFormat::Pic => Some("image/x-pic"),
        ImageFormat::Vsp => Some("image/x-vsp"),
        ImageFormat::Pcd => Some("image/x-photo-cd"),
        #[cfg(feature = "avif")]
        ImageFormat::Avif => Some("image/avif"),
        _ => hint.and_then(mime_from_path),
    }
}

fn preferred_candidates(config: &PluginConfig, mime: &'static str) -> Vec<DecodeCandidate> {
    let preferred = match config.format_decoder(mime) {
        Some("internal") => ProviderKind::Internal,
        Some("system") => ProviderKind::System,
        Some("ffmpeg") => ProviderKind::Ffmpeg,
        Some("susie64") => ProviderKind::Susie64,
        _ => return Vec::new(), // Unknown or removed providers inherit the old behavior.
    };
    decode_candidates(config, &DecodeInput { mime: Some(mime) })
        .into_iter()
        .filter(|candidate| candidate.provider == preferred)
        .collect()
}

pub(crate) fn has_format_preferences() -> bool {
    runtime_plugin_config()
        .lock()
        .map(|config| !config.formats.is_empty())
        .unwrap_or(false)
}

/// Try only the explicit preference. The caller retains the existing internal-first fallback.
pub(crate) fn decode_format_preference(data: &[u8], hint: Option<&Path>) -> Option<LoadedImage> {
    let config = current_runtime_plugin_config();
    if config.formats.is_empty() {
        return None;
    }
    let mime = detected_mime(data, hint)?;
    for candidate in preferred_candidates(&config, mime) {
        let decoded = match candidate.provider {
            ProviderKind::Internal => {
                crate::drawers::image::load_canvas_from_bytes_internal(data).ok()
            }
            ProviderKind::System => {
                system::decode_from_bytes(data, hint, candidate.module.as_ref())
            }
            ProviderKind::Ffmpeg => ffmpeg::decode_preferred_from_bytes(
                data,
                hint,
                &config.ffmpeg,
                candidate.module.as_ref(),
            ),
            ProviderKind::Susie64 => {
                susie64::decode_from_bytes(data, hint, &config.susie64, candidate.module.as_ref())
            }
        };
        if decoded.is_some() {
            return decoded;
        }
    }
    None
}

pub fn set_runtime_plugin_config(config: PluginConfig) {
    let store = runtime_plugin_config();
    if let Ok(mut value) = store.lock() {
        *value = config.clone();
    }
    if let Ok(mut value) = runtime_plugin_extensions().lock() {
        *value = compute_enabled_plugin_extensions(&config);
    }
}

pub fn decode_image_from_file_with_plugins(path: &Path) -> Option<LoadedImage> {
    let config = current_runtime_plugin_config();
    let input = DecodeInput {
        mime: mime_from_path(path),
    };
    for candidate in decode_candidates(&config, &input) {
        let decoded = match candidate.provider {
            ProviderKind::Internal => None,
            ProviderKind::System => system::decode_from_file(path, candidate.module.as_ref()),
            ProviderKind::Ffmpeg => {
                ffmpeg::decode_from_file(path, &config.ffmpeg, candidate.module.as_ref())
            }
            ProviderKind::Susie64 => {
                susie64::decode_from_file(path, &config.susie64, candidate.module.as_ref())
            }
        };
        if decoded.is_some() {
            return decoded;
        }
    }
    None
}

pub fn decode_image_from_bytes_with_plugins(
    data: &[u8],
    path_hint: Option<&Path>,
) -> Option<LoadedImage> {
    let config = current_runtime_plugin_config();
    let input = DecodeInput {
        mime: path_hint.and_then(mime_from_path),
    };
    for candidate in decode_candidates(&config, &input) {
        let decoded = match candidate.provider {
            ProviderKind::Internal => None,
            ProviderKind::System => {
                system::decode_from_bytes(data, path_hint, candidate.module.as_ref())
            }
            ProviderKind::Ffmpeg => ffmpeg::decode_from_bytes(
                data,
                path_hint,
                &config.ffmpeg,
                candidate.module.as_ref(),
            ),
            ProviderKind::Susie64 => susie64::decode_from_bytes(
                data,
                path_hint,
                &config.susie64,
                candidate.module.as_ref(),
            ),
        };
        if decoded.is_some() {
            return decoded;
        }
    }
    None
}

pub fn path_supported_by_plugins(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.to_ascii_lowercase());
    let Some(ext) = ext else {
        return false;
    };
    runtime_plugin_extensions()
        .lock()
        .map(|extensions| extensions.iter().any(|candidate| candidate == &ext))
        .unwrap_or(false)
}

#[allow(dead_code)]
pub fn enabled_plugin_extensions() -> Vec<String> {
    runtime_plugin_extensions()
        .lock()
        .map(|extensions| extensions.clone())
        .unwrap_or_default()
}

#[allow(dead_code)]
pub fn discover_plugin_paths(config: &PluginProviderConfig) -> Vec<PathBuf> {
    config
        .search_path
        .iter()
        .filter(|path| path.exists())
        .cloned()
        .collect()
}

pub fn discover_plugin_modules(
    provider_name: &str,
    config: &PluginProviderConfig,
) -> Vec<PluginModuleConfig> {
    let mut modules = Vec::new();
    for root in discover_plugin_paths(config) {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() || !matches_provider(provider_name, &path) {
                continue;
            }
            let plugin_name = path
                .file_stem()
                .and_then(OsStr::to_str)
                .unwrap_or("plugin")
                .to_string();
            modules.push(PluginModuleConfig {
                enable: true,
                path: Some(path.clone()),
                plugin_name: plugin_name.clone(),
                plugin_type: provider_default_type(provider_name).to_string(),
                ext: default_module_extensions(provider_name, &plugin_name),
            });
        }
    }
    modules.sort_by(|left, right| left.plugin_name.cmp(&right.plugin_name));
    modules
}

fn decode_candidates(config: &PluginConfig, input: &DecodeInput) -> Vec<DecodeCandidate> {
    let mut candidates = vec![DecodeCandidate {
        provider: ProviderKind::Internal,
        module: None,
        score: config.internal_priority,
        provider_priority: config.internal_priority,
    }];

    collect_provider_candidates(
        &mut candidates,
        ProviderKind::System,
        "system",
        &config.system,
        input,
    );
    collect_provider_candidates(
        &mut candidates,
        ProviderKind::Ffmpeg,
        "ffmpeg",
        &config.ffmpeg,
        input,
    );
    collect_provider_candidates(
        &mut candidates,
        ProviderKind::Susie64,
        "susie64",
        &config.susie64,
        input,
    );

    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.provider_priority.cmp(&left.provider_priority))
            .then_with(|| provider_rank(right.provider).cmp(&provider_rank(left.provider)))
    });
    candidates
}

fn collect_provider_candidates(
    candidates: &mut Vec<DecodeCandidate>,
    provider: ProviderKind,
    provider_name: &str,
    config: &PluginProviderConfig,
    input: &DecodeInput,
) {
    if !config.enable {
        return;
    }

    let modules = active_modules(provider_name, config);
    if modules.is_empty() {
        candidates.push(DecodeCandidate {
            provider,
            module: None,
            score: config.priority,
            provider_priority: config.priority,
        });
        return;
    }

    for module in modules {
        if module_supports_input(provider_name, &module, input) {
            candidates.push(DecodeCandidate {
                provider,
                score: module_priority(provider_name, &module, config.priority),
                provider_priority: config.priority,
                module: Some(module),
            });
        }
    }
}

fn collect_provider_extensions(
    provider_name: &str,
    config: &PluginProviderConfig,
    extensions: &mut BTreeSet<String>,
) {
    if !config.enable {
        return;
    }

    let modules = active_modules(provider_name, config);
    if modules.is_empty() {
        for ext in default_provider_extensions(provider_name) {
            extensions.insert(ext.to_string());
        }
        return;
    }

    for module in modules {
        for pattern in module_patterns(provider_name, &module) {
            for ext in extensions_for_mime_pattern(&pattern) {
                extensions.insert(ext.to_string());
            }
        }
    }
}

fn compute_enabled_plugin_extensions(config: &PluginConfig) -> Vec<String> {
    let mut extensions = BTreeSet::new();
    collect_provider_extensions("system", &config.system, &mut extensions);
    collect_provider_extensions("ffmpeg", &config.ffmpeg, &mut extensions);
    collect_provider_extensions("susie64", &config.susie64, &mut extensions);
    extensions.into_iter().collect()
}

fn active_modules(provider_name: &str, config: &PluginProviderConfig) -> Vec<PluginModuleConfig> {
    let modules = if config.modules.is_empty() {
        discover_plugin_modules(provider_name, config)
    } else {
        config.modules.clone()
    };
    modules.into_iter().filter(|module| module.enable).collect()
}

fn module_supports_input(
    provider_name: &str,
    module: &PluginModuleConfig,
    input: &DecodeInput,
) -> bool {
    let patterns = module_patterns(provider_name, module);
    if patterns.is_empty() {
        return true;
    }
    let Some(mime) = input.mime else {
        return false;
    };
    patterns.iter().any(|pattern| mime_matches(pattern, mime))
}

fn module_patterns(provider_name: &str, module: &PluginModuleConfig) -> Vec<String> {
    let configured = module
        .ext
        .iter()
        .filter(|ext| ext.enable)
        .flat_map(|ext| ext.mime.iter().cloned())
        .collect::<Vec<_>>();
    if !configured.is_empty() {
        return configured;
    }
    default_mime_patterns(provider_name, module)
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn module_priority(
    provider_name: &str,
    module: &PluginModuleConfig,
    fallback_priority: i32,
) -> i32 {
    let mut priorities = module
        .ext
        .iter()
        .filter(|ext| ext.enable)
        .flat_map(|ext| ext.modules.iter())
        .filter(|capability| capability.capability_type.eq_ignore_ascii_case("decode"))
        .map(|capability| priority_value(&capability.priority))
        .collect::<Vec<_>>();
    if priorities.is_empty() {
        priorities.push(fallback_priority.max(default_priority(provider_name)));
    }
    *priorities.iter().max().unwrap_or(&fallback_priority)
}

fn default_priority(provider_name: &str) -> i32 {
    match provider_name {
        "system" => 280,
        "ffmpeg" => 100,
        "susie64" => 100,
        _ => 100,
    }
}

fn priority_value(priority: &str) -> i32 {
    match priority.to_ascii_lowercase().as_str() {
        "high" => 400,
        "low" => 200,
        "middle" => 100,
        "lowest" => 0,
        _ => 100,
    }
}

fn provider_rank(provider: ProviderKind) -> i32 {
    match provider {
        ProviderKind::Internal => 4,
        ProviderKind::System => 3,
        ProviderKind::Ffmpeg => 2,
        ProviderKind::Susie64 => 1,
    }
}

fn mime_matches(pattern: &str, mime: &str) -> bool {
    if pattern == mime {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return mime.starts_with(&format!("{prefix}/"));
    }
    false
}

fn mime_from_path(path: &Path) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(OsStr::to_str)
        .map(|value| value.to_ascii_lowercase())?;
    match ext.as_str() {
        "ico" => Some("image/x-icon"),
        "mag" => Some("image/x-mag"),
        "maki" => Some("image/x-maki"),
        "pi" => Some("image/x-pi"),
        "pic" => Some("image/x-pic"),
        "vsp" => Some("image/x-vsp"),
        "pcd" => Some("image/x-photo-cd"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        "jp2" | "j2k" | "jpc" | "j2c" | "jpf" => Some("image/jp2"),
        "tif" | "tiff" => Some("image/tiff"),
        "heif" | "heic" => Some("image/heif"),
        "jxr" | "wdp" | "hdp" => Some("image/jxr"),
        _ => None,
    }
}

fn default_module_extensions(provider_name: &str, plugin_name: &str) -> Vec<PluginExtensionConfig> {
    let patterns = default_mime_patterns(
        provider_name,
        &PluginModuleConfig {
            enable: true,
            path: None,
            plugin_name: plugin_name.to_string(),
            plugin_type: provider_default_type(provider_name).to_string(),
            ext: Vec::new(),
        },
    );
    if patterns.is_empty() {
        return Vec::new();
    }
    vec![PluginExtensionConfig {
        enable: true,
        mime: patterns.into_iter().map(str::to_string).collect(),
        modules: vec![PluginCapabilityConfig {
            capability_type: "decode".to_string(),
            priority: if provider_name == "system" {
                "high".to_string()
            } else {
                "middle".to_string()
            },
        }],
    }]
}

fn default_mime_patterns(provider_name: &str, module: &PluginModuleConfig) -> Vec<&'static str> {
    let name = module.plugin_name.to_ascii_lowercase();
    match provider_name {
        "ffmpeg" => vec!["image/avif", "image/jp2"],
        "susie64" if name.contains("jpeg2k") => vec!["image/jp2"],
        "susie64" if name.contains("jpegt") => vec!["image/tiff"],
        "system" => vec!["image/*"],
        _ => Vec::new(),
    }
}

fn matches_provider(provider_name: &str, path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(OsStr::to_str)
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    match provider_name {
        "susie64" => {
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            {
                matches!(ext.as_str(), "sph" | "dll")
            }
            #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
            {
                false
            }
        }
        "ffmpeg" => {
            matches!(ext.as_str(), "dll" | "so" | "dylib" | "exe")
                && (name.contains("ffmpeg")
                    || name.contains("avcodec")
                    || name.contains("avformat")
                    || name.contains("avutil"))
        }
        "system" => false,
        _ => false,
    }
}

fn provider_default_type(provider_name: &str) -> &'static str {
    match provider_name {
        "susie64" => "image",
        "ffmpeg" => "image",
        "system" => "image",
        _ => "image",
    }
}

fn extensions_for_mime_pattern(pattern: &str) -> &'static [&'static str] {
    match pattern {
        "image/png" => &["png"],
        "image/jpeg" => &["jpg", "jpeg"],
        "image/gif" => &["gif"],
        "image/bmp" => &["bmp"],
        "image/webp" => &["webp"],
        "image/avif" => &["avif"],
        "image/jp2" => &["jp2", "j2k", "jpc", "j2c", "jpf"],
        "image/tiff" => &["tif", "tiff"],
        _ => &[],
    }
}

fn default_provider_extensions(provider_name: &str) -> &'static [&'static str] {
    match provider_name {
        "system" => &["avif", "heic", "heif", "jxr", "wdp", "hdp"],
        "ffmpeg" => &["avif", "jp2", "j2k", "jpc", "j2c", "jpf"],
        _ => &[],
    }
}

fn current_runtime_plugin_config() -> PluginConfig {
    runtime_plugin_config()
        .lock()
        .map(|config| config.clone())
        .unwrap_or_default()
}

fn runtime_plugin_config() -> &'static Mutex<PluginConfig> {
    static CONFIG: OnceLock<Mutex<PluginConfig>> = OnceLock::new();
    CONFIG.get_or_init(|| Mutex::new(PluginConfig::default()))
}

fn runtime_plugin_extensions() -> &'static Mutex<Vec<String>> {
    static CONFIG: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    CONFIG.get_or_init(|| Mutex::new(compute_enabled_plugin_extensions(&PluginConfig::default())))
}

#[cfg(test)]
#[path = "../../../tests/support/src/dependent/plugins/mod_tests.rs"]
mod tests;
