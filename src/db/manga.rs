use std::cmp::Ordering;

use crate::{Context, Result};
use inflector::Inflector;
use itertools::Itertools;
use lazy_static::lazy_static;
use mangaverse_entity::models::chapter::ChapterTable;
use mangaverse_entity::models::manga::MangaTable;
use mangaverse_entity::models::page::PageTable;
use mangaverse_entity::models::source::SourceTable;
use sqlx::mysql::MySqlRow;
use sqlx::pool::PoolConnection;
use sqlx::types::chrono::{NaiveDateTime, Utc};
use sqlx::{FromRow, MySql, QueryBuilder, Row, Acquire};
use uuid::Uuid;

use super::chapter::{add_extra_chaps, delete_extra_chaps, update_chapter};

lazy_static! {
    static ref JUNK_SOURCE: SourceTable = SourceTable {
        id: String::default(),
        name: String::default(),
        priority: 2,
    };
}

pub struct MangaTableWrapper<'a> {
    pub contents: MangaTable<'a>,
    pub source_id: String,
}

impl FromRow<'_, MySqlRow> for MangaTableWrapper<'_> {
    fn from_row(row: &'_ MySqlRow) -> std::result::Result<Self, sqlx::Error> {
        Ok(MangaTableWrapper {
            contents: MangaTable {
                id: row.try_get("manga_id")?,
                linked_id: row.try_get("linked_id")?,
                is_listed: row.try_get("is_listed")?,
                name: row.try_get("name")?,
                cover_url: row.try_get("cover_url")?,
                url: row.try_get("url")?,
                last_updated: row.try_get("last_updated")?,
                status: row.try_get("status")?,
                is_main: row.try_get("is_main")?,
                description: row.try_get("description")?,
                last_watch_time: row.try_get("last_watch_time")?,
                public_id: row.try_get("public_id")?,
                is_old: row.try_get("is_old")?,
                source: &JUNK_SOURCE,
                chapters: Vec::default(),
                authors: Vec::default(),
                artists: Vec::default(),
                genres: Vec::default(),
                titles: Vec::default(),
            },
            source_id: row.try_get("source_id")?,
        })
    }
}

pub async fn update_manga(
    stored: &MangaTable<'_>,
    mng: &mut MangaTable<'_>,
    conn: &mut PoolConnection<MySql>,
) -> Result<()> {
    println!("Checking {}", stored.url);

    let t1 = stored.name == mng.name
        && stored.cover_url == mng.cover_url
        && stored.description == mng.description;

    let t = t1 && stored.last_updated == mng.last_updated && stored.status == mng.status;

    let f = stored.genres == mng.genres;

    if !t {
        println!("Updating Metadata for {}", stored.url);
        // update sql
        sqlx::query!("UPDATE manga SET name = ?, cover_url = ?, last_updated = ?, status = ?, description = ? where manga_id = ?", mng.name, mng.cover_url, mng.last_updated, mng.status, mng.description, stored.id).execute(&mut *conn).await?;
    }

    if !f || !t1 {
        println!("Updating Manga Listing for {}", stored.url);

        let genres_all = itertools::Itertools::intersperse(
            mng.genres.iter().map(|f| f.name.to_title_case()),
            ", ".to_string(),
        )
        .collect::<String>();
        let description_small = &mng.description[..255.min(mng.description.len())];

        sqlx::query!(
            "UPDATE manga_listing SET cover_url = ? , name = ?, genres = ?, description_small = ? where manga_id = ?",
            mng.cover_url,
            mng.name,
            genres_all,
            description_small,
            stored.id
        )
        .execute(&mut *conn)
        .await?;
    }

    //handle collection updates probably by a generic function

    if !f {
        sqlx::query!("DELETE from manga_genre where manga_id = ?", stored.id)
            .execute(&mut *conn)
            .await?;

        let mut q = QueryBuilder::new("INSERT into manga_genre(manga_id, genre_id) ");

        q.push_values(mng.genres.as_slice(), |mut b, genre| {
            b.push_bind(stored.id.as_str());
            b.push_bind(genre.id.as_str());
        });

        q.build().execute(&mut *conn).await?;

        println!("Inserted updated genres into manga");
    }

    let fut = stored.chapters.iter().zip(mng.chapters.iter());

    for (a, b) in fut {
        let f = update_chapter(a, b, conn).await;
        if f.is_err() {
            println!("{:#?}", f.expect_err("If Check failed"));
        }
    }

    match stored.chapters.len().cmp(&mng.chapters.len()) {
        Ordering::Less => {
            //add extra
            println!("Yay! New Chapters added for {}", stored.url);
            for r in &mut mng.chapters[stored.chapters.len()..] {
                r.chapter_id = Uuid::new_v4().to_string();
                r.manga_id = stored.id.clone();
            }
            add_extra_chaps(&mng.chapters[stored.chapters.len()..], conn).await?;
        }
        Ordering::Greater => {
            //delete extra
            println!("Deleting chapters for {}... strange", stored.url);
            delete_extra_chaps(
                stored
                    .chapters
                    .iter()
                    .skip(mng.chapters.len())
                    .map(|f| f.chapter_id.as_str())
                    .collect::<Vec<_>>()
                    .as_slice(),
                conn,
            )
            .await?;
        }
        Ordering::Equal => {
            println!("No chapter updates for {}", stored.url);
        }
    }

    sqlx::query!(
        "UPDATE manga SET last_watch_time = ? where manga_id = ?",
        Utc::now().timestamp_millis(),
        stored.id
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

pub async fn get_manga_from_url<'a>(
    url: &'a str,
    conn: &mut PoolConnection<MySql>,
    c: &'a Context,
) -> Result<MangaTable<'a>> {
    let mut r: MangaTableWrapper<'a> = sqlx::query_as("SELECT * from manga where url = ?")
        .bind(url)
        .fetch_one(&mut *conn)
        .await?;

    populate_relations(&mut r, conn, c).await?;

    Ok(r.contents)
}

pub async fn get_manga_from_id<'a>(
    id: &'a str,
    conn: &mut PoolConnection<MySql>,
    c: &'a Context,
) -> Result<MangaTable<'a>> {
    let mut r: MangaTableWrapper<'a> = sqlx::query_as("SELECT * from manga where manga_id = ?")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;

    populate_relations(&mut r, conn, c).await?;

    Ok(r.contents)
}

async fn populate_relations<'a>(
    r: &mut MangaTableWrapper<'a>,
    conn: &mut PoolConnection<MySql>,
    c: &'a Context,
) -> Result<()> {
    r.contents.titles = sqlx::query!(
        "SELECT title as data from title where linked_id = ?",
        r.contents.linked_id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|f| f.data)
    .collect();

    r.contents.authors = sqlx::query!(
        "SELECT author.name as data from author, manga_author where manga_author.author_id = author.author_id and manga_author.manga_id = ?",
        r.contents.id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|f| f.data)
    .collect();

    r.contents.artists = sqlx::query!(
        "SELECT author.name as data from author, manga_artist where manga_artist.author_id = author.author_id and manga_artist.manga_id = ?",
        r.contents.id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|f| f.data)
    .collect();

    r.contents.genres = sqlx::query!(
        "SELECT genre.name as data from genre, manga_genre where manga_genre.genre_id = genre.genre_id and manga_genre.manga_id = ?",
        r.contents.id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .filter_map(|f| c.genres.get(f.data.as_str()))
    .collect();

    r.contents.source = c
        .sources
        .get(
            sqlx::query!(
                "SELECT source_id as data from source where source_id = ?",
                r.source_id
            )
            .fetch_one(&mut *conn)
            .await?
            .data
            .as_str(),
        )
        .unwrap();

    r.contents.chapters = get_chapters(r.contents.id.as_str(), conn).await?;

    Ok(())
}

pub async fn insert_manga(
    mng: &mut MangaTable<'_>,
    conn: &mut PoolConnection<MySql>,
) -> Result<()> {
    //WIP

    println!("Adding {}", mng.url);

    mng.id = Uuid::new_v4().to_string();
    mng.linked_id = Uuid::new_v4().to_string();
    mng.last_watch_time = Some(Utc::now().timestamp_millis());
    mng.public_id = Uuid::new_v4().to_string();
    mng.artists = mng.artists.iter().map(|f| f.to_lowercase()).collect();
    mng.authors = mng.authors.iter().map(|f| f.to_uppercase()).collect();

    //insert metadata

    sqlx::query!("INSERT INTO manga(manga_id, linked_id, is_listed, name, cover_url, url, last_updated, status, is_main, description, source_id, last_watch_time, public_id, is_old) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", mng.id, mng.linked_id, true, mng.name, mng.cover_url, mng.url, mng.last_updated, mng.status, false, mng.description, mng.source.id, mng.last_watch_time, mng.public_id, false).execute(&mut *conn).await?;

    //look for matches using the titles table and set priority and linked_id

    let mut q = QueryBuilder::new("SELECT source.priority, manga.linked_id FROM source, manga where manga.source_id = source.source_id AND manga.is_main = 1 AND manga.linked_id = (select linked_id from title where title IN (");

    let mut sep = q.separated(',');

    for t in &mng.titles {
        sep.push_bind(t);
    }

    q.push(") limit 1)");

    let pri = q.build().fetch_optional(&mut *conn).await?;

    println!("After priorty fetch");

    if let Some(p) = pri {
        let act_pri = p.try_get::<i32, usize>(0)?;
        let act_link = p.try_get::<String, usize>(1)?;

        match mng.source.priority.cmp(&act_pri) {
            Ordering::Equal => {
                //break link... it's actually different
                sqlx::query!("UPDATE manga set is_main = 1 where manga_id = ?", mng.id)
                    .execute(&mut *conn)
                    .await?;
            }
            Ordering::Greater => {
                mng.linked_id = act_link;
                sqlx::query!(
                    "UPDATE manga set linked_id = ? where manga_id = ?",
                    mng.linked_id,
                    mng.id
                )
                .execute(&mut *conn)
                .await?;
            }
            Ordering::Less => {
                mng.linked_id = act_link;
                let mut txt = conn.begin().await?;
                
                sqlx::query!(
                    "UPDATE manga set linked_id = ? where manga_id = ?",
                    mng.linked_id,
                    mng.id
                )
                .execute(&mut txt)
                .await?;
                sqlx::query!(
                    "UPDATE manga set is_main = 0 where linked_id = ?",
                    mng.linked_id
                )
                .execute(&mut txt)
                .await?;
                sqlx::query!("UPDATE manga set is_main = 1 where manga_id = ?", mng.id)
                    .execute(&mut txt)
                    .await?;

                txt.commit().await?;
            }
        }
    } else {
        sqlx::query!("UPDATE manga set is_main = 1 where manga_id = ?", mng.id)
            .execute(&mut *conn)
            .await?;
    }

    println!("After is main update");

    //what I'm about to write is horrible... don't do this at least not without a unique constraint

    for t in &mng.titles {
        sqlx::query!(
            "INSERT INTO title (title, linked_id, title_id)
            SELECT * FROM (SELECT ? as title, ? as linked_id , ? as title_id) AS tmp
            WHERE NOT EXISTS (
                SELECT title FROM title WHERE title = ? AND linked_id = ?
            ) LIMIT 1",
            t.as_str(),
            mng.linked_id.as_str(),
            Uuid::new_v4().to_string(),
            t.as_str(),
            mng.linked_id.as_str()
        )
        .execute(&mut *conn)
        .await?;
    }

    println!("After title insert");

    //insert all the relations

    //genres

    let mut q = QueryBuilder::new("INSERT into manga_genre(genre_id, manga_id) ");

    q.push_values(mng.genres.as_slice(), |mut b, gen| {
        b.push_bind(gen.id.as_str());
        b.push_bind(mng.id.as_str());
    });

    q.build().execute(&mut *conn).await?;

    //first insert into authors table to check if author exists... then do an insert into select statement

    if !mng.artists.is_empty() || !mng.authors.is_empty() {
        let mut q = QueryBuilder::new("INSERT into author(author_id, name) ");

        q.push_values(
            mng.artists.iter().chain(mng.authors.iter()),
            |mut b, author| {
                b.push_bind(Uuid::new_v4().to_string());
                b.push_bind(author);
            },
        );

        q.push(" ON DUPLICATE KEY update author_id = author_id");

        q.build().execute(&mut *conn).await?;
    }

    println!("After author insert");

    //authors

    if !mng.authors.is_empty() {
        q = QueryBuilder::new("INSERT into manga_author(manga_id, author_id) select ");
        q.push_bind(mng.id.as_str());
        q.push(" as manga_id, author.author_id as author_id from author where author.name IN (");

        let mut sep = q.separated(',');

        for t in &mng.authors {
            sep.push_bind(t);
        }

        q.push(')');

        q.build().execute(&mut *conn).await?;
    }

    println!("After manga_author insert");

    //artists

    if !mng.artists.is_empty() {
        q = QueryBuilder::new("INSERT into manga_artist(manga_id, author_id) select ");
        q.push_bind(mng.id.as_str());
        q.push(" as manga_id, author.author_id as author_id from author where author.name IN (");

        let mut sep = q.separated(',');

        for t in &mng.artists {
            sep.push_bind(t);
        }

        q.push(')');

        q.build().execute(&mut *conn).await?;
    }

    println!("After manga_artist insert");

    //chapters

    for r in &mut mng.chapters {
        r.chapter_id = Uuid::new_v4().to_string();
        r.manga_id = mng.id.clone();
    }

    add_extra_chaps(&mng.chapters, conn).await?;

    println!("After chapters insert");

    let genres_all = itertools::Itertools::intersperse(
        mng.genres.iter().map(|f| f.name.to_title_case()),
        ", ".to_string(),
    )
    .collect::<String>();
    let description_small = &mng.description[..255.min(mng.description.len())];

    sqlx::query!("INSERT into manga_listing(manga_id, cover_url, name, genres, description_small, public_id) VALUES(?, ?, ?, ?, ?, ?)", mng.id, mng.cover_url, mng.name, genres_all, description_small, mng.public_id).execute(&mut *conn).await?;

    println!("After listing insert");

    println!("Finished inserting {}", mng.url);

    Ok(())
}

struct ChapterAndPages {
    pub chapter_id: String,
    pub chapter_name: String,
    pub chapter_number: String,
    pub updated_at: Option<NaiveDateTime>,
    pub manga_id: String,
    pub last_watch_time: i64,
    pub sequence_number: i32,

    pub all_pages: Option<String>,
}

pub async fn get_chapters(id: &str, conn: &mut PoolConnection<MySql>) -> Result<Vec<ChapterTable>> {
    //do a hack
    //use group concat to eliminate multiple sql calls and speed shit up
    //use space as separator

    let y = sqlx::query_as!(ChapterAndPages, "SELECT chapter.*, group_concat(chapter_page.chapter_page_id, ' ' ,chapter_page.url, ' ', chapter_page.page_number, ' ', chapter_page.chapter_id SEPARATOR ' ') as all_pages from chapter, chapter_page where chapter_page.chapter_id = chapter.chapter_id and chapter.manga_id = ? group by chapter_id order by sequence_number ASC", id).fetch_all(conn).await?;

    Ok(y.into_iter()
        .map(|f| ChapterTable {
            chapter_id: f.chapter_id,
            chapter_name: f.chapter_name,
            chapter_number: f.chapter_number,
            last_watch_time: f.last_watch_time,
            manga_id: f.manga_id,
            sequence_number: f.sequence_number,
            updated_at: f.updated_at,

            pages: f
                .all_pages
                .unwrap()
                .split_whitespace()
                .tuples()
                .filter_map(|(id, url, pg, ch_id)| {
                    Some(PageTable {
                        chapter_id: ch_id.to_string(),
                        url: url.to_string(),
                        page_number: str::parse(pg).ok()?,
                        id: str::parse(id).ok()?,
                    })
                })
                .collect(),
        })
        .collect())
}
