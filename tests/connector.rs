use zixcel_imap::{build_plan, doctor, parse_config};

const CONFIG: &str = r#"
schema = "zixcel://imap/config/v1"
config_id = "operations-mail"
account_ref = "mailbox:operations"
secret_ref = "secret://imap/operations/observer"
server = "imap.example.invalid"
port = 993
tls_mode = "implicit-tls"
content_policy = "headers-only"
mailboxes = ["INBOX", "Reports"]
"#;

#[test]
fn normalized_mailbox_order_has_one_plan_identity() {
    let alternate = CONFIG.replace(
        "[\"INBOX\", \"Reports\"]",
        "[\"Reports\", \"INBOX\", \"INBOX\"]",
    );
    let first = build_plan(&parse_config(CONFIG).expect("config")).expect("plan");
    let second = build_plan(&parse_config(&alternate).expect("config")).expect("plan");
    assert_eq!(first, second);
}

#[test]
fn input_contract_is_closed_bounded_and_credential_free() {
    assert!(parse_config(&format!("{CONFIG}\npassword = \"no\"\n")).is_err());
    assert!(parse_config(&format!("{CONFIG}\nfuture = true\n")).is_err());
    assert!(parse_config(&" ".repeat(1_048_577)).is_err());
    let url = CONFIG.replace("imap.example.invalid", "https://imap.example.invalid");
    assert!(parse_config(&url).is_err());
}

#[test]
fn doctor_proves_that_execution_is_outside_this_crate() {
    let report = doctor();
    assert!(!report.network_client_linked);
    assert!(!report.secret_resolution_enabled);
    assert!(!report.execution_enabled);
}
