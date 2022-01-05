use std::{env, error::Error};
use regex::Regex;
use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*, builder::{CreateEmbed, CreateMessage}
};

use percent_encoding::{utf8_percent_encode, CONTROLS};
use unescape::unescape;

struct Handler;

async fn search(wiki: &str, query: &str, client: &reqwest::Client) -> Result<CreateEmbed, Box<dyn Error>> {
    let api_title_regex = Regex::new(r#""title":"(.+?)".*"#).unwrap();
    // Backslash match at the end to prevent panic when unescaping unicode
    let api_excerpt_regex = Regex::new(r#""extract":"(.+?)\\?""#).unwrap();

    let page_title_regex = Regex::new(r"<title>(.*) - Wikipedia</title>").unwrap();
    let page_thumbnail_regex= Regex::new(r#"<meta property="og:image" content="(.+?)""#).unwrap();

    let search_url = format!("{}/w/api.php?action=query&format=json&list=search&formatversion=2&srwhat=nearmatch&srinfo=&srprop=&srsearch={}", wiki, query); 
    let mut info_url = format!("{}/w/api.php?format=json&action=query&prop=extracts&exchars=300&explaintext&redirects=1&titles=", wiki);

    let body = client.get(&search_url).send()
        .await?.text()
        .await?; 

    let mut e = CreateEmbed::default();
    match api_title_regex.captures(&body) {
        Some(v) => {
            let page_url = format!("{}/wiki/{}", wiki, utf8_percent_encode(&v[1], CONTROLS));
            let page_text = client.get(&page_url).send()
                .await?.text()
                .await?; 

            let page_title = match page_title_regex.captures(&page_text) {
                Some(v) => String::from(&v[1]),
                None => String::from("")
            };

            info_url.push_str(&page_title);

            let page_excerpt = client.get(&info_url).send()
                .await?.text()
                .await?; 

            e.title(&page_title);
            e.url(&page_url);
            e.description(match &api_excerpt_regex.captures(&page_excerpt) {
                Some(v) => String::from(unescape(&v[1]).unwrap()),
                None => String::from("")
            });
            e.thumbnail(match page_thumbnail_regex.captures(&page_text) {
                Some(v) => String::from(unescape(&v[1]).unwrap()),
                None => String::from("")
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
            return
        }

        let query_regex = Regex::new(r"\[\[(.+?)\]\]").unwrap(); 
        let mut embeds = Vec::<CreateEmbed>::new();
        let client = reqwest::Client::new();
        for capture in query_regex.captures_iter(&msg.content) {
            match search("https://en.wikipedia.org", &capture[1], &client).await {
                Ok(v) => {    
                    embeds.push(v);
                }
                Err(e) => println!("{:?}", e)
            };
        }

        if embeds.len() > 0 {
            if let Err(why) = msg.channel_id.send_message(&ctx.http, |m| {
                m.set_embeds(embeds);
                m
            }).await {
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
    let mut client = Client::builder(&token).event_handler(Handler).await.expect("can't create client");
    if let Err(why) = client.start().await {
        println!("Error: {:?}", why)
    }
}
