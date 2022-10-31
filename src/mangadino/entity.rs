use std::collections::{HashMap, HashSet};

use lazy_static::lazy_static;
use mangaverse_entity::models::{genre::Genre, manga::MangaTable, source::SourceTable};
use scraper::{Html, Selector};
use sqlx::{MySql, Pool};

use crate::{db::source::insert_source_if_not_exists, MSError, Result};

const SOURCE_NAME: &str = "mangadino";

lazy_static! {
    static ref GENRE_SELECTOR: Selector = Selector::parse("select[name='genre'] > option").unwrap();
    static ref NAME_SELECTOR: Selector = Selector::parse("h1.p16").unwrap();
    static ref COVERURL_SELECTOR: Selector = Selector::parse("div.s-img > img").unwrap();
    static ref METADATA_AND_CHAPTER_SELECTOR: Selector = Selector::parse("tbody").unwrap();
    static ref METADATA_LABEL_SELECTOR: Selector = Selector::parse("tr").unwrap();
    static ref METADATA_VALUE_SELECTOR: Selector = Selector::parse("td").unwrap();
    static ref MANGA_GENRE_SELECTOR: Selector = Selector::parse("a").unwrap();
    static ref UPDATED_LABEL_SELECTOR: Selector = Selector::parse("span.stre-label").unwrap();
    static ref UPDATED_VALUE_SELECTOR: Selector = Selector::parse("span.stre-value").unwrap();
    static ref CHAPTER_LABEL_SELECTOR: Selector = Selector::parse("a.chapter-name").unwrap();
    static ref CHAPTER_VALUE_SELECTOR: Selector = Selector::parse("span.chapter-time").unwrap();
    static ref DESCRIPTION_SELECTOR: Selector =
        Selector::parse(".panel-story-info-description").unwrap();
    static ref IMAGES_SELECTOR: Selector =
        Selector::parse("div.container-chapter-reader > img").unwrap();
}

pub async fn get_mangadino_genres() -> Result<HashSet<String>> {
    let url = "https://mangadino.com/action/";

    let response_text = reqwest::get(url).await?.text().await?;

    let doc = Html::parse_document(&response_text);

    Ok(doc
        .select(&GENRE_SELECTOR)
        .skip(1)
        .map(|f| f.text().collect::<String>().trim().to_lowercase())
        .collect())
}

pub async fn get_mangadino_source(pool: &Pool<MySql>) -> Result<SourceTable> {
    insert_source_if_not_exists(SOURCE_NAME, 3, pool).await
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

        mng.name.extend(
            doc.select(&NAME_SELECTOR)
                .next()
                .ok_or(MSError {
                    message: "Failed to get name".to_string(),
                    err_type: crate::MSErrorType::TextParseError,
                })?
                .text(),
        );

        mng.name = mng.name.trim().to_string();

        mng.titles.push(mng.name.clone());

        mng.cover_url.push_str(
            doc.select(&COVERURL_SELECTOR)
                .next()
                .and_then(|f| f.value().attr("data-src"))
                .ok_or(MSError {
                    message: "Failed to get cover url link".to_string(),
                    err_type: crate::MSErrorType::TextParseError,
                })?,
        );

        let mut tables = doc.select(&METADATA_AND_CHAPTER_SELECTOR);

        let mtdta = tables.next();

        if let Some(x) = mtdta {
            let mut in_sel = x.select(&METADATA_LABEL_SELECTOR);

            for t in in_sel.by_ref() {
                let mut rt = t.select(&METADATA_VALUE_SELECTOR);

                let key = rt.next();
                let val = rt.next();

                if key.is_none() || val.is_none() {
                    continue;
                }

                let key = key.unwrap();
                let val = val.unwrap();

                match key {
                    x if x.inner_html().to_lowercase() == "alternative" => {
                        let act_val = val.inner_html();
                        if act_val == "-" {
                            continue;
                        }
                        mng.titles
                            .extend(act_val.split(&[';']).map(|f| f.to_string()))
                    }
                    x if x.inner_html().to_lowercase() == "author" => {
                        let act_val = val.inner_html();
                        if act_val == "-" {
                            continue;
                        }
                        mng.authors = act_val.split(&[',']).map(|f| f.to_string()).collect();
                    }
                    x if x.inner_html().to_lowercase() == "artist" => {
                        let act_val = val.inner_html();
                        if act_val == "-" {
                            continue;
                        }
                        mng.artists = act_val.split(&[',']).map(|f| f.to_string()).collect();
                    }
                    x if x.inner_html().to_lowercase() == "genre" => {
                        let act_val = val.inner_html();
                        if act_val == "-" {
                            continue;
                        }
                        mng.genres = val
                            .select(&MANGA_GENRE_SELECTOR)
                            .map(|f| f.inner_html().trim().to_lowercase())
                            .filter_map(|f| map.get(f.as_str()))
                            .collect();
                    }
                    x if x.inner_html().to_lowercase() == "status" => {
                        let act_val = val.inner_html();
                        if act_val == "-" {
                            continue;
                        }
                        mng.status = act_val.trim().to_uppercase();
                    }
                    _ => {}
                }
            }
        }

        let mtdta = tables.next();

        if let Some(_x) = mtdta {}
    }

    //     if let Some(x) = doc.select(&SUMMARY_SELECTOR).next() {
    //         mng.description
    //             .extend(x.select(&DESCRIPTION_SELECTOR).flat_map(|f| f.text()));

    //         mng.description = mng.description.trim().to_string();

    //         mng.genres.extend(
    //             x.select(&MANGA_GENRE_SELECTOR)
    //                 .filter_map(|f| map.get(f.text().collect::<String>().to_lowercase().trim())),
    //         );
    //     }

    //     if let Some(x) = doc.select(&STATUS_SELECTOR).next() {
    //         mng.status.extend(x.text());
    //         mng.status = mng.status.trim().to_uppercase();
    //     } else {
    //         mng.status.push_str("Not Available");
    //     }

    //     if let Some(x) = doc.select(&AUTHOR_SELECTOR).next() {
    //         mng.authors
    //             .push(x.text().collect::<String>().trim().to_string());
    //     }

    //     if let Some(x) = doc.select(&ARTIST_SELECTOR).next() {
    //         mng.artists
    //             .push(x.text().collect::<String>().trim().to_string());
    //     }

    //     for (idx, i) in doc.select(&CHAPTER_SELECTOR).enumerate() {
    //         if let Some(x) = i.value().attr("href") {
    //             let mut t = ChapterTable {
    //                 sequence_number: idx as i32,
    //                 last_watch_time: Utc::now().timestamp_millis(),
    //                 ..Default::default()
    //             };
    //             let mut r = String::from(WEBSITE_HOST);
    //             r.push_str(x);

    //             t.chapter_id = r.to_string();

    //             mng.chapters.push(t);
    //         }
    //     }
    // }

    // {
    //     for yt in mng.chapters.iter_mut() {
    //         let r = yt.chapter_id.clone();
    //         populate_chapter(yt, r.as_str()).await;
    //     }

    //     mng.chapters.reverse();

    //     let sz = mng.chapters.len() as i32;

    //     for t in mng.chapters.iter_mut() {
    //         t.sequence_number = sz - t.sequence_number - 1;
    //     }
    // }

    todo!()

    // Ok(mng)
}
