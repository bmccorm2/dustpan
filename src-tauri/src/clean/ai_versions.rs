//! Old versions of AI CLIs that keep every release side by side.

use super::{Cat, Pending};
use crate::safety;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// (tool id, label, versions root, active symlink), relative to home.
const TOOLS: &[(&str, &str, &str, &str)] = &[
    ("claude", "Claude", ".local/share/claude/versions", ".local/bin/claude"),
    ("cursor-agent", "Cursor Agent", ".local/share/cursor-agent/versions", ".local/bin/cursor-agent"),
    ("copilot", "Copilot", ".copilot/pkg/universal", ".local/bin/copilot"),
];

pub(super) fn scan(home: &Path) -> Vec<Pending> {
    TOOLS
        .iter()
        .filter_map(|&(tool, name, root, link)| {
            let paths = old_versions(home, &home.join(root), &home.join(link));
            (!paths.is_empty()).then(|| Pending {
                id: format!("aiver:{tool}"),
                category: Cat::Ai,
                label: format!("{name} old versions"),
                paths,
                recommended: true,
                blocked_reason: None,
            })
        })
        .collect()
}

/// Children of `root` except the active one and the newest other one.
fn old_versions(home: &Path, root: &Path, link: &Path) -> Vec<PathBuf> {
    let Ok(target) = std::fs::canonicalize(link) else { return vec![] };
    let Ok(canon_root) = std::fs::canonicalize(root) else { return vec![] };
    let Some(active) = target
        .strip_prefix(&canon_root)
        .ok()
        .and_then(|rest| rest.components().next())
        .map(|c| c.as_os_str().to_owned())
    else {
        return vec![];
    };
    let Ok(rd) = std::fs::read_dir(root) else { return vec![] };
    let mut others: Vec<(SystemTime, PathBuf)> = rd
        .flatten()
        .filter(|e| e.file_name() != active)
        .map(|e| {
            let m = e.path().symlink_metadata().and_then(|m| m.modified());
            (m.unwrap_or(SystemTime::UNIX_EPOCH), e.path())
        })
        .collect();
    others.sort_by_key(|o| std::cmp::Reverse(o.0));
    others
        .into_iter()
        .skip(1)
        .map(|(_, p)| p)
        .filter(|p| safety::check(p, home).is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::leftovers::backdate;
    use std::fs;

    #[test]
    fn keeps_active_and_newest() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().canonicalize().unwrap();
        let root = h.join(".local/share/claude/versions");
        for (v, age) in [("1.0.0", 1), ("1.1.0", 10), ("1.2.0", 20), ("1.3.0", 30)] {
            fs::create_dir_all(root.join(v)).unwrap();
            fs::write(root.join(v).join("claude"), b"bin").unwrap();
            backdate(&root.join(v), age);
        }
        fs::create_dir_all(h.join(".local/bin")).unwrap();
        // The active version is an older one.
        std::os::unix::fs::symlink(root.join("1.2.0/claude"), h.join(".local/bin/claude")).unwrap();
        let items = scan(&h);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "aiver:claude");
        assert_eq!(items[0].label, "Claude old versions");
        let mut p = items[0].paths.clone();
        p.sort();
        // Keeps 1.2.0 (active) and 1.0.0 (newest other).
        assert_eq!(p, vec![root.join("1.1.0"), root.join("1.3.0")]);
    }

    #[test]
    fn broken_link_skips_tool() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path().canonicalize().unwrap();
        let root = h.join(".local/share/claude/versions");
        for v in ["a", "b", "c"] {
            fs::create_dir_all(root.join(v)).unwrap();
        }
        fs::create_dir_all(h.join(".local/bin")).unwrap();
        std::os::unix::fs::symlink(root.join("gone/claude"), h.join(".local/bin/claude")).unwrap();
        assert!(scan(&h).is_empty());
    }
}
