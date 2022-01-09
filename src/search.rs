use lazy_static::lazy_static;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use regex::Regex;
use rusqlite::Connection;
use serenity::builder::CreateEmbed;
use std::error::Error;
use unescape::unescape;

pub static DATABASE_LOCATION: &str = "data/config.db";

pub struct Search {
    pub alias: Option<String>,
    pub query: String,
}

lazy_static! {
    static ref API_TITLE_REGEX: Regex = Regex::new(r#""title":"(?P<title>.+?)".*"#).unwrap();
    // Backslash match at the end to prevent panic when unescaping unicode
    static ref API_EXCERPT_REGEX: Regex = Regex::new(r#""extract":"(?P<summary>.+?)\\?""#).unwrap();
    static ref PAGE_THUMBNAIL_REGEX: Regex = Regex::new(r#"<meta property="og:image" content="(?P<thumbnail>.+?)""#).unwrap();
}

pub async fn search(
    search: Search,
    client: &reqwest::Client,
    server: &String,
) -> Result<CreateEmbed, Box<dyn Error>> {
    let wiki = get_wiki(search.alias, server).unwrap_or("https://en.wikipedia.org".to_string());
    let search_url = format!(
        "{}/w/api.php?action=query&format=json&list=search&formatversion=2&srwhat=text&srinfo=&srprop=&srlimit=1&srsearch={}",
        wiki,
        &utf8_percent_encode(&search.query, NON_ALPHANUMERIC).collect::<String>()
    );
    let mut info_url = format!("{}/w/api.php?format=json&action=query&prop=extracts&exchars=500&explaintext&redirects=1&titles=", wiki);

    let body = client.get(&search_url).send().await?.text().await?;

    let mut e = CreateEmbed::default();
    match API_TITLE_REGEX.captures(&body) {
        Some(v) => {
            let page_url = format!(
                "{}/wiki/{}",
                wiki,
                &utf8_percent_encode(&v["title"], NON_ALPHANUMERIC).collect::<String>()
            );
            let page_text = client.get(&page_url).send().await?.text().await?;

            info_url
                .push_str(&utf8_percent_encode(&v["title"], NON_ALPHANUMERIC).collect::<String>());

            let page_excerpt = client.get(&info_url).send().await?.text().await?;

            e.title(&v["title"]);
            e.url(&page_url);
            e.description(match &API_EXCERPT_REGEX.captures(&page_excerpt) {
                Some(v) => {
                    unescape(&v["summary"]).unwrap_or("No summary could be found".to_string())
                }
                None => String::from("No summary could be found"),
            });
            e.thumbnail(match PAGE_THUMBNAIL_REGEX.captures(&page_text) {
                Some(v) => String::from(unescape(&v["thumbnail"]).unwrap()),
                None => String::from(""),
            });
        }
        None => {
            e.title(format!("No results found for {}", &search.query));
        }
    }
    Ok(e)
}

fn get_wiki(alias: Option<String>, server: &String) -> Option<String> {
    let connection = rusqlite::Connection::open(DATABASE_LOCATION).unwrap();
    let mut statement = connection
        .prepare(format!("SELECT wiki FROM {} WHERE alias = :alias", server).as_str())
        .unwrap();
    let result = statement.query_row(
        &[(":alias", &alias.unwrap_or("default".to_string()))],
        |row| Ok(row.get::<_, String>(0).unwrap().to_string()),
    );

    match result {
        Ok(v) => return Some(v),
        Err(_) => return None,
    }
}

pub fn set_wiki(alias: String, wiki: String, server: &String) {
    let connection = Connection::open(DATABASE_LOCATION).unwrap();
    let mut statement = connection
        .prepare(
            format!(
                "UPDATE {}
                SET wiki = :wiki
                WHERE alias = :alias",
                server
            )
            .as_str(),
        )
        .unwrap();
    statement
        .execute(&[(":alias", &alias), (":wiki", &wiki)])
        .unwrap();

    statement = connection
        .prepare(
            format!(
                "INSERT OR IGNORE INTO {}
                (alias, wiki) VALUES (:alias, :wiki)",
                server
            )
            .as_str(),
        )
        .unwrap();
    statement
        .execute(&[(":alias", &alias), (":wiki", &wiki)])
        .unwrap();
}

pub fn delete_wiki(alias: String, server: &String) {
    let connection = Connection::open(DATABASE_LOCATION).unwrap();
    let mut statement = connection
        .prepare(
            format!(
                "DELETE FROM {}
                WHERE alias = :alias",
                server
            )
            .as_str(),
        )
        .unwrap();
    statement.execute(&[(":alias", &alias)]).unwrap();
}
