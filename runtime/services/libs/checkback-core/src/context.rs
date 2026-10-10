use std::fmt;

#[derive(Clone)]
pub struct RequestContext {
    pub tenant_id: String,
    pub subject_id: String,
    pub subject_email: Option<String>,
    pub subject_name: Option<String>,
}

impl fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestContext")
            .field("tenant_id", &self.tenant_id)
            .field("subject_id", &self.subject_id)
            .field("subject_email", &self.subject_email)
            .field("subject_name", &self.subject_name)
            .finish()
    }
}
