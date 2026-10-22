use crate::{Headers, Selection, Stream};
use futures_util::StreamExt;
use zixcel_mail_core::{Error, Result};
pub(super) async fn headers(
    session: &mut async_imap::Session<Stream>,
    folder: &str,
    selection: &Selection,
) -> Result<Vec<Headers>> {
    let mailbox = session
        .examine(folder)
        .await
        .map_err(|_| Error("imap_examine_failed"))?;
    if mailbox.uid_validity != Some(selection.uid_validity) {
        return Err(Error("uid_validity_changed"));
    }
    let set = selection
        .uids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut stream = session
        .uid_fetch(set, "(UID BODY.PEEK[HEADER]<0.65536>)")
        .await
        .map_err(|_| Error("imap_fetch_failed"))?;
    let mut result = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(item) = stream.next().await {
        let item = item.map_err(|_| Error("imap_fetch_failed"))?;
        let uid = item.uid.ok_or(Error("uid_missing"))?;
        if !selection.uids.contains(&uid) || !seen.insert(uid) {
            return Err(Error("unexpected_uid"));
        }
        let header = item.header().ok_or(Error("header_missing"))?;
        // A full requested prefix could be truncated. Never present it as a complete header.
        if header.len() >= 65536 || !header.ends_with(b"\r\n\r\n") {
            return Err(Error("header_incomplete_or_large"));
        }
        result.push(Headers {
            uid,
            raw: header.to_vec(),
        });
    }
    if result.len() != selection.uids.len() {
        return Err(Error("messages_missing_or_changed"));
    }
    Ok(result)
}
