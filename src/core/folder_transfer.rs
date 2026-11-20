//! LocalSend represents a folder as files with slash-separated relative names.

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use localsend::{
    model::transfer::{FileDto, FileMetadata},
    util::filename::{Rules, sanitize},
};
use tokio::fs;

pub const MAX_FILES: usize = 10_000;
const MAX_DEPTH: usize = 128;
const MAX_PATH_BYTES: usize = 4096;
const PREVIEW_FILES: usize = 20;

pub type OfferedFiles = HashMap<String, (FileDto, PathBuf)>;

pub async fn prepare(paths: Vec<String>) -> Result<OfferedFiles> {
    ensure!(
        !paths.is_empty() && paths.len() <= MAX_FILES,
        "Select between 1 and {MAX_FILES} items"
    );
    let mut files = HashMap::new();
    for path in paths.into_iter().map(PathBuf::from) {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Choose a named file or folder with a valid UTF-8 name")?
            .to_owned();
        let mut pending = vec![(path, name, 1)];
        let previous_count = files.len();
        while let Some((path, name, depth)) = pending.pop() {
            let metadata = fs::symlink_metadata(&path)
                .await
                .with_context(|| format!("Could not open {}", path.display()))?;
            if metadata.is_dir() {
                ensure!(
                    depth < MAX_DEPTH,
                    "Folder nesting exceeds {MAX_DEPTH} levels"
                );
                let mut directory = fs::read_dir(&path)
                    .await
                    .with_context(|| format!("Could not read {}", path.display()))?;
                while let Some(entry) = directory.next_entry().await? {
                    let leaf = entry.file_name();
                    let leaf = leaf
                        .to_str()
                        .context("A file or folder has a name that is not valid UTF-8")?;
                    pending.push((entry.path(), format!("{name}/{leaf}"), depth + 1));
                }
            } else if metadata.is_file() || (depth == 1 && metadata.is_symlink()) {
                // An explicitly selected file may be a link. While walking a folder, the
                // selection includes only regular files and directories under that tree.
                let metadata = if metadata.is_symlink() {
                    fs::metadata(&path).await?
                } else {
                    metadata
                };
                ensure!(
                    metadata.is_file(),
                    "Choose the original folder instead of a symbolic link to it"
                );
                ensure!(
                    files.len() < MAX_FILES,
                    "This transfer contains more than {MAX_FILES} files"
                );
                ensure!(
                    name.len() <= MAX_PATH_BYTES,
                    "A relative file name is too long: {name}"
                );
                let id = uuid::Uuid::new_v4().to_string();
                let dto = FileDto {
                    id: id.clone(),
                    file_name: name,
                    size: metadata.len(),
                    file_type: "application/octet-stream".into(),
                    sha256: None,
                    preview: None,
                    metadata: FileMetadata::from_fs_metadata(&metadata),
                };
                files.insert(id, (dto, path));
            }
        }
        ensure!(
            files.len() > previous_count,
            "The selection contains no transferable files"
        );
    }
    Ok(files)
}

pub fn relative_name(name: &str) -> Result<PathBuf> {
    ensure!(
        name.len() <= MAX_PATH_BYTES,
        "A relative file name is too long"
    );
    let mut path = PathBuf::new();
    for (index, part) in name.split('/').enumerate() {
        ensure!(
            index < MAX_DEPTH && !part.is_empty() && part != "." && part != "..",
            "The sender offered an invalid relative file name"
        );
        ensure!(
            !part.contains('\0'),
            "The sender offered a file name containing a null byte"
        );
        let mut cleaned = sanitize(part, Rules::current());
        // Leave room for a numeric prefix if a file or root folder already exists.
        cleaned.truncate(cleaned.floor_char_boundary(230));
        path.push(cleaned);
    }
    Ok(path)
}

pub fn preview_names(mut names: Vec<String>) -> Vec<String> {
    names.sort_unstable();
    if names.len() > PREVIEW_FILES {
        let others = names.len() - PREVIEW_FILES;
        names.truncate(PREVIEW_FILES);
        names.push(format!("… and {others} more"));
    }
    names
}

/// Reserve one fresh destination for each advertised root in this offer. Never
/// merge an incoming folder with a preexisting directory or follow a symlink.
pub async fn receive_parent(
    destination: &Path,
    relative: &Path,
    roots: &mut HashMap<String, PathBuf>,
) -> Result<PathBuf> {
    let parent = relative.parent().context("Missing relative parent")?;
    let mut components = parent.iter();
    let Some(root) = components.next() else {
        return Ok(destination.to_owned());
    };
    let root = root.to_str().context("Invalid folder name")?;
    if !roots.contains_key(root) {
        roots.insert(root.to_owned(), create_root(destination, root).await?);
    }
    let mut current = roots[root].clone();
    for component in components {
        current.push(component);
        match fs::DirBuilder::new().mode(0o700).create(&current).await {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                ensure!(
                    fs::symlink_metadata(&current).await?.is_dir(),
                    "Incoming folder conflicts with an existing file"
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(current)
}

async fn create_root(destination: &Path, name: &str) -> Result<PathBuf> {
    for suffix in 0..10_000 {
        let path = destination.join(unique_name(name, suffix));
        match fs::DirBuilder::new().mode(0o700).create(&path).await {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    bail!("Too many folders with this name")
}

pub fn unique_name(name: &str, suffix: usize) -> String {
    if suffix == 0 {
        name.to_owned()
    } else {
        format!("{suffix} - {name}")
    }
}
