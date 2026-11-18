//! Letters without their case or accents, so that typing `lodz` finds `Łódź`.
//!
//! Some languages count an accented letter as a letter of its own, and for them it
//! is kept: `ä` is not `a` in Swedish, and Turkish has a dotless `ı`.

/// A letter folded for comparison in the language `locale` names. `None` folds as
/// English does.
#[must_use]
pub fn fold_char(ch: char, locale: Option<&str>) -> char {
    let language = locale.map(|tag| {
        tag.split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_lowercase()
    });
    let language = language.as_deref().unwrap_or("en");
    // Turkish capital I is the dotless one.
    if matches!(language, "tr" | "az") && ch == 'I' {
        return 'ı';
    }
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    let own_letter = match language {
        "sv" | "fi" => matches!(lower, 'å' | 'ä' | 'ö'),
        "da" | "nb" | "nn" | "no" => matches!(lower, 'å' | 'æ' | 'ø'),
        "de" => matches!(lower, 'ä' | 'ö' | 'ü'),
        "es" => lower == 'ñ',
        "tr" | "az" => matches!(lower, 'ı' | 'ö' | 'ü' | 'ç' | 'ş' | 'ğ'),
        _ => false,
    };
    if own_letter {
        return lower;
    }
    match lower {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' | 'ĉ' => 'c',
        'ď' | 'đ' => 'd',
        'è'..='ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ğ' | 'ĝ' => 'g',
        'ì'..='ï' | 'ī' | 'į' | 'ı' => 'i',
        'ł' | 'ľ' | 'ĺ' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ř' | 'ŕ' => 'r',
        'š' | 'ś' | 'ş' | 'ș' => 's',
        'ť' | 'ț' => 't',
        'ù'..='ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ž' | 'ź' | 'ż' => 'z',
        other => other,
    }
}

/// Text folded letter by letter. It keeps its length in characters, so a place in
/// the folded text is the same place in the original.
#[must_use]
pub fn fold_text(text: &str, locale: Option<&str>) -> String {
    text.chars().map(|ch| fold_char(ch, locale)).collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "a test fails by panicking, and its arithmetic is on values it wrote itself"
)]
mod tests {
    use super::*;

    #[test]
    fn folding_keeps_the_number_of_characters() {
        for text in ["Łódź", "Crème Brûlée", "İstanbul", "ǅ"] {
            assert_eq!(fold_text(text, None).chars().count(), text.chars().count());
        }
    }
}
