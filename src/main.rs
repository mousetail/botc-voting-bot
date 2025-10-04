mod commands;
mod state;
use std::{fmt::Display, fs::OpenOptions};

use crate::{
    commands::{assign_player_to_cottage, set_accusation, set_number_of_players, start_vote},
    state::State,
};
use commands::set_defense;
use poise::serenity_prelude::{self as serenity, GuildId, RoleId};
use serde::Deserialize;
use tokio::sync::RwLock;

#[derive(Debug)]
enum Error {
    Serenity(#[allow(unused)] poise::serenity_prelude::Error),
    Silent,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl From<poise::serenity_prelude::Error> for Error {
    fn from(value: poise::serenity_prelude::Error) -> Self {
        Error::Serenity(value)
    }
}
type Context<'a> = poise::Context<'a, (Config, RwLock<state::State>), Error>;

#[derive(Deserialize)]
struct Config {
    token: String,
    guild_id: GuildId,
    storyteller_role: RoleId,
}

fn get_initial_state() -> state::State {
    let reader = match OpenOptions::new().read(true).open("state.yaml") {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return State::default(),
        k => k.unwrap(),
    };
    serde_yml::from_reader(reader).unwrap()
}

#[tokio::main]
async fn main() {
    let config: Config =
        serde_yml::from_reader(OpenOptions::new().read(true).open("config.yaml").unwrap()).unwrap();
    let state = get_initial_state();

    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT;

    let token = config.token.clone();

    let framework = poise::Framework::<(Config, RwLock<State>), _>::builder()
        .setup(move |ctx, _, framework| {
            Box::pin(async move {
                let commands =
                    poise::builtins::create_application_commands(&framework.options().commands);

                config.guild_id.set_commands(ctx, commands).await.unwrap();
                Ok((config, RwLock::new(state)))
            })
        })
        .options(poise::FrameworkOptions {
            on_error: |err| {
                Box::pin(async move {
                    match err {
                        err => poise::builtins::on_error(err).await.unwrap(),
                    }
                })
            },

            commands: vec![
                start_vote(),
                set_number_of_players(),
                assign_player_to_cottage(),
                set_accusation(),
                set_defense(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .build();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}
