use std::path::Path;

use canner::{Tin, TinID, WrappedKey};
use sqlx::{SqlitePool, query, query_as};
use tokio::sync::{mpsc, oneshot};

use crate::{RoomID, UserID};

enum CellarEvent {
    Upload(Tin, Option<WrappedKey>),
    Download(TinID),
    Unbind(TinID),
}

enum CellarResponce {
    SuccessfullyUploaded,
    SuccessfullyDownloaded(Tin, Option<WrappedKey>),
}

enum CellarError {
    IoError(std::io::Error),
    SqlError(sqlx::Error),
    CannerError(canner::CannerError),
    PermissionDenied,
    NotFound,
}

struct CellarOrder {
    from: UserID,
    event: CellarEvent,
    sender: oneshot::Sender<Result<CellarResponce, CellarError>>,
}

type CellarPool = SqlitePool;
type CellarResult = Result<CellarResponce, CellarError>;

pub async fn setup_cellar_db(pool: &CellarPool) -> Result<(), sqlx::Error> {
    query(
        "CREATE TABLE IF NOT EXISTS index (
            user UUID NOT NULL,
            id BLOB NOT NULL,
            wrapped_key BLOB,
            room UUID NOT NULL,
            PRIMARY KEY (room, id)
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn cellar(mut rx: mpsc::Receiver<CellarOrder>, pool: &CellarPool, save_path: &Path) {
    while let Some(order) = rx.recv().await {
        let _ = order.sender.send(match order.event {
            CellarEvent::Upload(tin, wkey) => upload(order.from, tin, wkey, save_path, pool).await,
            CellarEvent::Download(id) =>
        });
    }
}

async fn upload(
    user: UserID,
    tin: Tin,
    wrapped_key: Option<WrappedKey>,
    save_path: &Path,
    pool: &CellarPool,
) -> CellarResult {
    let tin_id = tin.get_id();
    let path = save_path.join(format!("{}.tin", hex::encode(tin_id)));

    tokio::fs::write(path, tin.into_bytes())
        .await
        .map_err(|e| CellarError::IoError(e))?;

    query(
        "INSERT INTO index (
            user, id, wrapped_key
        )　VALUES ($1, $2, $3)",
    )
    .bind(user)
    .bind(&tin_id[..])
    .bind(wrapped_key.map(|w| w.0.to_vec()))
    .execute(pool)
    .await
    .map_err(CellarError::SqlError);

    Ok(CellarResponce::SuccessfullyUploaded)
}

pub async fn download(room: RoomID, tin_id: TinID, save_path: &Path, pool: &CellarPool) -> CellarResult {
    let (id, wrapped_key) = query_as("SELECT id, wrapped_key FROM index WHERE room = $1 AND id = $2")
        .bind(room)
        .bind(&tin_id[..])
        .fetch_optional(pool)
        .await
        .map_err(CellarError::SqlError)?
        .ok_or(CellarError::NotFound)?;

    tokio::fs::read(save_path).await.map_err(CellarError::IoError).and_then(|bin| Tin::from_bytes(bin).map_err(CellarError::CannerError))?;

    Ok(())
}
