pub fn normalize_locale_tag(raw: Option<&str>) -> String {
    let raw = raw.unwrap_or("en").trim();
    if raw.is_empty() {
        return "en".to_string();
    }

    let raw = raw
        .split_once('.')
        .map(|(locale, _)| locale)
        .unwrap_or(raw)
        .split_once('@')
        .map(|(locale, _)| locale)
        .unwrap_or(raw)
        .replace('-', "_");

    let mut parts = raw.split('_');
    let language = parts.next().unwrap_or("en").to_ascii_lowercase();
    let region = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase());

    match region {
        Some(region) => format!("{language}_{region}"),
        None => language,
    }
}

pub fn resource_locale_fallbacks(locale: &str) -> Vec<String> {
    let normalized = normalize_locale_tag(Some(locale));
    let mut fallbacks = Vec::new();
    push_unique(&mut fallbacks, normalized.clone());

    if let Some((language, _)) = normalized.split_once('_') {
        push_unique(&mut fallbacks, language.to_string());
    }

    push_unique(&mut fallbacks, "en".to_string());
    fallbacks
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}
