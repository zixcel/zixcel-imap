use super::{
    ApiDoc, AppState, delete_account, icloud_upsert, list_accounts, list_domain_groups,
    list_domains, list_mail, list_mail_domains, list_status, list_sync_status, ms365_authorize,
    update_domain,
};
use axum::{
    Router,
    routing::{delete, get, patch, post},
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub(super) fn api() -> Router<AppState> {
    Router::new()
        .route("/api/accounts", get(list_accounts))
        .route("/api/domains", get(list_domains))
        .route("/api/domains/tree", get(list_domain_groups))
        .route("/api/domains/{account_id}/{domain}", patch(update_domain))
        .route("/api/status", get(list_status))
        .route("/api/mail", get(list_mail))
        .route("/api/mail/domains", get(list_mail_domains))
        .route("/api/sync-status", get(list_sync_status))
        .route("/api/accounts/ms365/authorize", post(ms365_authorize))
        .route("/api/accounts/icloud", post(icloud_upsert))
        .route("/api/accounts/{account_id}", delete(delete_account))
        .merge(SwaggerUi::new("/api/docs").url("/api/openapi.json", ApiDoc::openapi()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_routes_and_vendored_documentation_construct_without_a_database() {
        let _router = api();
    }

    #[test]
    fn openapi_preserves_account_and_domain_capture_contracts() {
        let document = serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document");
        let paths = document["paths"].as_object().expect("paths");
        assert!(paths["/api/accounts/{account_id}"].get("delete").is_some());
        assert!(
            paths["/api/domains/{account_id}/{domain}"]
                .get("patch")
                .is_some()
        );
    }
}
