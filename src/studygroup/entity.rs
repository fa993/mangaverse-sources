use std::collections::{HashMap, HashSet};

use mangaverse_entity::models::{
    chapter::ChapterTable, genre::Genre, manga::MangaTable, page::PageTable, source::SourceTable,
};
use scraper::{Html, Selector};
use sqlx::types::chrono::Utc;
use sqlx::{MySql, Pool};

use crate::{MSError, Result};

use crate::db::source::insert_source_if_not_exists;

use lazy_static::lazy_static;

const SOURCE_NAME: &str = "studygroup";

const WEBSITE_HOST: &str = "https://studygroupmanga.com";
const AUTHOR: &str = "Author(s) :";
const ALTERNATIVE_NAME: &str = "Alternative(s) :";
const STATUS: &str = "Status :";
const GENRES: &str = "Genre(s) :";
const DESCRIPTION: &str = "Synopsis(s) :";

lazy_static! {
    static ref TABLE_LABEL_SELECTOR: Selector = Selector::parse("table td > em").unwrap();
    static ref TABLE_VALUE_SELECTOR: Selector =
        Selector::parse("table td.has-text-align-left").unwrap();
    static ref COVERURL_SELECTOR: Selector = Selector::parse("figure > img").unwrap();
    static ref CHAPTER_SELECTOR: Selector = Selector::parse("td.table-episodes-title a").unwrap();
    static ref IMAGES_SELECTOR: Selector = Selector::parse("img.aligncenter").unwrap();
}

pub async fn get_studygroup_source(pool: &Pool<MySql>) -> Result<SourceTable> {
    insert_source_if_not_exists(SOURCE_NAME, 0, pool).await
}

pub async fn get_studygroup_genres() -> Result<HashSet<String>> {
    let url = WEBSITE_HOST;

    let response_text = reqwest::get(url).await?.text().await?;

    let doc = Html::parse_document(&response_text);

    let labels = doc.select(&TABLE_LABEL_SELECTOR);
    let vals = doc.select(&TABLE_VALUE_SELECTOR);

    let mut both = labels.zip(vals);

    both.find_map(|(lab, v)| {
        if lab.text().collect::<String>() == GENRES {
            Some(
                v.text()
                    .collect::<String>()
                    .split('-')
                    .map(str::trim)
                    .map(ToString::to_string)
                    .collect(),
            )
        } else {
            None
        }
    })
    .ok_or(MSError {
        message: "Failed to get cover url link".to_string(),
        err_type: crate::MSErrorType::TextParseError,
    })
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

    {
        let doc =
            Html::parse_document(reqwest::get(mng.url.as_str()).await?.text().await?.as_str());

        mng.name = String::from("Study Group");

        mng.titles.push(mng.name.clone());

        mng.cover_url.push_str(
            doc.select(&COVERURL_SELECTOR)
                .next()
                .and_then(|f| f.value().attr("src"))
                .ok_or(MSError {
                    message: "Failed to get cover url link".to_string(),
                    err_type: crate::MSErrorType::TextParseError,
                })?,
        );

        let iter_label = doc.select(&TABLE_LABEL_SELECTOR);
        let iter_value = doc.select(&TABLE_VALUE_SELECTOR);

        let metadata_table = iter_label.zip(iter_value);

        for (label, value) in metadata_table {
            println!("{:#?}", value.text().collect::<String>().split(" - ").map(str::trim).map(str::to_lowercase).collect::<Vec<String>>());
            match label.text().collect::<String>().as_str() {
                AUTHOR => mng.authors.extend(
                    value
                        .text()
                        .collect::<String>()
                        .split(',')
                        .map(str::trim)
                        .map(ToString::to_string),
                ),
                ALTERNATIVE_NAME => mng.titles.extend(
                    value
                        .text()
                        .collect::<String>()
                        .split(',')
                        .map(str::trim)
                        .map(ToString::to_string),
                ),
                STATUS => mng
                    .status
                    .extend(value.text().map(|f| f.trim().to_uppercase())),
                GENRES => mng.genres.extend(
                    value
                        .text()
                        .collect::<String>()
                        .split('-')
                        .map(str::trim)
                        .map(str::to_lowercase)
                        .filter_map(|f| map.get(&f)),
                ),
                DESCRIPTION => mng.description.extend(value.text().map(|f| f.trim())),
                _ => {}
            };
        }

        let mut rt = doc.select(&CHAPTER_SELECTOR).enumerate();
        rt.next();
        rt.next();
        rt.next();

        let mut rtt = rt.peekable();

        while let Some((idx, i)) = rtt.next() {
            if rtt.peek().is_none() {
                break;
            }
            if let Some(x) = i.value().attr("href") {
                let mut t = ChapterTable {
                    sequence_number: idx as i32,
                    last_watch_time: Utc::now().timestamp_millis(),
                    ..Default::default()
                };

                t.chapter_id = x.to_string();

                mng.chapters.push(t);
            }
        }
    }

    {
        for yt in mng.chapters.iter_mut() {
            let r = yt.chapter_id.clone();
            populate_chapter(yt, r.as_str()).await;
        }

        mng.chapters.reverse();

        let sz = mng.chapters.len() as i32;

        for t in mng.chapters.iter_mut() {
            t.sequence_number = sz - t.sequence_number - 1;
        }
    }

    println!("{:#?}", mng);

    Ok(mng)
}

async fn populate_chapter(t: &mut ChapterTable, x: &str) -> Result<()> {
    let y = Html::parse_document(reqwest::get(x).await?.text().await?.as_str());
    for (idxn, f) in y.select(&IMAGES_SELECTOR).enumerate() {
        if let Some(dt) = f.value().attr("src") {
            let mut r = PageTable {
                page_number: idxn as i32,
                ..Default::default()
            };
            r.url.push_str(dt);
            t.pages.push(r);
        }
    }
    Ok(())
}
