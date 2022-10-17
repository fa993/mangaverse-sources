use mangaverse_entity::models::source::SourceTable;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::Error;
use crate::Result;

pub mod entity;

pub async fn insert_source_if_not_exists(
    src_name: &str,
    pri: i32,
    pool: &MySqlPool,
) -> Result<SourceTable> {
    let exists = sqlx::query_as!(
        SourceTable,
        "select source_id as id, name, priority from source where name = ?",
        src_name
    )
    .fetch_optional(pool)
    .await?;
    if exists.is_some() {
        exists.ok_or(Error::NoError)
    } else {
        let y = SourceTable {
            id: Uuid::new_v4().to_string(),
            name: src_name.to_string(),
            priority: pri,
        };
        sqlx::query!(
            "INSERT INTO source(source_id, name, priority) VALUES(?, ?, ?)",
            y.id.as_str(),
            y.name.as_str(),
            y.priority
        )
        .execute(pool)
        .await?;
        Ok(y)
    }
}
