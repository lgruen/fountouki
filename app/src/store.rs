//! Storage handle: a cloneable, interior-mutable wrapper around a
//! `KeyValueStore` so scenes can read/write persisted state any time without
//! threading `&mut` through the frame context. Backed by an in-memory store for
//! now (capture/dev); a file-backed (native) and localStorage (wasm) impl land
//! with the platform glue.
use fountouki_core::storage::{KeyValueStore, MemStore};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct Db(Rc<RefCell<Box<dyn KeyValueStore>>>);

impl Db {
    pub fn mem() -> Db {
        Db(Rc::new(RefCell::new(Box::new(MemStore::new()))))
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
