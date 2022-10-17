use std::collections::{HashMap, HashSet};

use isahc::prelude::*;
use mangaverse_entity::models::{
    chapter::ChapterTable, genre::Genre, manga::MangaTable, page::PageTable, source::SourceTable,
};
use scraper::{Html, Selector};
use sqlx::{
    types::chrono::{NaiveDateTime, Utc}, MySqlPool,
};

use crate::{Error, Result};

use super::insert_source_if_not_exists;

use lazy_static::lazy_static;

const SOURCE_NAME: &str = "readm";

const WEBSITE_HOST: &str = "https://readm.org";

lazy_static! {
    static ref GENRE_SELECTOR: Selector =
        Selector::parse("ul.advanced-search-categories li").unwrap();
    static ref NAME_SELECTOR: Selector = Selector::parse("h1.page-title").unwrap();
    static ref COVERURL_SELECTOR: Selector = Selector::parse("img.series-profile-thumb").unwrap();
    static ref TITLES_SELECTOR: Selector = Selector::parse("div.sub-title").unwrap();
    static ref SUMMARY_SELECTOR: Selector = Selector::parse("div.series-summary-wrapper").unwrap();
    static ref MANGA_GENRE_SELECTOR: Selector = Selector::parse("a").unwrap();
    static ref STATUS_SELECTOR: Selector = Selector::parse(".series-status").unwrap();
    static ref AUTHOR_SELECTOR: Selector = Selector::parse("span#first_episode > a").unwrap();
    static ref ARTIST_SELECTOR: Selector = Selector::parse("span#last_episode > a").unwrap();
    static ref CHAPTER_SELECTOR: Selector = Selector::parse("td.table-episodes-title a").unwrap();
    static ref DESCRIPTION_SELECTOR: Selector = Selector::parse("p").unwrap();
    static ref CHAPTER_UPDATED_AT_SELECTOR: Selector = Selector::parse("div.media-date").unwrap();
    static ref CHAPTER_NUMBER_SELECTOR: Selector = Selector::parse("span.light-title").unwrap();
    static ref IMAGES_SELECTOR: Selector = Selector::parse("img.img-responsive").unwrap();
}

pub async fn get_readm_source(pool: &MySqlPool) -> Result<SourceTable> {
    insert_source_if_not_exists(SOURCE_NAME, 1, pool).await
}

pub async fn get_readm_genres() -> Result<HashSet<String>> {
    let url = "https://readm.org/advanced-search";

    let response_text = isahc::get_async(url).await?.text().await?;

    let doc = Html::parse_document(&response_text);

    Ok(doc
        .select(&GENRE_SELECTOR)
        .filter_map(|f| {
            let r = f.text().collect::<String>().trim().to_lowercase();
            if r == "uncategorized" {
                None
            } else {
                Some(r)
            }
        })
        .collect())
}

#[allow(unused_must_use)]
pub async fn get_manga<'a>(
    url: String,
    sc: &'a SourceTable,
    map: &'a HashMap<String, Genre>,
) -> Result<MangaTable<'a>> {
    let mut mng: MangaTable = MangaTable::new(sc);
    mng.is_listed = true;
    mng.url = url;

    let doc = Html::parse_document(
        isahc::get_async(mng.url.as_str())
            .await?
            .text()
            .await?
            .as_str(),
    );

    mng.name.extend(
        doc.select(&NAME_SELECTOR)
            .next()
            .ok_or(Error::TextParseError)?
            .text(),
    );

    mng.name = mng.name.trim().to_string();

    mng.titles.push(mng.name.clone());

    mng.cover_url.push_str(WEBSITE_HOST);

    mng.cover_url.push_str(
        doc.select(&COVERURL_SELECTOR)
            .next()
            .and_then(|f| f.value().attr("src"))
            .ok_or(Error::TextParseError)?,
    );

    if let Some(x) = doc.select(&TITLES_SELECTOR).next() {
        mng.titles.extend(
            x.text()
                .collect::<String>()
                .split(&[',', ';'])
                .map(|t| t.trim().to_string()),
        );
    }

    if let Some(x) = doc.select(&SUMMARY_SELECTOR).next() {
        mng.description
            .extend(x.select(&DESCRIPTION_SELECTOR).flat_map(|f| f.text()));

        mng.description = mng.description.trim().to_string();

        mng.genres.extend(
            x.select(&MANGA_GENRE_SELECTOR)
                .filter_map(|f| map.get(f.text().collect::<String>().to_lowercase().trim())),
        );
    }

    if let Some(x) = doc.select(&STATUS_SELECTOR).next() {
        mng.status.extend(x.text());
        mng.status = mng.status.trim().to_uppercase();
    } else {
        mng.status.push_str("Not Available");
    }

    if let Some(x) = doc.select(&AUTHOR_SELECTOR).next() {
        mng.authors
            .push(x.text().collect::<String>().trim().to_string());
    }

    if let Some(x) = doc.select(&ARTIST_SELECTOR).next() {
        mng.artists
            .push(x.text().collect::<String>().trim().to_string());
    }

    for (idx, i) in doc.select(&CHAPTER_SELECTOR).enumerate() {
        if let Some(x) = i.value().attr("href") {
            let mut t = ChapterTable {
                sequence_number: idx as i32,
                last_watch_time: Utc::now().timestamp_millis(),
                ..Default::default()
            };
            let mut r = String::from(WEBSITE_HOST);
            r.push_str(x);
            populate_chapter(&mut t, r.as_str()).await;

            mng.chapters.push(t);
        }
    }

    mng.chapters.reverse();

    let sz = mng.chapters.len() as i32;

    for t in mng.chapters.iter_mut() {
        t.sequence_number = sz - t.sequence_number - 1;
    }

    Ok(mng)
}

async fn populate_chapter(t: &mut ChapterTable, x: &str) -> Result<()> {
    let y = Html::parse_document(&isahc::get_async(x).await?.text().await?);
    if let Some(dt) = y.select(&CHAPTER_UPDATED_AT_SELECTOR).next() {
        let mut u = dt.text().collect::<String>().trim().to_string();
        u.push_str(" 00:00:00");

        t.updated_at = NaiveDateTime::parse_from_str(u.as_str(), "%d %B %Y %T").ok();
    }
    if let Some(dt) = y.select(&CHAPTER_NUMBER_SELECTOR).next() {
        t.chapter_number = dt.text().collect::<String>();
    }
    for (idxn, f) in y.select(&IMAGES_SELECTOR).enumerate() {
        if let Some(dt) = f.value().attr("src") {
            let mut r = PageTable {
                page_number: idxn as i32,
                ..Default::default()
            };
            r.url.push_str(WEBSITE_HOST);
            r.url.push_str(dt);
            t.pages.push(r);
        }
    }
    Ok(())
}
