//! Language tags: tidying them into the standard form, and reading which way their
//! script runs.

use crate::keys::Dir;

/// A tag such as `en_US`, `PT-br` or `de_DE.UTF-8@euro` in standard form: `en-US`,
/// `pt-BR`, `de-DE`. `None` if it is not a language tag.
#[must_use]
pub fn to_locale(tag: &str) -> Option<String> {
    // A POSIX tag carries an encoding and a modifier, as in `de_DE.UTF-8@euro`.
    let tag = tag.trim().split(['.', '@']).next().unwrap_or_default();
    if !tag
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return None;
    }
    let mut parts = tag.split(['-', '_']).filter(|part| !part.is_empty());
    let language = parts.next().filter(|language| {
        (2..=3).contains(&language.len()) && language.chars().all(|c| c.is_ascii_alphabetic())
    })?;
    let mut locale = language.to_ascii_lowercase();
    for part in parts {
        locale.push('-');
        match part.len() {
            // A script, such as Latn.
            4 if part.chars().all(|c| c.is_ascii_alphabetic()) => {
                locale.push_str(&part[..1].to_ascii_uppercase());
                locale.push_str(&part[1..].to_ascii_lowercase());
            }
            // A region, such as US.
            2 => locale.push_str(&part.to_ascii_uppercase()),
            _ => locale.push_str(&part.to_ascii_lowercase()),
        }
    }
    Some(locale)
}

/// The language part of a tag, lowercased.
#[must_use]
pub fn language_of(tag: &str) -> Option<String> {
    Some(to_locale(tag)?.split('-').next()?.to_string())
}

/// Which way text in a language runs.
#[must_use]
pub fn direction_of(tag: &str) -> Dir {
    const RIGHT_TO_LEFT: [&str; 10] = ["ar", "he", "fa", "ur", "yi", "ps", "sd", "ug", "dv", "ckb"];
    let arabic_script = tag
        .split(['-', '_'])
        .any(|part| part.eq_ignore_ascii_case("arab") || part.eq_ignore_ascii_case("hebr"));
    match language_of(tag) {
        Some(language) if arabic_script || RIGHT_TO_LEFT.contains(&language.as_str()) => Dir::Rtl,
        _ => Dir::Ltr,
    }
}

/// The first of the person's preferred languages that the application supports,
/// matching on language when the region differs.
#[must_use]
pub fn pick_locale<'a>(preferred: &[&str], supported: &[&'a str]) -> Option<&'a str> {
    preferred.iter().find_map(|wanted| {
        let wanted_locale = to_locale(wanted)?;
        let exact = supported
            .iter()
            .find(|have| to_locale(have).as_deref() == Some(&wanted_locale));
        let same_language = || {
            supported
                .iter()
                .find(|have| language_of(have) == language_of(wanted))
        };
        exact.or_else(same_language).copied()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_tidied_into_standard_form() {
        assert_eq!(to_locale("en_US").as_deref(), Some("en-US"));
        assert_eq!(to_locale("PT-br").as_deref(), Some("pt-BR"));
        assert_eq!(to_locale("zh_hant_tw").as_deref(), Some("zh-Hant-TW"));
        assert_eq!(to_locale("  fr "), Some("fr".into()));
        assert_eq!(to_locale("12"), None);
        assert_eq!(to_locale(""), None);
    }

    #[test]
    fn right_to_left_languages_and_scripts_are_recognised() {
        assert_eq!(direction_of("ar-EG"), Dir::Rtl);
        assert_eq!(direction_of("he"), Dir::Rtl);
        assert_eq!(
            direction_of("pa-Arab-PK"),
            Dir::Rtl,
            "Punjabi in Arabic script"
        );
        assert_eq!(direction_of("en-GB"), Dir::Ltr);
        assert_eq!(direction_of("nonsense!"), Dir::Ltr);
    }

    #[test]
    fn the_first_supported_preference_wins_and_a_language_matches_across_regions() {
        let supported = ["en-US", "fr-FR", "pt-BR"];
        assert_eq!(
            pick_locale(&["de", "fr_CA", "en"], &supported),
            Some("fr-FR")
        );
        assert_eq!(pick_locale(&["pt-br"], &supported), Some("pt-BR"));
        assert_eq!(pick_locale(&["ja"], &supported), None);
    }
}
