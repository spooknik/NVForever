//! Caravan, the card game (docs/spec-notes/caravan.md): the cards (CCRD) and decks (CDCK),
//! the cards the player owns and the deck they built, the bet, the game's rules and the
//! opponent. The engine draws the menu (nvf-play `caravan.rs`).
//!
//! A game is two [`Side`]s, the player's ([`PLAYER`]) and the opponent's ([`OPPONENT`]),
//! each with a deck, a hand and three caravans, one per track. [`Game::apply`] carries out
//! a [`Move`] and says what happened as [`Event`]s; [`choose`] is the opponent's choice.

use crate::game::Game as Rules;
use fnv_sim::FormId;
use rand_chacha::ChaCha8Rng;
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The menu's code to scripts [documented: GECK wiki GetMenuTargetRef; caravan.md §1].
pub const MENU_CODE: u16 = 1083;
/// The fewest cards in a deck [documented: HelpCaravanDeckBuilding; owner ruling].
pub const MIN_DECK: usize = 30;
/// Cards each side draws at the start [documented: HelpCaravanStartingCaravans].
pub const HAND: usize = 8;
pub const TRACKS: usize = 3;
pub const PLAYER: usize = 0;
pub const OPPONENT: usize = 1;
/// A caravan is sold at these values [documented: HelpCaravanContractWar].
pub const SOLD_MIN: u32 = 21;
pub const SOLD_MAX: u32 = 26;
/// The help messages (MESG) [observed: FalloutNV.esm].
pub const HELP_BETTING: FormId = FormId(0x0000_0186);
pub const HELP_DECK: FormId = FormId(0x0000_0187);
pub const HELP_STARTING: FormId = FormId(0x0000_0188);
pub const HELP_WAR: FormId = FormId(0x0000_0189);
/// The caps (the currency bet) [documented: FormID 0000000F].
pub const CAPS: FormId = FormId(0xF);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Suit {
    #[default]
    Hearts,
    Spades,
    Diamonds,
    Clubs,
    Joker,
}

impl Suit {
    /// From the card's first INTV [documented: xEdit, suit 1–5].
    pub fn from_code(c: u32) -> Option<Suit> {
        Some(match c {
            1 => Suit::Hearts,
            2 => Suit::Spades,
            3 => Suit::Diamonds,
            4 => Suit::Clubs,
            5 => Suit::Joker,
            _ => return None,
        })
    }

    pub fn letter(self) -> char {
        match self {
            Suit::Hearts => 'h',
            Suit::Spades => 's',
            Suit::Diamonds => 'd',
            Suit::Clubs => 'c',
            Suit::Joker => 'j',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Rank {
    /// An ace (1) or a numbered card (2–10).
    Num(u8),
    Jack,
    Queen,
    King,
    #[default]
    Joker,
}

impl Rank {
    /// From the card's second INTV [documented: xEdit, 1 Ace, 2–10, 12 Jack, 13 Queen, 14
    /// King, 15 Joker].
    pub fn from_code(c: u32) -> Option<Rank> {
        Some(match c {
            1..=10 => Rank::Num(c as u8),
            12 => Rank::Jack,
            13 => Rank::Queen,
            14 => Rank::King,
            15 => Rank::Joker,
            _ => return None,
        })
    }

    pub fn number(self) -> Option<u32> {
        match self {
            Rank::Num(n) => Some(n as u32),
            _ => None,
        }
    }

    pub fn is_face(self) -> bool {
        self.number().is_none()
    }

    pub fn short(self) -> String {
        match self {
            Rank::Num(1) => "A".into(),
            Rank::Num(n) => n.to_string(),
            Rank::Jack => "J".into(),
            Rank::Queen => "Q".into(),
            Rank::King => "K".into(),
            Rank::Joker => "Jo".into(),
        }
    }
}

/// A card in play: its form (for its name and textures), suit and rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Card {
    pub form: FormId,
    pub suit: Suit,
    pub rank: Rank,
}

impl Default for Card {
    fn default() -> Card {
        Card::new(FormId(0), Suit::default(), Rank::default())
    }
}

impl Card {
    pub fn new(form: FormId, suit: Suit, rank: Rank) -> Card {
        Card { form, suit, rank }
    }

    /// `A`, `2`…`10`, `J`, `Q`, `K` then the suit letter (`10h`, `Ks`), or `Jo` (a joker);
    /// the form is 0 (tests and the test aid).
    pub fn parse(s: &str) -> Option<Card> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("jo") {
            return Some(Card::new(FormId(0), Suit::Joker, Rank::Joker));
        }
        let (r, su) = s.split_at(s.len().checked_sub(1)?);
        let suit = match su.to_ascii_lowercase().as_str() {
            "h" => Suit::Hearts,
            "s" => Suit::Spades,
            "d" => Suit::Diamonds,
            "c" => Suit::Clubs,
            _ => return None,
        };
        let rank = match r.to_ascii_uppercase().as_str() {
            "A" => Rank::Num(1),
            "J" => Rank::Jack,
            "Q" => Rank::Queen,
            "K" => Rank::King,
            n => Rank::Num(n.parse::<u8>().ok().filter(|n| (2..=10).contains(n))?),
        };
        Some(Card::new(FormId(0), suit, rank))
    }

    pub fn short(&self) -> String {
        match self.rank {
            Rank::Joker => "Jo".into(),
            r => format!("{}{}", r.short(), self.suit.letter()),
        }
    }
}

/// A caravan card's record (CCRD; caravan.md §2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CardDef {
    pub name: String,
    pub suit: Suit,
    pub rank: Rank,
    /// TX00, TX01: the face and the back, under `textures\`.
    pub face: Option<String>,
    pub back: Option<String>,
}

impl CardDef {
    /// From the record's subrecords [documented: schemas/fnv CCRD, xEdit]; `None` when its
    /// suit or value cannot be read.
    pub fn from_raw(subs: &[fnv_esm::Subrecord]) -> Option<CardDef> {
        let mut c = CardDef::default();
        let mut intv = Vec::new();
        for s in subs {
            match &s.sig {
                b"FULL" => c.name = zstring(&s.data),
                b"TX00" => c.face = Some(zstring(&s.data)).filter(|p| !p.is_empty()),
                b"TX01" => c.back = Some(zstring(&s.data)).filter(|p| !p.is_empty()),
                b"INTV" => intv.extend(u32_at(&s.data, 0)),
                _ => {}
            }
        }
        c.suit = Suit::from_code(*intv.first()?)?;
        c.rank = Rank::from_code(*intv.get(1)?)?;
        Some(c)
    }
}

/// A Caravan deck's record (CDCK; caravan.md §2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeckDef {
    pub name: String,
    /// One per CARD subrecord, in the record's order.
    pub cards: Vec<FormId>,
}

impl DeckDef {
    /// `map` turns a file's FormID into the load order's [observed: CARD is a CCRD FormID,
    /// u32].
    pub fn from_raw(subs: &[fnv_esm::Subrecord], map: &dyn Fn(u32) -> Option<u32>) -> DeckDef {
        let mut d = DeckDef::default();
        for s in subs {
            match &s.sig {
                b"FULL" => d.name = zstring(&s.data),
                b"CARD" => d.cards.extend(u32_at(&s.data, 0).filter(|&x| x != 0).and_then(map).map(FormId)),
                _ => {}
            }
        }
        d
    }
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).and_then(|x| <[u8; 4]>::try_from(x).ok()).map(u32::from_le_bytes)
}

fn zstring(b: &[u8]) -> String {
    // In this run's code page (fnv_util::text, #1439).
    fnv_util::text::decode_z(b)
}

/// The player's cards, deck and record against each opponent (saved with the game).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CaravanState {
    /// Every card AddCardToPlayer gave, in the order given (a form given twice is here
    /// twice).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cards: Vec<FormId>,
    /// The last deck built (caravan.md §4).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub deck: Vec<FormId>,
    /// The help messages shown so far (bits of [`Help`]).
    #[serde(skip_serializing_if = "is_zero", default)]
    pub help: u8,
    /// By opponent (the actor).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub records: BTreeMap<FormId, Record>,
}

impl CaravanState {
    pub fn is_empty(&self) -> bool {
        *self == CaravanState::default()
    }
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

/// The player's games against one opponent (caravan.md §7).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub won: u32,
    pub lost: u32,
    /// Caps won and lost over every game.
    pub winnings: u64,
    pub losses: u64,
    pub biggest_pot: u32,
}

impl Record {
    pub fn net(&self) -> i64 {
        self.winnings as i64 - self.losses as i64
    }
}

/// A help message, shown once over the screen it explains (caravan.md §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Help {
    Betting,
    Deck,
    Starting,
    War,
}

impl Help {
    fn bit(self) -> u8 {
        1 << self as u8
    }

    pub fn message(self) -> FormId {
        match self {
            Help::Betting => HELP_BETTING,
            Help::Deck => HELP_DECK,
            Help::Starting => HELP_STARTING,
            Help::War => HELP_WAR,
        }
    }
}

/// The help message's title and text the first time it is asked for, else `None`.
pub fn first_help(g: &mut Rules, which: Help) -> Option<(String, String)> {
    if g.player.caravan.help & which.bit() != 0 {
        return None;
    }
    let m = g.data.messages.get(&which.message())?;
    let text = m.text.clone()?;
    g.player.caravan.help |= which.bit();
    Some((m.title.clone().unwrap_or_default(), text))
}

/// AddCardToPlayer: a card form for the player's collection; `false` when it is not a
/// caravan card.
pub fn add_card(g: &mut Rules, card: FormId) -> bool {
    if !g.data.caravan_cards.contains_key(&card) {
        return false;
    }
    g.player.caravan.cards.push(card);
    true
}

/// The cards the deck builder offers: each form owned once, in suit-blind rank order
/// (aces first, jokers last) as the screenshots show [observed].
pub fn owned(g: &Rules) -> Vec<FormId> {
    let mut out: Vec<FormId> = Vec::new();
    for &c in &g.player.caravan.cards {
        if !out.contains(&c) && g.data.caravan_cards.contains_key(&c) {
            out.push(c);
        }
    }
    let key = |f: &FormId| g.data.caravan_cards.get(f).map(|d| (d.rank, d.suit)).unwrap_or_default();
    out.sort_by_key(|f| (key(f), f.0));
    out
}

/// The cards of forms, those that are caravan cards.
pub fn cards_of(g: &Rules, forms: &[FormId]) -> Vec<Card> {
    forms.iter().filter_map(|&f| g.data.caravan_cards.get(&f).map(|d| Card::new(f, d.suit, d.rank))).collect()
}

/// The saved deck, the cards no longer owned taken out.
pub fn saved_deck(g: &Rules) -> Vec<FormId> {
    let own = owned(g);
    let mut out: Vec<FormId> = Vec::new();
    for &c in &g.player.caravan.deck {
        if own.contains(&c) && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// A random deck: between [`MIN_DECK`] cards and every card owned [design; caravan.md §4].
pub fn randomize(owned: &[FormId], rng: &mut ChaCha8Rng) -> Vec<FormId> {
    let mut all = owned.to_vec();
    shuffle(&mut all, rng);
    let lo = MIN_DECK.min(all.len());
    let n = lo + (rng.next_u32() as usize) % (all.len() - lo + 1);
    all.truncate(n);
    all
}

pub fn shuffle<T>(v: &mut [T], rng: &mut ChaCha8Rng) {
    for i in (1..v.len()).rev() {
        let j = (rng.next_u32() as usize) % (i + 1);
        v.swap(i, j);
    }
}

/// A request from ShowCaravanMenu (caravan.md §1); the engine opens the menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaravanRequest {
    /// The calling reference: the opponent.
    pub opponent: FormId,
    pub deck: FormId,
    /// 0–3; not used (the deck's face cards set the skill, §6).
    pub difficulty: i32,
    /// The most of its currency the opponent bets.
    pub max_bet_share: f32,
}

/// The bet (caravan.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bet {
    /// The opponent's bid.
    pub bid: u32,
    /// The player's ante, what each side puts in.
    pub ante: u32,
    /// The player's and the opponent's currency.
    pub funds: u32,
    pub opponent_funds: u32,
}

impl Bet {
    pub fn new(funds: u32, opponent_funds: u32, share: f32) -> Bet {
        let bid = (opponent_funds as f32 * share.clamp(0.0, 1.0)).floor() as u32;
        Bet { bid, ante: 0, funds, opponent_funds }
    }

    /// The most the ante can be: the opponent raises to half again its bid [design].
    pub fn max(&self) -> u32 {
        (self.bid + self.bid / 2).min(self.funds).min(self.opponent_funds)
    }

    pub fn can_match(&self) -> bool {
        self.ante != self.bid.min(self.max())
    }

    pub fn auto_match(&mut self) -> bool {
        let before = self.ante;
        self.ante = self.bid.min(self.max());
        self.ante != before
    }

    /// A tenth of the bid at a time [design].
    pub fn step(&self) -> u32 {
        (self.bid / 10).max(1)
    }

    pub fn can_raise(&self) -> bool {
        self.ante < self.max()
    }

    pub fn raise(&mut self) -> bool {
        let before = self.ante;
        self.ante = (self.ante + self.step()).min(self.max());
        self.ante != before
    }

    /// Accept is dimmed at an ante of 0 [observed], unless the opponent has nothing to
    /// bid [design].
    pub fn can_accept(&self) -> bool {
        self.ante > 0 || self.max() == 0
    }

    pub fn pot(&self) -> u32 {
        self.ante * 2
    }
}

/// A card on a caravan with the face cards put on it, in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Placed {
    pub card: Card,
    pub faces: Vec<Card>,
}

impl Placed {
    pub fn base(&self) -> u32 {
        self.card.rank.number().unwrap_or(0)
    }

    /// The card's worth: each King on it doubles it [owner ruling 2026-10-19].
    pub fn value(&self) -> u32 {
        let kings = self.faces.iter().filter(|f| f.rank == Rank::King).count() as u32;
        self.base() << kings.min(16)
    }

    fn queens(&self) -> impl Iterator<Item = &Card> {
        self.faces.iter().filter(|f| f.rank == Rank::Queen)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
}

impl Dir {
    fn flip(self) -> Dir {
        match self {
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
        }
    }
}

/// One side's pile on a track.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Caravan {
    pub cards: Vec<Placed>,
}

impl Caravan {
    pub fn value(&self) -> u32 {
        self.cards.iter().map(Placed::value).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// The last card's suit, or the last Queen's on it.
    pub fn suit(&self) -> Option<Suit> {
        let last = self.cards.last()?;
        Some(last.queens().last().map_or(last.card.suit, |q| q.suit))
    }

    /// Up or down by the last two cards, reversed by each Queen on the last.
    pub fn direction(&self) -> Option<Dir> {
        let n = self.cards.len();
        if n < 2 {
            return None;
        }
        let (a, b) = (self.cards[n - 2].base(), self.cards[n - 1].base());
        let d = if b > a { Dir::Up } else { Dir::Down };
        Some(if self.cards[n - 1].queens().count() % 2 == 1 { d.flip() } else { d })
    }

    /// Whether a numbered card or ace can go on the end (caravan.md §5).
    pub fn accepts(&self, card: Card) -> bool {
        let Some(n) = card.rank.number() else { return false };
        let Some(last) = self.cards.last() else { return true };
        if n == last.base() {
            return false;
        }
        if self.suit() == Some(card.suit) {
            return true;
        }
        match self.direction() {
            None => true,
            Some(Dir::Up) => n > last.base(),
            Some(Dir::Down) => n < last.base(),
        }
    }

    pub fn sold(&self) -> bool {
        (SOLD_MIN..=SOLD_MAX).contains(&self.value())
    }

    pub fn over(&self) -> bool {
        self.value() > SOLD_MAX
    }

    fn all_cards(&self) -> impl Iterator<Item = &Card> {
        self.cards.iter().flat_map(|p| std::iter::once(&p.card).chain(p.faces.iter()))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Side {
    /// The cards still to draw; the next is the last.
    pub deck: Vec<Card>,
    pub hand: Vec<Card>,
    pub caravans: [Caravan; TRACKS],
}

impl Side {
    pub fn new(mut deck: Vec<Card>, rng: &mut ChaCha8Rng) -> Side {
        shuffle(&mut deck, rng);
        let mut s = Side { deck, ..Default::default() };
        for _ in 0..HAND {
            s.draw();
        }
        s
    }

    fn draw(&mut self) -> Option<Card> {
        let c = self.deck.pop()?;
        self.hand.push(c);
        Some(c)
    }

    pub fn started(&self) -> bool {
        self.caravans.iter().all(|c| !c.is_empty())
    }

    /// Cards in the hand and the deck.
    pub fn cards_left(&self) -> usize {
        self.hand.len() + self.deck.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Each side starts its three caravans.
    Opening,
    /// The contract war.
    War,
    Over,
}

/// Why the game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// All three tracks sold.
    Sold,
    /// The loser had no cards left (owner ruling).
    OutOfCards,
    /// The loser gave up (Cancel).
    Forfeit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// A card from the hand onto a caravan: a numbered card on the end of one of the
    /// mover's own (`on` is `None`); a face card on the card `on` of any caravan.
    Play { card: usize, side: usize, track: usize, on: Option<usize> },
    /// A card from the hand thrown away; another drawn.
    Discard { card: usize },
    /// Every card of one of the mover's caravans thrown away.
    DiscardTrack { track: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Illegal {
    Over,
    NotYourTurn,
    NoSuchCard,
    NoSuchTarget,
    /// Numbered cards go on the mover's own caravans.
    NotYours,
    /// The opening allows numbered cards on empty caravans, and discards.
    Opening,
    /// The caravan does not take that card (direction, suit, a repeated value).
    Order,
    /// A Queen goes on a caravan's last card.
    QueenNotLast,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Played { by: usize, card: Card, side: usize, track: usize },
    /// Cards that left the table (a Jack's, a Joker's).
    Removed { side: usize, track: usize, cards: Vec<Card> },
    Drew { side: usize, card: Card },
    Discarded { side: usize, card: Card },
    TrackDiscarded { side: usize, track: usize, cards: Vec<Card> },
    OpeningOver,
    Over { winner: usize, end: End },
}

#[derive(Debug, Clone)]
pub struct Game {
    pub sides: [Side; 2],
    pub turn: usize,
    pub phase: Phase,
    pub winner: Option<(usize, End)>,
    pub moves: u32,
}

impl Game {
    /// Both decks shuffled and 8 cards drawn each; the opponent goes first [observed].
    pub fn new(player: Vec<Card>, opponent: Vec<Card>, rng: &mut ChaCha8Rng) -> Game {
        let p = Side::new(player, rng);
        let o = Side::new(opponent, rng);
        Game { sides: [p, o], turn: OPPONENT, phase: Phase::Opening, winner: None, moves: 0 }
    }

    pub fn other(side: usize) -> usize {
        1 - side
    }

    /// The track's winner: the side whose caravan is sold and higher, or sold alone; none
    /// while neither is sold or both are at the same value.
    pub fn track_winner(&self, track: usize) -> Option<usize> {
        let (a, b) = (&self.sides[PLAYER].caravans[track], &self.sides[OPPONENT].caravans[track]);
        match (a.sold(), b.sold()) {
            (true, true) if a.value() > b.value() => Some(PLAYER),
            (true, true) if b.value() > a.value() => Some(OPPONENT),
            (true, false) => Some(PLAYER),
            (false, true) => Some(OPPONENT),
            _ => None,
        }
    }

    pub fn legal(&self, by: usize, m: Move) -> Result<(), Illegal> {
        if self.phase == Phase::Over {
            return Err(Illegal::Over);
        }
        if by != self.turn {
            return Err(Illegal::NotYourTurn);
        }
        let me = &self.sides[by];
        match m {
            Move::Discard { card } => me.hand.get(card).map(|_| ()).ok_or(Illegal::NoSuchCard),
            Move::DiscardTrack { track } => {
                if self.phase == Phase::Opening {
                    return Err(Illegal::Opening);
                }
                match me.caravans.get(track) {
                    Some(c) if !c.is_empty() => Ok(()),
                    _ => Err(Illegal::NoSuchTarget),
                }
            }
            Move::Play { card, side, track, on } => {
                let c = *me.hand.get(card).ok_or(Illegal::NoSuchCard)?;
                let target = self.sides.get(side).and_then(|s| s.caravans.get(track)).ok_or(Illegal::NoSuchTarget)?;
                if c.rank.is_face() {
                    if self.phase == Phase::Opening {
                        return Err(Illegal::Opening);
                    }
                    let i = on.ok_or(Illegal::NoSuchTarget)?;
                    if i >= target.cards.len() {
                        return Err(Illegal::NoSuchTarget);
                    }
                    if c.rank == Rank::Queen && i + 1 != target.cards.len() {
                        return Err(Illegal::QueenNotLast);
                    }
                    return Ok(());
                }
                if side != by {
                    return Err(Illegal::NotYours);
                }
                if self.phase == Phase::Opening {
                    return if target.is_empty() { Ok(()) } else { Err(Illegal::Opening) };
                }
                if target.accepts(c) {
                    Ok(())
                } else {
                    Err(Illegal::Order)
                }
            }
        }
    }

    /// Every legal move for the side to move.
    pub fn moves_for(&self, by: usize) -> Vec<Move> {
        let mut out = Vec::new();
        if self.phase == Phase::Over || by != self.turn {
            return out;
        }
        let me = &self.sides[by];
        for (i, c) in me.hand.iter().enumerate() {
            if c.rank.is_face() {
                for side in 0..2 {
                    for track in 0..TRACKS {
                        for on in 0..self.sides[side].caravans[track].cards.len() {
                            out.push(Move::Play { card: i, side, track, on: Some(on) });
                        }
                    }
                }
            } else {
                for track in 0..TRACKS {
                    out.push(Move::Play { card: i, side: by, track, on: None });
                }
            }
            out.push(Move::Discard { card: i });
        }
        for track in 0..TRACKS {
            out.push(Move::DiscardTrack { track });
        }
        out.retain(|&m| self.legal(by, m).is_ok());
        out
    }

    pub fn apply(&mut self, by: usize, m: Move) -> Result<Vec<Event>, Illegal> {
        self.legal(by, m)?;
        let mut ev = Vec::new();
        let opening = self.phase == Phase::Opening;
        match m {
            Move::Discard { card } => {
                let c = self.sides[by].hand.remove(card);
                ev.push(Event::Discarded { side: by, card: c });
                self.draw(by, &mut ev);
            }
            Move::DiscardTrack { track } => {
                let pile = std::mem::take(&mut self.sides[by].caravans[track]);
                ev.push(Event::TrackDiscarded { side: by, track, cards: pile.all_cards().copied().collect() });
            }
            Move::Play { card, side, track, on } => {
                let c = self.sides[by].hand.remove(card);
                ev.push(Event::Played { by, card: c, side, track });
                match (c.rank, on) {
                    (Rank::Num(_), _) => self.sides[side].caravans[track].cards.push(Placed { card: c, faces: Vec::new() }),
                    (Rank::Jack, Some(i)) => {
                        let gone = self.sides[side].caravans[track].cards.remove(i);
                        let mut cards = vec![gone.card];
                        cards.extend(gone.faces);
                        cards.push(c);
                        ev.push(Event::Removed { side, track, cards });
                    }
                    (Rank::Joker, Some(i)) => {
                        self.sides[side].caravans[track].cards[i].faces.push(c);
                        self.joker((side, track, i), &mut ev);
                    }
                    (_, Some(i)) => self.sides[side].caravans[track].cards[i].faces.push(c),
                    _ => {}
                }
                if !opening {
                    self.draw(by, &mut ev);
                }
            }
        }
        self.moves += 1;
        if opening && self.sides.iter().all(Side::started) {
            self.phase = Phase::War;
            ev.push(Event::OpeningOver);
        }
        if self.phase == Phase::War {
            let won: Vec<Option<usize>> = (0..TRACKS).map(|t| self.track_winner(t)).collect();
            if won.iter().all(Option::is_some) {
                let player = won.iter().filter(|w| **w == Some(PLAYER)).count();
                let winner = if player >= 2 { PLAYER } else { OPPONENT };
                self.finish(winner, End::Sold, &mut ev);
                return Ok(ev);
            }
        }
        self.pass_turn(by, &mut ev);
        Ok(ev)
    }

    /// The side gives up.
    pub fn forfeit(&mut self, side: usize) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.phase != Phase::Over {
            self.finish(Game::other(side), End::Forfeit, &mut ev);
        }
        ev
    }

    fn finish(&mut self, winner: usize, end: End, ev: &mut Vec<Event>) {
        self.phase = Phase::Over;
        self.winner = Some((winner, end));
        ev.push(Event::Over { winner, end });
    }

    /// The other side moves, unless in the opening it has started its three caravans and
    /// this side has not; a side with no cards left loses (owner ruling).
    fn pass_turn(&mut self, by: usize, ev: &mut Vec<Event>) {
        let next = Game::other(by);
        self.turn = if self.phase == Phase::Opening && self.sides[next].started() && !self.sides[by].started() { by } else { next };
        let t = self.turn;
        if self.sides[t].hand.is_empty() {
            self.finish(Game::other(t), End::OutOfCards, ev);
        }
    }

    fn draw(&mut self, side: usize, ev: &mut Vec<Event>) {
        if let Some(card) = self.sides[side].draw() {
            ev.push(Event::Drew { side, card });
        }
    }

    /// A Joker on the card at `at` (side, track, place): on an ace every other numbered
    /// card of its suit, on 2–10 every other card of its value, leaves the table with its
    /// face cards (caravan.md §5).
    fn joker(&mut self, at: (usize, usize, usize), ev: &mut Vec<Event>) {
        let target = self.sides[at.0].caravans[at.1].cards[at.2].card;
        let hit = |c: &Card| match target.rank {
            Rank::Num(1) => c.suit == target.suit,
            r => c.rank == r,
        };
        for side in 0..2 {
            for track in 0..TRACKS {
                let pile = std::mem::take(&mut self.sides[side].caravans[track].cards);
                let mut gone = Vec::new();
                let mut kept = Vec::with_capacity(pile.len());
                for (i, p) in pile.into_iter().enumerate() {
                    if (side, track, i) != at && hit(&p.card) {
                        gone.push(p.card);
                        gone.extend(p.faces);
                    } else {
                        kept.push(p);
                    }
                }
                self.sides[side].caravans[track].cards = kept;
                if !gone.is_empty() {
                    ev.push(Event::Removed { side, track, cards: gone });
                }
            }
        }
    }
}

/// How well an opponent plays, from 0 to 1, by the share of face cards in its deck: 10 %
/// or fewer → 0, 35 % or more → 1 [design; owner ruling 2026-10-19; caravan.md §6].
pub fn skill(deck: &[Card]) -> f32 {
    if deck.is_empty() {
        return 0.0;
    }
    let faces = deck.iter().filter(|c| c.rank.is_face()).count() as f32;
    ((faces / deck.len() as f32 - 0.10) / 0.25).clamp(0.0, 1.0)
}

/// How good the position is for `side` [design].
pub fn evaluate(g: &Game, side: usize) -> f32 {
    if let Some((w, _)) = g.winner {
        return if w == side { 1.0e6 } else { -1.0e6 };
    }
    let other = Game::other(side);
    let mut s = 0.0;
    for t in 0..TRACKS {
        let (mine, theirs) = (&g.sides[side].caravans[t], &g.sides[other].caravans[t]);
        let (o, th) = (mine.value(), theirs.value());
        if mine.sold() && (!theirs.sold() || o > th) {
            s += 100.0 + o as f32;
        } else if mine.sold() && theirs.sold() && o == th {
            s += 40.0;
        } else if mine.sold() {
            s += 30.0;
        } else if mine.over() {
            s -= 30.0 + 4.0 * (o - SOLD_MAX) as f32;
        } else {
            s += 2.0 * o as f32;
        }
        if theirs.sold() && (!mine.sold() || th > o) {
            s -= 60.0;
        }
    }
    s + 0.5 * g.sides[side].cards_left() as f32
}

/// The side's choice: the best move by [`evaluate`] after it, with noise and fewer attacks
/// the lower the skill [design; caravan.md §6]. `None` when the side cannot move.
pub fn choose(g: &Game, side: usize, skill: f32, rng: &mut ChaCha8Rng) -> Option<Move> {
    let skill = skill.clamp(0.0, 1.0);
    let mut best: Option<(f32, Move)> = None;
    for m in g.moves_for(side) {
        let mut after = g.clone();
        if after.apply(side, m).is_err() {
            continue;
        }
        let noise = (rng.next_u32() as f32 / u32::MAX as f32 * 2.0 - 1.0) * 40.0 * (1.0 - skill);
        let attack = matches!(m, Move::Play { side: s, on: Some(_), .. } if s != side);
        let mut score = evaluate(&after, side) + noise;
        if attack {
            score -= 25.0 * (1.0 - skill);
        }
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, m));
        }
    }
    best.map(|(_, m)| m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::SeedableRng;

    fn c(s: &str) -> Card {
        Card::parse(s).unwrap()
    }

    fn pile(cards: &[&str]) -> Caravan {
        Caravan { cards: cards.iter().map(|s| Placed { card: c(s), faces: Vec::new() }).collect() }
    }

    /// A game in the contract war with hands and caravans as given, the player to move.
    fn war(hand: &[&str], mine: [&[&str]; 3], theirs: [&[&str]; 3]) -> Game {
        let side = |h: &[&str], cs: [&[&str]; 3]| Side {
            deck: (0..20).map(|_| c("2c")).collect(),
            hand: h.iter().map(|s| c(s)).collect(),
            caravans: [pile(cs[0]), pile(cs[1]), pile(cs[2])],
        };
        Game { sides: [side(hand, mine), side(&["9h", "8h", "7h", "6h", "5h"], theirs)], turn: PLAYER, phase: Phase::War, winner: None, moves: 0 }
    }

    #[test]
    fn cards_read_as_the_records_code_them() {
        assert_eq!(Suit::from_code(4), Some(Suit::Clubs));
        assert_eq!(Rank::from_code(10), Some(Rank::Num(10)));
        assert_eq!(Rank::from_code(14), Some(Rank::King));
        assert_eq!(Rank::from_code(11), None);
        assert_eq!(c("10h").rank, Rank::Num(10));
        assert_eq!(c("As").rank, Rank::Num(1));
        assert_eq!(c("Jo").rank, Rank::Joker);
        assert!(Card::parse("11h").is_none());
        assert_eq!(c("Kd").short(), "Kd");
    }

    #[test]
    fn kings_double_and_double_again() {
        let mut p = Placed { card: c("10h"), faces: vec![c("Ks")] };
        assert_eq!(p.value(), 20);
        p.faces.push(c("Kd"));
        assert_eq!(p.value(), 40);
        p.faces.push(c("Qh"));
        assert_eq!(p.value(), 40);
    }

    #[test]
    fn a_caravan_keeps_its_direction_or_its_suit() {
        let k = pile(&["3h", "7c"]);
        assert_eq!(k.direction(), Some(Dir::Up));
        assert_eq!(k.suit(), Some(Suit::Clubs));
        assert!(k.accepts(c("9h")), "up");
        assert!(!k.accepts(c("5h")), "down, another suit");
        assert!(k.accepts(c("5c")), "down, the same suit");
        assert!(!k.accepts(c("7h")), "the same value as the last");
        assert!(!k.accepts(c("7c")), "the same value even in suit");
        assert!(pile(&[]).accepts(c("Ad")));
        assert!(pile(&["5d"]).accepts(c("2s")), "one card sets no direction");
        assert!(!pile(&["5d"]).accepts(c("5s")));
        assert!(!k.accepts(c("Kd")), "face cards are not added to the end");
    }

    #[test]
    fn a_queen_reverses_the_direction_and_changes_the_suit() {
        let mut k = pile(&["3h", "7c"]);
        k.cards[1].faces.push(c("Qd"));
        assert_eq!(k.direction(), Some(Dir::Down));
        assert_eq!(k.suit(), Some(Suit::Diamonds));
        assert!(k.accepts(c("5h")));
        assert!(!k.accepts(c("9h")));
        assert!(k.accepts(c("9d")));
    }

    #[test]
    fn the_opening_lays_one_card_on_each_empty_caravan() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let deck: Vec<Card> = (2..=10).flat_map(|n| ["h", "s", "c", "d"].map(|s| c(&format!("{n}{s}")))).collect();
        let mut g = Game::new(deck.clone(), deck, &mut rng);
        assert_eq!(g.turn, OPPONENT, "the opponent goes first");
        assert_eq!(g.sides[PLAYER].hand.len(), HAND);
        assert_eq!(g.legal(PLAYER, Move::Discard { card: 0 }), Err(Illegal::NotYourTurn));
        assert_eq!(g.legal(OPPONENT, Move::DiscardTrack { track: 0 }), Err(Illegal::Opening));
        for _ in 0..3 {
            for by in [OPPONENT, PLAYER] {
                let track = g.sides[by].caravans.iter().position(Caravan::is_empty).unwrap();
                let ev = g.apply(by, Move::Play { card: 0, side: by, track, on: None }).unwrap();
                assert!(!ev.iter().any(|e| matches!(e, Event::Drew { .. })), "laying a card draws none in the opening");
            }
        }
        assert_eq!(g.phase, Phase::War);
        assert_eq!(g.sides[PLAYER].hand.len(), 5);
        let ev = g.apply(OPPONENT, Move::Discard { card: 0 }).unwrap();
        assert!(ev.iter().any(|e| matches!(e, Event::Drew { side: OPPONENT, .. })));
        assert_eq!(g.sides[OPPONENT].hand.len(), 5);
    }

    #[test]
    fn a_discard_in_the_opening_keeps_the_turn_with_the_side_behind() {
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let deck: Vec<Card> = (2..=10).flat_map(|n| ["h", "s", "c", "d"].map(|s| c(&format!("{n}{s}")))).collect();
        let mut g = Game::new(deck.clone(), deck, &mut rng);
        g.apply(OPPONENT, Move::Discard { card: 0 }).unwrap();
        for i in 0..3 {
            g.apply(PLAYER, Move::Play { card: 0, side: PLAYER, track: i, on: None }).unwrap();
            if i < 2 {
                g.apply(OPPONENT, Move::Play { card: 0, side: OPPONENT, track: i, on: None }).unwrap();
            }
        }
        assert_eq!(g.turn, OPPONENT, "the player has started all three; the opponent catches up");
        g.apply(OPPONENT, Move::Play { card: 0, side: OPPONENT, track: 2, on: None }).unwrap();
        assert_eq!(g.phase, Phase::War);
        assert_eq!(g.turn, PLAYER);
    }

    #[test]
    fn numbered_cards_go_on_ones_own_caravans_and_face_cards_anywhere() {
        let g = war(&["9h", "Kc", "Qs", "Jd"], [&["3h", "7c"], &["10s"], &["2d"]], [&["5c"], &["6h", "8h"], &["Ad"]]);
        assert_eq!(g.legal(PLAYER, Move::Play { card: 0, side: OPPONENT, track: 0, on: None }), Err(Illegal::NotYours));
        assert!(g.legal(PLAYER, Move::Play { card: 0, side: PLAYER, track: 0, on: None }).is_ok());
        assert!(g.legal(PLAYER, Move::Play { card: 1, side: OPPONENT, track: 1, on: Some(0) }).is_ok());
        assert_eq!(g.legal(PLAYER, Move::Play { card: 2, side: OPPONENT, track: 1, on: Some(0) }), Err(Illegal::QueenNotLast));
        assert!(g.legal(PLAYER, Move::Play { card: 2, side: OPPONENT, track: 1, on: Some(1) }).is_ok());
        assert_eq!(g.legal(PLAYER, Move::Play { card: 3, side: OPPONENT, track: 1, on: None }), Err(Illegal::NoSuchTarget));
    }

    #[test]
    fn a_jack_takes_the_card_and_its_face_cards() {
        let mut g = war(&["Jd"], [&["3h"], &["4h"], &["5h"]], [&["6c", "9c"], &["7h"], &["8h"]]);
        g.sides[OPPONENT].caravans[0].cards[1].faces.push(c("Ks"));
        let ev = g.apply(PLAYER, Move::Play { card: 0, side: OPPONENT, track: 0, on: Some(1) }).unwrap();
        assert_eq!(g.sides[OPPONENT].caravans[0].value(), 6);
        assert!(ev.iter().any(|e| matches!(e, Event::Removed { side: OPPONENT, track: 0, cards } if cards.len() == 3)));
    }

    #[test]
    fn a_joker_on_an_ace_clears_its_suit_and_on_a_number_its_value() {
        let mut g = war(&["Jo", "Jo"], [&["Ah", "4h", "7c"], &["4s"], &["9h"]], [&["4d", "5h"], &["7h"], &["8c"]]);
        g.apply(PLAYER, Move::Play { card: 0, side: PLAYER, track: 0, on: Some(0) }).unwrap();
        assert_eq!(g.sides[PLAYER].caravans[0].cards.len(), 2, "4h went, the ace stays, 7c stays");
        assert!(g.sides[PLAYER].caravans[2].is_empty(), "9h went");
        assert_eq!(g.sides[OPPONENT].caravans[0].cards.len(), 1, "5h went");
        assert!(g.sides[OPPONENT].caravans[1].is_empty(), "7h went");
        g.turn = PLAYER;
        g.apply(PLAYER, Move::Play { card: 0, side: PLAYER, track: 1, on: Some(0) }).unwrap();
        assert_eq!(g.sides[PLAYER].caravans[1].cards.len(), 1, "the 4 the joker is on stays");
        assert!(g.sides[OPPONENT].caravans[0].is_empty(), "4d went");
    }

    #[test]
    fn tracks_sell_between_21_and_26_above_the_other_side() {
        let g = war(&["2c"], [&["10h", "9h", "4h"], &["10s", "8s", "5s"], &["10c", "9c"]], [&["10d", "6d", "5d"], &["10h", "8h", "4h", "2h"], &["10d", "9d", "8d"]]);
        assert_eq!(g.track_winner(0), Some(PLAYER), "23 against 21");
        assert_eq!(g.track_winner(1), Some(OPPONENT), "23 against 24");
        assert_eq!(g.track_winner(2), None, "19 against 27: neither sold");
        let tied = war(&["2c"], [&["10h", "9h", "4h"], &["4s"], &["5c"]], [&["10d", "9d", "4d"], &["7h"], &["5d"]]);
        assert_eq!(tied.track_winner(0), None, "23 against 23 sells neither");
    }

    #[test]
    fn the_game_ends_when_every_track_is_sold() {
        let mut g = war(&["2c"], [&["10h", "9h", "4h"], &["10s", "9s", "5s"], &["10c", "9c"]], [&["10d", "6d", "5d"], &["7h"], &["5d"]]);
        let ev = g.apply(PLAYER, Move::Play { card: 0, side: PLAYER, track: 2, on: None }).unwrap();
        assert_eq!(g.winner, Some((PLAYER, End::Sold)));
        assert!(ev.contains(&Event::Over { winner: PLAYER, end: End::Sold }));
    }

    #[test]
    fn a_side_without_cards_loses() {
        let mut g = war(&["9h"], [&["3h"], &["4h"], &["5h"]], [&["6c"], &["7h"], &["8h"]]);
        g.sides[OPPONENT].hand.clear();
        g.sides[OPPONENT].deck.clear();
        g.apply(PLAYER, Move::Discard { card: 0 }).unwrap();
        assert_eq!(g.winner, Some((PLAYER, End::OutOfCards)));
    }

    #[test]
    fn discarding_a_track_takes_all_its_cards() {
        let mut g = war(&["9h"], [&["3h", "8h"], &["4h"], &["5h"]], [&["6c"], &["7h"], &["8h"]]);
        g.apply(PLAYER, Move::DiscardTrack { track: 0 }).unwrap();
        assert!(g.sides[PLAYER].caravans[0].is_empty());
        assert_eq!(g.sides[PLAYER].hand.len(), 1, "no card drawn");
    }

    #[test]
    fn the_bet_follows_the_bid_and_the_funds() {
        let mut b = Bet::new(18, 216, 0.25);
        assert_eq!(b.bid, 54);
        assert!(!b.can_accept());
        assert!(b.auto_match());
        assert_eq!(b.ante, 18, "the player's funds");
        assert!(!b.can_raise());
        assert_eq!(b.pot(), 36);
        let mut b = Bet::new(500, 200, 0.5);
        b.auto_match();
        assert_eq!(b.ante, 100);
        while b.raise() {}
        assert_eq!(b.ante, 150, "half again the bid");
        let b = Bet::new(500, 0, 0.5);
        assert!(b.can_accept(), "nothing to bid: play for nothing");
    }

    #[test]
    fn more_face_cards_make_a_stronger_opponent() {
        let numbers: Vec<Card> = (0..40).map(|_| c("5h")).collect();
        let mut faces = numbers.clone();
        faces.truncate(26);
        faces.extend((0..14).map(|_| c("Ks")));
        assert_eq!(skill(&numbers), 0.0);
        assert!(skill(&faces) > 0.9);
    }

    #[test]
    fn the_opponent_finishes_games() {
        let deck: Vec<Card> = (1..=10).flat_map(|n| ["h", "s", "c", "d"].map(|s| c(&format!("{}{s}", if n == 1 { "A".into() } else { n.to_string() })))).chain(["Kh", "Qs", "Jd", "Jo"].map(c)).collect();
        for seed in 0..20 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let mut g = Game::new(deck.clone(), deck.clone(), &mut rng);
            let mut turns = 0;
            while g.phase != Phase::Over {
                let by = g.turn;
                let m = choose(&g, by, if by == PLAYER { 0.2 } else { 0.9 }, &mut rng).expect("a move");
                g.apply(by, m).unwrap();
                turns += 1;
                assert!(turns < 500, "seed {seed}: the game never ends");
            }
        }
    }

    #[test]
    fn a_random_deck_has_at_least_thirty_cards() {
        let owned: Vec<FormId> = (0..54).map(FormId).collect();
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        for _ in 0..20 {
            let d = randomize(&owned, &mut rng);
            assert!((MIN_DECK..=54).contains(&d.len()));
        }
    }
}
