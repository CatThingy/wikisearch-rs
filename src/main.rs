use regex::Regex;
use serenity::{
    async_trait,
    builder::CreateEmbed,
    model::{channel::Message, gateway::Ready},
    prelude::*,
};
use std::{env, error::Error};

use lazy_static::lazy_static;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use unescape::unescape;

struct Handler;

lazy_static! {
    static ref API_TITLE_REGEX: Regex = Regex::new(r#""title":"(.+?)".*"#).unwrap();
    // Backslash match at the end to prevent panic when unescaping unicode
    static ref API_EXCERPT_REGEX: Regex = Regex::new(r#""extract":"(.+?)\\?""#).unwrap();
    static ref PAGE_TITLE_REGEX: Regex = Regex::new(r"<title>(.*) - Wikipedia</title>").unwrap();
    static ref PAGE_THUMBNAIL_REGEX: Regex = Regex::new(r#"<meta property="og:image" content="(.+?)""#).unwrap();
    static ref QUERY_REGEX: Regex = Regex::new(r"\[\[(.+?)\]\]").unwrap();
}

async fn search(wiki: &str, query: &str, client: &reqwest::Client) -> Result<CreateEmbed, Box<dyn Error>> {
    let search_url = format!("{}/w/api.php?action=query&format=json&list=search&formatversion=2&srwhat=nearmatch&srinfo=&srprop=&srsearch={}", wiki, query);
    let mut info_url = format!("{}/w/api.php?format=json&action=query&prop=extracts&exchars=300&explaintext&redirects=1&titles=", wiki);

    let body = client.get(&search_url).send().await?.text().await?;

    let mut e = CreateEmbed::default();
    match API_TITLE_REGEX.captures(&body) {
        Some(v) => {
            let page_url = format!(
                "{}/wiki/{}",
                wiki,
                utf8_percent_encode(&v[1], NON_ALPHANUMERIC)
            );
            let page_text = client.get(&page_url).send().await?.text().await?;

            let page_title = match PAGE_TITLE_REGEX.captures(&page_text) {
                Some(v) => String::from(&v[1]),
                None => String::from(""),
            };

            info_url.push_str(&page_title);

            let page_excerpt = client.get(&info_url).send().await?.text().await?;

            e.title(&page_title);
            e.url(&page_url);
            e.description(match &API_EXCERPT_REGEX.captures(&page_excerpt) {
                Some(v) => String::from(unescape(&v[1]).unwrap()),
                None => String::from(""),
            });
            e.thumbnail(match PAGE_THUMBNAIL_REGEX.captures(&page_text) {
                Some(v) => String::from(unescape(&v[1]).unwrap()),
                None => String::from(""),
            });
        }
        None => {
            e.title(format!("No results found for {}", query));
        }
    }
    Ok(e)
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        let mut embeds = Vec::<CreateEmbed>::new();
        let client = reqwest::Client::new();

        if QUERY_REGEX.is_match(&msg.content) {
            let captures = QUERY_REGEX.captures_iter(&msg.content);
            let enumerated_captures = QUERY_REGEX.captures_iter(&msg.content).enumerate();

            for capture in captures {
                let mut e = CreateEmbed::default();
                e.title(format!("Searching for {}...", &capture[1]));
                embeds.push(e);
            }

            let mut message = match msg
                .channel_id
                .send_message(&ctx.http, |m| {
                    m.set_embeds(embeds.clone());
                    m
                })
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    println!("{:?}", e);
                    return;
                }
            };

            for (i, capture) in enumerated_captures {
                match search("https://en.wikipedia.org", &capture[1], &client).await {
                    Ok(v) => {
                        embeds[i] = v;
                    }
                    Err(e) => println!("{:?}", e),
                };
            }

            if let Err(why) = message
                .edit(&ctx.http, |m| {
                    m.set_embeds(embeds);
                    m
                })
                .await
            {
                println!("damnit: {:?}", why);
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} connected succesfully", ready.user.name);
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
