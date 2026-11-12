//! AI packages (`PACK`) as typed data: type, flags, locations, targets, schedule,
//! conditions, the idle collection and the type-specific extras. Read from the Layer 1
//! schema's decoded values (`schemas/fnv/ai.json`), never from byte offsets here.
//! Layout and meanings: docs/spec-notes/packages.md §1; what the game does with them:
//! §3. Choosing a package is [`crate::game::Game::select_package`].

use crate::cond::Condition;
use fnv_sim::FormId;
use serde::Serialize;
use serde_json::Value as Json;

/// PKDT package type [documented: xEdit wbPackageTypeEnum; packages.md §1].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PackageType {
    Find,
    Follow,
    Escort,
    Eat,
    Sleep,
    Wander,
    #[default]
    Travel,
    Accompany,
    UseItemAt,
    Ambush,
    FleeNotCombat,
    Sandbox,
    Patrol,
    Guard,
    Dialogue,
    UseWeapon,
    /// A code the documentation does not name (none in the masters).
    Other(u8),
}

impl PackageType {
    pub const ALL: [PackageType; 16] = [
        PackageType::Find,
        PackageType::Follow,
        PackageType::Escort,
        PackageType::Eat,
        PackageType::Sleep,
        PackageType::Wander,
        PackageType::Travel,
        PackageType::Accompany,
        PackageType::UseItemAt,
        PackageType::Ambush,
        PackageType::FleeNotCombat,
        PackageType::Sandbox,
        PackageType::Patrol,
        PackageType::Guard,
        PackageType::Dialogue,
        PackageType::UseWeapon,
    ];

    /// Whether a package of this type has a done state: its goal can be met, its
    /// procedure reads 17 DONE, and its end script and OnPackageDone run then. Sandbox,
    /// Sleep, Eat, Guard, Wander, Follow and Accompany never read DONE [observed:
    /// packages.md section 4; issue #2012, a script-added Sandbox package kept for 1.5
    /// game hours with no OnPackageDone]; "On End idle only works for packages which have
    /// a 'done' state" [documented: GECK wiki, Category:Packages].
    pub fn has_done_state(self) -> bool {
        matches!(
            self,
            PackageType::Travel
                | PackageType::Patrol
                | PackageType::Dialogue
                | PackageType::UseItemAt
                | PackageType::FleeNotCombat
                | PackageType::Find
                | PackageType::Escort
                | PackageType::Ambush
                | PackageType::UseWeapon
        )
    }

    pub fn from_code(c: u8) -> PackageType {
        match c {
            0 => PackageType::Find,
            1 => PackageType::Follow,
            2 => PackageType::Escort,
            3 => PackageType::Eat,
            4 => PackageType::Sleep,
            5 => PackageType::Wander,
            6 => PackageType::Travel,
            7 => PackageType::Accompany,
            8 => PackageType::UseItemAt,
            9 => PackageType::Ambush,
            10 => PackageType::FleeNotCombat,
            12 => PackageType::Sandbox,
            13 => PackageType::Patrol,
            14 => PackageType::Guard,
            15 => PackageType::Dialogue,
            16 => PackageType::UseWeapon,
            c => PackageType::Other(c),
        }
    }

    /// The code in the record.
    pub fn code(self) -> u8 {
        match self {
            PackageType::Find => 0,
            PackageType::Follow => 1,
            PackageType::Escort => 2,
            PackageType::Eat => 3,
            PackageType::Sleep => 4,
            PackageType::Wander => 5,
            PackageType::Travel => 6,
            PackageType::Accompany => 7,
            PackageType::UseItemAt => 8,
            PackageType::Ambush => 9,
            PackageType::FleeNotCombat => 10,
            PackageType::Sandbox => 12,
            PackageType::Patrol => 13,
            PackageType::Guard => 14,
            PackageType::Dialogue => 15,
            PackageType::UseWeapon => 16,
            PackageType::Other(c) => c,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PackageType::Find => "Find",
            PackageType::Follow => "Follow",
            PackageType::Escort => "Escort",
            PackageType::Eat => "Eat",
            PackageType::Sleep => "Sleep",
            PackageType::Wander => "Wander",
            PackageType::Travel => "Travel",
            PackageType::Accompany => "Accompany",
            PackageType::UseItemAt => "Use Item At",
            PackageType::Ambush => "Ambush",
            PackageType::FleeNotCombat => "Flee Not Combat",
            PackageType::Sandbox => "Sandbox",
            PackageType::Patrol => "Patrol",
            PackageType::Guard => "Guard",
            PackageType::Dialogue => "Dialogue",
            PackageType::UseWeapon => "Use Weapon",
            PackageType::Other(_) => "Other",
        }
    }

    /// What `GetCurrentAIPackage` returns while a package of this type runs: the game
    /// numbers package types differently from the record (packages.md §1.1). The pairs
    /// are matched by name against the function's documented table [documented: GECK
    /// wiki, GetCurrentAIPackage]; `fnvtool ai observed` checks them against the AI
    /// observer's samples and packages.md §4 lists which pairs were seen in the game.
    /// Find has no entry of its name in that table; it runs as 0, which the table calls
    /// "EXPLORE" [observed: FFER24FindFood on CheyenneREF, packages.md §4].
    pub fn runtime_code(self) -> Option<u8> {
        Some(match self {
            PackageType::Follow => 1,
            PackageType::Escort => 2,
            PackageType::Eat => 3,
            PackageType::Sleep => 4,
            PackageType::Wander => 13,
            PackageType::Travel => 14,
            PackageType::Accompany => 15,
            PackageType::UseItemAt => 16,
            PackageType::Ambush => 17,
            PackageType::FleeNotCombat => 18,
            PackageType::UseWeapon => 33,
            PackageType::Dialogue => 34,
            PackageType::Guard => 35,
            PackageType::Sandbox => 36,
            PackageType::Patrol => 37,
            PackageType::Find => 0,
            PackageType::Other(_) => return None,
        })
    }
}

/// PKDT general flags [documented: xEdit wbPackageFlags; GECK wiki, GetPackageFlag].
pub mod flags {
    pub const OFFERS_SERVICES: u32 = 1 << 0;
    pub const MUST_REACH_LOCATION: u32 = 1 << 1;
    pub const MUST_COMPLETE: u32 = 1 << 2;
    pub const LOCK_DOORS_AT_START: u32 = 1 << 3;
    pub const LOCK_DOORS_AT_END: u32 = 1 << 4;
    pub const LOCK_DOORS_AT_LOCATION: u32 = 1 << 5;
    pub const UNLOCK_DOORS_AT_START: u32 = 1 << 6;
    pub const UNLOCK_DOORS_AT_END: u32 = 1 << 7;
    pub const UNLOCK_DOORS_AT_LOCATION: u32 = 1 << 8;
    pub const CONTINUE_IF_PC_NEAR: u32 = 1 << 9;
    pub const ONCE_PER_DAY: u32 = 1 << 10;
    pub const SKIP_FALLOUT_BEHAVIOR: u32 = 1 << 12;
    pub const ALWAYS_RUN: u32 = 1 << 13;
    pub const ALWAYS_SNEAK: u32 = 1 << 17;
    pub const ALLOW_SWIMMING: u32 = 1 << 18;
    pub const ALLOW_FALLS: u32 = 1 << 19;
    pub const HEAD_TRACKING_OFF: u32 = 1 << 20;
    pub const WEAPONS_UNEQUIPPED: u32 = 1 << 21;
    pub const DEFENSIVE_COMBAT: u32 = 1 << 22;
    pub const WEAPON_DRAWN: u32 = 1 << 23;
    pub const NO_IDLE_ANIMS: u32 = 1 << 24;
    pub const PRETEND_IN_COMBAT: u32 = 1 << 25;
    pub const CONTINUE_DURING_COMBAT: u32 = 1 << 26;
    pub const NO_COMBAT_ALERT: u32 = 1 << 27;
    pub const NO_WARN_ATTACK_BEHAVIOR: u32 = 1 << 28;

    pub const NAMES: [(u32, &str); 25] = [
        (OFFERS_SERVICES, "Offers Services"),
        (MUST_REACH_LOCATION, "Must Reach Location"),
        (MUST_COMPLETE, "Must Complete"),
        (LOCK_DOORS_AT_START, "Lock Doors At Package Start"),
        (LOCK_DOORS_AT_END, "Lock Doors At Package End"),
        (LOCK_DOORS_AT_LOCATION, "Lock Doors At Location"),
        (UNLOCK_DOORS_AT_START, "Unlock Doors At Package Start"),
        (UNLOCK_DOORS_AT_END, "Unlock Doors At Package End"),
        (UNLOCK_DOORS_AT_LOCATION, "Unlock Doors At Location"),
        (CONTINUE_IF_PC_NEAR, "Continue If PC Near"),
        (ONCE_PER_DAY, "Once Per Day"),
        (SKIP_FALLOUT_BEHAVIOR, "Skip Fallout Behavior"),
        (ALWAYS_RUN, "Always Run"),
        (ALWAYS_SNEAK, "Always Sneak"),
        (ALLOW_SWIMMING, "Allow Swimming"),
        (ALLOW_FALLS, "Allow Falls"),
        (HEAD_TRACKING_OFF, "Head-Tracking Off"),
        (WEAPONS_UNEQUIPPED, "Weapons Unequipped"),
        (DEFENSIVE_COMBAT, "Defensive Combat"),
        (WEAPON_DRAWN, "Weapon Drawn"),
        (NO_IDLE_ANIMS, "No Idle Anims"),
        (PRETEND_IN_COMBAT, "Pretend In Combat"),
        (CONTINUE_DURING_COMBAT, "Continue During Combat"),
        (NO_COMBAT_ALERT, "No Combat Alert"),
        (NO_WARN_ATTACK_BEHAVIOR, "No Warn/Attack Behavior"),
    ];
}

/// PKDT "Fallout behaviour" flags [documented: xEdit PACK PKDT].
pub mod fallout {
    pub const HELLOS_TO_PLAYER: u16 = 1 << 0;
    pub const RANDOM_CONVERSATIONS: u16 = 1 << 1;
    pub const OBSERVE_COMBAT: u16 = 1 << 2;
    pub const REACTION_TO_PLAYER_ACTIONS: u16 = 1 << 4;
    pub const FRIENDLY_FIRE_COMMENTS: u16 = 1 << 5;
    pub const AGGRO_RADIUS: u16 = 1 << 6;
    pub const ALLOW_IDLE_CHATTER: u16 = 1 << 7;
    pub const AVOID_RADIATION: u16 = 1 << 8;
}

/// PKDT type-specific flags: which bits mean something depends on the package type
/// [documented: xEdit wbPKDTSpecificFlagsDecider]. Use the methods of
/// [`PackageDef`], which check the type.
pub mod type_flags {
    /// Wander and Sandbox.
    pub const NO_EATING: u16 = 1 << 0;
    pub const NO_SLEEPING: u16 = 1 << 1;
    pub const NO_CONVERSATION: u16 = 1 << 2;
    pub const NO_IDLE_MARKERS: u16 = 1 << 3;
    pub const NO_FURNITURE: u16 = 1 << 4;
    pub const NO_WANDERING: u16 = 1 << 5;
    /// Find, Escort, Eat and Use Item At.
    pub const ALLOW_BUYING: u16 = 1 << 8;
    pub const ALLOW_KILLING: u16 = 1 << 9;
    pub const ALLOW_STEALING: u16 = 1 << 10;
    /// Use Item At.
    pub const SIT_DOWN: u16 = 1 << 2;
    /// Ambush.
    pub const HIDE_WHILE_AMBUSHING: u16 = 1 << 0;
    /// Guard.
    pub const REMAIN_NEAR_REFERENCE: u16 = 1 << 3;
}

/// Where a package happens (PLDT, PLD2) [documented: xEdit wbPxDTLocationDecider;
/// GECK wiki, Category:Packages "Standard Location"].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "form", rename_all = "snake_case")]
pub enum LocationKind {
    /// Near a placed reference (`None`: the stored FormID is no form of the load order).
    NearReference(Option<FormId>),
    /// Anywhere in a cell; the radius is not used.
    InCell(Option<FormId>),
    /// Where the actor is when the package starts.
    NearCurrent,
    /// Where the actor was placed in the editor.
    NearEditor,
    /// An object by base form.
    ObjectId(Option<FormId>),
    /// An object by kind ([`object_type_name`]).
    ObjectType(u32),
    /// Near the actor's linked reference (XLKR).
    NearLinked,
    /// At the package's own (first) location.
    AtPackageLocation,
    Other(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Location {
    #[serde(flatten)]
    pub kind: LocationKind,
    /// Game units.
    pub radius: i32,
}

/// What a package acts on (PTDT, PTD2) [documented: xEdit PACK PTDT].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "form", rename_all = "snake_case")]
pub enum TargetKind {
    /// A placed reference; `0x00000014` is the player [observed: 511 targets of the
    /// masters].
    Reference(Option<FormId>),
    /// Any object with this base form (or of this form list, faction, leveled list).
    ObjectId(Option<FormId>),
    ObjectType(u32),
    /// The actor's linked reference (XLKR).
    LinkedReference,
    Other(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Target {
    #[serde(flatten)]
    pub kind: TargetKind,
    /// "Count / Distance": a count for object targets, a distance for actor targets
    /// [documented: xEdit field name; which applies per package type is in the type's
    /// GECK wiki page].
    pub count_distance: i32,
}

/// Names of the object-type codes of locations and targets [documented: xEdit
/// wbObjectTypeEnum]. The masters use 0, 5, 7, 10-14, 17, 18, 21-23 and, once, 28
/// [observed: `fnvtool ai packages` object_types].
pub fn object_type_name(code: u32) -> Option<&'static str> {
    const NAMES: [&str; 29] = [
        "None",
        "Activators",
        "Armor",
        "Books",
        "Clothing",
        "Containers",
        "Doors",
        "Ingredients",
        "Lights",
        "Misc",
        "Flora",
        "Furniture",
        "Weapons: Any",
        "Ammo",
        "NPCs",
        "Creatures",
        "Keys",
        "Alchemy",
        "Food",
        "All: Combat Wearable",
        "All: Wearable",
        "Weapons: Ranged",
        "Weapons: Melee",
        "Weapons: None",
        "Actor Effects: Any",
        "Actor Effects: Range Target",
        "Actor Effects: Range Touch",
        "Actor Effects: Range Self",
        "Actors: Any",
    ];
    NAMES.get(code as usize).copied()
}

/// When a package may run (PSDT) [documented: xEdit PACK PSDT; packages.md §1].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Schedule {
    /// −1 any, 0–11 January–December, 12–15 spring, summer, autumn, winter.
    pub month: i8,
    /// −1 any, 0–6 Sunday–Saturday, 7 weekdays, 8 weekends, 9 Monday / Wednesday /
    /// Friday, 10 Tuesday / Thursday.
    pub day_of_week: i8,
    /// 0 any, else the day of the month.
    pub date: i8,
    /// −1 any, else the starting hour 0–23.
    pub hour: i8,
    /// Hours.
    pub duration: u32,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule { month: -1, day_of_week: -1, date: 0, hour: -1, duration: 0 }
    }
}

impl Schedule {
    /// No restriction on when the package runs.
    pub fn is_any(&self) -> bool {
        self.month < 0 && self.day_of_week < 0 && self.date <= 0 && self.hour < 0
    }

    /// Hours since the starting hour (0 up to 24); `None` without a starting hour.
    pub fn hours_in(&self, hour: f64) -> Option<f64> {
        (self.hour >= 0).then(|| (hour - self.hour as f64).rem_euclid(24.0))
    }

    /// Whether the time of day is inside the schedule's window: from the starting hour
    /// for `duration` hours, across midnight if need be; any time of day without a
    /// starting hour [observed: nine scheduled packages of Goodsprings ran only inside
    /// that window, 35,668 samples, apart from the minutes it takes an actor to switch;
    /// packages.md §4].
    pub fn in_window(&self, hour: f64) -> bool {
        self.hours_in(hour).is_none_or(|h| h < self.duration as f64)
    }

    /// Whether the schedule lets the package run at `clock`: the window of the day and
    /// the day itself. A window that started before midnight belongs to the day it
    /// started on [hypothesis]. Day of the week: 0–6 Sunday–Saturday, 7 weekdays,
    /// 8 weekends, 9 Monday / Wednesday / Friday, 10 Tuesday / Thursday; month 0–11,
    /// 12–15 spring (March–May), summer, autumn, winter [documented: xEdit names;
    /// hypothesis: which months a season covers; no master package sets a month or a
    /// date]; date: the day of the month.
    pub fn allows(&self, clock: &Clock) -> bool {
        let Some(h) = self.hours_in(clock.hour) else {
            return self.day_allows(&clock.today);
        };
        if h >= self.duration as f64 {
            return false;
        }
        // The window started today when the clock has not passed midnight since.
        let started_today = clock.hour >= self.hour as f64;
        self.day_allows(if started_today { &clock.today } else { &clock.yesterday })
    }

    fn day_allows(&self, d: &Day) -> bool {
        let dow = match self.day_of_week {
            x if x < 0 => true,
            x @ 0..=6 => d.day_of_week == x as u8,
            7 => (1..=5).contains(&d.day_of_week),
            8 => d.day_of_week == 0 || d.day_of_week == 6,
            9 => matches!(d.day_of_week, 1 | 3 | 5),
            10 => matches!(d.day_of_week, 2 | 4),
            _ => false,
        };
        let month = match self.month {
            x if x < 0 => true,
            x @ 0..=11 => d.month == x as u8,
            // Seasons: spring is March to May, and so on.
            x @ 12..=15 => (d.month + 10) % 12 / 3 == (x - 12) as u8,
            _ => false,
        };
        dow && month && (self.date <= 0 || d.day == self.date as u8)
    }
}

/// A calendar day as schedules see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Day {
    /// 0 Sunday … 6 Saturday.
    pub day_of_week: u8,
    /// 0 January … 11 December.
    pub month: u8,
    /// Day of the month, from 1.
    pub day: u8,
}

/// The time a schedule is checked against.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Clock {
    /// GameHour, 0 up to 24.
    pub hour: f64,
    pub today: Day,
    pub yesterday: Day,
}

/// FURN marker flags [observed: docs/spec-notes/furniture.md §1].
pub mod furniture {
    /// Furniture to sit on.
    pub const SIT: u32 = 1 << 30;
    /// Furniture to sleep on.
    pub const SLEEP: u32 = 1 << 31;
}

/// Why a package may or may not be chosen ([`crate::game::Game::check_package`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageCheck {
    Valid,
    /// The form is not a package of the load order.
    Missing,
    OutOfSchedule,
    /// It needs the actor's linked reference and the actor has none.
    NoLinkedRef,
    /// A Once Per Day package the actor chose less than a day ago.
    OncePerDay,
    /// The actor found no way to the package's location a short while ago
    /// ([`crate::game::Game::block_package`]).
    Unreachable,
    ConditionsFail,
}

/// A package's idle collection (IDLF, IDLC, IDLT, IDLA) [documented: xEdit
/// wbIdleAnimation; GECK wiki, Idle Collection].
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct IdleCollection {
    pub run_in_sequence: bool,
    pub do_once: bool,
    /// Seconds between idles.
    pub timer: f32,
    /// IDLE forms.
    pub idles: Vec<FormId>,
}

/// Use Weapon fire pattern (PKW3) [documented: xEdit PACK PKW3].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
pub struct UseWeapon {
    pub always_hit: bool,
    pub do_no_damage: bool,
    pub crouch_to_reload: bool,
    pub hold_fire_when_blocked: bool,
    /// Volley fire rather than automatic fire.
    pub volley_fire: bool,
    /// Fire again and again rather than a number of bursts.
    pub repeat_fire: bool,
    pub bursts: u16,
    pub shots_per_volley: [u16; 2],
    /// Seconds, minimum and maximum.
    pub pause_between_volleys: [f32; 2],
}

/// Dialogue package data (PKDD) [documented: xEdit PACK PKDD].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
pub struct DialogueData {
    pub fov: f32,
    pub topic: Option<FormId>,
    pub no_headtracking: bool,
    pub dont_control_target_movement: bool,
    /// "Say To" rather than a conversation.
    pub say_to: bool,
}

/// What a package plays and says at begin, end or change (after POBA, POEA, POCA); the
/// result scripts are in [`crate::GameData::packages`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct PackageEventData {
    pub idle: Option<FormId>,
    pub topic: Option<FormId>,
}

/// One AI package.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct PackageDef {
    pub ty: PackageType,
    /// General flags ([`flags`]).
    pub flags: u32,
    /// Fallout behaviour flags ([`fallout`]).
    pub fallout_flags: u16,
    /// Type-specific flags as stored ([`type_flags`]); 0 when the record's PKDT is the
    /// 8-byte form, which ends after the Fallout behaviour flags [observed: 13 packages
    /// of the masters].
    pub type_flags: u16,
    pub location: Option<Location>,
    pub location2: Option<Location>,
    pub target: Option<Target>,
    pub target2: Option<Target>,
    pub schedule: Schedule,
    pub conditions: Vec<Condition>,
    /// CNAM: the combat style used while the package runs.
    pub combat_style: Option<FormId>,
    pub idles: Option<IdleCollection>,
    /// PKPT: a patrol that starts over at its end.
    pub patrol_repeatable: Option<bool>,
    /// PKE2.
    pub escort_distance: Option<u32>,
    /// PKFD: the trigger radius of a follow package's start location.
    pub follow_trigger_radius: Option<f32>,
    pub use_weapon: Option<UseWeapon>,
    pub dialogue: Option<DialogueData>,
    /// Begin, end, change.
    pub events: [PackageEventData; 3],
}

impl PackageDef {
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    fn type_flag(&self, types: &[PackageType], bit: u16) -> bool {
        types.contains(&self.ty) && self.type_flags & bit != 0
    }

    /// A Wander or Sandbox switch (`type_flags::NO_EATING` … `NO_WANDERING`). The same
    /// bits are set on packages of other types in the masters, where no flag is
    /// documented for them (packages.md §2); they count only for these two types here.
    pub fn sandbox_flag(&self, bit: u16) -> bool {
        self.type_flag(&[PackageType::Wander, PackageType::Sandbox], bit)
    }

    /// Allow Buying / Killing / Stealing (Find, Escort, Eat, Use Item At).
    pub fn search_flag(&self, bit: u16) -> bool {
        self.type_flag(&[PackageType::Find, PackageType::Escort, PackageType::Eat, PackageType::UseItemAt], bit)
    }

    pub fn sit_down(&self) -> bool {
        self.type_flag(&[PackageType::UseItemAt], type_flags::SIT_DOWN)
    }

    pub fn hide_while_ambushing(&self) -> bool {
        self.type_flag(&[PackageType::Ambush], type_flags::HIDE_WHILE_AMBUSHING)
    }

    pub fn remain_near_reference(&self) -> bool {
        self.type_flag(&[PackageType::Guard], type_flags::REMAIN_NEAR_REFERENCE)
    }

    /// The names of the general flags that are set.
    pub fn flag_names(&self) -> Vec<&'static str> {
        flags::NAMES.iter().filter(|(b, _)| self.flags & b != 0).map(|(_, n)| *n).collect()
    }

    /// The names of the type-specific flags that are set and documented for the type.
    pub fn type_flag_names(&self) -> Vec<&'static str> {
        use type_flags as t;
        let mut out = Vec::new();
        let mut add = |on: bool, name: &'static str| {
            if on {
                out.push(name)
            }
        };
        add(self.sandbox_flag(t::NO_EATING), "No Eating");
        add(self.sandbox_flag(t::NO_SLEEPING), "No Sleeping");
        add(self.sandbox_flag(t::NO_CONVERSATION), "No Conversation");
        add(self.sandbox_flag(t::NO_IDLE_MARKERS), "No Idle Markers");
        add(self.sandbox_flag(t::NO_FURNITURE), "No Furniture");
        add(self.sandbox_flag(t::NO_WANDERING), "No Wandering");
        add(self.sit_down(), "Sit Down");
        add(self.search_flag(t::ALLOW_BUYING), "Allow Buying");
        add(self.search_flag(t::ALLOW_KILLING), "Allow Killing");
        add(self.search_flag(t::ALLOW_STEALING), "Allow Stealing");
        add(self.hide_while_ambushing(), "Hide While Ambushing");
        add(self.remain_near_reference(), "Remain Near Reference to Guard");
        out
    }

    /// Whether the package depends on the actor's own linked reference (XLKR): a
    /// location "near linked reference" or a target "linked reference". A Patrol is not
    /// one by its type: its route starts at its location, and only a patrol located
    /// "near linked reference" (the default patrols) needs the actor's link [observed:
    /// GSVictorRef, who has no linked reference, ran VGSVictorCircuitPatrol, located near
    /// a patrol marker, for 13,225 samples, `fnvtool ai observed`; documented: GECK
    /// wiki, Patrol Package, for the linked case].
    pub fn uses_linked_ref(&self) -> bool {
        [self.location, self.location2].iter().flatten().any(|l| l.kind == LocationKind::NearLinked)
            || [self.target, self.target2].iter().flatten().any(|t| t.kind == TargetKind::LinkedReference)
    }
}

/// The integer code of an enumeration value of PACK subrecord `sub`, field `field`.
pub(crate) type CodeOf<'a> = &'a dyn Fn(&[u8; 4], &str, &Json) -> Option<i64>;

/// What the reader needs from its caller: the schema's enumeration codes and the
/// plugin's FormID mapping.
pub(crate) struct ReadCtx<'a> {
    pub code: CodeOf<'a>,
    /// A plugin-local FormID in the load order.
    pub map: &'a dyn Fn(u32) -> Option<u32>,
}

fn int(v: &Json, k: &str) -> i64 {
    v.get(k).and_then(Json::as_i64).unwrap_or(0)
}

fn num(v: &Json, k: &str) -> f32 {
    v.get(k).and_then(Json::as_f64).unwrap_or(0.0) as f32
}

fn bits(v: Option<&Json>) -> u32 {
    match v {
        Some(f) => f.get("value").and_then(Json::as_u64).or_else(|| f.as_u64()).unwrap_or(0) as u32,
        None => 0,
    }
}

fn hex(v: &Json) -> Option<u32> {
    u32::from_str_radix(v.as_str()?.strip_prefix("0x")?, 16).ok()
}

/// A FormID field of the schema (`"0x…"`), mapped; 0 is "none".
fn form(v: Option<&Json>, ctx: &ReadCtx) -> Option<FormId> {
    v.and_then(hex).filter(|&x| x != 0).and_then(ctx.map).map(FormId)
}

/// A FormID kept raw in the schema (a `u32` whose meaning depends on a type field).
/// The player's reference 0x14 and the other built-in forms below 0x800 belong to the
/// game itself and map through the first master [documented: xEdit hardcoded forms].
fn raw_form(raw: u32, ctx: &ReadCtx) -> Option<FormId> {
    (raw != 0).then(|| (ctx.map)(raw)).flatten().map(FormId)
}

fn location(sub: &[u8; 4], v: &Json, ctx: &ReadCtx) -> Location {
    let raw = int(v, "location_raw") as u32;
    let code = v.get("type").and_then(|t| (ctx.code)(sub, "type", t)).unwrap_or(-1);
    let kind = match code {
        0 => LocationKind::NearReference(raw_form(raw, ctx)),
        1 => LocationKind::InCell(raw_form(raw, ctx)),
        2 => LocationKind::NearCurrent,
        3 => LocationKind::NearEditor,
        4 => LocationKind::ObjectId(raw_form(raw, ctx)),
        5 => LocationKind::ObjectType(raw),
        6 => LocationKind::NearLinked,
        7 => LocationKind::AtPackageLocation,
        c => LocationKind::Other(c),
    };
    Location { kind, radius: int(v, "radius") as i32 }
}

fn target(sub: &[u8; 4], v: &Json, ctx: &ReadCtx) -> Target {
    let raw = int(v, "target_raw") as u32;
    let code = v.get("type").and_then(|t| (ctx.code)(sub, "type", t)).unwrap_or(-1);
    let kind = match code {
        0 => TargetKind::Reference(raw_form(raw, ctx)),
        1 => TargetKind::ObjectId(raw_form(raw, ctx)),
        2 => TargetKind::ObjectType(raw),
        3 => TargetKind::LinkedReference,
        c => TargetKind::Other(c),
    };
    Target { kind, count_distance: int(v, "count_distance") as i32 }
}

fn flag_set(v: Option<&Json>, name: &str) -> bool {
    v.and_then(|f| f.get("set")).and_then(Json::as_array).is_some_and(|a| a.iter().any(|n| n.as_str() == Some(name)))
}

/// Read a package from its decoded subrecords, in record order. `conditions` receives
/// each CTDA before the begin / end / change sections (whose CTDA-free scripts are read
/// elsewhere) and returns the parsed condition.
pub(crate) fn read(
    decoded: &[(&[u8; 4], Option<Json>)],
    ctx: &ReadCtx,
    conditions: &mut dyn FnMut(&Option<Json>) -> Option<Condition>,
) -> PackageDef {
    let mut p = PackageDef::default();
    // 0 before the sections, then 1 begin, 2 end, 3 change.
    let mut section = 0usize;
    for (sig, value) in decoded {
        let sig = *sig;
        match sig {
            b"POBA" => section = 1,
            b"POEA" => section = 2,
            b"POCA" => section = 3,
            b"CTDA" if section == 0 => p.conditions.extend(conditions(value)),
            _ => {}
        }
        let Some(v) = value else { continue };
        if section > 0 {
            let e = &mut p.events[section - 1];
            match sig {
                b"INAM" => e.idle = form(Some(v), ctx),
                b"TNAM" => e.topic = form(Some(v), ctx),
                _ => {}
            }
            continue;
        }
        match sig {
            b"PKDT" => {
                p.flags = bits(v.get("general_flags"));
                let code = v.get("type").and_then(|t| (ctx.code)(sig, "type", t)).unwrap_or(-1);
                p.ty = PackageType::from_code(code.clamp(0, 255) as u8);
                p.fallout_flags = bits(v.get("fallout_behavior_flags")) as u16;
                p.type_flags = int(v, "type_specific_flags_raw") as u16;
            }
            b"PLDT" => p.location = Some(location(sig, v, ctx)),
            // Stored twice in a record, both copies the same [observed: every master
            // package with one, `fnvtool ai survey` location2_copies]; the first is kept.
            b"PLD2" if p.location2.is_none() => p.location2 = Some(location(sig, v, ctx)),
            b"PTDT" => p.target = Some(target(sig, v, ctx)),
            b"PTD2" => p.target2 = Some(target(sig, v, ctx)),
            b"PSDT" => {
                p.schedule = Schedule {
                    month: int(v, "month") as i8,
                    day_of_week: v.get("day_of_week").and_then(|t| (ctx.code)(sig, "day_of_week", t)).unwrap_or(-1) as i8,
                    date: int(v, "date") as i8,
                    hour: int(v, "time") as i8,
                    duration: int(v, "duration_hours") as u32,
                }
            }
            b"CNAM" => p.combat_style = form(Some(v), ctx),
            b"IDLF" => {
                let c = p.idles.get_or_insert_with(Default::default);
                c.run_in_sequence = flag_set(Some(v), "Run In Sequence");
                c.do_once = flag_set(Some(v), "Do Once");
            }
            b"IDLT" => p.idles.get_or_insert_with(Default::default).timer = v.as_f64().unwrap_or(0.0) as f32,
            b"IDLA" => {
                p.idles.get_or_insert_with(Default::default).idles =
                    v.as_array().into_iter().flatten().filter_map(|x| form(Some(x), ctx)).collect()
            }
            b"PKPT" => p.patrol_repeatable = Some(v.get("repeatable").and_then(|t| (ctx.code)(sig, "repeatable", t)).is_some_and(|c| c != 0)),
            b"PKE2" => p.escort_distance = v.as_u64().map(|x| x as u32),
            b"PKFD" => p.follow_trigger_radius = v.as_f64().map(|x| x as f32),
            b"PKW3" => {
                let f = v.get("flags");
                p.use_weapon = Some(UseWeapon {
                    always_hit: flag_set(f, "Always Hit"),
                    do_no_damage: flag_set(f, "Do No Damage"),
                    crouch_to_reload: flag_set(f, "Crouch To Reload"),
                    hold_fire_when_blocked: flag_set(f, "Hold Fire When Blocked"),
                    volley_fire: v.get("fire_rate").and_then(|t| (ctx.code)(sig, "fire_rate", t)) == Some(1),
                    repeat_fire: v.get("fire_count").and_then(|t| (ctx.code)(sig, "fire_count", t)) == Some(1),
                    bursts: int(v, "number_of_bursts") as u16,
                    shots_per_volley: [int(v, "shots_per_volley_min") as u16, int(v, "shots_per_volley_max") as u16],
                    pause_between_volleys: [num(v, "pause_between_volleys_min"), num(v, "pause_between_volleys_max")],
                });
            }
            b"PKDD" => {
                let f = v.get("flags");
                p.dialogue = Some(DialogueData {
                    fov: num(v, "fov"),
                    topic: form(v.get("topic"), ctx),
                    no_headtracking: flag_set(f, "No Headtracking"),
                    dont_control_target_movement: flag_set(f, "Don't Control Target Movement"),
                    say_to: v.get("dialogue_type").and_then(|t| (ctx.code)(sig, "dialogue_type", t)) == Some(1),
                });
            }
            _ => {}
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_codes_round_trip() {
        for t in PackageType::ALL {
            assert_eq!(PackageType::from_code(t.code()), t);
        }
        assert_eq!(PackageType::from_code(11), PackageType::Other(11));
        assert_eq!(PackageType::Travel.code(), 6);
        assert_eq!(PackageType::Travel.runtime_code(), Some(14));
        assert_eq!(PackageType::Find.runtime_code(), Some(0));
        // The run-time codes are distinct.
        let mut codes: Vec<u8> = PackageType::ALL.iter().filter_map(|t| t.runtime_code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 16);
    }

    #[test]
    fn type_specific_flags_count_only_for_their_type() {
        let mut p = PackageDef { ty: PackageType::Travel, type_flags: 0x3F, ..Default::default() };
        assert!(p.type_flag_names().is_empty());
        p.ty = PackageType::Sandbox;
        assert_eq!(p.type_flag_names().len(), 6);
        p.ty = PackageType::UseItemAt;
        assert_eq!(p.type_flag_names(), vec!["Sit Down"]);
        p.ty = PackageType::Guard;
        assert!(p.remain_near_reference());
        p.ty = PackageType::Ambush;
        assert!(p.hide_while_ambushing());
    }

    #[test]
    fn linked_reference_use_comes_from_locations_and_targets() {
        let near = |kind| Some(Location { kind, radius: 0 });
        let patrol = PackageDef { ty: PackageType::Patrol, location: near(LocationKind::NearReference(Some(FormId(1)))), ..Default::default() };
        assert!(!patrol.uses_linked_ref());
        let default_patrol = PackageDef { ty: PackageType::Patrol, location: near(LocationKind::NearLinked), ..Default::default() };
        assert!(default_patrol.uses_linked_ref());
        let follow = PackageDef {
            ty: PackageType::Follow,
            target: Some(Target { kind: TargetKind::LinkedReference, count_distance: 0 }),
            ..Default::default()
        };
        assert!(follow.uses_linked_ref());
    }

    #[test]
    fn schedule_any() {
        assert!(Schedule::default().is_any());
        assert!(!Schedule { hour: 8, ..Default::default() }.is_any());
        assert!(!Schedule { day_of_week: 7, ..Default::default() }.is_any());
    }

    fn at(hour: f64, dow: u8) -> Clock {
        let day = |d: u8| Day { day_of_week: d, month: 9, day: 19 };
        Clock { hour, today: day(dow), yesterday: day((dow + 6) % 7) }
    }

    #[test]
    fn schedule_window_runs_from_the_hour_for_the_duration() {
        let chair = Schedule { hour: 8, duration: 4, ..Default::default() };
        assert!(!chair.in_window(7.99));
        assert!(chair.in_window(8.0) && chair.in_window(11.99));
        assert!(!chair.in_window(12.0));
        // Across midnight.
        let sleep = Schedule { hour: 22, duration: 8, ..Default::default() };
        assert!(sleep.in_window(22.0) && sleep.in_window(23.5) && sleep.in_window(0.0) && sleep.in_window(5.99));
        assert!(!sleep.in_window(6.0) && !sleep.in_window(21.99));
        // No starting hour: any time; a full day: any time; no duration: never.
        assert!(Schedule::default().in_window(3.0));
        assert!(Schedule { hour: 6, duration: 24, ..Default::default() }.in_window(5.0));
        assert!(!Schedule { hour: 6, duration: 0, ..Default::default() }.in_window(6.0));
        assert!(sleep.allows(&at(23.0, 2)) && !sleep.allows(&at(12.0, 2)));
    }

    #[test]
    fn schedule_days() {
        // Weekdays, 22:00 for 8 hours: Friday night runs into Saturday morning; Sunday
        // night's window started on a weekend day... Sunday (0) is not a weekday.
        let s = Schedule { hour: 22, duration: 8, day_of_week: 7, ..Default::default() };
        assert!(s.allows(&at(23.0, 5)));
        assert!(s.allows(&at(3.0, 6)), "Saturday 03:00 belongs to Friday's window");
        assert!(!s.allows(&at(23.0, 6)));
        assert!(!s.allows(&at(3.0, 1)), "Monday 03:00 belongs to Sunday's window");
        let weekend = Schedule { day_of_week: 8, ..Default::default() };
        assert!(weekend.allows(&at(12.0, 0)) && weekend.allows(&at(12.0, 6)) && !weekend.allows(&at(12.0, 3)));
        let mwf = Schedule { day_of_week: 9, ..Default::default() };
        assert!(mwf.allows(&at(1.0, 1)) && mwf.allows(&at(1.0, 3)) && mwf.allows(&at(1.0, 5)) && !mwf.allows(&at(1.0, 2)));
        let tt = Schedule { day_of_week: 10, ..Default::default() };
        assert!(tt.allows(&at(1.0, 2)) && tt.allows(&at(1.0, 4)) && !tt.allows(&at(1.0, 5)));
        let tuesday = Schedule { day_of_week: 2, ..Default::default() };
        assert!(tuesday.allows(&at(1.0, 2)) && !tuesday.allows(&at(1.0, 3)));
        // Month (October = 9 in the test clock), season and date.
        assert!(Schedule { month: 9, ..Default::default() }.allows(&at(1.0, 0)));
        assert!(!Schedule { month: 8, ..Default::default() }.allows(&at(1.0, 0)));
        assert!(Schedule { month: 14, ..Default::default() }.allows(&at(1.0, 0)), "October is autumn");
        assert!(!Schedule { month: 12, ..Default::default() }.allows(&at(1.0, 0)));
        assert!(Schedule { date: 19, ..Default::default() }.allows(&at(1.0, 0)));
        assert!(!Schedule { date: 20, ..Default::default() }.allows(&at(1.0, 0)));
    }
}
