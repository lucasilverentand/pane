use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pane_core::{BufferManager, FileNode, FileNodeKind, Workspace};

uniffi::setup_scaffolding!();

// Re-export types for UniFFI

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiFileNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub children: Vec<FfiFileNode>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiProject {
    pub name: String,
    pub root: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiBuffer {
    pub path: String,
    pub content: String,
    pub dirty: bool,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PaneError {
    #[error("workspace error: {message}")]
    Workspace { message: String },
    #[error("buffer error: {message}")]
    Buffer { message: String },
    #[error("agent error: {message}")]
    Agent { message: String },
}

#[derive(uniffi::Object)]
pub struct PaneEngine {
    workspace: Mutex<Option<Workspace>>,
    buffers: Mutex<BufferManager>,
}

#[uniffi::export]
impl PaneEngine {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            workspace: Mutex::new(None),
            buffers: Mutex::new(BufferManager::new()),
        })
    }

    /// Create a new workspace with the given name.
    pub fn create_workspace(&self, name: String) {
        let mut ws = self.workspace.lock().unwrap();
        *ws = Some(Workspace::new(name));
    }

    /// Add a project directory to the current workspace.
    pub fn add_project(&self, path: String) -> Result<(), PaneError> {
        let mut ws = self.workspace.lock().unwrap();
        let workspace = ws
            .as_mut()
            .ok_or_else(|| PaneError::Workspace {
                message: "no workspace open".into(),
            })?;
        workspace.add_project(&path).map_err(|e| PaneError::Workspace {
            message: e.to_string(),
        })
    }

    /// Get all projects in the current workspace.
    pub fn list_projects(&self) -> Result<Vec<FfiProject>, PaneError> {
        let ws = self.workspace.lock().unwrap();
        let workspace = ws.as_ref().ok_or_else(|| PaneError::Workspace {
            message: "no workspace open".into(),
        })?;
        Ok(workspace
            .projects
            .iter()
            .map(|p| FfiProject {
                name: p.name.clone(),
                root: p.root.to_string_lossy().into_owned(),
            })
            .collect())
    }

    /// Get the file tree for a project by index.
    pub fn file_tree(&self, project_index: u32) -> Result<FfiFileNode, PaneError> {
        let ws = self.workspace.lock().unwrap();
        let workspace = ws.as_ref().ok_or_else(|| PaneError::Workspace {
            message: "no workspace open".into(),
        })?;
        let project = workspace
            .projects
            .get(project_index as usize)
            .ok_or_else(|| PaneError::Workspace {
                message: format!("project index {project_index} out of bounds"),
            })?;
        let tree = project.file_tree().map_err(|e| PaneError::Workspace {
            message: e.to_string(),
        })?;
        Ok(to_ffi_node(&tree))
    }

    /// Open a file and return its content.
    pub fn open_file(&self, path: String) -> Result<FfiBuffer, PaneError> {
        let mut buffers = self.buffers.lock().unwrap();
        let buffer = buffers.open(&path).map_err(|e| PaneError::Buffer {
            message: e.to_string(),
        })?;
        Ok(FfiBuffer {
            path: buffer.path.to_string_lossy().into_owned(),
            content: buffer.content.clone(),
            dirty: buffer.dirty,
        })
    }

    /// Update a buffer's content (marks it as dirty).
    pub fn update_buffer(&self, path: String, content: String) -> Result<(), PaneError> {
        let mut buffers = self.buffers.lock().unwrap();
        buffers
            .update_content(&path, content)
            .map_err(|e| PaneError::Buffer {
                message: e.to_string(),
            })
    }

    /// Save a buffer to disk.
    pub fn save_buffer(&self, path: String) -> Result<(), PaneError> {
        let mut buffers = self.buffers.lock().unwrap();
        buffers.save(&path).map_err(|e| PaneError::Buffer {
            message: e.to_string(),
        })
    }

    /// Close a buffer.
    pub fn close_buffer(&self, path: String) {
        let mut buffers = self.buffers.lock().unwrap();
        buffers.close(PathBuf::from(path));
    }

    /// Simple ping to verify the bridge works.
    pub fn ping(&self) -> String {
        "pong from rust engine".to_string()
    }
}

fn to_ffi_node(node: &FileNode) -> FfiFileNode {
    FfiFileNode {
        name: node.name.clone(),
        path: node.path.to_string_lossy().into_owned(),
        is_directory: node.kind == FileNodeKind::Directory,
        children: node.children.iter().map(to_ffi_node).collect(),
    }
}
