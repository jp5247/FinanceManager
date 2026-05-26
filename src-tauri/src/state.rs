use fm_profile::Session;
use fm_storage::{DataRoot, FilesystemStorage, StorageError};
use std::path::Path;
use std::sync::Mutex;

/// Tauri managed state: the data root and the (optional) unlocked session.
///
/// The PDF extractor is intentionally NOT held here. `pdfium-render`'s
/// bindings are `!Send`, which would block this struct from satisfying
/// Tauri's `State: Send + Sync + 'static` requirement. We pay the ~100 ms
/// dynamic-library load on every upload instead — cheap relative to user
/// click latency.
pub struct AppState {
    pub data_root: DataRoot,
    pub storage: FilesystemStorage,
    pub session: Mutex<Option<Session>>,
}

impl AppState {
    pub fn new(data_path: &Path) -> Result<Self, StorageError> {
        let data_root = DataRoot::new(data_path.to_path_buf())?;
        let storage = FilesystemStorage::new(data_root.clone());
        Ok(Self {
            data_root,
            storage,
            session: Mutex::new(None),
        })
    }
}
