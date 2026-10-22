#![forbid(unsafe_code)]
//! Provider-neutral mail operations; caller owns authentication, custody and approval.
pub use zixcel_imap_client as imap;
pub use zixcel_mail_core as core;
pub use zixcel_mail_message as message;
pub use zixcel_mail_planning as planning;
pub use zixcel_smtp_client as smtp;
