use crate::{
    analytics::queue::{PendingUpload, UploadQueue},
    domain::sql::Sql,
    errors::AppError,
    providers::{
        queue_reader::QueueReader,
        queue_writer::QueueWriter,
        sqlite::{Database, matches::match_uuid, sqlite},
    },
};
use async_trait::async_trait;
use rusqlite::params;
use uuid::Uuid;

/// The upload queue: the team of a match is its team A.
pub struct SqliteQueue(Database);

impl SqliteQueue {
    pub fn new(db: Database) -> Self {
        Self(db)
    }
}

#[async_trait]
impl QueueReader for SqliteQueue {
    async fn load(&self) -> Result<UploadQueue, AppError> {
        self.0.read(|tx| {
            tx.prepare(
                "SELECT q.match_id, m.team_a, q.queued_at, q.retry_count
                 FROM upload_queue q JOIN matches m ON m.id = q.match_id
                 ORDER BY q.position",
            )
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok(PendingUpload {
                            match_id: row.get::<_, Sql<Uuid>>(0)?.0.to_string(),
                            team_id: row.get::<_, Sql<Uuid>>(1)?.0,
                            queued_at: row.get(2)?,
                            // saved as counted, shown up to 255
                            retry_count: u8::try_from(row.get::<_, i64>(3)?).unwrap_or(u8::MAX),
                        })
                    })?
                    .collect::<Result<_, _>>()
            })
            .map(UploadQueue::of)
            .map_err(sqlite)
        })
    }
}

#[async_trait]
impl QueueWriter for SqliteQueue {
    /// In place of the queue saved, in its order.
    async fn save(&self, queue: &UploadQueue) -> Result<(), AppError> {
        let pending = queue
            .pending()
            .iter()
            .map(|p| match_uuid(&p.match_id).map(|id| (id, p)))
            .collect::<Result<Vec<_>, _>>()?;
        self.0.write(|tx| {
            tx.execute("DELETE FROM upload_queue", []).map_err(sqlite)?;
            pending
                .iter()
                .zip(0_i64..)
                .try_for_each(|((id, p), position)| {
                    tx.execute(
                        "INSERT INTO upload_queue (match_id, position, queued_at, retry_count)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![Sql(*id), position, p.queued_at, p.retry_count],
                    )
                    .map(|_| ())
                    .map_err(sqlite)
                })
        })
    }
}
