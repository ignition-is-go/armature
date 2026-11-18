//! The files a person last chose or dropped. The primitive hears only their names and
//! sizes, so the files themselves are kept here for the application to read.

use std::cell::RefCell;

use armature_core::primitive::FileInfo;

thread_local! {
    static CHOSEN: RefCell<Vec<web_sys::File>> = const { RefCell::new(Vec::new()) };
}

/// Keeps the files and describes them for the primitive.
pub fn remember(list: Option<web_sys::FileList>) -> Vec<FileInfo> {
    let files: Vec<web_sys::File> = list
        .map(|list| (0..list.length()).filter_map(|at| list.get(at)).collect())
        .unwrap_or_default();
    let described = files
        .iter()
        .map(|file| FileInfo {
            name: file.name(),
            kind: file.type_(),
            size: file.size() as u64,
            path: None,
        })
        .collect();
    CHOSEN.with_borrow_mut(|chosen| {
        // A file offered again replaces the one of the same name.
        for file in files {
            chosen.retain(|held| held.name() != file.name());
            chosen.push(file);
        }
    });
    described
}

/// The file a person chose or dropped under this name, to read or upload. Names come
/// from a file input's value.
#[must_use]
pub fn chosen_file(name: &str) -> Option<web_sys::File> {
    CHOSEN.with_borrow(|chosen| chosen.iter().find(|file| file.name() == name).cloned())
}
