//! Storage handle: a cloneable, interior-mutable wrapper around a
//! `KeyValueStore` so scenes can read/write persisted state any time without
//! threading `&mut` through the frame context.
//!
//! The real app uses [`Db::persistent`] — localStorage on web (`web/storage.js`),
//! a JSON file on native desktop — so the sync token, mute, and local progress
//! survive a reload. The `--capture`/`--playtest` harnesses use [`Db::mem`]
//! (in-memory, deterministic, ephemeral).
use fountouki_core::storage::{KeyValueStore, MemStore};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct Db(Rc<RefCell<Box<dyn KeyValueStore>>>);

impl Db {
    pub fn mem() -> Db {
        Db(Rc::new(RefCell::new(Box::new(MemStore::new()))))
    }

    /// The real app's persistent store: localStorage on web, a JSON file on
    /// native. Runs the one-time legacy key migration on first read.
    pub fn persistent() -> Db {
        #[cfg(target_arch = "wasm32")]
        let store: Box<dyn KeyValueStore> = Box::new(web::WebStore);
        #[cfg(not(target_arch = "wasm32"))]
        let store: Box<dyn KeyValueStore> = Box::new(file::FileStore::load());
        let mut store = store;
        fountouki_core::storage::migrate_legacy(&mut *store);
        Db(Rc::new(RefCell::new(store)))
    }

    pub fn from_store(store: Box<dyn KeyValueStore>) -> Db {
        Db(Rc::new(RefCell::new(store)))
    }
    pub fn get(&self, key: &str) -> Option<String> {
        self.0.borrow().get(key)
    }
    pub fn set(&self, key: &str, val: &str) {
        self.0.borrow_mut().set(key, val);
    }
    pub fn remove(&self, key: &str) {
        self.0.borrow_mut().remove(key);
    }
    /// Borrow the underlying store for the core `load_*`/`save_*` helpers
    /// (which take `&S`/`&mut S: KeyValueStore`). Keep borrows short-lived.
    pub fn borrow_kv(&self) -> std::cell::Ref<'_, Box<dyn KeyValueStore>> {
        self.0.borrow()
    }
    pub fn borrow_kv_mut(&self) -> std::cell::RefMut<'_, Box<dyn KeyValueStore>> {
        self.0.borrow_mut()
    }
}

/// Persist the shared (app-wide) mute flag. Shared by every scene's mute button
/// so the toggle survives a relaunch and the games agree on the state.
pub fn persist_mute(db: &Db, muted: bool) {
    let mut s = {
        let kv = db.borrow_kv();
        fountouki_core::settings::load_shared(&**kv)
    };
    s.muted = muted;
    let mut kv = db.borrow_kv_mut();
    fountouki_core::settings::save_shared(&mut **kv, &s);
}

/// localStorage-backed store for the web build, via the `fountouki_storage`
/// JS plugin (`web/storage.js`) and sapp_jsutils string marshalling.
#[cfg(target_arch = "wasm32")]
mod web {
    use fountouki_core::storage::KeyValueStore;
    use sapp_jsutils::{JsObject, JsObjectWeak};

    extern "C" {
        fn fountouki_ls_get(key: JsObjectWeak) -> JsObject;
        fn fountouki_ls_set(key: JsObjectWeak, val: JsObjectWeak);
        fn fountouki_ls_remove(key: JsObjectWeak);
    }

    pub struct WebStore;

    impl KeyValueStore for WebStore {
        fn get(&self, key: &str) -> Option<String> {
            let k = JsObject::string(key);
            let mut s = String::new();
            unsafe { fountouki_ls_get(k.weak()) }.to_string(&mut s);
            // Values are always non-empty JSON, so "" == absent (see storage.js).
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        fn set(&mut self, key: &str, val: &str) {
            let k = JsObject::string(key);
            let v = JsObject::string(val);
            unsafe { fountouki_ls_set(k.weak(), v.weak()) };
        }
        fn remove(&mut self, key: &str) {
            let k = JsObject::string(key);
            unsafe { fountouki_ls_remove(k.weak()) };
        }
    }
}

/// File-backed store for native desktop: a JSON map loaded at boot and
/// rewritten on each mutation. Best-effort — IO errors no-op (matching the
/// `KeyValueStore` contract). Native is a dev/optional target; the canonical
/// build is web.
#[cfg(not(target_arch = "wasm32"))]
mod file {
    use fountouki_core::storage::KeyValueStore;
    use nanoserde::{DeJson, SerJson};
    use std::collections::HashMap;
    use std::path::PathBuf;

    pub struct FileStore {
        path: PathBuf,
        map: HashMap<String, String>,
    }

    impl FileStore {
        /// Load the store from the default path (`$HOME/.fountouki-store.json`).
        pub fn load() -> FileStore {
            Self::at(Self::default_path())
        }

        /// Load from an explicit path (used by tests).
        pub fn at(path: PathBuf) -> FileStore {
            let map = std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| HashMap::deserialize_json(&s).ok())
                .unwrap_or_default();
            FileStore { path, map }
        }

        fn default_path() -> PathBuf {
            let base = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            base.join(".fountouki-store.json")
        }

        fn flush(&self) {
            let _ = std::fs::write(&self.path, self.map.serialize_json());
        }
    }

    impl KeyValueStore for FileStore {
        fn get(&self, key: &str) -> Option<String> {
            self.map.get(key).cloned()
        }
        fn set(&mut self, key: &str, val: &str) {
            self.map.insert(key.to_string(), val.to_string());
            self.flush();
        }
        fn remove(&mut self, key: &str) {
            self.map.remove(key);
            self.flush();
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::file::FileStore;
    use fountouki_core::storage::KeyValueStore;

    #[test]
    fn file_store_persists_across_reload() {
        let path = std::env::temp_dir().join("fountouki-store-test.json");
        let _ = std::fs::remove_file(&path);

        let key = "fountouki.shared.settings.v1";
        {
            let mut s = FileStore::at(path.clone());
            assert_eq!(s.get(key), None);
            s.set(key, "{\"muted\":true}");
        }
        // A fresh handle reads the same path → the value survived "relaunch".
        let reloaded = FileStore::at(path.clone());
        assert_eq!(reloaded.get(key), Some("{\"muted\":true}".to_string()));

        let _ = std::fs::remove_file(&path);
    }
}
