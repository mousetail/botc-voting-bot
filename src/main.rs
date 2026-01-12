mod commands;
mod state;
use std::{
    fmt::Display,
    fs::{File, OpenOptions},
    io::Write,
};

use crate::{
    commands::{assign_player_to_cottage, set_accusation, set_number_of_players, start_vote},
    state::State,
};
use commands::{raise_hand, set_defense, vote};
use poise::{
    FrameworkError,
    serenity_prelude::{
        self as serenity, ComponentInteractionDataKind, CreateInteractionResponseMessage,
        EditMessage, GuildId, Interaction, RoleId, futures::lock::Mutex,
    },
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

type DiscordState = (Config, RwLock<State>, Mutex<File>);
type Context<'a> = poise::Context<'a, DiscordState, Error>;

#[derive(Deserialize, Debug)]
struct Config {
    token: String,
    guild_id: GuildId,
    storyteller_role: RoleId,
    dead_role: RoleId,
    ghost_vote_available_role: RoleId,
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

    let framework = poise::Framework::<DiscordState, _>::builder()
        .setup(move |ctx, _, framework| {
            Box::pin(async move {
                let commands =
                    poise::builtins::create_application_commands(&framework.options().commands);

                config.guild_id.set_commands(ctx, commands).await.unwrap();
                Ok((
                    config,
                    RwLock::new(state),
                    Mutex::new(
                        OpenOptions::new()
                            .append(true)
                            .create(true)
                            .open("message_log.jsonl")
                            .unwrap(),
                    ),
                ))
            })
        })
        .options(poise::FrameworkOptions {
            on_error: |err| {
                Box::pin(async move {
                    match err {
                        FrameworkError::CommandCheckFailed { .. } => (),
                        err => poise::builtins::on_error(err).await.unwrap(),
                    }
                })
            },
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
    _framework: poise::FrameworkContext<'a, DiscordState, Error>,
    state: &'a DiscordState,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Message { new_message } => {
            let mut value = state.2.lock().await;

            let mut message = serde_json::to_vec(&(
                "new_message",
                std::time::SystemTime::now(),
                new_message.id,
                &new_message.content,
                &new_message.author,
                new_message.channel_id,
                new_message.thread.as_ref().map(|i| (i.id, &i.name)),
                new_message
                    .attachments
                    .iter()
                    .map(|i| &i.url)
                    .collect::<Vec<_>>(),
            ))
            .unwrap();
            message.push(10);
            value.write_all(&message).unwrap();
        }
        serenity::FullEvent::MessageUpdate {
            old_if_available: _,
            new: _,
            event: ev,
        } => {
            let mut value = state.2.lock().await;

            let mut message = serde_json::to_vec(&(
                "message_edit",
                std::time::SystemTime::now(),
                ev.id,
                ev.id,
                &ev.content,
                &ev.author,
                &ev.channel_id,
                &ev.reactions,
            ))
            .unwrap();
            message.push(10);
            value.write_all(&message).unwrap();
        }
        serenity::FullEvent::MessageDelete {
            channel_id,
            deleted_message_id,
            guild_id,
        } => {
            let mut value = state.2.lock().await;

            let mut message = serde_json::to_vec(&(
                "message_delete",
                std::time::SystemTime::now(),
                channel_id,
                deleted_message_id,
                guild_id,
            ))
            .unwrap();

            message.push(10);
            value.write_all(&message).unwrap();
        }
        serenity::FullEvent::ReactionAdd { add_reaction } => {
            let mut value = state.2.lock().await;

            let mut message = serde_json::to_vec(&(
                "reaction_add",
                std::time::SystemTime::now(),
                add_reaction.channel_id,
                &add_reaction.emoji,
                add_reaction.message_id,
                add_reaction.user_id,
            ))
            .unwrap();

            message.push(10);
            value.write_all(&message).unwrap();
        }
        serenity::FullEvent::ReactionRemove { removed_reaction } => {
            let mut value = state.2.lock().await;

            let mut message = serde_json::to_vec(&(
                "reaction_remove",
                std::time::SystemTime::now(),
                removed_reaction.channel_id,
                &removed_reaction.emoji,
                removed_reaction.message_id,
                removed_reaction.user_id,
            ))
            .unwrap();

            message.push(10);
            value.write_all(&message).unwrap();
        }
        serenity::FullEvent::ThreadCreate { thread } => {
            let mut value = state.2.lock().await;

            let mut message = serde_json::to_vec(&(
                "thread_create",
                std::time::SystemTime::now(),
                thread.id,
                &thread.name,
                &thread.thread_metadata,
            ))
            .unwrap();

            message.push(10);
            value.write_all(&message).unwrap();
        }
        serenity::FullEvent::ThreadMemberUpdate { thread_member } => {
            let mut value = state.2.lock().await;

            let mut message = serde_json::to_vec(&(
                "thread_member_update",
                std::time::SystemTime::now(),
                thread_member.id,
                thread_member.user_id,
                thread_member.inner.join_timestamp,
            ))
            .unwrap();

            message.push(10);
            value.write_all(&message).unwrap();
        }
        serenity::FullEvent::InteractionCreate {
            interaction: Interaction::Component(component_interaction),
        } => {
            if let ComponentInteractionDataKind::Button = component_interaction.data.kind {
                let up = component_interaction.data.custom_id == "hand_up_button";
                println!("Received a hand {up} up response");

                let mut ok = false;
                {
                    let State {
                        players,
                        current_vote,
                        number_of_players,
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
                            .edit(
                                ctx,
                                EditMessage::new().content(format_vote(
                                    players,
                                    vote,
                                    *number_of_players,
                                )),
                            )
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
        }
        _ => (),
    }

    Ok(())
}
