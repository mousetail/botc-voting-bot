use std::{collections::HashMap, fmt::Display, fs::OpenOptions};

use poise::serenity_prelude::{ChannelId, MessageId, UserId};
use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize)]
pub enum VoteState {
    None,
    HandRaised,
    HandLowered,
    Yes,
    No,
}

pub struct FormatMention(pub UserId);

impl Display for FormatMention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<@{}>", self.0)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Vote {
    pub nominator: UserId,
    pub nominee: UserId,

    pub accusation: String,
    pub defense: String,

    pub clock_hand: u32,

    pub vote_state: HashMap<UserId, VoteState>,

    pub description: String,

    pub message_id: MessageId,
    pub channel_id: ChannelId,
}

#[derive(Serialize, Deserialize, Default)]
pub struct State {
    pub players: HashMap<u32, (UserId, ChannelId)>,
    pub number_of_players: u32,
    pub current_vote: Option<Vote>,
}

impl State {
    pub fn save(&self) {
        serde_yml::to_writer(
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open("state.yaml")
                .unwrap(),
            self,
        )
        .unwrap()
    }
}

pub fn format_vote<'a>(
    Vote {
        nominator,
        nominee,
        clock_hand,
        accusation,
        defense,
        vote_state,
        description,

        message_id: _,
        channel_id: _,
    }: &Vote,
) -> String {
    return format!(
        r"
{} nominates {}

Accusation:

> {accusation}

Defense:

> {defense}

{description}

    ",
        FormatMention(*nominator),
        FormatMention(*nominee)
    );
}

pub struct PrintCottages<'a>(pub &'a State);

impl<'a> Display for PrintCottages<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for i in 1..self.0.number_of_players + 1 {
            write!(f, "{i}: ")?;
            match self.0.players.get(&i) {
                Some((player, channel)) => {
                    writeln!(f, "{} <#{}>", FormatMention(*player), channel)?
                }
                None => writeln!(f, "unassigned")?,
            };
        }
        Ok(())
    }
}
