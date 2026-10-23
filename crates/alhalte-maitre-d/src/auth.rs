//! Authentication providers: Google OAuth and the local password login.
//!
//! [`AuthProvider`] answers "how does this instance turn a browser request into
//! an account?":
//!
//! - [`GoogleAuthProvider`] is the normal flow: a single-use state, the fixed
//!   `{public_url}/auth/google/callback` redirect URI, and a `sub` exchange.
//! - [`LocalAuthProvider`] is the local password flow enabled by
//!   `maitre-d --local-login`: the browser is sent to the SPA `/login` route,
//!   which posts the account name and password to `/api/auth/local/login`.
//!
//! Both resolve to an [`AuthIdentity`], which is hashed with the instance salt
//! before it reaches SQLite. The local login has its own SQLite file
//! (`maitre-d-local.sqlite3`), so local accounts never mix with Google accounts.

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;

use crate::crypto::{SubHash, hash_sub};
use crate::error::{MaitreDError, Result};
use crate::oauth::OAuthProvider;

/// Default handle name of an account created by a provider.
pub const DEFAULT_HANDLE_NAME: &str = "Anonymous";

/// Maximum accepted account name length for the local login form, in Unicode
/// scalar values.
pub const MAX_ACCOUNT_NAME_LEN: usize = 100;

/// Prefix of the synthetic subject a local login derives from an account name.
///
/// The subject is hashed with the instance salt exactly like a Google `sub`, so
/// the local database stores a `sub_hash` and nothing that identifies the name
/// directly.
pub const LOCAL_SUBJECT_PREFIX: &str = "alhalte/local-login/";

/// Which login flow an [`AuthProvider`] implements.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AuthKind {
    /// Google OAuth 2.0 — the normal flow of a public instance.
    #[default]
    Google,
    /// Local password login: account name + password, backed by the local DB.
    Local,
}

impl AuthKind {
    /// Returns the stable name of the flow (`google` / `local`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Local => "local",
        }
    }

    /// Returns true for [`AuthKind::Local`].
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }
}

/// Identity a provider resolved from a request, before the account lookup.
///
/// The subject is the raw provider identifier (a Google `sub`, or the synthetic
/// local key). [`AuthIdentity::into_sub_hash`] hashes and zeroizes it, so the raw
/// value never reaches SQLite, a log line, or an HTTP response; [`Debug`](fmt::Debug)
/// redacts it for the same reason.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthIdentity {
    subject: String,
    handle_name: String,
}

impl AuthIdentity {
    /// Creates an identity from a provider subject and default handle name.
    pub fn new(subject: impl Into<String>, handle_name: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            handle_name: handle_name.into(),
        }
    }

    /// Returns the raw provider subject.
    ///
    /// The value is zeroized by [`AuthIdentity::into_sub_hash`]; callers must not
    /// log or persist it.
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Returns the default handle name for a newly created account.
    pub fn handle_name(&self) -> &str {
        &self.handle_name
    }

    /// Replaces the default handle name (`POST /auth/google/token`).
    #[must_use]
    pub fn with_handle_name(mut self, handle_name: impl Into<String>) -> Self {
        self.handle_name = handle_name.into();
        self
    }

    /// Hashes the subject with the instance salt and zeroizes it.
    ///
    /// Returns the stored [`SubHash`] and the handle name to use when the account
    /// is created.
    ///
    /// spec: alhalte-maitre-d/crypto 0001 sub is stored as a salted 128-bit hash
    pub fn into_sub_hash(mut self, salt: &str) -> (SubHash, String) {
        let sub_hash = hash_sub(salt, &mut self.subject);
        (sub_hash, self.handle_name)
    }
}

impl fmt::Debug for AuthIdentity {
    /// Renders the subject as redacted: it is a raw Google `sub` for Google
    /// logins and must never appear in a log line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthIdentity")
            .field("subject", &"<redacted>")
            .field("handle_name", &self.handle_name)
            .finish()
    }
}

/// Validates and normalizes the account name submitted to the local login form.
///
/// Returns the trimmed name when it is non-empty, at most
/// [`MAX_ACCOUNT_NAME_LEN`] characters, and contains no control characters. The
/// trimmed name is both the login key and the initial handle name, so two logins
/// with the same name land on the same account.
pub fn parse_local_account_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > MAX_ACCOUNT_NAME_LEN {
        return None;
    }
    if name.chars().any(char::is_control) {
        return None;
    }
    Some(name.to_string())
}

/// Percent-encodes `value` as a single query-string component.
///
/// Used to build the local login redirect and the OAuth authorization URL; the
/// encoding is RFC 3986 unreserved-set based, so `/`, `?`, `&` and `#` survive
/// only in their escaped form.
pub fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Abstraction over the login flows `alhalte-maitre-d` can serve.
///
/// The HTTP layer never branches on "local or Google": it asks the provider for
/// the next step. A provider that does not implement a flow returns `None` (for
/// the methods that answer "is this step mine?") or
/// [`MaitreDError::AuthFlowUnavailable`] (for the Google exchange calls).
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Returns which login flow this provider serves.
    fn kind(&self) -> AuthKind;

    /// Returns the target `GET /auth/login` must redirect the browser to.
    ///
    /// `None` means the provider continues the login inside maitre-d: Google
    /// issues a single-use state and redirects to `accounts.google.com`.
    fn login_redirect(&self, return_to: &str) -> Option<String>;

    /// Resolves the identity of a local login form submission.
    ///
    /// `None` means the name is unusable (empty, too long, or control characters)
    /// or that this provider has no local form.
    fn local_identity(&self, _account_name: &str) -> Option<AuthIdentity> {
        None
    }

    /// Builds the Google authorization URL bound to `state`.
    ///
    /// `None` for providers without a Google redirect.
    fn authorization_url(&self, redirect_uri: &str, state: &str) -> Option<String>;

    /// Exchanges a Google authorization code for an identity.
    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<AuthIdentity>;

    /// Verifies a Google ID token and returns its identity.
    async fn verify_id_token(&self, id_token: &str) -> Result<AuthIdentity>;
}

/// [`AuthProvider`] for the normal Google OAuth 2.0 flow.
///
/// Wraps any [`OAuthProvider`], so `maitre-d` uses the same production provider
/// and the same offline mock as before.
pub struct GoogleAuthProvider {
    oauth: Arc<dyn OAuthProvider>,
}

impl GoogleAuthProvider {
    /// Wraps `oauth` as the Google login flow.
    pub fn new(oauth: Arc<dyn OAuthProvider>) -> Self {
        Self { oauth }
    }

    /// Returns the wrapped [`OAuthProvider`].
    pub fn oauth(&self) -> &Arc<dyn OAuthProvider> {
        &self.oauth
    }
}

impl fmt::Debug for GoogleAuthProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleAuthProvider").finish()
    }
}

#[async_trait]
impl AuthProvider for GoogleAuthProvider {
    fn kind(&self) -> AuthKind {
        AuthKind::Google
    }

    fn login_redirect(&self, _return_to: &str) -> Option<String> {
        None
    }

    fn authorization_url(&self, redirect_uri: &str, state: &str) -> Option<String> {
        Some(self.oauth.authorization_url(redirect_uri, state))
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<AuthIdentity> {
        let user_info = self.oauth.exchange_code(code, redirect_uri).await?;
        Ok(AuthIdentity::new(user_info.sub, DEFAULT_HANDLE_NAME))
    }

    async fn verify_id_token(&self, id_token: &str) -> Result<AuthIdentity> {
        let user_info = self.oauth.verify_id_token(id_token).await?;
        Ok(AuthIdentity::new(user_info.sub, DEFAULT_HANDLE_NAME))
    }
}

/// [`AuthProvider`] for the local password login.
///
/// The browser is sent to the SPA `/login` route. The account name and password
/// are posted to `/api/auth/local/login`, which verifies the Argon2id hash in the
/// local SQLite file. A name that has never been seen creates an account; a known
/// name reuses it, because the derived subject is deterministic.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalAuthProvider;

impl LocalAuthProvider {
    /// Creates the local login provider.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AuthProvider for LocalAuthProvider {
    fn kind(&self) -> AuthKind {
        AuthKind::Local
    }

    fn login_redirect(&self, return_to: &str) -> Option<String> {
        Some(format!("/login?return_to={}", percent_encode(return_to)))
    }

    fn local_identity(&self, account_name: &str) -> Option<AuthIdentity> {
        let name = parse_local_account_name(account_name)?;
        Some(AuthIdentity::new(
            format!("{LOCAL_SUBJECT_PREFIX}{name}"),
            name,
        ))
    }

    fn authorization_url(&self, _redirect_uri: &str, _state: &str) -> Option<String> {
        None
    }

    async fn exchange_code(&self, _code: &str, _redirect_uri: &str) -> Result<AuthIdentity> {
        Err(MaitreDError::AuthFlowUnavailable(
            "Google OAuth code exchange",
        ))
    }

    async fn verify_id_token(&self, _id_token: &str) -> Result<AuthIdentity> {
        Err(MaitreDError::AuthFlowUnavailable(
            "Google ID token verification",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::MockOAuthProvider;

    // spec: alhalte-maitre-d/local-login 0001 the local provider derives the
    // subject from the trimmed account name only
    #[test]
    fn local_provider_derives_a_deterministic_subject() {
        let provider = LocalAuthProvider::new();
        assert_eq!(provider.kind(), AuthKind::Local);
        assert!(provider.kind().is_local());
        assert_eq!(provider.kind().as_str(), "local");

        let identity = provider.local_identity("  Alice  ").unwrap();
        assert_eq!(identity.handle_name(), "Alice");
        assert_eq!(
            identity.subject(),
            format!("{LOCAL_SUBJECT_PREFIX}Alice"),
            "the trimmed name is the login key"
        );

        // The same name resolves to the same account, a different one does not.
        let again = provider.local_identity("Alice").unwrap();
        let other = provider.local_identity("Bob").unwrap();
        assert_eq!(
            identity.clone().into_sub_hash("salt"),
            again.into_sub_hash("salt")
        );
        assert_ne!(
            provider
                .local_identity("Alice")
                .unwrap()
                .into_sub_hash("salt")
                .0,
            other.into_sub_hash("salt").0
        );

        // The subject is redacted from Debug and zeroized by the hash. The
        // handle name is not a secret, so only the subject itself must not leak.
        let debug = format!("{identity:?}");
        assert!(!debug.contains(&format!("{LOCAL_SUBJECT_PREFIX}Alice")));
        assert!(debug.contains("<redacted>"));

        // The local provider has no Google flow.
        assert_eq!(provider.authorization_url("http://x/cb", "state"), None);
        assert!(provider.login_redirect("http://app.test/").is_some());
    }

    // spec: alhalte-maitre-d/local-login 0002 only sane account names are accepted
    #[test]
    fn local_account_names_are_validated() {
        assert_eq!(parse_local_account_name("  "), None);
        assert_eq!(parse_local_account_name(""), None);
        assert_eq!(parse_local_account_name("bad\nname"), None);
        assert_eq!(
            parse_local_account_name(&"x".repeat(MAX_ACCOUNT_NAME_LEN + 1)),
            None
        );
        assert_eq!(
            parse_local_account_name(&"x".repeat(MAX_ACCOUNT_NAME_LEN)),
            Some("x".repeat(MAX_ACCOUNT_NAME_LEN))
        );
        assert_eq!(
            parse_local_account_name("  アリス "),
            Some("アリス".to_string())
        );
    }

    // spec: alhalte-maitre-d/local-login 0003 the SPA redirect keeps return_to
    #[test]
    fn local_login_redirect_encodes_return_to() {
        let provider = LocalAuthProvider::new();
        assert_eq!(
            provider
                .login_redirect("http://app.test/rooms/1?x=1&y=2#key")
                .as_deref(),
            Some("/login?return_to=http%3A%2F%2Fapp.test%2Frooms%2F1%3Fx%3D1%26y%3D2%23key")
        );
        assert_eq!(
            provider.login_redirect("https://app.test/").as_deref(),
            Some("/login?return_to=https%3A%2F%2Fapp.test%2F")
        );
    }

    // spec: alhalte-maitre-d/oauth-exchange 0001 the Google provider wraps OAuthProvider
    #[tokio::test]
    async fn google_provider_delegates_to_the_oauth_provider() {
        let mock = Arc::new(MockOAuthProvider::new());
        mock.register_code("code-1", "google-sub-1");
        mock.register_token("token-1", "google-sub-1");
        let provider = GoogleAuthProvider::new(mock);

        assert_eq!(provider.kind(), AuthKind::Google);
        assert!(!provider.kind().is_local());
        assert_eq!(provider.login_redirect("http://app.test/"), None);
        assert_eq!(provider.local_identity("Alice"), None);
        assert!(provider.authorization_url("http://x/cb", "state").is_some());

        let identity = provider
            .exchange_code("code-1", "http://x/cb")
            .await
            .unwrap();
        assert_eq!(identity.subject(), "google-sub-1");
        assert_eq!(identity.handle_name(), DEFAULT_HANDLE_NAME);

        let identity = provider.verify_id_token("token-1").await.unwrap();
        assert_eq!(identity.subject(), "google-sub-1");

        assert!(
            provider
                .exchange_code("unknown", "http://x/cb")
                .await
                .is_err()
        );
    }
}
