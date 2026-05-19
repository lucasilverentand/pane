use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub projects: Vec<Project>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub kind: FileNodeKind,
    pub children: Vec<FileNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileNodeKind {
    File,
    Directory,
}

impl Workspace {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            projects: Vec::new(),
        }
    }

    pub fn add_project(&mut self, path: impl AsRef<Path>) -> Result<(), WorkspaceError> {
        let path = path.as_ref();
        if !path.is_dir() {
            return Err(WorkspaceError::NotADirectory(path.to_path_buf()));
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unnamed".into());
        self.projects.push(Project {
            name,
            root: path.to_path_buf(),
        });
        Ok(())
    }
}

impl Project {
    pub fn file_tree(&self) -> Result<FileNode, WorkspaceError> {
        build_file_tree(&self.root, 3)
    }
}

fn build_file_tree(path: &Path, max_depth: usize) -> Result<FileNode, WorkspaceError> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    if path.is_file() {
        return Ok(FileNode {
            name,
            path: path.to_path_buf(),
            kind: FileNodeKind::File,
            children: Vec::new(),
        });
    }

    let mut children = Vec::new();
    if max_depth > 0 {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .map_err(|e| WorkspaceError::Io(e.to_string()))?
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let entry_name = entry.file_name().to_string_lossy().into_owned();
            // Skip hidden files and common noise
            if entry_name.starts_with('.')
                || entry_name == "node_modules"
                || entry_name == "target"
                || entry_name == ".build"
            {
                continue;
            }
            children.push(build_file_tree(&entry.path(), max_depth - 1)?);
        }

        // Sort: directories first, then files, alphabetical within each group
        children.sort_by(|a, b| {
            match (&a.kind, &b.kind) {
                (FileNodeKind::Directory, FileNodeKind::File) => std::cmp::Ordering::Less,
                (FileNodeKind::File, FileNodeKind::Directory) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });
    }

    Ok(FileNode {
        name,
        path: path.to_path_buf(),
        kind: FileNodeKind::Directory,
        children,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("io error: {0}")]
    Io(String),
}
