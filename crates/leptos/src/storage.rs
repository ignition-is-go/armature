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
