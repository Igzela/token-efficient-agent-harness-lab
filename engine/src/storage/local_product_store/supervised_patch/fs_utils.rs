use serde_json::{json, Value};
use std::path::Path;

const DEFAULT_IGNORE_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".next",
    "dist",
    "build",
    "__pycache__",
    ".pytest_cache",
    ".git",
];

const MAX_FILE_BYTES: u64 = 1_048_576; // 1 MB
pub(crate) const MAX_WORKSPACE_COPY_FILES: usize = 20_000;
pub(crate) const MAX_WORKSPACE_COPY_BYTES: u64 = 200 * 1_048_576; // 200 MB
pub(crate) const MAX_WORKSPACE_COPY_FILE_BYTES: u64 = 10 * 1_048_576; // 10 MB
pub(crate) const MAX_REVIEW_DIFF_BYTES: usize = 256 * 1024;

fn is_ignored_dir(name: &str) -> bool {
    DEFAULT_IGNORE_DIRS.contains(&name)
}

fn is_binary_content(data: &[u8]) -> bool {
    let check_len = data.len().min(8192);
    data[..check_len].contains(&0)
}

pub(crate) fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn compute_manifest(dir: &Path) -> Result<Value, String> {
    let (files, hashes) = collect_workspace_files(dir)?;
    let entries: Vec<Value> = files
        .iter()
        .zip(hashes.iter())
        .map(|(f, h)| json!({"path": f, "hash": h}))
        .collect();
    Ok(json!({
        "files": entries,
        "computed_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    }))
}

#[allow(clippy::type_complexity)]
pub(crate) fn diff_against_manifest(
    workspace_dir: &Path,
    manifest: &Value,
) -> Result<(Vec<String>, Vec<String>, Vec<String>), String> {
    let (current_files, current_hashes) = collect_workspace_files(workspace_dir)?;
    let manifest_entries = manifest
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut source_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for entry in &manifest_entries {
        if let (Some(path), Some(hash)) = (
            entry.get("path").and_then(Value::as_str),
            entry.get("hash").and_then(Value::as_str),
        ) {
            source_map.insert(path.to_string(), hash.to_string());
        }
    }

    let mut current_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (path, hash) in current_files.iter().zip(current_hashes.iter()) {
        current_map.insert(path.clone(), hash.clone());
    }

    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();

    for (path, hash) in &current_map {
        match source_map.get(path) {
            Some(source_hash) => {
                if source_hash != hash {
                    modified.push(path.clone());
                }
            }
            None => added.push(path.clone()),
        }
    }
    for path in source_map.keys() {
        if !current_map.contains_key(path) {
            deleted.push(path.clone());
        }
    }

    added.sort();
    modified.sort();
    deleted.sort();
    Ok((added, modified, deleted))
}

#[derive(Default)]
struct WorkspaceCopyStats {
    files: usize,
    bytes: u64,
}

pub(crate) fn copy_dir_contents(src: &Path, dst: &Path) -> Result<(), String> {
    let mut stats = WorkspaceCopyStats::default();
    copy_dir_contents_inner(src, dst, &mut stats)
}

fn copy_dir_contents_inner(
    src: &Path,
    dst: &Path,
    stats: &mut WorkspaceCopyStats,
) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let entries = std::fs::read_dir(src).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') || file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() && is_ignored_dir(&name_str) {
            continue;
        }
        let target = dst.join(&name);
        if file_type.is_dir() {
            copy_dir_contents_inner(&path, &target, stats)?;
        } else if file_type.is_file() {
            let meta = entry.metadata().map_err(|e| e.to_string())?;
            if meta.len() > MAX_WORKSPACE_COPY_FILE_BYTES {
                return Err(format!(
                    "workspace copy file exceeds limit: {} ({} bytes)",
                    path.display(),
                    meta.len()
                ));
            }
            stats.files += 1;
            stats.bytes += meta.len();
            if stats.files > MAX_WORKSPACE_COPY_FILES {
                return Err(format!(
                    "workspace copy file limit exceeded: {} > {}",
                    stats.files, MAX_WORKSPACE_COPY_FILES
                ));
            }
            if stats.bytes > MAX_WORKSPACE_COPY_BYTES {
                return Err(format!(
                    "workspace copy byte limit exceeded: {} > {}",
                    stats.bytes, MAX_WORKSPACE_COPY_BYTES
                ));
            }
            std::fs::copy(&path, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub(crate) fn collect_workspace_files(dir: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    let mut pairs = Vec::new();
    collect_files_recursive(dir, dir, &mut pairs)?;
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let files = pairs.iter().map(|(p, _)| p.clone()).collect();
    let hashes = pairs.iter().map(|(_, h)| h.clone()).collect();
    Ok((files, hashes))
}

fn collect_files_recursive(
    base: &Path,
    dir: &Path,
    pairs: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') || is_ignored_dir(&name) {
                continue;
            }
            collect_files_recursive(base, &path, pairs)?;
        } else if file_type.is_file() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
            if meta.len() > MAX_FILE_BYTES {
                continue;
            }
            let content = std::fs::read(&path).map_err(|e| e.to_string())?;
            if is_binary_content(&content) {
                continue;
            }
            let relative = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let hash = hex_encode(&sha256_bytes(&content));
            pairs.push((relative, hash));
        }
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn generate_review_diff(
    workspace_dir: &Path,
    added: &[String],
    modified: &[String],
    deleted: &[String],
) -> String {
    let mut diff = String::new();

    for path in added {
        let full = workspace_dir.join(path);
        let content = match std::fs::read_to_string(&full) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let line_count = content.lines().count();
        diff.push_str(&format!(
            "--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{line_count} @@\n"
        ));
        for line in content.lines() {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
    }

    for path in modified {
        let full = workspace_dir.join(path);
        let content = match std::fs::read_to_string(&full) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let line_count = content.lines().count();
        diff.push_str(&format!(
            "--- a/{path}\n+++ b/{path}\n@@ -1,{line_count} +1,{line_count} @@\n"
        ));
        for line in content.lines() {
            diff.push(' ');
            diff.push_str(line);
            diff.push('\n');
        }
    }

    for path in deleted {
        diff.push_str(&format!("--- a/{path}\n+++ /dev/null\n@@ -1,0 +0,0 @@\n"));
        diff.push_str(&format!("(deleted: {path})\n"));
    }

    truncate_text(diff, MAX_REVIEW_DIFF_BYTES)
}

fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256Writer::new();
    hasher.write(data);
    hasher.finalize()
}

struct Sha256Writer {
    state: [u32; 8],
    buffer: Vec<u8>,
    total_len: u64,
}

impl Sha256Writer {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: Vec::new(),
            total_len: 0,
        }
    }

    fn write(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        self.total_len += data.len() as u64;
        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.buffer.drain(..64);
            self.process_block(&block);
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len * 8;
        self.buffer.push(0x80);
        while (self.buffer.len() % 64) != 56 {
            self.buffer.push(0);
        }
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());
        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.buffer.drain(..64);
            self.process_block(&block);
        }
        let mut result = [0u8; 32];
        for (i, &word) in self.state.iter().enumerate() {
            result[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
        }
        result
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..(i + 1) * 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

pub(crate) fn scan_for_secrets(dir: &Path) -> Result<Vec<String>, String> {
    let mut findings = Vec::new();
    scan_recursive(dir, dir, &mut findings)?;
    Ok(findings)
}

fn scan_recursive(base: &Path, dir: &Path, findings: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !name.starts_with('.') && !is_ignored_dir(&name) {
                scan_recursive(base, &path, findings)?;
            }
        } else if file_type.is_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                for line in content.lines() {
                    let lower = line.to_lowercase();
                    if let Some(pattern) = secret_pattern(&lower) {
                        let relative = path.strip_prefix(base).unwrap_or(&path).to_string_lossy();
                        findings.push(format!(
                            "{relative}: sensitive pattern detected ({pattern})"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn secret_pattern(lowercase_line: &str) -> Option<&'static str> {
    if lowercase_line.contains("api_key") {
        Some("api_key")
    } else if lowercase_line.contains("api-key") {
        Some("api-key")
    } else if lowercase_line.contains("secret_key") {
        Some("secret_key")
    } else if lowercase_line.contains("password") {
        Some("password")
    } else if lowercase_line.contains("bearer ") {
        Some("bearer")
    } else if lowercase_line.contains("private_key") {
        Some("private_key")
    } else {
        None
    }
}

pub(crate) fn truncate_text(mut text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut split = max_bytes;
    while split > 0 && !text.is_char_boundary(split) {
        split -= 1;
    }
    text.truncate(split);
    text.push_str("\n[truncated]\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collect_files_recursive_skips_dotfiles() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("visible.txt"), "content").unwrap();
        fs::write(root.join(".hidden.txt"), "secret").unwrap();
        fs::write(root.join(".source_manifest.json"), "{}").unwrap();

        let mut pairs = Vec::new();
        collect_files_recursive(root, root, &mut pairs).unwrap();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let files: Vec<String> = pairs.iter().map(|(p, _)| p.clone()).collect();

        assert_eq!(files, vec!["visible.txt"]);
        assert!(!files.iter().any(|f| f.contains(".source_manifest")));
        assert!(!files.iter().any(|f| f.starts_with('.')));
    }

    #[test]
    fn collect_files_recursive_skips_dot_directories() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("top.txt"), "content").unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), "stuff").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/nested.txt"), "data").unwrap();

        let mut pairs = Vec::new();
        collect_files_recursive(root, root, &mut pairs).unwrap();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let files: Vec<String> = pairs.iter().map(|(p, _)| p.clone()).collect();

        assert_eq!(files, vec!["sub/nested.txt", "top.txt"]);
    }

    #[test]
    fn collect_files_recursive_no_dotfiles_in_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("real_patch.rs"), "fn main() {}").unwrap();
        fs::write(root.join(".source_manifest.json"), r#"{"files":[]}"#).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git/index"), "binary").unwrap();

        let mut pairs = Vec::new();
        collect_files_recursive(root, root, &mut pairs).unwrap();
        let files: Vec<String> = pairs.iter().map(|(p, _)| p.clone()).collect();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "real_patch.rs");
        assert!(!files.iter().any(|f| f.starts_with('.')));
    }

    #[test]
    fn collect_workspace_files_path_hash_alignment() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Write multiple files with known content so we can verify hash alignment
        fs::write(root.join("aaa.txt"), "alpha").unwrap();
        fs::write(root.join("bbb.txt"), "beta").unwrap();
        fs::write(root.join("ccc.txt"), "gamma").unwrap();

        let (files, hashes) = collect_workspace_files(root).unwrap();

        // Files must be sorted
        assert_eq!(files, vec!["aaa.txt", "bbb.txt", "ccc.txt"]);
        // Each hash must correspond to its file's content
        let expected_aaa = hex_encode(&sha256_bytes(b"alpha"));
        let expected_bbb = hex_encode(&sha256_bytes(b"beta"));
        let expected_ccc = hex_encode(&sha256_bytes(b"gamma"));
        assert_eq!(hashes[0], expected_aaa, "hash mismatch for aaa.txt");
        assert_eq!(hashes[1], expected_bbb, "hash mismatch for bbb.txt");
        assert_eq!(hashes[2], expected_ccc, "hash mismatch for ccc.txt");
    }
}
