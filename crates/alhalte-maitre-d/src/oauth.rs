//! Google OAuth 2.0 integration and abstraction trait.

use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::auth::percent_encode;
use crate::error::{MaitreDError, Result};

/// User info obtained from Google OAuth, containing strictly only the `sub` identifier.
///
/// In accordance with ARCHITECTURE.md, Google accounts only inherit `sub`. The
/// value is transient: callers hash it with [`crate::crypto::hash_sub`], which
/// zeroizes it, and the database only ever sees the hash (Issue #190).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoogleUserInfo {
    /// Google unique subject identifier.
    pub sub: String,
}

impl fmt::Debug for GoogleUserInfo {
    /// Redacts `sub`: the raw Google identifier must never reach a log line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleUserInfo")
            .field("sub", &"<redacted>")
            .finish()
    }
}

/// Abstract contract for Google OAuth token exchange and ID token verification.
#[async_trait]
pub trait OAuthProvider: Send + Sync {
    /// Generates the Google OAuth authorization redirect URL.
    fn authorization_url(&self, redirect_uri: &str, state: &str) -> String;

    /// Exchanges an authorization code for Google user information (`sub`).
    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<GoogleUserInfo>;

    /// Verifies an ID token and retrieves the Google user information (`sub`).
    async fn verify_id_token(&self, id_token: &str) -> Result<GoogleUserInfo>;
}

/// Production implementation of [`OAuthProvider`] using HTTP requests to Google endpoints.
pub struct GoogleOAuthProvider {
    client_id: String,
    client_secret: String,
    client: reqwest::Client,
}

impl GoogleOAuthProvider {
    /// Creates a new [`GoogleOAuthProvider`] with credentials.
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Deserialize)]
struct GoogleTokenResponse {
    #[serde(default)]
    id_token: Option<String>,
}

#[derive(Deserialize)]
struct GoogleTokenInfoResponse {
    sub: Option<String>,
    aud: Option<String>,
}

#[async_trait]
impl OAuthProvider for GoogleOAuthProvider {
    fn authorization_url(&self, redirect_uri: &str, state: &str) -> String {
        format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid&state={}",
            percent_encode(&self.client_id),
            percent_encode(redirect_uri),
            percent_encode(state),
        )
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<GoogleUserInfo> {
        let params = [
            ("code", code),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ];

        let resp = self
            .client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                MaitreDError::OAuth(format!("Failed to connect to Google token endpoint: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MaitreDError::OAuth(format!(
                "Google token endpoint returned error status {status}: {body}"
            )));
        }

        let token_resp: GoogleTokenResponse = resp.json().await.map_err(|e| {
            MaitreDError::OAuth(format!("Failed to parse Google token response: {e}"))
        })?;

        let id_token = token_resp.id_token.ok_or_else(|| {
            MaitreDError::OAuth("Missing id_token in Google token response".to_string())
        })?;

        self.verify_id_token(&id_token).await
    }

    async fn verify_id_token(&self, id_token: &str) -> Result<GoogleUserInfo> {
        let resp = self
            .client
            .get("https://oauth2.googleapis.com/tokeninfo")
            .query(&[("id_token", id_token)])
            .send()
            .await
            .map_err(|e| {
                MaitreDError::OAuth(format!("Failed to connect to Google tokeninfo: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MaitreDError::OAuth(format!(
                "Google tokeninfo endpoint returned error {status}: {body}"
            )));
        }

        let token_info: GoogleTokenInfoResponse = resp.json().await.map_err(|e| {
            MaitreDError::OAuth(format!("Failed to parse Google tokeninfo response: {e}"))
        })?;

        if let Some(aud) = &token_info.aud
            && !self.client_id.is_empty()
            && aud != &self.client_id
        {
            return Err(MaitreDError::OAuth(format!(
                "Audience mismatch: expected {}, got {aud}",
                self.client_id
            )));
        }

        let sub = token_info
            .sub
            .ok_or_else(|| MaitreDError::OAuth("Missing sub in Google tokeninfo".to_string()))?;

        Ok(GoogleUserInfo { sub })
    }
}

/// Offline mock implementation of [`OAuthProvider`] for unit and integration testing.
#[derive(Debug, Default)]
pub struct MockOAuthProvider {
    codes: RwLock<HashMap<String, String>>,
    tokens: RwLock<HashMap<String, String>>,
    fallback_sub: RwLock<Option<String>>,
}

impl MockOAuthProvider {
    /// Creates a new empty [`MockOAuthProvider`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a mapping from authorization code to `sub`.
    pub fn register_code(&self, code: impl Into<String>, sub: impl Into<String>) {
        if let Ok(mut map) = self.codes.write() {
            map.insert(code.into(), sub.into());
        }
    }

    /// Registers a mapping from ID token to `sub`.
    pub fn register_token(&self, token: impl Into<String>, sub: impl Into<String>) {
        if let Ok(mut map) = self.tokens.write() {
            map.insert(token.into(), sub.into());
        }
    }

    /// Sets a default fallback `sub` returned when an unregistered code/token is received.
    pub fn set_fallback_sub(&self, sub: impl Into<String>) {
        if let Ok(mut fb) = self.fallback_sub.write() {
            *fb = Some(sub.into());
        }
    }
}

#[async_trait]
impl OAuthProvider for MockOAuthProvider {
    fn authorization_url(&self, redirect_uri: &str, state: &str) -> String {
        format!(
            "https://mock-google-oauth.example.com/auth?redirect_uri={redirect_uri}&state={state}"
        )
    }

    async fn exchange_code(&self, code: &str, _redirect_uri: &str) -> Result<GoogleUserInfo> {
        if let Ok(map) = self.codes.read()
            && let Some(sub) = map.get(code)
        {
            return Ok(GoogleUserInfo { sub: sub.clone() });
        }

        if let Ok(fb) = self.fallback_sub.read()
            && let Some(sub) = fb.as_ref()
        {
            return Ok(GoogleUserInfo { sub: sub.clone() });
        }

        Err(MaitreDError::OAuth(format!(
            "MockOAuthProvider: unrecognized authorization code '{code}'"
        )))
    }

    async fn verify_id_token(&self, id_token: &str) -> Result<GoogleUserInfo> {
        if let Ok(map) = self.tokens.read()
            && let Some(sub) = map.get(id_token)
        {
            return Ok(GoogleUserInfo { sub: sub.clone() });
        }

        if let Ok(fb) = self.fallback_sub.read()
            && let Some(sub) = fb.as_ref()
        {
            return Ok(GoogleUserInfo { sub: sub.clone() });
        }

        Err(MaitreDError::OAuth(format!(
            "MockOAuthProvider: unrecognized id_token '{id_token}'"
        )))
    }
}
