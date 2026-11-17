use armature_core::preference::Storage;

/// The browser's `localStorage`, for keeping preferences between visits. Where the
/// browser refuses access, such as in some private modes, reads find nothing and
/// writes are dropped.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalStorage;

fn store() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

impl Storage for LocalStorage {
    fn get(&self, key: &str) -> Option<String> {
        store()?.get_item(key).ok()?
    }

    fn set(&mut self, key: &str, value: &str) {
        if let Some(store) = store() {
            let _ = store.set_item(key, value);
        }
    }

    fn remove(&mut self, key: &str) {
        if let Some(store) = store() {
            let _ = store.remove_item(key);
        }
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
                js_sys::decode_uri_component(value)
                    .map(String::from)
                    .unwrap_or_else(|_| value.to_string())
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
