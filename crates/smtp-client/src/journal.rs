use crate::Outcome;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
use zixcel_mail_core::{Error, Result};
fn validate(root: &Path, digest: &str) -> Result<()> {
    if !root.is_absolute()
        || fs::canonicalize(root).map_err(|_| Error("journal_unavailable"))? != root
    {
        return Err(Error("unsafe_journal_path"));
    }
    let m = fs::symlink_metadata(root).map_err(|_| Error("journal_unavailable"))?;
    let uid = fs::metadata("/proc/self")
        .map_err(|_| Error("journal_owner_unavailable"))?
        .uid();
    if !m.is_dir()
        || m.mode() & 0o077 != 0
        || m.uid() != uid
        || digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error("unsafe_journal"));
    }
    Ok(())
}
pub(super) fn admit(root: &Path, digest: &str) -> Result<()> {
    validate(root, digest)?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join(format!("{digest}.attempt")))
        .map_err(|_| Error("submission_attempt_exists_or_unavailable"))?;
    f.write_all(b"attempt admitted; unresolved until observation\n")
        .and_then(|_| f.sync_all())
        .map_err(|_| Error("journal_write_failed"))?;
    File::open(root)
        .and_then(|d| d.sync_all())
        .map_err(|_| Error("journal_sync_failed"))
}
pub(super) fn record(root: &Path, digest: &str, outcome: &Outcome) -> Result<()> {
    validate(root, digest)?;
    let state = match outcome {
        Outcome::AcceptedNotDeliveryConfirmed => {
            b"smtp_accepted_not_delivery_confirmed\n".as_slice()
        }
        Outcome::UnknownDoNotRetry => b"unknown_do_not_retry\n".as_slice(),
    };
    let mut f = OpenOptions::new()
        .append(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(root.join(format!("{digest}.attempt")))
        .map_err(|_| Error("journal_write_failed"))?;
    let m = f.metadata().map_err(|_| Error("journal_write_failed"))?;
    if !m.is_file() || m.mode() & 0o077 != 0 {
        return Err(Error("unsafe_journal_marker"));
    }
    f.write_all(state)
        .and_then(|_| f.sync_all())
        .map_err(|_| Error("journal_write_failed"))
}
