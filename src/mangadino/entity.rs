use std::collections::HashSet;

use isahc::prelude::*;
use lazy_static::lazy_static;
use mangaverse_entity::models::source::SourceTable;
use scraper::{Html, Selector};
use sqlx::MySqlPool;

use crate::{readm::insert_source_if_not_exists, Result};

const SOURCE_NAME: &str = "mangadino";

lazy_static! {
    static ref GENRE_SELECTOR: Selector = Selector::parse("select[name='genre'] > option").unwrap();
    static ref NAME_SELECTOR: Selector = Selector::parse("h1.page-title").unwrap();
    static ref COVERURL_SELECTOR: Selector = Selector::parse("img.series-profile-thumb").unwrap();
    static ref METADATA_LABEL_SELECTOR: Selector = Selector::parse("td.table-label").unwrap();
    static ref METADATA_VALUE_SELECTOR: Selector = Selector::parse("td.table-value").unwrap();
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

    let response_text = isahc::get_async(url).await?.text().await?;

    let doc = Html::parse_document(&response_text);

    Ok(doc
        .select(&GENRE_SELECTOR)
        .skip(1)
        .map(|f| f.text().collect::<String>().trim().to_lowercase())
        .collect())
}

pub async fn get_mangadino_source(pool: &MySqlPool) -> Result<SourceTable> {
    insert_source_if_not_exists(SOURCE_NAME, 3, pool).await
}
