use armature_core::preference::Storage;

/// The browser's `localStorage`, for keeping preferences between visits. Where the
/// browser refuses access, such as in some private modes, reads find nothing and
/// writes are dropped.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalStorage;

// Reads and writes on a browser store. Where the browser refuses, reads find nothing
// and writes are dropped.
fn read(store: Option<web_sys::Storage>, key: &str) -> Option<String> {
    store?.get_item(key).ok()?
}

fn write(store: Option<web_sys::Storage>, key: &str, value: &str) {
    if let Some(store) = store {
        let _ = store.set_item(key, value);
    }
}

fn erase(store: Option<web_sys::Storage>, key: &str) {
    if let Some(store) = store {
        let _ = store.remove_item(key);
    }
}

fn store() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

impl Storage for LocalStorage {
    fn get(&self, key: &str) -> Option<String> {
        read(store(), key)
    }
    fn set(&mut self, key: &str, value: &str) {
        write(store(), key, value);
    }
    fn remove(&mut self, key: &str) {
        erase(store(), key);
    }
}

/// Cookies, for preferences the server should see on the next request, so it can
/// render the right thing the first time. Each is kept for a year, for the whole site.
#[derive(Clone, Copy, Debug, Default)]
pub struct CookieStorage;

fn cookies() -> Option<String> {
    let document = web_sys::window()?.document()?;
    let cookie = js_sys::Reflect::get(&document, &"cookie".into()).ok()?;
    cookie.as_string()
}

fn write_cookie(text: &str) {
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        let _ = js_sys::Reflect::set(&document, &"cookie".into(), &text.into());
    }
}

fn encode(text: &str) -> String {
    js_sys::encode_uri_component(text).into()
}

impl Storage for CookieStorage {
    fn get(&self, key: &str) -> Option<String> {
        let name = encode(key);
        cookies()?.split("; ").find_map(|pair| {
            let (found, value) = pair.split_once('=')?;
            (found == name).then(|| {
                js_sys::decode_uri_component(value).map_or_else(|_| value.to_string(), String::from)
            })
        })
    }

    fn set(&mut self, key: &str, value: &str) {
        write_cookie(&format!(
            "{}={}; path=/; max-age=31536000; samesite=lax",
            encode(key),
            encode(value)
        ));
    }

    fn remove(&mut self, key: &str) {
        write_cookie(&format!(
            "{}=; path=/; max-age=0; samesite=lax",
            encode(key)
        ));
    }
}

/// Whether a media query holds now.
fn media_matches(query: &str) -> bool {
    let list = web_sys::window().and_then(|window| window.match_media(query).ok().flatten());
    list.is_some_and(|list| list.matches())
}

/// Whether the operating system asks for a dark colour scheme, which is what a
/// stored choice of `system` resolves to.
#[must_use]
pub fn system_prefers_dark() -> bool {
    media_matches("(prefers-color-scheme: dark)")
}

/// The browser's `sessionStorage`: kept for as long as the tab is open, and not
/// shared between tabs.
#[derive(Clone, Copy, Debug, Default)]
pub struct SessionStorage;

fn session() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok()?
}

impl Storage for SessionStorage {
    fn get(&self, key: &str) -> Option<String> {
        read(session(), key)
    }
    fn set(&mut self, key: &str, value: &str) {
        write(session(), key, value);
    }
    fn remove(&mut self, key: &str) {
        erase(session(), key);
    }
}

/// Whether the person has asked their system for less motion. Skip or shorten
/// animations when they have.
#[must_use]
pub fn prefers_reduced_motion() -> bool {
    media_matches("(prefers-reduced-motion: reduce)")
}

/// Whether the person last used the keyboard or a pointer. The page's root element
/// carries the same as `data-armature-modality`, so a style sheet can show focus
/// boldly for the keyboard alone.
#[must_use]
pub fn modality() -> armature_core::preference::Modality {
    let root = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element());
    match root
        .and_then(|root| root.get_attribute("data-armature-modality"))
        .as_deref()
    {
        Some("keyboard") => armature_core::preference::Modality::Keyboard,
        _ => armature_core::preference::Modality::Pointer,
    }
}
