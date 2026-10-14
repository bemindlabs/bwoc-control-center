//! Load an agent's saved memory so a chat turn starts fully incarnated.
//!
//! `bwoc run` (and the harness) load the agent's PERSONA (AGENTS.md) but not
//! its Tier-1 memory files, and many agents have no `deepMemoryCmd` (Tier-2).
//! So before a chat turn we read the agent's Tier-1 `memories/` files and inject
//! them as context. (A Tier-2 deep-memory wake-up is not run here.)

use std::path::Path;

const MAX_CHARS: usize = 8000;

/// Read an agent's memory: the `MEMORY.md` index if present, else the other
/// `memories/*.md` files concatenated (README skipped). Returns None when the
/// agent has no memory. Capped to keep the chat prompt bounded.
pub fn load(workspace: &Path, id: &str) -> Option<String> {
    let dir = workspace.join("agents").join(id).join("memories");
    if !dir.is_dir() {
        return None;
    }

    let index = dir.join("MEMORY.md");
    if index.is_file() {
        if let Ok(c) = std::fs::read_to_string(&index) {
            let c = c.trim();
            if !c.is_empty() {
                return Some(cap(c));
            }
        }
    }

    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter(|p| {
            !p.file_name()
                .map(|n| n.eq_ignore_ascii_case("README.md"))
                .unwrap_or(false)
        })
        .collect();
    names.sort();

    let mut parts = Vec::new();
    for p in names {
        if let Ok(c) = std::fs::read_to_string(&p) {
            let c = c.trim();
            if !c.is_empty() {
                let name = p.file_name().unwrap().to_string_lossy();
                parts.push(format!("### {name}\n{c}"));
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(cap(&parts.join("\n\n")))
    }
}

fn cap(s: &str) -> String {
    if s.len() <= MAX_CHARS {
        s.to_string()
    } else {
        let mut end = MAX_CHARS;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}\n…(memory truncated)", &s[..end])
    }
}
