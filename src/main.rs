use std::env;
use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*
};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content == "!ping" {
            if let Err(why) = msg.channel_id.say(&ctx.http, "hello from rust").await {
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
        println!("Error: {:?}", why);
    }
}
