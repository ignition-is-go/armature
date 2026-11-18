//! What to tell the platform about a text field so it offers the right on-screen
//! keyboard and autofill.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Email,
    Phone,
    Url,
    /// Whole numbers.
    Number,
    /// Numbers that may have a fractional part.
    Decimal,
    Search,
    /// A code sent by message or produced by an authenticator.
    OneTimeCode,
    NewPassword,
    CurrentPassword,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputHints {
    /// Which on-screen keyboard to show.
    pub keyboard: &'static str,
    /// What autofill may offer.
    pub autofill: &'static str,
    pub spellcheck: bool,
    /// Whether the platform may capitalise what is typed.
    pub capitalize: bool,
    /// Whether the typed characters are hidden.
    pub concealed: bool,
}

/// The hints for a kind of field. The names are the web's, which other platforms map
/// from.
#[must_use]
pub const fn input_hints(kind: FieldKind) -> InputHints {
    let plain = InputHints {
        keyboard: "text",
        autofill: "off",
        spellcheck: false,
        capitalize: false,
        concealed: false,
    };
    match kind {
        FieldKind::Text => InputHints {
            spellcheck: true,
            capitalize: true,
            autofill: "on",
            ..plain
        },
        FieldKind::Email => InputHints {
            keyboard: "email",
            autofill: "email",
            ..plain
        },
        FieldKind::Phone => InputHints {
            keyboard: "tel",
            autofill: "tel",
            ..plain
        },
        FieldKind::Url => InputHints {
            keyboard: "url",
            autofill: "url",
            ..plain
        },
        FieldKind::Number => InputHints {
            keyboard: "numeric",
            ..plain
        },
        FieldKind::Decimal => InputHints {
            keyboard: "decimal",
            ..plain
        },
        FieldKind::Search => InputHints {
            keyboard: "search",
            ..plain
        },
        FieldKind::OneTimeCode => InputHints {
            keyboard: "numeric",
            autofill: "one-time-code",
            ..plain
        },
        FieldKind::NewPassword => InputHints {
            autofill: "new-password",
            concealed: true,
            ..plain
        },
        FieldKind::CurrentPassword => InputHints {
            autofill: "current-password",
            concealed: true,
            ..plain
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_one_time_code_gets_a_number_pad_and_code_autofill() {
        let hints = input_hints(FieldKind::OneTimeCode);
        assert_eq!(
            (hints.keyboard, hints.autofill),
            ("numeric", "one-time-code")
        );
    }

    #[test]
    fn passwords_are_concealed_and_never_spellchecked() {
        for kind in [FieldKind::NewPassword, FieldKind::CurrentPassword] {
            let hints = input_hints(kind);
            assert!(hints.concealed && !hints.spellcheck && !hints.capitalize);
        }
        assert_eq!(input_hints(FieldKind::NewPassword).autofill, "new-password");
    }

    #[test]
    fn only_prose_is_spellchecked_and_capitalised() {
        assert!(input_hints(FieldKind::Text).spellcheck);
        assert!(!input_hints(FieldKind::Email).spellcheck);
        assert!(!input_hints(FieldKind::Url).capitalize);
    }
}
