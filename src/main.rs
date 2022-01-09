mod search;
mod slash_commands;
use regex::Regex;
use rusqlite::Connection;
use search::{search, Search, DATABASE_LOCATION};
use serenity::{
    async_trait,
    builder::CreateEmbed,
    client::bridge::gateway::GatewayIntents,
    model::{
        channel::Message,
        gateway::Ready,
        guild::{Guild, GuildUnavailable},
        id::GuildId,
        interactions::{
            application_command::{
                ApplicationCommand, ApplicationCommandInteractionDataOptionValue,
                ApplicationCommandOptionType,
            },
            Interaction, InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
        },
    },
    prelude::*,
};
use slash_commands::{all_endpoints, delete_endpoint, set_endpoint};
use std::{collections::HashMap, env, fs::create_dir_all};

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
                ("default", "https://en.wikipedia.org/w/api.php"),
                // Top 10 wikipedias
                ("en", "https://en.wikipedia.org/w/api.php"),
                ("de", "https://de.wikipedia.org/w/api.php"),
                ("fr", "https://fr.wikipedia.org/w/api.php"),
                ("ja", "https://ja.wikipedia.org/w/api.php"),
                ("es", "https://es.wikipedia.org/w/api.php"),
                ("ru", "https://ru.wikipedia.org/w/api.php"),
                ("pt", "https://pt.wikipedia.org/w/api.php"),
                ("zh", "https://zh.wikipedia.org/w/api.php"),
                ("it", "https://it.wikipedia.org/w/api.php"),
                ("ar", "https://ar.wikipedia.org/w/api.php"),
            ];
            connection
                .execute(
                    format!(
                        "CREATE TABLE IF NOT EXISTS {} 
                    (
                        alias TEXT NOT NULL UNIQUE,
                        endpoint TEXT
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
                        INSERT INTO {} VALUES (:alias, :endpoint)
                        ",
                        name
                    )
                    .as_str(),
                )
                .unwrap();

            for value in default_values.into_iter() {
                statement
                    .execute(&[(":alias", value.0), (":endpoint", value.1)])
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

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} connected succesfully", ready.user.name);

        if let Err(why) = ApplicationCommand::create_global_application_command(
            &ctx.http,
            |command| {
                command
                    .name("alias")
                    .description("Modify server wiki aliases")
                    .create_option(|option| {
                        option
                            .name("set")
                            .description("Add or edit an alias")
                            .kind(ApplicationCommandOptionType::SubCommand)
                            .create_sub_option(|subopt| {
                                subopt
                                    .name("alias")
                                    .required(true)
                                    .description(
                                        "Alias to use in a search i.e. [[alias|query]]",
                                    )
                                    .kind(ApplicationCommandOptionType::String)
                            })
                            .create_sub_option(|subopt| {
                                subopt
                                    .name("endpoint")
                                    .required(true)
                                    .description("The API endpoint associated with the alias.\nThis ends in /api.php")
                                    .kind(ApplicationCommandOptionType::String)
                            })
                    })
                    .create_option(|option| {
                        option
                            .name("delete")
                            .description("Delete an alias")
                            .kind(ApplicationCommandOptionType::SubCommand)
                            .create_sub_option(|subopt| {
                                subopt
                                    .name("alias")
                                    .description("Alias to delete")
                                    .kind(ApplicationCommandOptionType::String)
                            })
                    })
                    .create_option(|option| {
                        option
                            .name("list")
                            .description("List all aliases")
                            .kind(ApplicationCommandOptionType::SubCommand)
                    })
            },
        )
        .await{
            println!("e: {}", why)
        };
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

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            if let Err(why) = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| {
                            message
                                .content("Processing...")
                                .flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
                        })
                })
                .await
            {
                println!("Cannot respond to slash command: {}", why);
            } else {
                let content = match command.data.name.as_str() {
                    "alias" => {
                        let subcmd = &command.data.options[0];
                        let mut options = HashMap::<String, String>::new();
                        let server = &format!("s{}", command.guild_id.unwrap().to_string());
                        for option in &subcmd.options {
                            if let ApplicationCommandInteractionDataOptionValue::String(value) =
                                option
                                    .resolved
                                    .as_ref()
                                    .expect("Incorrect command definition")
                            {
                                options.insert(option.name.to_string(), value.to_string());
                            }
                        }
                        match subcmd.name.as_str() {
                            "set" => {
                                set_endpoint(&options["alias"], &options["endpoint"], server);
                                format!(
                                    "Added `{}` as an alias for `{}`",
                                    &options["alias"], &options["endpoint"]
                                )
                            }
                            "delete" => {
                                delete_endpoint(&options["alias"], server);
                                format!("Deleted alias `{}`", options["alias"])
                            }
                            "list" => all_endpoints(server),
                            _ => "Command not recognized".to_string(),
                        }
                    }

                    _ => "Not implemented".to_string(),
                };
                command
                    .edit_original_interaction_response(&ctx.http, |response| {
                        response.content(content)
                    })
                    .await
                    .unwrap();
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let token = env::var("WIKISEARCH_TOKEN").expect("give me a token man");
    let application_id = env::var("WIKISEARCH_ID")
        .expect("give me an id man")
        .parse()
        .expect("that's not a valid id man");
    let mut client = Client::builder(&token)
        .event_handler(Handler)
        .application_id(application_id)
        .intents(GatewayIntents::GUILD_MESSAGES)
        .await
        .expect("can't create client");
    if let Err(why) = client.start().await {
        println!("Error: {:?}", why)
    }
}
