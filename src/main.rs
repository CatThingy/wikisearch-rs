mod search;
use regex::Regex;
use rusqlite::Connection;
use search::{search, Search, DATABASE_LOCATION};
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
use std::{env, fs::create_dir_all};

use lazy_static::lazy_static;

struct Handler;

lazy_static! {
    static ref QUERY_REGEX: Regex =
        Regex::new(r"\[\[(?:(?P<wiki>.+)\|)?(?P<query>.+?)\|?\]\]").unwrap();
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
                    Err(e) => println!("{}", e),
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
