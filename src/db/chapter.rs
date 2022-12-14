use mangaverse_entity::models::chapter::ChapterTable;
use sqlx::{pool::PoolConnection, MySql, QueryBuilder};

use crate::Result;

pub async fn update_chapter(
    ori: &ChapterTable,
    lat: &ChapterTable,
    conn: &mut PoolConnection<MySql>,
) -> Result<()> {
    let chk_met = ori.chapter_name == lat.chapter_name
        && ori.chapter_number == lat.chapter_number
        && ori.updated_at == lat.updated_at;

    let chk_pg = ori
        .pages
        .iter()
        .zip(lat.pages.iter())
        .all(|e| e.0.url == e.1.url);

    if !chk_met {
        sqlx::query!("UPDATE chapter SET chapter_name = ?, chapter_number = ?, updated_at = ? where chapter_id = ?", lat.chapter_name, lat.chapter_number, lat.updated_at, ori.chapter_id).execute(&mut *conn).await?;
    }

    if !chk_pg {
        //remove previous...
        sqlx::query!(
            "DELETE FROM chapter_page where chapter_id = ?",
            ori.chapter_id
        )
        .execute(&mut *conn)
        .await?;

        //add new
        let mut q = QueryBuilder::new("INSERT into chapter_page(url, page_number, chapter_id) ");

        q.push_values(lat.pages.as_slice(), |mut b, page| {
            b.push_bind(page.url.as_str());
            b.push_bind(page.page_number);
            b.push_bind(ori.chapter_id.as_str());
        });

        q.build().execute(&mut *conn).await?;
    }

    Ok(())
}

pub async fn delete_extra_chaps(chp_ids: &[&str], conn: &mut PoolConnection<MySql>) -> Result<()> {
    for t in chp_ids {
        sqlx::query!("DELETE FROM chapter_page where chapter_id = ?", t)
            .execute(&mut *conn)
            .await?;
        sqlx::query!("DELETE FROM chapter where chapter_id = ?", t)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

pub async fn add_extra_chaps(
    chps: &[ChapterTable],
    conn: &mut PoolConnection<MySql>,
) -> Result<()> {
    for lat in chps {
        sqlx::query!("INSERT INTO chapter(chapter_name, chapter_number, updated_at, chapter_id, manga_id, sequence_number, last_watch_time) VALUES(?, ?, ?, ?, ?, ?, ?)", lat.chapter_name, lat.chapter_number, lat.updated_at, lat.chapter_id, lat.manga_id, lat.sequence_number, lat.last_watch_time).execute(&mut *conn).await?;

        let mut q = QueryBuilder::new("INSERT into chapter_page(url, page_number, chapter_id) ");

        q.push_values(lat.pages.as_slice(), |mut b, page| {
            b.push_bind(page.url.as_str());
            b.push_bind(page.page_number);
            b.push_bind(lat.chapter_id.as_str());
        });

        q.build().execute(&mut *conn).await?;
    }

    Ok(())
}
