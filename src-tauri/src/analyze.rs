//! Disk usage scanner: one walk, then instant drill-down from cached folder sizes.

use crate::fsutil;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct AnalyzeState(pub Mutex<Option<Scan>>);

pub struct Scan {
    pub root: PathBuf,
    pub dir_sizes: HashMap<PathBuf, u64>,
    pub unreadable: Vec<PathBuf>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub files: u64,
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub kind: &'static str,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub path: String,
    pub total: u64,
    pub unreadable: u32,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Meta {
    dev: u64,
    ino: u64,
    nlink: u64,
    bytes: u64,
    is_dir: bool,
}

/// Walks `root` once (no symlinks, same device only) and sums sizes per folder.
pub fn scan(root: &Path, progress: impl Fn(Progress)) -> Result<Scan, String> {
    let root_meta = std::fs::symlink_metadata(root).map_err(|e| format!("{}: {e}", root.display()))?;
    if !root_meta.is_dir() {
        return Err("Not a folder".into());
    }
    let root_dev = root_meta.dev();

    let walk = jwalk::WalkDirGeneric::<((), Option<Meta>)>::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .sort(false)
        .process_read_dir(move |_, _, _, children| {
            for c in children.iter_mut().flatten() {
                let Ok(m) = std::fs::symlink_metadata(c.path()) else { continue };
                if m.dev() != root_dev {
                    // Another volume mounted inside the tree: skip it entirely.
                    c.read_children_path = None;
                    continue;
                }
                c.client_state = Some(Meta {
                    dev: m.dev(),
                    ino: m.ino(),
                    nlink: m.nlink(),
                    bytes: m.blocks() * 512,
                    is_dir: m.is_dir(),
                });
            }
        });

    let mut direct: HashMap<PathBuf, u64> = HashMap::new();
    direct.insert(root.to_path_buf(), root_meta.blocks() * 512);
    let mut unreadable = Vec::new();
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let (mut files, mut bytes) = (0u64, root_meta.blocks() * 512);
    let mut last = Instant::now();

    for entry in walk {
        let Ok(e) = entry else { continue };
        if e.read_children_error.is_some() {
            unreadable.push(e.path());
        }
        if e.depth == 0 {
            continue;
        }
        let Some(m) = e.client_state else { continue };
        if m.is_dir {
            // A folder's own blocks count toward itself.
            *direct.entry(e.path()).or_insert(0) += m.bytes;
        } else if m.nlink > 1 && !seen.insert((m.dev, m.ino)) {
            continue;
        } else {
            files += 1;
            *direct.entry(e.parent_path.to_path_buf()).or_insert(0) += m.bytes;
        }
        bytes += m.bytes;
        if last.elapsed() >= Duration::from_millis(250) {
            last = Instant::now();
            progress(Progress { files, bytes });
        }
    }
    progress(Progress { files, bytes });

    // Roll sizes up: deepest folders first.
    let mut dirs: Vec<PathBuf> = direct.keys().cloned().collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    let mut totals = direct;
    for d in dirs {
        if d == root {
            continue;
        }
        let v = totals[&d];
        if let Some(p) = d.parent()
            && let Some(t) = totals.get_mut(p)
        {
            *t += v;
        }
    }
    Ok(Scan { root: root.to_path_buf(), dir_sizes: totals, unreadable })
}

/// Kind of an entry from its extension.
pub fn kind(name: &str, is_dir: bool) -> &'static str {
    let ext = Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if is_dir {
        return match ext.as_str() {
            "app" => "app",
            "sparsebundle" => "diskImage",
            _ => "folder",
        };
    }
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "heic" | "gif" | "webp" | "tiff" | "raw" => "image",
        "mp4" | "mov" | "mkv" | "avi" | "m4v" => "video",
        "mp3" | "m4a" | "wav" | "flac" | "aac" => "audio",
        "zip" | "tar" | "gz" | "tgz" | "xz" | "7z" | "rar" => "archive",
        "dmg" | "iso" | "img" | "sparseimage" | "sparsebundle" => "diskImage",
        _ => "other",
    }
}

/// Direct children of `path`, biggest first.
pub fn list(scan: &Scan, path: &Path) -> Result<DirListing, String> {
    if !path.starts_with(&scan.root) {
        return Err("Outside the scanned folder".into());
    }
    let rd = std::fs::read_dir(path).map_err(|e| e.to_string())?;
    let mut entries: Vec<Entry> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let m = std::fs::symlink_metadata(&p).ok()?;
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = m.is_dir();
            let size = if is_dir { scan.dir_sizes.get(&p).copied().unwrap_or(0) } else { m.blocks() * 512 };
            Some(Entry { kind: kind(&name, is_dir), name, path: p.to_string_lossy().to_string(), size, is_dir })
        })
        .collect();
    entries.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    let total = scan
        .dir_sizes
        .get(path)
        .copied()
        .unwrap_or_else(|| entries.iter().map(|e| e.size).sum());
    let unreadable = scan.unreadable.iter().filter(|u| u.starts_with(path)).count() as u32;
    Ok(DirListing { path: path.to_string_lossy().to_string(), total, unreadable, entries })
}

/// Moves `path` to the Trash and updates cached sizes.
pub fn trash(scan: &mut Scan, path: &Path, home: &Path) -> fsutil::ActionResult {
    let cached = scan.dir_sizes.get(path).copied();
    let r = fsutil::trash_paths(&[path.to_path_buf()], home);
    if !r.done.is_empty() {
        let size = cached.unwrap_or(r.bytes);
        for a in path.ancestors().skip(1) {
            if let Some(v) = scan.dir_sizes.get_mut(a) {
                *v = v.saturating_sub(size);
            }
            if a == scan.root {
                break;
            }
        }
        scan.dir_sizes.retain(|k, _| !k.starts_with(path));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let r = t.path().canonicalize().unwrap().join("root");
        fs::create_dir_all(r.join("a/b")).unwrap();
        fs::create_dir_all(r.join("c")).unwrap();
        fs::write(r.join("a/b/big.bin"), vec![1u8; 300_000]).unwrap();
        fs::write(r.join("a/x.txt"), vec![1u8; 50_000]).unwrap();
        fs::write(r.join("c/y.png"), vec![1u8; 100_000]).unwrap();
        (t, r)
    }

    #[test]
    fn sizes_add_up() {
        let (_t, r) = tree();
        let s = scan(&r, |_| {}).unwrap();
        let root_total = s.dir_sizes[&r];
        assert_eq!(root_total, fsutil::allocated_size(&r));
        assert_eq!(s.dir_sizes[&r.join("a")], fsutil::allocated_size(&r.join("a")));
        assert_eq!(s.dir_sizes[&r.join("a/b")], fsutil::allocated_size(&r.join("a/b")));
        let l = list(&s, &r).unwrap();
        assert_eq!(l.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["a", "c"]);
        assert_eq!(l.total, root_total);
        let own = fs::symlink_metadata(&r).unwrap().blocks() * 512;
        assert_eq!(l.entries.iter().map(|e| e.size).sum::<u64>() + own, root_total);
        assert_eq!(list(&s, &r.join("c")).unwrap().entries[0].kind, "image");
    }

    #[test]
    fn hard_link_counted_once() {
        let (_t, r) = tree();
        let before = scan(&r, |_| {}).unwrap().dir_sizes[&r];
        fs::hard_link(r.join("a/b/big.bin"), r.join("c/link.bin")).unwrap();
        let after = scan(&r, |_| {}).unwrap().dir_sizes[&r];
        assert_eq!(before, after);
    }

    #[test]
    fn symlink_not_followed() {
        let (t, r) = tree();
        let outside = t.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("huge"), vec![1u8; 1_000_000]).unwrap();
        std::os::unix::fs::symlink(&outside, r.join("c/link")).unwrap();
        let s = scan(&r, |_| {}).unwrap();
        assert!(s.dir_sizes[&r] < 1_000_000);
        let l = list(&s, &r.join("c")).unwrap();
        let link = l.entries.iter().find(|e| e.name == "link").unwrap();
        assert!(!link.is_dir);
    }

    #[test]
    fn unreadable_counted() {
        let (_t, r) = tree();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(r.join("c"), fs::Permissions::from_mode(0o000)).unwrap();
        let s = scan(&r, |_| {}).unwrap();
        fs::set_permissions(r.join("c"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(list(&s, &r).unwrap().unreadable, 1);
    }

    #[test]
    fn kinds() {
        assert_eq!(kind("a.PNG", false), "image");
        assert_eq!(kind("a.mov", false), "video");
        assert_eq!(kind("a.flac", false), "audio");
        assert_eq!(kind("a.tgz", false), "archive");
        assert_eq!(kind("a.dmg", false), "diskImage");
        assert_eq!(kind("Foo.app", true), "app");
        assert_eq!(kind("src", true), "folder");
        assert_eq!(kind("notes.txt", false), "other");
        assert_eq!(kind("Makefile", false), "other");
    }
}

#[cfg(test)]
mod smoke {
    #[test]
    #[ignore]
    fn real_home() {
        let h = dirs::home_dir().unwrap();
        let t = std::time::Instant::now();
        let s = super::scan(&h, |_| {}).unwrap();
        let l = super::list(&s, &h).unwrap();
        println!("{} dirs, total {}, unreadable {}, {:?}", s.dir_sizes.len(), l.total, l.unreadable, t.elapsed());
        for e in l.entries.iter().take(8) { println!("{} {} {}", e.name, e.size, e.kind); }
    }
}
