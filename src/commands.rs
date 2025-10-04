use std::collections::HashMap;

use poise::{
    CreateReply,
    serenity_prelude::{
        ChannelId, CreateActionRow, CreateButton, EditMessage, ReactionType, UserId,
    },
};

use crate::{
    Context, Error,
    state::{FormatMention, PrintCottages, Vote, format_vote},
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
    callback: impl AsyncFnOnce(&mut Vote) -> Result<T, Error>,
) -> Result<T, Error> {
    let (_config, state) = ctx.data();

    let mut state = state.write().await;
    let vote = match &mut state.current_vote {
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

    let result = callback(vote).await?;

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
    state
        .players
        .insert(cottage_number, (player_id, channel_id));
    state.save();
    let state = state.downgrade();

    ctx.reply(format!("{}", PrintCottages(&state))).await?;

    Ok(())
}

#[poise::command(prefix_command, slash_command, check = "is_storyteller")]
pub async fn set_defense(ctx: Context<'_>, defense: String) -> Result<(), Error> {
    mutate_active_vote(ctx, async move |vote| -> Result<(), Error> {
        vote.defense = defense;

        let mut message = ctx
            .http()
            .get_message(vote.channel_id, vote.message_id)
            .await?;
        message
            .edit(ctx, EditMessage::new().content(format_vote(vote)))
            .await?;

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
    mutate_active_vote(ctx, async move |vote| -> Result<(), Error> {
        vote.accusation = accusation;

        let mut message = ctx
            .http()
            .get_message(vote.channel_id, vote.message_id)
            .await?;
        message
            .edit(ctx, EditMessage::new().content(format_vote(vote)))
            .await?;

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
pub async fn start_vote(
    ctx: Context<'_>,
    #[description = "Whoever does the nominatino"] nominator: UserId,
    #[description = "Whoever gets nominated"] nominee: UserId,
    #[description = "eg. \"It will take 5 to tie, 6 to execute\""] description: String,
) -> Result<(), Error> {
    let (_config, state) = ctx.data();

    let state_read = state.read().await;
    let clockhand = match state_read.players.iter().find(|(_, (b, _))| *b == nominee) {
        Some(e) => e.0 % state_read.number_of_players + 1,
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
