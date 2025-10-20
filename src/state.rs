use std::{collections::HashMap, fmt::Display, fs::OpenOptions, num::NonZeroU32};

use poise::serenity_prelude::{ChannelId, MessageId, UserId};
use serde::{Deserialize, Serialize};

#[derive(Hash, Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Debug)]
pub struct CottageNumber(pub NonZeroU32);

impl CottageNumber {
    pub fn new(value: u32) -> Option<CottageNumber> {
        Some(CottageNumber(NonZeroU32::new(value)?))
    }

    pub fn next(self, number_of_players: u32) -> CottageNumber {
        CottageNumber::new(self.0.get() % number_of_players + 1).unwrap()
    }
}

pub type PlayerMap = HashMap<CottageNumber, (UserId, ChannelId)>;

#[derive(Serialize, Deserialize, Debug)]
pub enum VoteState {
    None,
    HandRaised,
    HandLowered,
    Yes,
    No,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum DeadState {
    Alive,
    DeadVoteAvailable,
    DeadVoteUsed,
}

pub struct FormatMention(pub UserId);

impl Display for FormatMention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<@{}>", self.0)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Vote {
    pub nominator: UserId,
    pub nominee: UserId,

    pub accusation: String,
    pub defense: String,

    pub clock_hand: CottageNumber,

    pub vote_state: HashMap<UserId, VoteState>,
    #[serde(default)]
    pub dead_state: HashMap<UserId, DeadState>,

    pub description: String,

    pub message_id: MessageId,
    pub channel_id: ChannelId,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct State {
    pub players: PlayerMap,
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

pub fn format_vote(
    players: &PlayerMap,
    Vote {
        nominator,
        nominee,
        clock_hand,
        accusation,
        defense,
        vote_state,
        description,
        dead_state,
        ..
    }: &Vote,
    number_of_players: u32,
) -> String {
    format!(
        r"
{} nominates {}

**Accusation:**
> {accusation}
**Defense:**
> {defense}

{}
{description}

    ",
        FormatMention(*nominator),
        FormatMention(*nominee),
        FormatVotes {
            vote_state,
            players,
            nominee: *nominee,
            clock_hand: *clock_hand,
            number_of_players,
            dead_state
        }
    )
}

pub struct PrintCottages<'a>(pub &'a State);

impl<'a> Display for PrintCottages<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for i in 1..self.0.number_of_players + 1 {
            write!(f, "{i}: ")?;
            match self.0.players.get(&CottageNumber::new(i).unwrap()) {
                Some((player, channel)) => {
                    writeln!(f, "{} <#{}>", FormatMention(*player), channel)?
                }
                None => writeln!(f, "unassigned")?,
            };
        }
        Ok(())
    }
}

struct FormatVotes<'a> {
    vote_state: &'a HashMap<UserId, VoteState>,
    dead_state: &'a HashMap<UserId, DeadState>,
    players: &'a PlayerMap,
    nominee: UserId,
    clock_hand: CottageNumber,
    number_of_players: u32,
}

impl<'a> Display for FormatVotes<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let start_player_index = self
            .players
            .iter()
            .find(|(_, (user_id, _))| *user_id == self.nominee);
        let Some(start_player_index) = start_player_index.map(|i| *i.0) else {
            write!(f, "[Starting Player Not On Table]")?;
            return Ok(());
        };

        let number_of_players = self.number_of_players;
        let mut clockhand_player = None;

        for i in 0..number_of_players {
            let cottage =
                CottageNumber::new((i + start_player_index.0.get()) % number_of_players + 1)
                    .unwrap();

            let Some(player_id) = self.players.get(&cottage).map(|i| i.0) else {
                write!(f, "[Empty Cottage]\n")?;
                continue;
            };

            if cottage == self.clock_hand {
                clockhand_player = Some(player_id)
            }

            let vote_state = self.vote_state.get(&player_id);
            let dead_state = self.dead_state.get(&player_id).unwrap_or(&DeadState::Alive);
            writeln!(
                f,
                "{}: {}{} {} {}",
                i + 1,
                FormatMention(player_id),
                match dead_state {
                    DeadState::Alive => "",
                    DeadState::DeadVoteAvailable => " (Dead)",
                    DeadState::DeadVoteUsed => " (Dead Vote Used)",
                },
                match vote_state {
                    None => " ",
                    Some(VoteState::HandRaised) => "🙋",
                    Some(VoteState::HandLowered) => "🙅‍♂️",
                    Some(VoteState::Yes) => "✅",
                    Some(VoteState::No) => "❌",
                    _ => "?",
                },
                if self.clock_hand == cottage {
                    "⬅️"
                } else {
                    ""
                }
            )?;
        }

        if let Some(clockhand_player) = clockhand_player {
            writeln!(f, "Clockhand on {}", FormatMention(clockhand_player))?
        }

        Ok(())
    }
}
