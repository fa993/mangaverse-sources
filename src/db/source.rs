use mangaverse_entity::models::source::SourceTable;
use sqlx::MySql;
use sqlx::Pool;
use uuid::Uuid;

use crate::MSError;
use crate::Result;

pub async fn insert_source_if_not_exists(
    src_name: &str,
    pri: i32,
    pool: &Pool<MySql>,
) -> Result<SourceTable> {
    let exists = sqlx::query_as!(
        SourceTable,
        "select source_id as id, name, priority from source where name = ?",
        src_name
    )
    .fetch_optional(pool)
    .await?;
    if exists.is_some() {
        exists.ok_or(MSError {
            message: "exists if check".to_string(),
            err_type: crate::MSErrorType::NoError,
        })
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
