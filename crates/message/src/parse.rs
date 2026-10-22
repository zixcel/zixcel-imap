use mailparse::MailHeaderMap;
use serde::Serialize;
use zixcel_mail_core::{Error, MAX_MESSAGE, Result};
#[derive(Debug, Serialize)]
pub struct Summary {
    pub from: Option<String>,
    pub to: Option<String>,
    pub subject: Option<String>,
    pub date: Option<String>,
}
/// Header summary only: do not render HTML, follow links or decode attachments implicitly.
pub fn summarize(raw: &[u8]) -> Result<Summary> {
    if raw.len() > MAX_MESSAGE {
        return Err(Error("message_size_limit"));
    }
    let end = raw
        .windows(4)
        .position(|b| b == b"\r\n\r\n")
        .ok_or(Error("header_incomplete"))?
        + 4;
    if end > 65536 {
        return Err(Error("header_size_limit"));
    }
    let (h, _) = mailparse::parse_headers(&raw[..end]).map_err(|_| Error("invalid_headers"))?;
    Ok(Summary {
        from: h.get_first_value("From"),
        to: h.get_first_value("To"),
        subject: h.get_first_value("Subject"),
        date: h.get_first_value("Date"),
    })
}
