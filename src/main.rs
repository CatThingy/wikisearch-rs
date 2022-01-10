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
        guild::GuildUnavailable,
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

fn init_server(server: &str) {
    let connection = Connection::open(DATABASE_LOCATION).unwrap();
    match connection
        .query_row(
            "SELECT count(*) FROM config WHERE server = :server",
            &[(":server", server)],
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

            let mut statement = connection
                .prepare(
                    "
                        INSERT INTO config VALUES (:server, :alias, :endpoint)
                        ",
                )
                .unwrap();

            for value in default_values.into_iter() {
                statement
                    .execute(&[
                        (":server", server),
                        (":alias", value.0),
                        (":endpoint", value.1),
                    ])
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
            init_server(&server);

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
        if let Err(why) = ApplicationCommand::set_global_application_commands(
            &ctx.http,
            |commands| {
                commands
                    .create_application_command(|command| {
                        command
                            .name("endpoint")
                            .description("Modify server wiki endpoints")
                            .create_option(|option| {
                                option
                                    .name("set")
                                    .description("Add or edit an endpoint")
                                    .kind(ApplicationCommandOptionType::SubCommand)
                                    .create_sub_option(|subopt| {
                                        subopt
                                            .name("alias")
                                            .description(
                                                "Alias to use for the endpoint i.e. [[alias|query]]",
                                            )
                                            .required(true)
                                            .kind(ApplicationCommandOptionType::String)
                                    })
                                    .create_sub_option(|subopt| {
                                        subopt
                                            .name("url")
                                            .description("The API endpoint associated with the alias.\nThis ends in /api.php")
                                            .required(true)
                                            .kind(ApplicationCommandOptionType::String)
                                    })
                            })
                            .create_option(|option| {
                                option
                                    .name("delete")
                                    .description("Delete an endpoint")
                                    .kind(ApplicationCommandOptionType::SubCommand)
                                    .create_sub_option(|subopt| {
                                        subopt
                                            .name("alias")
                                            .description("Alias of the endpoint to delete")
                                            .required(true)
                                            .kind(ApplicationCommandOptionType::String)
                                    })
                            })
                            .create_option(|option| {
                                option
                                    .name("list")
                                    .description("List all available endpoints")
                                    .kind(ApplicationCommandOptionType::SubCommand)
                            })
                },
            )
        }).await {
            println!("Error: {}", why);
        };
    }

    async fn guild_delete(&self, _: Context, guild: GuildUnavailable) {
        let server = format!("s{}", guild.id);
        let connection = Connection::open(DATABASE_LOCATION).unwrap();
        match connection.execute(
            "DELETE FROM config WHERE server = :server",
            &[(":server", &server)],
        ) {
            Err(e) => println!("{}", e),
            _ => {}
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
                        init_server(server);
                        match subcmd.name.as_str() {
                            "set" => {
                                match set_endpoint(&options["alias"], &options["endpoint"], server)
                                {
                                    Ok(v) => v,
                                    Err(e) => format!("An error occured: {:?}", e),
                                }
                            }
                            "delete" => match delete_endpoint(&options["alias"], server) {
                                Ok(v) => v,
                                Err(e) => format!("An error occured: {:?}", e),
                            },
                            "list" => match all_endpoints(server) {
                                Ok(v) => v,
                                Err(e) => format!("An error occured: {:?}", e),
                            },
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
    create_dir_all("data").unwrap();
    let connection = Connection::open(DATABASE_LOCATION).unwrap();

    connection.execute("CREATE TABLE IF NOT EXISTS config (server TEXT NOT NULL, alias TEXT NOT NULL, endpoint TEXT NOT NULL, PRIMARY KEY (server, alias))", []).unwrap();

    let token = env::var("WIKISEARCH_TOKEN").expect("give me a token man");
    let application_id = env::var("WIKISEARCH_ID")
        .expect("give me an id man")
        .parse()
        .expect("that's not a valid id man");
    let mut client = Client::builder(&token)
        .event_handler(Handler)
        .application_id(application_id)
        .intents(GatewayIntents::GUILD_MESSAGES | GatewayIntents::GUILDS)
        .await
        .expect("can't create client");
    if let Err(why) = client.start().await {
        println!("Error: {:?}", why)
    }
}
