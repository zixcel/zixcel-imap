use super::super::{OidcClient, types::OAuthClient};
use aws_lc_rs::{
    rsa::{KeyPair, KeySize},
    signature::KeyPair as _,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::sync::OnceLock;

fn key() -> &'static KeyPair {
    static KEY: OnceLock<KeyPair> = OnceLock::new();
    KEY.get_or_init(|| KeyPair::generate(KeySize::Rsa2048).unwrap())
}
pub(super) fn jwk() -> Value {
    let public = key().public_key();
    json!({"kty":"RSA", "kid":"generated-test-key", "alg":"RS256", "use":"sig",
        "n":URL_SAFE_NO_PAD.encode(public.modulus().big_endian_without_leading_zero()),
        "e":URL_SAFE_NO_PAD.encode(public.exponent().big_endian_without_leading_zero())})
}
pub(super) fn client() -> OidcClient {
    OidcClient {
        oauth: OAuthClient::<oauth2::EndpointNotSet, oauth2::EndpointNotSet>::new(
            oauth2::ClientId::new("fixture-client".into()),
        )
        .set_auth_uri(oauth2::AuthUrl::new("https://example.invalid/authorize".into()).unwrap())
        .set_token_uri(oauth2::TokenUrl::new("https://example.invalid/token".into()).unwrap()),
        issuer: "https://example.invalid".into(),
        client_id: "fixture-client".into(),
        keys: vec![jwk()],
    }
}
pub(super) fn claims() -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    json!({"iss":"https://example.invalid", "sub":"synthetic-subject", "aud":"fixture-client",
        "iat":now, "exp":now+3600, "nonce":"single-use-test-nonce", "email":"fixture@example.invalid"})
}
pub(super) fn token(header: Value, claims: Value) -> String {
    let input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap()),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
    );
    let mut signature = vec![0; key().public_modulus_len()];
    key()
        .sign(
            &aws_lc_rs::signature::RSA_PKCS1_SHA256,
            &aws_lc_rs::rand::SystemRandom::new(),
            input.as_bytes(),
            &mut signature,
        )
        .unwrap();
    format!("{}.{}", input, URL_SAFE_NO_PAD.encode(signature))
}
pub(super) fn header() -> Value {
    json!({"alg":"RS256", "kid":"generated-test-key"})
}
pub(super) fn verify(claims: Value) -> anyhow::Result<super::super::IdentityClaims> {
    client().verify(
        &token(header(), claims),
        "single-use-test-nonce",
        "synthetic-access",
        "synthetic-code",
    )
}
