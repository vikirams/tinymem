use anyhow::{Context, Result, ensure};
use ignore::{DirEntry, WalkBuilder};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

pub(crate) const MAX_FILE_BYTES: u64 = 1024 * 1024;
pub(crate) const CHUNK_LINES: usize = 80;

pub(crate) struct SourceFile {
    pub path: String,
    pub content: String,
    pub hash: String,
    pub language: &'static str,
}

pub(crate) fn repository_root(root: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .with_context(|| format!("cannot resolve repository root {}", root.display()))?;
    ensure!(root.is_dir(), "repository root must be a directory");
    ensure!(root.to_str().is_some(), "repository root must be UTF-8");
    Ok(root)
}

pub(crate) fn walk(root: &Path) -> ignore::Walk {
    WalkBuilder::new(root)
        .require_git(false)
        .parents(false)
        .git_global(false)
        .git_exclude(false)
        .hidden(true)
        .follow_links(false)
        .add_custom_ignore_filename(".tinymemignore")
        .filter_entry(allowed_entry)
        .build()
}

fn allowed_entry(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
    // Walk's hidden filter can be overridden by ignore-file whitelists; ours cannot.
    if name.starts_with('.') {
        return false;
    }
    if entry.file_type().is_some_and(|kind| kind.is_dir()) {
        return !matches!(
            name.as_str(),
            "node_modules" | "target" | "dist" | "build" | "vendor" | "venv" | "__pycache__"
        );
    }
    !name.starts_with(".env")
        && !["credentials", "credential", "secrets", "secret"]
            .iter()
            .any(|prefix| name == *prefix || name.starts_with(&format!("{prefix}.")))
}

/// The walker suppresses ignore-file I/O failures. Check independently and fail closed.
pub(crate) fn validate_ignores(directory: &Path) -> Result<()> {
    for name in [".gitignore", ".ignore", ".tinymemignore"] {
        let path = directory.join(name);
        let metadata = match path.symlink_metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot inspect ignore file {}", path.display()));
            }
        };
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "ignore file must be a regular non-symlink file: {}",
            path.display()
        );
        let bytes = fs::read(&path)
            .with_context(|| format!("cannot read ignore file {}", path.display()))?;
        std::str::from_utf8(&bytes)
            .with_context(|| format!("ignore file must be UTF-8: {}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn language(path: &Path) -> Option<&'static str> {
    Some(
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "py" | "pyi" => "python",
            "js" | "jsx" | "mjs" | "cjs" => "javascript",
            "ts" | "tsx" => "typescript",
            "rs" => "rust",
            "go" => "go",
            "java" => "java",
            "c" | "h" => "c",
            "cpp" | "hpp" | "cc" => "cpp",
            "rb" => "ruby",
            "sh" | "bash" => "shell",
            "md" | "mdx" | "rst" | "txt" => "text",
            "toml" | "json" | "yaml" | "yml" | "ini" | "cfg" => "config",
            "sql" => "sql",
            "html" | "css" | "scss" | "vue" | "svelte" => "web",
            _ => return None,
        },
    )
}

/// None means intentionally excluded (unsupported, oversized, binary or non-UTF-8).
pub(crate) fn read_source(root: &Path, path: &Path) -> Result<Option<SourceFile>> {
    let Some(language) = language(path) else {
        return Ok(None);
    };
    // Refuse symlinks (including ancestors) replacing previously indexed paths.
    // Canonical roots are required by the caller; no relative traversal is accepted.
    if !path.starts_with(root)
        || path
            .canonicalize()
            .with_context(|| format!("cannot resolve source {}", path.display()))?
            != path
    {
        return Ok(None);
    }
    let metadata = path
        .symlink_metadata()
        .with_context(|| format!("cannot inspect source {}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("cannot open source {}", path.display()))?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("cannot read {}", path.display()))?;
    if bytes.len() as u64 > MAX_FILE_BYTES || bytes.contains(&0) {
        return Ok(None);
    }
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let Ok(content) = String::from_utf8(bytes) else {
        return Ok(None);
    };
    let relative = path.strip_prefix(root)?;
    let path = relative
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .context("source path must be UTF-8")
        })
        .collect::<Result<Vec<_>>>()?
        .join("/");
    Ok(Some(SourceFile {
        path,
        content,
        hash,
        language,
    }))
}
