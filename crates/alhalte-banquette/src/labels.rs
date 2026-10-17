//! Label identifiers, hashes, and reserved state key constants for `alhalte-banquette`.

use tin::Label;

/// Format identifier for whole room state: `alhalte/banquette`.
pub const IDENT_ALHALTE_BANQUETTE: &str = "alhalte/banquette";
/// Format identifier for individual scene state: `alhalte/banquette/scene`.
pub const IDENT_ALHALTE_BANQUETTE_SCENE: &str = "alhalte/banquette/scene";
/// Format identifier for a single sealed log chunk: `alhalte/banquette/log/chunk`.
pub const IDENT_ALHALTE_BANQUETTE_LOG_CHUNK: &str = "alhalte/banquette/log/chunk";
/// Format identifier for log chunk connectivity and head definition: `alhalte/banquette/log/biblio`.
pub const IDENT_ALHALTE_BANQUETTE_LOG_BIBLIO: &str = "alhalte/banquette/log/biblio";
/// Format identifier for background image change event: `alhalte/banquette/log/change-bg-img`.
pub const IDENT_ALHALTE_BANQUETTE_LOG_CHANGE_BG_IMG: &str = "alhalte/banquette/log/change-bg-img";
/// Format identifier for BGM change event: `alhalte/banquette/log/change-bgm`.
pub const IDENT_ALHALTE_BANQUETTE_LOG_CHANGE_BGM: &str = "alhalte/banquette/log/change-bgm";
/// Format identifier for character entrance event: `alhalte/banquette/log/character-came`.
pub const IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME: &str = "alhalte/banquette/log/character-came";
/// Format identifier for character departure event: `alhalte/banquette/log/character-gone`.
pub const IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE: &str = "alhalte/banquette/log/character-gone";

/// Format identifier for account ID: `alhalte/maitre-d/account-id`.
pub const IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID: &str = "alhalte/maitre-d/account-id";
/// Format identifier for session ID: `alhalte/maitre-d/session-id`.
pub const IDENT_ALHALTE_MAITRE_D_SESSION_ID: &str = "alhalte/maitre-d/session-id";

/// Precomputed 64-bit label for `alhalte/banquette`.
pub const LABEL_ALHALTE_BANQUETTE: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_BANQUETTE.as_bytes(), 0);
/// Precomputed 64-bit label for `alhalte/banquette/scene`.
pub const LABEL_ALHALTE_BANQUETTE_SCENE: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_BANQUETTE_SCENE.as_bytes(), 0);
/// Precomputed 64-bit label for `alhalte/banquette/log/chunk`.
pub const LABEL_ALHALTE_BANQUETTE_LOG_CHUNK: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_BANQUETTE_LOG_CHUNK.as_bytes(), 0);
/// Precomputed 64-bit label for `alhalte/banquette/log/biblio`.
pub const LABEL_ALHALTE_BANQUETTE_LOG_BIBLIO: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_BANQUETTE_LOG_BIBLIO.as_bytes(), 0);
/// Precomputed 64-bit label for `alhalte/banquette/log/change-bg-img`.
pub const LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BG_IMG: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_BANQUETTE_LOG_CHANGE_BG_IMG.as_bytes(), 0);
/// Precomputed 64-bit label for `alhalte/banquette/log/change-bgm`.
pub const LABEL_ALHALTE_BANQUETTE_LOG_CHANGE_BGM: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_BANQUETTE_LOG_CHANGE_BGM.as_bytes(), 0);
/// Precomputed 64-bit label for `alhalte/banquette/log/character-came`.
pub const LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_CAME.as_bytes(), 0);
/// Precomputed 64-bit label for `alhalte/banquette/log/character-gone`.
pub const LABEL_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_BANQUETTE_LOG_CHARACTER_GONE.as_bytes(), 0);

/// Precomputed 64-bit label for `alhalte/maitre-d/account-id`.
pub const LABEL_ALHALTE_MAITRE_D_ACCOUNT_ID: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_MAITRE_D_ACCOUNT_ID.as_bytes(), 0);
/// Precomputed 64-bit label for `alhalte/maitre-d/session-id`.
pub const LABEL_ALHALTE_MAITRE_D_SESSION_ID: Label =
    xxhash_rust::const_xxh64::xxh64(IDENT_ALHALTE_MAITRE_D_SESSION_ID.as_bytes(), 0);

/// Reserved state key in `"/"` storing administrator account IDs: `std::admins`.
pub const KEY_STD_ADMINS: &str = "std::admins";
/// Reserved state key storing active scene BGM: `std::bgm`.
pub const KEY_STD_BGM: &str = "std::bgm";
/// Reserved state key storing active scene background image: `std::b-img`.
pub const KEY_STD_B_IMG: &str = "std::b-img";
/// Character attribute key for human-readable name: `std::name`.
pub const KEY_STD_NAME: &str = "std::name";
/// Character attribute key for owner account ID: `std::owner_account`.
pub const KEY_STD_OWNER_ACCOUNT: &str = "std::owner_account";
