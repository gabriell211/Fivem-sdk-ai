use serde::Serialize;
use std::{fs, path::{Component, Path, PathBuf}};
use walkdir::WalkDir;

use crate::{AppError, AppResult};

const MAX_READ_BYTES: u64 = 4 * 1024 * 1024;
const MAX_WRITE_BYTES: usize = 2 * 1024 * 1024;
const MAX_FILES: usize = 8_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFile {
    pub path: String,
    pub size: u64,
}

pub fn root(workspace: &str) -> AppResult<PathBuf> {
    let root = fs::canonicalize(workspace)?;
    if !root.is_dir() {
        return Err(AppError::InvalidInput("workspace must be a directory".into()));
    }
    Ok(root)
}

fn checked_relative(path: &str) -> AppResult<PathBuf> {
    let candidate = Path::new(path);
    if candidate.is_absolute() || candidate.as_os_str().is_empty() {
        return Err(AppError::InvalidInput("path must be relative".into()));
    }
    if candidate.components().any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
        return Err(AppError::InvalidInput("path traversal is not allowed".into()));
    }
    Ok(candidate.to_path_buf())
}

fn should_skip(entry: &walkdir::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    entry.depth() > 0 && entry.file_type().is_dir() && matches!(name.as_ref(), ".git" | "node_modules" | "target" | ".sdkai")
}

pub fn list_files(workspace: &str) -> AppResult<Vec<WorkspaceFile>> {
    let root = root(workspace)?;
    let mut files = Vec::new();

    for entry in WalkDir::new(&root).follow_links(false).into_iter().filter_entry(|entry| !should_skip(entry)) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(&root).map_err(|_| AppError::InvalidInput("file escaped workspace".into()))?;
        files.push(WorkspaceFile {
            path: relative.to_string_lossy().replace('\\', "/"),
            size: entry.metadata()?.len(),
        });
        if files.len() >= MAX_FILES {
            break;
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

pub fn read_file(workspace: &str, path: &str) -> AppResult<String> {
    let root = root(workspace)?;
    let relative = checked_relative(path)?;
    let target = fs::canonicalize(root.join(relative))?;
    if !target.starts_with(&root) || !target.is_file() {
        return Err(AppError::InvalidInput("file is outside workspace".into()));
    }
    let metadata = fs::metadata(&target)?;
    if metadata.len() > MAX_READ_BYTES {
        return Err(AppError::InvalidInput("file is too large for the editor".into()));
    }
    Ok(fs::read_to_string(target)?)
}

pub fn write_file(workspace: &str, path: &str, content: &str) -> AppResult<()> {
    if content.len() > MAX_WRITE_BYTES {
        return Err(AppError::InvalidInput("write exceeds 2 MiB safety limit".into()));
    }
    let root = root(workspace)?;
    let relative = checked_relative(path)?;
    let target = root.join(relative);
    let parent = target.parent().ok_or_else(|| AppError::InvalidInput("invalid target path".into()))?;
    fs::create_dir_all(parent)?;
    let canonical_parent = fs::canonicalize(parent)?;
    if !canonical_parent.starts_with(&root) {
        return Err(AppError::InvalidInput("write escaped workspace through a symlink".into()));
    }
    if target.exists() {
        let canonical_target = fs::canonicalize(&target)?;
        if !canonical_target.starts_with(&root) {
            return Err(AppError::InvalidInput("write target escaped workspace".into()));
        }
    }
    fs::write(target, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_is_rejected() {
        assert!(checked_relative("../secret").is_err());
        assert!(checked_relative("a/../../secret").is_err());
    }

    #[test]
    fn regular_relative_paths_are_allowed() {
        assert_eq!(checked_relative("resources/test/client.lua").unwrap(), PathBuf::from("resources/test/client.lua"));
    }
}
