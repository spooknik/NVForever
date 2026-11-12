//! Creatures' alert look (docs/spec-notes/actors.md §8.10): a creature out of the Normal
//! detection state "draws" its unarmed stance as people draw a weapon (the owner: "it's
//! like they are drawing a weapon, i.e. the hands"): its `h2hequip.kf` once, then
//! `h2haim.kf` held over its locomotion by track priority; calm again, `h2hunequip.kf`
//! (else `h2hholster.kf`) once, and its locomotion alone after that.
//!
//! - **Which creatures**: every creature skeleton folder with an `h2haim.kf` at its root
//!   (deathclaw, ghoul, gecko, radscorpion, mantis, yao guai … 25 folders in the base
//!   meshes archive) [observed: `fnv.bsa.listing/1` of `Fallout - Meshes.bsa`, 2026-10-16].
//!   No `Weapon` node is needed: the clips move the body only. Creatures that move on
//!   `h2h` clips when alert (`creature_pace.rs`, `Locomotion::alert`: cazador, dog and
//!   coyote, mirelurk king) have that set as their alert look while they fight, and draw
//!   the stance only while they warn the player ([`CreatureStance::warn_only`]; issue
//!   #1073: the coyote's head-down warning; the dog's `h2haim.kf` keys every bone at 25,
//!   over its idle's 20 and under its walk's 30 and run's 31, so it shows while it stands
//!   and its run shows as it charges) [design]. Drawn while fighting, a dog
//!   that stopped at the player (body gap 5–17 units, `npc_combat` measures it from the
//!   posed torso) was 27–42 away moments later, past the hit gap, and ran in again: 7
//!   blows instead of 15 in smoke `combat-dogs` [observed in NVForever, 2026-10-16;
//!   hypothesis: its aim pose holds the torso further back than its idle].
//! - **At rest** nothing of the stance plays: the creatures' holster clips are poses of
//!   the whole body that would stop their legs [design]. The draw cycle is
//!   `fnv_actor::weapon::WeaponAnim`, as for people's weapons (`weapons.rs`), on the same
//!   blend layers 2 and 3 (a creature has no `WeaponHold`).
//! - **When**: `fnv_game` says the creature stands alerted (a combat state, SetAlert:
//!   `Game::alert_stance`), as
//!   `alert.rs` draws people's weapons; and while it warns the player
//!   (`Game::warning_to`, not alerted), drawn once and held [owner decision 2026-10-25:
//!   the original's warning gecko drew and put away its stance every 2 s (capture
//!   `warnings`, idles.md §14.3), which looks like an unintended quirk; draw and hold
//!   instead]. The draw clip is the gecko's hiss.
//! - Attacks (`attack_anims.rs`) play over the aim on layers 4 and 5 and are found as
//!   before: a creature with a stance still has no `WeaponHold`.

use crate::limbs::ActorRig;
use crate::locomotion::{Dying, Locomotion};
use crate::weapons::WeaponHold;
use bevy::prelude::*;
use fnv_actor::weapon::{ClipFacts, EquipClip, Stage, StanceFacts, WeaponAnim};
use fnv_sim::FormId;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts};
use nvf_assets::anim_play::{BlendPlayer, BoundClip, Layer};
use nvf_assets::model::ModelCtx;
use nvf_play::script::ScriptState;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

pub struct CreatureStancePlugin;

impl Plugin for CreatureStancePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Stances>().add_systems(Update, (attach.in_set(StanceAttach), alerted).chain().after(nvf_play::PlaySet::Scripts));
    }
}

/// Where creatures are given their stance (`outfits::arm_creatures` runs after it, so a
/// creature that takes up a weapon loses the stance at once).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StanceAttach;

/// The unarmed stance clips of one creature skeleton, shared by the creatures using it.
pub struct CreatureClips {
    /// Equip, aim and unequip; never holster (module notes).
    pub clips: BTreeMap<EquipClip, Arc<BoundClip>>,
    pub facts: StanceFacts,
}

/// On a creature's root: its unarmed stance and draw cycle (laid on by `locomotion::drive`).
#[derive(Component)]
pub struct CreatureStance {
    pub clips: Arc<CreatureClips>,
    pub anim: WeaponAnim,
    /// Whether the stance should be drawn (alerted).
    pub drawn: bool,
    /// The stage last logged.
    logged: Stage,
    /// What the creature does with its stance, last logged.
    mode: Mode,
    /// Drawn only while it warns the player: a creature with an `h2h` movement set
    /// (module notes).
    pub warn_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Calm,
    /// Warning the player: drawn and held (module notes).
    Warning,
    Alerted,
}

impl CreatureStance {
    /// The stance `clips`, at rest, to be drawn when `drawn` (a creature taking up its
    /// own attack again after a body weapon, `body_weapons.rs`).
    pub fn new(clips: Arc<CreatureClips>, drawn: bool) -> CreatureStance {
        CreatureStance { clips, anim: WeaponAnim::new(false), drawn, logged: Stage::Holstered, mode: Mode::Calm, warn_only: false }
    }

    /// Advance the draw cycle by `dt`.
    pub fn step(&mut self, dt: f32) {
        self.anim.step(self.drawn, dt, &self.clips.facts);
    }

    /// The stance's clips as player layers 2 and 3 (0 and 1 are locomotion's); at rest
    /// none.
    pub fn layers(&self, player: &mut BlendPlayer) {
        let c = &self.clips;
        let [a, b] = self.anim.layers(&c.facts);
        let layer = |l: Option<(EquipClip, f32, f32)>| l.and_then(|(clip, phase, weight)| Some(Layer { clip: c.clips.get(&clip)?.clone(), phase, weight }));
        player.layers[2] = layer(a);
        player.layers[3] = layer(b);
    }
}

/// Stances by skeleton folder (`None`: the folder has no `h2haim.kf`).
#[derive(Resource, Default)]
struct Stances(HashMap<String, Option<Arc<CreatureClips>>>);

/// On a creature looked at that has no stance.
#[derive(Component)]
pub struct NoStance;

/// The folder of a skeleton path (`creatures\deathclaw\skeleton.nif` → `meshes\creatures\deathclaw`).
fn folder_of(skeleton: &str) -> String {
    let s = fnv_vfs::normalize(&fnv_actor::resolve::mesh_path(skeleton));
    s.rfind('\\').map_or(String::new(), |i| s[..i].to_string())
}

/// The clip files of the stance in `folder`: (clip, path), the holster clip standing in
/// for a missing unequip.
pub fn clip_paths(folder: &str) -> [(EquipClip, Vec<String>); 3] {
    let p = |stem: &str| format!("{folder}\\h2h{stem}.kf");
    [(EquipClip::Equip, vec![p("equip")]), (EquipClip::Aim, vec![p("aim")]), (EquipClip::Unequip, vec![p("unequip"), p("holster")])]
}

fn load(ctx: &mut ActorCtx, rig: &ActorRig, folder: &str) -> Option<CreatureClips> {
    let mut clips = BTreeMap::new();
    let mut facts = StanceFacts::new();
    for (c, paths) in clip_paths(folder) {
        if let Some(clip) = paths.iter().find_map(|p| ctx.clip(p)) {
            facts.insert(c, ClipFacts::from_parts(clip.start, clip.stop, &clip.text_keys));
            clips.insert(c, Arc::new(bind_clip(&rig.skel, clip)));
        }
    }
    clips.contains_key(&EquipClip::Aim).then_some(CreatureClips { clips, facts })
}

type Fresh<'w> = (Entity, &'w ActorRig, &'w Locomotion);
/// Animated creatures not looked at yet.
type Unseen = (Without<CreatureStance>, Without<NoStance>, Without<WeaponHold>);

/// Give every animated creature its stance, once.
fn attach(
    mut commands: Commands,
    fresh: Query<Fresh, Unseen>,
    mut stances: ResMut<Stances>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
) {
    if fresh.is_empty() {
        return;
    }
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for (e, rig, lo) in &fresh {
        let folder = folder_of(&rig.skeleton);
        // A creature with an `h2h` movement set has its alert look in that, and draws the
        // stance only to warn (module notes).
        let warn_only = lo.alert.is_some();
        let found = folder.starts_with("meshes\\creatures\\").then(|| {
            stances
                .0
                .entry(folder.clone())
                .or_insert_with(|| {
                    let s = load(&mut ctx, rig, &folder).map(Arc::new);
                    if let Some(s) = &s {
                        info!("creature stance: {folder}: clips {:?}", s.clips.keys().collect::<Vec<_>>());
                    }
                    s
                })
                .clone()
        });
        match found.flatten() {
            Some(clips) => {
                commands.entity(e).insert(CreatureStance { clips, anim: WeaponAnim::new(false), drawn: false, logged: Stage::Holstered, mode: Mode::Calm, warn_only });
            }
            None => {
                commands.entity(e).insert(NoStance);
            }
        }
    }
}

/// Draw the stance of alerted creatures and of those warning the player, put it away when
/// they are calm; log each change.
fn alerted(script: Option<Res<ScriptState>>, mut q: Query<(&ActorRig, &mut CreatureStance, Option<&Name>), Without<Dying>>) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    for (rig, mut s, name) in &mut q {
        let id = FormId(rig.reference);
        // As `detection.rs` names actors: the base's editor ID from the entity's name.
        let label = || format!("{} {:08X}", name.and_then(|n| n.as_str().split_whitespace().nth(2)).unwrap_or(""), rig.reference).trim().to_string();
        // Alerted or in a combat state (`Game::alert_stance`), not GetIsAlerted alone
        // (#2121): a fighting creature keeps its alerted stance.
        let mode = if game.alert_stance(id) {
            Mode::Alerted
        } else if game.draws_weapon(id) {
            Mode::Warning
        } else {
            Mode::Calm
        };
        if mode != s.mode {
            s.mode = mode;
            info!("creature stance: {} {}", label(), match mode {
                Mode::Alerted => "draws (alerted)",
                Mode::Warning => "draws (warning the player)",
                Mode::Calm => "puts away (calm)",
            });
        }
        s.drawn = if s.warn_only { mode == Mode::Warning } else { mode != Mode::Calm };
        if s.anim.stage != s.logged {
            s.logged = s.anim.stage;
            info!("creature stance: {} {:?}", label(), s.logged);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_holster_clip_stands_in_for_a_missing_unequip() {
        let p = clip_paths(r"meshes\creatures\deathclaw");
        assert_eq!(p[0].1, [r"meshes\creatures\deathclaw\h2hequip.kf"]);
        assert_eq!(p[2].1, [r"meshes\creatures\deathclaw\h2hunequip.kf", r"meshes\creatures\deathclaw\h2hholster.kf"]);
        assert_eq!(folder_of(r"Creatures\Ghoul\Skeleton.NIF"), r"meshes\creatures\ghoul");
    }

    #[test]
    fn equip_then_aim_then_unequip_then_rest() {
        let f = |duration: f32| ClipFacts { duration, blend_time: 0.2, prn: Vec::new(), switch: None };
        let facts = StanceFacts::from([(EquipClip::Equip, f(1.0)), (EquipClip::Aim, f(2.0)), (EquipClip::Unequip, f(1.0))]);
        let clips = Arc::new(CreatureClips { clips: BTreeMap::new(), facts });
        let mut s = CreatureStance { clips, anim: WeaponAnim::new(false), drawn: true, logged: Stage::Holstered, mode: Mode::Calm, warn_only: false };
        let mut stages = vec![s.anim.stage];
        for i in 0..300 {
            s.drawn = i < 150;
            s.step(1.0 / 60.0);
            if stages.last() != Some(&s.anim.stage) {
                stages.push(s.anim.stage);
            }
        }
        assert_eq!(stages, [Stage::Holstered, Stage::Equipping, Stage::Drawn, Stage::Unequipping, Stage::Holstered]);
    }
}
