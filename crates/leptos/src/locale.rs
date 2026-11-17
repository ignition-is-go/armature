//! Numbers and dates written the way a language writes them, from the browser's own
//! locale data. `armature_core` has a small built-in table for hosts without one.

use armature_core::date::Date;
use js_sys::{Array, Function, Intl, Object, Reflect};
use wasm_bindgen::{JsCast, JsValue};

fn locales(locale: &str) -> Array {
    Array::of1(&locale.into())
}

fn options(pairs: &[(&str, &str)]) -> Object {
    let options = Object::new();
    for (name, value) in pairs {
        let _ = Reflect::set(&options, &(*name).into(), &(*value).into());
    }
    options
}

fn call(format: &Function, value: &JsValue) -> String {
    format
        .call1(&JsValue::NULL, value)
        .ok()
        .and_then(|text| text.as_string())
        .unwrap_or_default()
}

/// A number as `locale` writes it: `1,234.5`, `1.234,5`, `1 234,5`.
pub fn format_number(value: f64, locale: &str) -> String {
    call(
        &Intl::NumberFormat::new(&locales(locale), &Object::new()).format(),
        &value.into(),
    )
}

fn date_text(date: Date, locale: &str, pairs: &[(&str, &str)]) -> String {
    let mut pairs = pairs.to_vec();
    // The date is a calendar day, not a moment, so it must not shift with the zone.
    pairs.push(("timeZone", "UTC"));
    let moment =
        js_sys::Date::new(&js_sys::Date::utc(date.year as f64, date.month as f64 - 1.0).into());
    moment.set_utc_date(date.day as u32);
    let format = Intl::DateTimeFormat::new(&locales(locale), &options(&pairs)).format();
    call(format.unchecked_ref(), &moment.into())
}

/// A date as `locale` writes it in full: `17 November 2026`, `17. November 2026`.
pub fn format_date(date: Date, locale: &str) -> String {
    date_text(date, locale, &[("dateStyle", "long")])
}

/// A month's name in `locale`, counting from 1.
pub fn month_name(month: u8, locale: &str) -> String {
    let date = Date {
        year: 2026,
        month: month.clamp(1, 12),
        day: 1,
    };
    date_text(date, locale, &[("month", "long")])
}

/// A weekday's name in `locale`, with Sunday as 0.
pub fn weekday_name(weekday: u8, locale: &str) -> String {
    // The first of November 2026 is a Sunday.
    let date = Date {
        year: 2026,
        month: 11,
        day: 1 + weekday % 7,
    };
    date_text(date, locale, &[("weekday", "long")])
}
