use crate::models::CosThread;
use std::io;
use std::path::PathBuf;
use uuid::Uuid;

/// Persists task snapshots under `~/Library/Application Support/Cos/Threads`.
/// Writes are atomic and files are sorted by last update on load.
#[derive(Debug, Clone)]
pub struct ThreadStore {
    directory: PathBuf,
}

impl Default for ThreadStore {
    fn default() -> Self {
        Self {
            directory: crate::application_support_dir().join("Cos/Threads"),
        }
    }
}

impl ThreadStore {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn load_all(&self) -> io::Result<Vec<CosThread>> {
        std::fs::create_dir_all(&self.directory)?;
        let mut threads = Vec::new();
        for entry in std::fs::read_dir(&self.directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with('.') || !name.ends_with(".json") {
                continue;
            }
            let data = std::fs::read(entry.path())?;
            match serde_json::from_slice::<CosThread>(&data) {
                Ok(thread) => threads.push(thread),
                Err(_) => continue,
            }
        }
        threads.sort_by(|lhs, rhs| rhs.updated_at.cmp(&lhs.updated_at));
        Ok(threads)
    }

    pub fn save(&self, thread: &CosThread) -> io::Result<()> {
        self.upsert(thread)
    }

    pub fn upsert(&self, thread: &CosThread) -> io::Result<()> {
        std::fs::create_dir_all(&self.directory)?;
        let target = self
            .directory
            .join(format!("{}.json", thread.id.to_string().to_uppercase()));
        let temporary = target.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(thread).map_err(io::Error::other)?;
        std::fs::write(&temporary, data)?;
        // Atomic replace: rename(2) swaps the file in place on the same volume.
        std::fs::rename(&temporary, &target)?;
        Ok(())
    }

    pub fn delete(&self, id: Uuid) -> io::Result<()> {
        let target = self
            .directory
            .join(format!("{}.json", id.to_string().to_uppercase()));
        match std::fs::remove_file(&target) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}
