use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::types::{StoredWorkspace, WorkspaceRegistry};

use super::helpers::normalize_workspace_path;

pub(crate) fn load_workspace_registry(path: &Path) -> WorkspaceRegistry {
    let Ok(raw) = fs::read_to_string(path) else {
        return WorkspaceRegistry::default();
    };

    serde_json::from_str::<WorkspaceRegistry>(&raw).unwrap_or_default()
}

pub(crate) fn save_workspace_registry(path: &Path, registry: &WorkspaceRegistry) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let json =
        serde_json::to_string_pretty(registry).context("failed to serialize workspace registry")?;
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub(crate) fn default_workspace_registry_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    let config_root = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(&home).join(".config"));
    Ok(config_root.join("codex-glances").join("workspaces.json"))
}

pub(crate) fn add_workspace_registry_entry(path: &Path) -> Result<PathBuf> {
    let normalized = normalize_workspace_path(path)?;
    let registry_path = default_workspace_registry_path()?;
    let mut registry = load_workspace_registry(&registry_path);
    let normalized_text = normalized.to_string_lossy().into_owned();

    if registry
        .workspaces
        .iter()
        .any(|entry| entry.path == normalized_text)
    {
        return Ok(normalized);
    }

    registry.workspaces.push(StoredWorkspace {
        path: normalized_text,
        display_name: None,
        tags: Vec::new(),
        pinned: false,
    });
    registry
        .workspaces
        .sort_by(|left, right| left.path.cmp(&right.path));
    save_workspace_registry(&registry_path, &registry)?;
    Ok(normalized)
}

pub(crate) fn toggle_workspace_pinned(path: &str) -> Result<bool> {
    let registry_path = default_workspace_registry_path()?;
    let mut registry = load_workspace_registry(&registry_path);

    if let Some(entry) = registry
        .workspaces
        .iter_mut()
        .find(|entry| entry.path == path)
    {
        entry.pinned = !entry.pinned;
        let pinned = entry.pinned;
        save_workspace_registry(&registry_path, &registry)?;
        return Ok(pinned);
    }

    registry.workspaces.push(StoredWorkspace {
        path: path.to_string(),
        display_name: None,
        tags: Vec::new(),
        pinned: true,
    });
    save_workspace_registry(&registry_path, &registry)?;
    Ok(true)
}
