use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct DiskUsage {
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub usage_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryUsage {
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub usage_pct: f64,
}

pub fn disk_usage(mount_path: &str) -> Result<DiskUsage, String> {
    let c_path = std::ffi::CString::new(mount_path).map_err(|e| e.to_string())?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };

    let ret = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if ret != 0 {
        return Err(format!(
            "statvfs failed for {}: errno {}",
            mount_path,
            std::io::Error::last_os_error()
        ));
    }

    let block_size = stat.f_frsize as u64;
    let total_bytes = stat.f_blocks as u64 * block_size;
    let free_bytes = stat.f_bavail as u64 * block_size;

    let usage_pct = if total_bytes > 0 {
        (1.0 - (free_bytes as f64 / total_bytes as f64)) * 100.0
    } else {
        0.0
    };

    Ok(DiskUsage {
        free_bytes,
        total_bytes,
        usage_pct,
    })
}

pub fn memory_usage() -> Result<MemoryUsage, String> {
    let contents = std::fs::read_to_string("/proc/meminfo")
        .map_err(|e| format!("failed to read /proc/meminfo: {e}"))?;

    let mut total_kb: Option<u64> = None;
    let mut available_kb: Option<u64> = None;

    for line in contents.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = parse_meminfo_value(line);
        } else if line.starts_with("MemAvailable:") {
            available_kb = parse_meminfo_value(line);
        }
        if total_kb.is_some() && available_kb.is_some() {
            break;
        }
    }

    let total_kb = total_kb.ok_or("MemTotal not found in /proc/meminfo")?;
    let available_kb = available_kb.ok_or("MemAvailable not found in /proc/meminfo")?;

    let total_bytes = total_kb * 1024;
    let available_bytes = available_kb * 1024;

    let usage_pct = if total_bytes > 0 {
        (1.0 - (available_bytes as f64 / total_bytes as f64)) * 100.0
    } else {
        0.0
    };

    Ok(MemoryUsage {
        available_bytes,
        total_bytes,
        usage_pct,
    })
}

fn parse_meminfo_value(line: &str) -> Option<u64> {
    // Lines look like "MemTotal:       16384000 kB"
    line.split_whitespace().nth(1)?.parse().ok()
}

pub fn db_file_size(db_path: &Path) -> Result<u64, String> {
    std::fs::metadata(db_path)
        .map(|m| m.len())
        .map_err(|e| format!("failed to stat db file {}: {e}", db_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_usage_root() {
        let result = disk_usage("/");
        assert!(
            result.is_ok(),
            "disk_usage(\"/\") failed: {:?}",
            result.err()
        );
        let usage = result.unwrap();
        assert!(usage.total_bytes > 0);
        assert!(usage.free_bytes <= usage.total_bytes);
        assert!(usage.usage_pct >= 0.0 && usage.usage_pct <= 100.0);
    }

    #[test]
    fn test_disk_usage_invalid_path() {
        let result = disk_usage("/nonexistent_mount_path_xyz");
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_usage() {
        let result = memory_usage();
        assert!(result.is_ok(), "memory_usage() failed: {:?}", result.err());
        let mem = result.unwrap();
        assert!(mem.total_bytes > 0);
        assert!(mem.available_bytes <= mem.total_bytes);
        assert!(mem.usage_pct >= 0.0 && mem.usage_pct <= 100.0);
    }

    #[test]
    fn test_parse_meminfo_value() {
        assert_eq!(
            parse_meminfo_value("MemTotal:       16384000 kB"),
            Some(16384000)
        );
        assert_eq!(
            parse_meminfo_value("MemAvailable:    8192000 kB"),
            Some(8192000)
        );
        assert_eq!(parse_meminfo_value("garbage"), None);
    }

    #[test]
    fn test_db_file_size_existing() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"hello world").unwrap();
        let size = db_file_size(tmp.path()).unwrap();
        assert_eq!(size, 11);
    }

    #[test]
    fn test_db_file_size_missing() {
        let result = db_file_size(Path::new("/nonexistent_db_file_xyz.db"));
        assert!(result.is_err());
    }
}
