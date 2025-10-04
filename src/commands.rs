use std::{collections::HashMap, time::Duration};

use poise::{
    CreateReply,
    serenity_prelude::{
        ChannelId, CreateActionRow, CreateButton, EditMessage, ReactionType, UserId,
    },
};

use crate::{
    Context, Error,
    state::{
        CottageNumber, FormatMention, PlayerMap, PrintCottages, State, Vote, VoteState, format_vote,
    },
};

async fn is_storyteller(ctx: Context<'_>) -> Result<bool, Error> {
    if ctx
        .author()
        .has_role(ctx, ctx.guild_id().unwrap(), ctx.data().0.storyteller_role)
        .await?
    {
        Ok(true)
    } else {
        ctx.send(
            CreateReply::default()
                .ephemeral(true)
                .content("You must be a storyteller to use this command"),
        )
        .await?;

        Ok(false)
    }
}

async fn mutate_active_vote<T>(
    ctx: Context<'_>,
    callback: impl FnOnce(&mut PlayerMap, &mut Vote) -> Result<T, Error>,
) -> Result<T, Error> {
    let (_config, state) = ctx.data();

    let mut state = state.write().await;
    let State {
        players,
        current_vote,
        number_of_players,
        ..
    } = &mut *state;

    let vote = match current_vote {
        Some(e) => e,
        None => {
            ctx.send(
                CreateReply::default()
                    .ephemeral(true)
                    .content("There is no currently active vote"),
            )
            .await?;
            return Err(Error::Silent);
        }
    };

    let result = callback(players, vote)?;

    let mut message = ctx
        .http()
        .get_message(vote.channel_id, vote.message_id)
        .await?;
    message
        .edit(
            ctx,
            EditMessage::new().content(format_vote(players, vote, *number_of_players)),
        )
        .await?;

    state.save();

    Ok(result)
}

#[poise::command(prefix_command, slash_command, check = "is_storyteller")]
pub async fn set_number_of_players(ctx: Context<'_>, number_of_players: u32) -> Result<(), Error> {
    let (_config, state) = ctx.data();

    let mut state = state.write().await;
    state.number_of_players = number_of_players;
    state.save();
    drop(state);

    ctx.reply(format!("Number of players set to {}", number_of_players))
        .await?;

    Ok(())
}

#[poise::command(prefix_command, slash_command, check = "is_storyteller")]
pub async fn assign_player_to_cottage(
    ctx: Context<'_>,
    cottage_number: u32,
    player_id: UserId,
    channel_id: ChannelId,
) -> Result<(), Error> {
    let (_config, state) = ctx.data();

    let mut state = state.write().await;
    state.players.insert(
        CottageNumber::new(cottage_number).unwrap(),
        (player_id, channel_id),
    );
    state.save();
    let state = state.downgrade();

    // We first send a blank message then edit it to avoid pinging every player
    let message = ctx
        .reply(format!("**Current Cottage Assignment**:\n",))
        .await?;

    tokio::time::sleep(Duration::from_millis(250)).await;

    message.edit(
        ctx,
        CreateReply::default().content(format!(
            "**Current Cottage Assignment**:\n{}",
            PrintCottages(&state)
        )),
    );

    Ok(())
}

#[poise::command(prefix_command, slash_command, check = "is_storyteller")]
pub async fn set_defense(ctx: Context<'_>, defense: String) -> Result<(), Error> {
    mutate_active_vote(ctx, move |_, vote| -> Result<(), Error> {
        vote.defense = defense;

        Ok(())
    })
    .await?;

    ctx.send(
        CreateReply::default()
            .ephemeral(true)
            .content("Defense Set"),
    )
    .await?;

    //state.save();

    Ok(())
}

#[poise::command(prefix_command, slash_command, check = "is_storyteller")]
pub async fn set_accusation(ctx: Context<'_>, accusation: String) -> Result<(), Error> {
    mutate_active_vote(ctx, move |_, vote| -> Result<(), Error> {
        vote.accusation = accusation;
        Ok(())
    })
    .await?;

    ctx.send(
        CreateReply::default()
            .ephemeral(true)
            .content("Defense Set"),
    )
    .await?;

    Ok(())
}

#[poise::command(prefix_command, slash_command, check = "is_storyteller")]
pub async fn raise_hand(
    ctx: Context<'_>,
    player_id: UserId,
    hand_state: bool,
) -> Result<(), Error> {
    let success = mutate_active_vote(ctx, |_, vote| {
        let entry = vote.vote_state.entry(player_id).or_insert(VoteState::None);

        let success = match entry {
            VoteState::Yes | VoteState::No => false,
            e => {
                *e = if hand_state {
                    VoteState::HandRaised
                } else {
                    VoteState::HandLowered
                };
                true
            }
        };

        Ok(success)
    })
    .await?;

    ctx.send(CreateReply::default().ephemeral(true).content(if !success {
        "Vote has already passed this player"
    } else if hand_state {
        "Hand Raised"
    } else {
        "Hand Lowered"
    }))
    .await?;

    Ok(())
}

#[poise::command(prefix_command, slash_command, check = "is_storyteller")]
pub async fn vote(ctx: Context<'_>, hand_state: bool) -> Result<(), Error> {
    mutate_active_vote(ctx, |players, vote| {
        vote.vote_state.insert(
            players.get(&vote.clock_hand).ok_or(Error::Silent)?.0,
            if hand_state {
                VoteState::Yes
            } else {
                VoteState::No
            },
        );

        vote.clock_hand = vote.clock_hand.next(players.len() as u32);

        Ok(())
    })
    .await?;

    ctx.send(CreateReply::default().ephemeral(true).content("Voted"))
        .await?;

    Ok(())
}

#[poise::command(prefix_command, slash_command, check = "is_storyteller")]
pub async fn start_vote(
    ctx: Context<'_>,
    #[description = "Whoever does the nominatino"] nominator: UserId,
    #[description = "Whoever gets nominated"] nominee: UserId,
    #[description = "eg. \"It will take 5 to tie, 6 to execute\""] description: String,
) -> Result<(), Error> {
    let (_config, state) = ctx.data();

    let state_read = state.read().await;
    let clockhand = match state_read.players.iter().find(|(_, (b, _))| *b == nominee) {
        Some((cottage_number, _)) => cottage_number.next(state_read.players.len() as u32),
        None => {
            ctx.reply("Nominee is not assigned to a cottage!").await?;
            return Ok(());
        }
    };
    drop(state_read);

    let reply_handle = ctx
        .send(
            CreateReply::default()
                .reply(true)
                .content(format!(
                    "{} has nominated {}",
                    FormatMention(nominator),
                    FormatMention(nominee)
                ))
                .components(vec![CreateActionRow::Buttons(vec![
                    CreateButton::new("hand_up_button")
                        .label("Hand Up")
                        .emoji(ReactionType::Unicode("🙋".to_string())),
                    CreateButton::new("hand_down_button")
                        .label("Hand Down")
                        .emoji('🙅'),
                ])]),
        )
        .await?;
    let message = reply_handle.message().await?;

    let vote = Vote {
        nominator,
        nominee,
        description,
        accusation: String::new(),
        defense: String::new(),
        clock_hand: clockhand,
        vote_state: HashMap::new(),
        message_id: message.id,
        channel_id: message.channel_id,
    };

    let mut state = state.write().await;
    state.current_vote = Some(vote);
    state.save();
    drop(state);

    println!("State dropped, and saved");

    Ok(())
}
