use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;

const LIMIT: u64 = 1_048_576;

pub(super) fn read_config(path: &Path) -> Result<String, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| "configuration must be a readable regular file".to_owned())?;
    if !file
        .metadata()
        .map_err(|_| "configuration metadata unavailable".to_owned())?
        .is_file()
    {
        return Err("configuration must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "configuration read failed".to_owned())?;
    if bytes.len() as u64 > LIMIT {
        return Err("configuration exceeds 1 MiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "configuration must be UTF-8".into())
}

#[cfg(test)]
mod tests {
    use super::read_config;
    #[test]
    fn refuses_large_invalid_and_special_files() {
        let root = std::env::temp_dir().join(format!("imap-config-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("input");
        std::fs::write(&file, b"valid").unwrap();
        assert_eq!(read_config(&file).unwrap(), "valid");
        std::fs::write(&file, vec![0; 1_048_577]).unwrap();
        assert!(read_config(&file).is_err());
        std::fs::write(&file, [255]).unwrap();
        assert!(read_config(&file).is_err());
        assert!(read_config(&root).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&file, root.join("linked")).unwrap();
            assert!(read_config(&root.join("linked")).is_err());
            let status = std::process::Command::new("mkfifo")
                .arg(root.join("pipe"))
                .status()
                .unwrap();
            assert!(status.success());
            let start = std::time::Instant::now();
            assert!(read_config(&root.join("pipe")).is_err());
            assert!(start.elapsed() < std::time::Duration::from_secs(1));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
