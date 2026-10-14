//! Workspace discovery + resolution.
//!
//! bwocd can serve either a single workspace (`--workspace`) or every workspace
//! under a root (`--workspace-root`, scanned **dynamically** each request so a
//! newly-incarnated workspace shows up without a restart). All cross-workspace
//! targeting resolves a workspace by **name against the discovered set** — never
//! an arbitrary caller-supplied path — so a remote controller can't retarget an
//! out-of-tree directory (traversal-safe by construction).

use serde::Deserialize;
use std::path::{Path, PathBuf};

pub struct Workspaces {
    /// Discovery root, scanned for child workspaces. `None` → single-workspace
    /// mode (only `primary` is served).
    pub root: Option<PathBuf>,
    /// Default workspace: the enroll/pending store lives here, and ops without an
    /// agent context (or an unresolvable one) fall back to it.
    pub primary: PathBuf,
}

/// A discovered workspace: its display name (directory basename) and absolute path.
#[derive(Clone)]
pub struct WorkspaceRef {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Deserialize)]
struct AgentsToml {
    #[serde(default, rename = "agent")]
    agents: Vec<AgentRef>,
}

#[derive(Deserialize)]
struct AgentRef {
    id: String,
}

fn is_workspace(dir: &Path) -> bool {
    dir.join(".bwoc/workspace.toml").is_file()
}

fn ws_name(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workspace")
        .to_string()
}

impl Workspaces {
    /// Single-workspace mode.
    pub fn single(primary: PathBuf) -> Self {
        Self {
            root: None,
            primary,
        }
    }

    /// Multi-workspace mode: `primary` (for enroll/pending + fallback) plus every
    /// workspace discovered under `root`.
    pub fn rooted(root: PathBuf, primary: PathBuf) -> Self {
        Self {
            root: Some(root),
            primary,
        }
    }

    pub fn is_multi(&self) -> bool {
        self.root.is_some()
    }

    /// All workspaces to serve, deduped by path and sorted by name. Always
    /// includes `primary` (even without a `.bwoc/workspace.toml`, so a
    /// misconfigured primary surfaces as a `bwoc` error instead of a silently
    /// empty fleet); in rooted mode also every `<root>/<child>` that has a
    /// `.bwoc/workspace.toml`. Scanned fresh on every call (dynamic discovery).
    pub fn discover(&self) -> Vec<WorkspaceRef> {
        let mut out: Vec<WorkspaceRef> = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

        let mut push = |path: PathBuf| {
            let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
            if seen.insert(canon) {
                out.push(WorkspaceRef {
                    name: ws_name(&path),
                    path,
                });
            }
        };

        push(self.primary.clone());
        if let Some(root) = &self.root {
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() && is_workspace(&p) {
                        push(p);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Resolve a workspace by name against the discovered set. Returns `None` for
    /// an unknown name — the caller then 400/404s rather than touching an
    /// arbitrary path (this is what keeps name-based targeting traversal-safe).
    pub fn by_name(&self, name: &str) -> Option<PathBuf> {
        self.discover()
            .into_iter()
            .find(|w| w.name == name)
            .map(|w| w.path)
    }

    /// Which discovered workspace registers `agent_id`, by reading each
    /// `.bwoc/agents.toml`. Falls back to `primary` in single-workspace mode
    /// (the id is assumed to live there — the original behaviour).
    pub fn for_agent(&self, agent_id: &str) -> Option<PathBuf> {
        if !self.is_multi() {
            return Some(self.primary.clone());
        }
        for ws in self.discover() {
            let toml_path = ws.path.join(".bwoc/agents.toml");
            let Ok(text) = std::fs::read_to_string(&toml_path) else {
                continue;
            };
            let Ok(parsed) = toml::from_str::<AgentsToml>(&text) else {
                continue;
            };
            if parsed.agents.iter().any(|a| a.id == agent_id) {
                return Some(ws.path);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkworkspace(root: &Path, name: &str, agents: &[&str]) -> PathBuf {
        let ws = root.join(name);
        std::fs::create_dir_all(ws.join(".bwoc")).unwrap();
        std::fs::write(ws.join(".bwoc/workspace.toml"), "[workspace]\n").unwrap();
        let mut toml = String::new();
        for id in agents {
            toml.push_str(&format!(
                "[[agent]]\nid = \"{id}\"\npath = \"agents/{id}\"\n\n"
            ));
        }
        std::fs::write(ws.join(".bwoc/agents.toml"), toml).unwrap();
        ws
    }

    fn tmproot(label: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "bwocd-ws-test-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn rooted_discovers_all_children_and_resolves_by_name_and_agent() {
        let root = tmproot("discover");
        let alpha = mkworkspace(&root, "alpha", &["agent-a1", "agent-a2"]);
        let _beta = mkworkspace(&root, "beta", &["agent-b1"]);
        std::fs::create_dir_all(root.join("not-a-workspace")).unwrap();

        let ws = Workspaces::rooted(root.clone(), alpha.clone());
        let names: Vec<String> = ws.discover().into_iter().map(|w| w.name).collect();
        assert_eq!(
            names,
            vec!["alpha", "beta"],
            "non-workspace dir excluded, sorted"
        );

        assert_eq!(ws.by_name("beta"), Some(root.join("beta")));
        assert_eq!(
            ws.by_name("nope"),
            None,
            "unknown name → None (no arbitrary path)"
        );

        assert_eq!(ws.for_agent("agent-b1"), Some(root.join("beta")));
        assert_eq!(ws.for_agent("agent-a2"), Some(alpha));
        assert_eq!(ws.for_agent("agent-ghost"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn primary_is_discovered_even_without_workspace_toml() {
        let root = tmproot("bare-primary");
        let bare = root.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let ws = Workspaces::single(bare.clone());
        let found: Vec<PathBuf> = ws.discover().into_iter().map(|w| w.path).collect();
        assert_eq!(found, vec![bare], "primary must never silently vanish");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn single_mode_always_resolves_to_primary() {
        let root = tmproot("single");
        let only = mkworkspace(&root, "only", &["agent-x"]);
        let ws = Workspaces::single(only.clone());
        assert!(!ws.is_multi());
        // single mode assumes the id lives in the one workspace (legacy behaviour)
        assert_eq!(ws.for_agent("agent-anything"), Some(only.clone()));
        assert_eq!(ws.discover().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
