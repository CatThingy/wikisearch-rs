use regex::Regex;
use rusqlite::Connection;
use serenity::{
    async_trait,
    builder::CreateEmbed,
    model::{
        channel::Message,
        gateway::Ready,
        guild::{Guild, GuildUnavailable},
    },
    prelude::*,
};
use std::{env, error::Error, fs::create_dir_all};

use lazy_static::lazy_static;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use unescape::unescape;

static DATABASE_LOCATION: &str = "data/config.db";

struct Handler;

struct Search {
    alias: Option<String>,
    query: String,
}

lazy_static! {
    static ref QUERY_REGEX: Regex = Regex::new(r"\[\[(?:(?P<wiki>.+)\|)?(?P<query>.+?)\|?\]\]").unwrap();
    static ref API_TITLE_REGEX: Regex = Regex::new(r#""title":"(?P<title>.+?)".*"#).unwrap();
    // Backslash match at the end to prevent panic when unescaping unicode
    static ref API_EXCERPT_REGEX: Regex = Regex::new(r#""extract":"(?P<summary>.+?)\\?""#).unwrap();
    static ref PAGE_THUMBNAIL_REGEX: Regex = Regex::new(r#"<meta property="og:image" content="(?P<thumbnail>.+?)""#).unwrap();
}

async fn search(
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

fn init_table(name: &str) {
    create_dir_all("data").unwrap();
    let connection = Connection::open(DATABASE_LOCATION).unwrap();
    match connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=:name",
            &[(":name", name)],
            |row| row.get(0),
        )
        .expect("err:")
    {
        0 => {
            let default_values = vec![
                ("default", "https://en.wikipedia.org"),
                // Top 10 wikipedias
                ("en", "https://en.wikipedia.org"),
                ("de", "https://de.wikipedia.org"),
                ("fr", "https://fr.wikipedia.org"),
                ("ja", "https://ja.wikipedia.org"),
                ("es", "https://es.wikipedia.org"),
                ("ru", "https://ru.wikipedia.org"),
                ("pt", "https://pt.wikipedia.org"),
                ("zh", "https://zh.wikipedia.org"),
                ("it", "https://it.wikipedia.org"),
                ("ar", "https://ar.wikipedia.org"),
            ];
            connection
                .execute(
                    format!(
                        "CREATE TABLE IF NOT EXISTS {} 
                    (
                        alias TEXT NOT NULL UNIQUE,
                        wiki TEXT
                    )",
                        name
                    )
                    .as_str(),
                    [],
                )
                .unwrap();

            let mut statement = connection
                .prepare(
                    format!(
                        "
                    INSERT INTO {} VALUES (:alias, :wiki)
                ",
                        name
                    )
                    .as_str(),
                )
                .unwrap();

            for value in default_values.into_iter() {
                statement
                    .execute(&[(":alias", value.0), (":wiki", value.1)])
                    .unwrap();
            }
        }
        _ => {}
    }
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

fn set_wiki(alias: String, wiki: String, server: &String) {
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

fn delete_wiki(alias: String, server: &String) {
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
    statement
        .execute(&[(":alias", &alias)])
        .unwrap();
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        let server = match &msg.guild_id {
            Some(v) => format!("s{}", v.to_string()),
            None => "default".to_string(),
        };

        let mut embeds = Vec::<CreateEmbed>::new();
        let client = reqwest::Client::new();

        if QUERY_REGEX.is_match(&msg.content) {
            init_table(&server);

            let captures = QUERY_REGEX.captures_iter(&msg.content);
            let mut captured_text = Vec::<Search>::new();

            for capture in captures {
                let mut e = CreateEmbed::default();
                e.title(format!("Searching for {}...", &capture["query"]));
                embeds.push(e);
                captured_text.push(Search {
                    alias: match capture.get(1) {
                        Some(v) => Some(v.as_str().to_string()),
                        _ => None,
                    },
                    query: capture["query"].to_string(),
                });
            }

            let mut message = match msg
                .channel_id
                .send_message(&ctx.http, |m| m.set_embeds(embeds))
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    println!("{:?}", e);
                    return;
                }
            };

            embeds = Vec::<CreateEmbed>::new();

            for capture in captured_text {
                match search(capture, &client, &server).await {
                    Ok(v) => {
                        embeds.push(v);
                    }
                    Err(e) => println!("{:?}", e),
                };
            }

            if let Err(why) = message.edit(&ctx.http, |m| m.set_embeds(embeds)).await {
                println!("damnit: {:?}", why);
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} connected succesfully", ready.user.name);
    }

    async fn guild_delete(&self, _: Context, _: GuildUnavailable, guild: Option<Guild>) {
        match guild {
            Some(g) => {
                let server = format!("s{}", g.id);
                let connection = Connection::open(DATABASE_LOCATION).unwrap();
                match connection.execute(format!("DROP TABLE {}", server).as_str(), []) {
                    Err(e) => println!("{}",e),
                    _ => {}
                }
            }

            None => {}
        }
    }
}

#[tokio::main]
async fn main() {
    let token = env::var("WIKISEARCH_TOKEN").expect("give me a token man");
    let mut client = Client::builder(&token)
        .event_handler(Handler)
        .await
        .expect("can't create client");
    if let Err(why) = client.start().await {
        println!("Error: {:?}", why)
    }
}
