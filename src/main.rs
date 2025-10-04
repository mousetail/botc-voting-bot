mod commands;
mod state;
use std::{fmt::Display, fs::OpenOptions};

use crate::{
    commands::{assign_player_to_cottage, set_accusation, set_number_of_players, start_vote},
    state::State,
};
use commands::{raise_hand, set_defense, vote};
use poise::serenity_prelude::{
    self as serenity, ComponentInteractionDataKind, CreateInteractionResponseMessage, EditMessage,
    GuildId, Interaction, RoleId,
};
use serde::Deserialize;
use state::format_vote;
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
            on_error: |err| Box::pin(async move { poise::builtins::on_error(err).await.unwrap() }),
            event_handler: |ctx, event, framework, state| {
                Box::pin(async move { event_handler(ctx, event, framework, state).await })
            },

            commands: vec![
                start_vote(),
                set_number_of_players(),
                assign_player_to_cottage(),
                set_accusation(),
                set_defense(),
                raise_hand(),
                vote(),
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

async fn event_handler<'a>(
    ctx: &'a poise::serenity_prelude::Context,
    event: &'a serenity::FullEvent,
    _framework: poise::FrameworkContext<'a, (Config, RwLock<State>), Error>,
    state: &'a (Config, RwLock<State>),
) -> Result<(), Error> {
    if let serenity::FullEvent::InteractionCreate {
        interaction: Interaction::Component(component_interaction),
    } = event
        && let ComponentInteractionDataKind::Button = component_interaction.data.kind
    {
        let up = component_interaction.data.custom_id == "hand_up_button";
        println!("Received a hand {up} up response");

        let mut ok = false;
        {
            let State {
                players,
                current_vote,
                ..
            } = &mut *state.1.write().await;

            if let Some(vote) = current_vote {
                let v = vote
                    .vote_state
                    .entry(component_interaction.user.id)
                    .or_insert(state::VoteState::None);

                match v {
                    state::VoteState::Yes | state::VoteState::No => (),
                    e => {
                        *e = if up {
                            state::VoteState::HandRaised
                        } else {
                            state::VoteState::HandLowered
                        }
                    }
                }

                let mut message = ctx
                    .http
                    .get_message(vote.channel_id, vote.message_id)
                    .await?;
                message
                    .edit(ctx, EditMessage::new().content(format_vote(players, vote)))
                    .await?;

                ok = true;
                component_interaction
                    .create_response(
                        ctx,
                        serenity::CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new(),
                        ),
                    )
                    .await?;
            }
        }

        state.1.read().await.save();

        if !ok {
            component_interaction
                .create_response(
                    ctx,
                    serenity::CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new(),
                    ),
                )
                .await?;
        }
    }

    Ok(())
}
