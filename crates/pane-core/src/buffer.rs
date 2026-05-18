use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct BufferManager {
    buffers: HashMap<PathBuf, Buffer>,
}

#[derive(Debug, Clone)]
pub struct Buffer {
    pub path: PathBuf,
    pub content: String,
    pub dirty: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum BufferError {
    #[error("io error: {0}")]
    Io(String),
    #[error("buffer not found: {0}")]
    NotFound(PathBuf),
}

impl BufferManager {
    pub fn new() -> Self {
        Self {
            buffers: HashMap::new(),
        }
    }

    pub fn open(&mut self, path: impl AsRef<Path>) -> Result<&Buffer, BufferError> {
        let path = path.as_ref().to_path_buf();
        if !self.buffers.contains_key(&path) {
            let content =
                std::fs::read_to_string(&path).map_err(|e| BufferError::Io(e.to_string()))?;
            self.buffers.insert(
                path.clone(),
                Buffer {
                    path: path.clone(),
                    content,
                    dirty: false,
                },
            );
        }
        Ok(&self.buffers[&path])
    }

    pub fn update_content(
        &mut self,
        path: impl AsRef<Path>,
        content: String,
    ) -> Result<(), BufferError> {
        let path = path.as_ref().to_path_buf();
        let buffer = self
            .buffers
            .get_mut(&path)
            .ok_or_else(|| BufferError::NotFound(path))?;
        buffer.content = content;
        buffer.dirty = true;
        Ok(())
    }

    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<(), BufferError> {
        let path = path.as_ref().to_path_buf();
        let buffer = self
            .buffers
            .get_mut(&path)
            .ok_or_else(|| BufferError::NotFound(path))?;
        std::fs::write(&buffer.path, &buffer.content)
            .map_err(|e| BufferError::Io(e.to_string()))?;
        buffer.dirty = false;
        Ok(())
    }

    pub fn close(&mut self, path: impl AsRef<Path>) {
        self.buffers.remove(path.as_ref());
    }

    pub fn is_dirty(&self, path: impl AsRef<Path>) -> bool {
        self.buffers
            .get(path.as_ref())
            .is_some_and(|b| b.dirty)
    }
}

impl Default for BufferManager {
    fn default() -> Self {
        Self::new()
    }
}
