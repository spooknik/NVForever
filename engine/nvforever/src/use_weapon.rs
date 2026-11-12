//! Scripted weapon use (docs/design/use-weapon.md): an actor with a weapon use
//! (`fnv_game::use_weapon`: the UseWeapon function, a Use Weapon package) that the AI has
//! brought to its place and turned to its target fires its shots at it, as the combat
//! AI's gunmen do (`npc_combat`: the weapon's own cycle, aimed from the eyes at the
//! target's chest, drawn from the muzzle), counting them on the game's use.
//!
//! - **When it fires**: with its weapon drawn (`alert.rs` draws it for a weapon use),
//!   facing its target (within [`USE_FACING`]), not restrained (SetRestrained
//!   "blocks weapon use from Use Weapon packages and UseWeapon" [documented: catalogue
//!   notes on SetRestrained]), its target where the use wants it (the function's last
//!   argument, within `fnv_game::use_weapon::TARGET_HERE_RADIUS`), and, with Hold Fire
//!   When Blocked, nobody else standing in its line of fire (`combat_tactics`) [design:
//!   what "blocked" covers is not documented; walls are not looked at]. Between volleys
//!   it waits its pause.
//! - **The weapon**: the use's weapon when it is a gun, else the gun drawn on it; any
//!   other weapon, or none, swings (**Melee** below) [design]. Its drawn model is the one it was spawned with: a use
//!   naming another weapon fires that weapon's shots from the model it holds (models are
//!   not swapped live yet).
//! - **Always Hit**: no spread at all, aimed straight at the target's chest [design: the
//!   shot can still meet cover between them]. **Do No Damage**: the shots are harmless
//!   (`nvf_play::combat::ActorFire::harmless`). **Crouch To Reload**: it crouches
//!   (`Game::sneaks`) while its weapon reloads.
//! - **The end**: all shots fired, or its target dead or gone: the game ends the use
//!   (`Game::end_weapon_use`).
//!
//! **Melee** (docs/design/use-weapon.md §3.1): a use naming a melee or unarmed weapon,
//! or naming none on an actor with no gun drawn (its melee weapon drawn, else its fists,
//! a creature's own attack), swings instead of firing: each of its "shots" is one blow
//! [documented: the page counts "how many times to use the weapon"]. As the combat AI's
//! melee fighters (`npc_combat`): the reach and spacing of `npc_combat::melee_spacing` /
//! `creature_spacing`, which this sets on the game's use (`WeaponUse::reach`) for the AI
//! to close in by (a script's use; a package's swings from where it stands); a blow every
//! `npc_combat::STRIKE_SECONDS` over the weapon's attack speed, sooner at a clip's `a:`
//! key (a combo, `attack_anims::SwingPick`), landing at its clip's `Hit` key while the
//! target is within reach. No block or attack-chance roll: the script wants the blows
//! [hypothesis]. **Always Hit**: a blow lands even when the target stepped out of reach
//! [hypothesis]. **Do No Damage**: the swing plays, its blow strikes nothing [design: a
//! harmless blow has no sound or impact yet]. Hold Fire When Blocked and the target's
//! place hold the swings as they hold shots; Crouch To Reload has nothing to reload.
//! Thrown weapons fire their projectile as guns do (`is_gun`: they have one).

use crate::attack_anims::{AttackTimes, Move, PlayAttack, SwingClip, SwingPick, DEFAULT_HIT};
use crate::combat_tactics as tactics;
use crate::limbs::ActorRig;
use crate::locomotion::Dying;
use crate::npc_combat::{creature_reach, creature_spacing, engage, facing_off, is_gun, melee_spacing, next_unit, CHEST, CREATURE_REACH, MUZZLE_AHEAD, STRIKE_SECONDS};
use crate::ragdoll::Corpse;
use crate::weapons::{HeldWeapon, Muzzle, WeaponHold};
use crate::world::bevy_to_game;
use bevy::prelude::*;
use fnv_actor::weapon::Stage;
use fnv_combat::attack::{ClipTiming, Cycle, Event, Input, Timing};
use fnv_combat::spread::{self, Stance};
use fnv_combat::vec::{self, V3};
use fnv_game::use_weapon::{After, Source, TARGET_HERE_RADIUS};
use fnv_sim::FormId;
use nvf_play::blocking::ActorBlocks;
use nvf_play::combat::{ActorFire, ActorStrike, Combat, CombatStats};
use nvf_play::phys::PhysState;
use nvf_play::player::{Player, EYE_HEIGHT};
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;
use std::collections::{BTreeMap, HashMap};

/// A scripted weapon use fires and swings only while its target is within this angle of
/// straight ahead, radians (30°: the combat AI's arc before it held its gunmen to 3°,
/// kept here, where nothing but the walker turns the actor) [design].
pub const USE_FACING: f32 = 30.0 * std::f32::consts::PI / 180.0;

pub struct UseWeaponPlugin;

impl Plugin for UseWeaponPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shooters>()
            .add_systems(Update, shoot.after(nvf_play::PlaySet::Scripts).run_if(resource_exists::<Combat>))
            .add_systems(Last, report);
    }
}

/// One actor carrying out a weapon use.
struct Shooter {
    /// The use's number (`WeaponUse::seq`) and the gun it fires.
    seq: u64,
    weapon: FormId,
    cycle: Cycle,
    pulled: bool,
    /// Seconds before it may pull the trigger again: a semi-automatic gun's delay, or
    /// the pause between volleys.
    wait: f32,
    /// What it last waited for, told once each time it changes.
    held: Option<&'static str>,
}

/// One actor carrying out a melee weapon use (module notes).
struct Swinger {
    seq: u64,
    /// Seconds before its next blow may begin.
    strike_in: f32,
    /// Blows begun, with the seconds until each lands.
    blows: Vec<(f32, ActorStrike)>,
    /// Its last blow's swing, for a combo.
    swing: Option<Swing>,
    held: Option<&'static str>,
    /// All its blows are begun: the use ends once they have landed.
    spent: bool,
}

/// A use's last melee blow: seconds since it began, its clip once `attack_anims` started
/// it, and the blows of its combo so far.
#[derive(Clone, Copy)]
struct Swing {
    t: f32,
    clip: Option<SwingClip>,
    run: u32,
}

impl Swing {
    /// What a blow begun now goes on with: the group its clip's `a:` key named, between
    /// that key and the clip's end (as `npc_combat`'s fighters).
    fn next(&self) -> Option<fnv_combat::melee::NextAttack> {
        let c = self.clip?;
        c.chain.filter(|(at, _)| self.t >= *at && self.t <= c.len).map(|(_, n)| n)
    }
}

/// What a use is carried out with.
enum Arm {
    Gun(FormId),
    /// A melee or unarmed weapon (`None`: the Fists).
    Melee(Option<FormId>),
    /// A creature's own attack: its damage and reach.
    Creature(f32, f32),
}

#[derive(Resource, Default)]
pub struct Shooters {
    by: HashMap<u32, Shooter>,
    swingers: HashMap<u32, Swinger>,
    stats: Stats,
    rng: u64,
}

impl Shooters {
    /// Whether `actor`'s melee use `seq` has begun its last blow and that blow has yet to
    /// land: the use is done (`WeaponUse::shot`), but `shoot` goes on with it until then.
    pub fn landing(&self, actor: u32, seq: u64) -> bool {
        self.swingers.get(&actor).is_some_and(|x| x.spent && x.seq == seq)
    }
}

/// Whether [`shoot`] acts on the weapon use `w` this frame: the actor is in place
/// (`WeaponUse::in_place`) and the use's target stands where the shooter aims at people
/// (`target_standing`: a drawn actor neither dying nor a corpse, or the player; its
/// `places`). A target that is no actor (a marker: the Ultra-Luxe kitchen's torchers
/// aim at one) or is not drawn, and an actor not yet in place, get nothing from it.
/// `npc_combat::fight` keeps its attacks off a package's use in a combat state only while
/// this holds (`npc_combat::weapon_use_holds_fight`), so one of the two always has an
/// actor in Combat, though either may still hold its fire (not facing, someone in the way)
/// (#2145 review) [design].
pub fn acts_on(w: &fnv_game::use_weapon::WeaponUse, target_standing: bool) -> bool {
    w.in_place && target_standing
}

#[derive(Default)]
struct Stats {
    actors: BTreeMap<u32, String>,
    from_scripts: usize,
    from_packages: usize,
    done: usize,
    shots: usize,
    sure: usize,
    harmless: usize,
    reloads: usize,
    crouched: usize,
    /// Times it held fire: someone in its line of fire, its target not where the use
    /// wants it, restrained.
    blocked: usize,
    target_away: usize,
    restrained: usize,
    /// Melee: blows begun, landed (harmless ones included), harmless, out of reach when
    /// they would have landed, and gone on a combo.
    blows: usize,
    landed: usize,
    harmless_blows: usize,
    missed: usize,
    chained: usize,
}

/// The bones a shot aims at, in order of preference: the target's chest as it is posed,
/// so a kneeling or seated target is aimed at where its body is, not at a standing
/// actor's chest height above it. The Silver Rush volunteer kneels restrained
/// (`DynamicIdle_RestrainedSit`) for Jean-Baptiste's Always Hit shot, which went over
/// him into the wall and left the scene waiting for its OnHit (#1550) [design].
const CHEST_BONES: [&str; 3] = ["Bip01 Spine1", "Bip01 Spine", "Bip01 Pelvis"];

type Shooting<'w> = (&'w ActorRig, &'w GlobalTransform, &'w InheritedVisibility, Option<&'w WeaponHold>, Has<Dying>, Has<Corpse>);

#[allow(clippy::too_many_arguments)]
pub(crate) fn shoot(
    time: Res<Time>,
    mut shooters: ResMut<Shooters>,
    combat: Res<Combat>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    player: Option<Res<Player>>,
    actors: Query<Shooting>,
    held: Query<&HeldWeapon>,
    muzzles: Query<(&Muzzle, &GlobalTransform)>,
    (mut fire, mut strike, mut anims): (MessageWriter<ActorFire>, MessageWriter<ActorStrike>, MessageWriter<PlayAttack>),
    mut times: ResMut<AttackTimes>,
    (blocks, phys): (Option<Res<ActorBlocks>>, Option<Res<PhysState>>),
    bones: Query<&GlobalTransform>,
) {
    let dt = time.delta_secs();
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    if rt.game.menu.is_some() || rt.game.dialogue.is_some() {
        return;
    }
    let c = &*combat;
    let weapons: HashMap<u32, FormId> = held.iter().map(|h| (h.reference, h.weapon)).collect();
    let muzzle_at: HashMap<u32, V3> = muzzles.iter().map(|(m, g)| (m.reference, bevy_to_game(g.translation()).to_array())).collect();
    // Feet and eyes (game space) of everyone standing, and where each one's chest is as
    // posed now ([`CHEST_BONES`]).
    let mut places: HashMap<u32, (V3, V3)> = HashMap::new();
    let mut chests: HashMap<u32, V3> = HashMap::new();
    for (rig, g, vis, _, dying, corpse) in &actors {
        if vis.get() && !dying && !corpse {
            let feet = bevy_to_game(g.translation()).to_array();
            let scale = g.affine().transform_vector3(Vec3::Z).length().max(0.1);
            places.insert(rig.reference, (feet, [feet[0], feet[1], feet[2] + EYE_HEIGHT * scale]));
            let bone = CHEST_BONES.iter().find_map(|n| rig.skel.by_name.get(*n)).and_then(|&i| rig.nodes.get(i)).and_then(|&e| bones.get(e).ok());
            if let Some(b) = bone {
                chests.insert(rig.reference, bevy_to_game(b.translation()).to_array());
            }
        }
    }
    if let Some(p) = player.as_deref() {
        places.insert(FormId::PLAYER_REF.0, (p.feet, p.eye()));
    }
    // The gap between an actor's body and its target's, when both are known (as
    // `npc_combat` measures it).
    let player_radius = phys.as_deref().and_then(|p| p.player.as_ref()).map(|c| c.params.radius);
    let body = |id: u32| blocks.as_deref().and_then(|b| b.list.iter().find(|x| x.actor == id).cloned());
    let gap = |me: u32, target: u32, tfeet: V3| -> Option<f32> {
        let mine = body(me)?;
        let radius = if target == FormId::PLAYER_REF.0 { player_radius? } else { body(target)?.radius };
        Some(mine.room([tfeet[0], tfeet[1]], radius))
    };
    // The two bodies' radii summed (`npc_combat::melee_spacing`).
    let radii = |me: u32, target: u32| -> Option<f32> {
        let radius = if target == FormId::PLAYER_REF.0 { player_radius? } else { body(target)?.radius };
        Some(body(me)?.radius + radius)
    };
    let s = &mut *shooters;
    let mut still: Vec<u32> = Vec::new();
    with_world(rt, &mut sim, |rt| {
        let g = &mut rt.game;
        for (rig, global, _, hold, _, _) in &actors {
            let id = rig.reference;
            let me = FormId(id);
            if g.ai_off(me) {
                continue;
            }
            let Some(&(feet, eyes)) = places.get(&id) else { continue };
            // A melee use whose last blow is begun is done, but goes on here until that
            // blow has landed.
            let landing = |w: &fnv_game::use_weapon::WeaponUse| s.landing(id, w.seq);
            let Some(w) = g.active_weapon_use(me).or(g.weapon_use(me).filter(|w| landing(w))).filter(|w| w.in_place).cloned() else { continue };
            let name = g.data.name(me);
            let target_name = if w.target == FormId::PLAYER_REF { "the player".to_string() } else { g.data.name(w.target) };
            // Its target dead or gone: the use is over [design].
            if g.extra.get(&w.target).is_some_and(|e| e.dead) || !g.present(w.target) {
                info!("use weapon: {name} {me} stops: {target_name} is dead or gone ({} shots fired)", w.fired);
                g.end_weapon_use(me);
                s.stats.done += 1;
                continue;
            }
            // Nothing to fire at, or not there yet: the test `npc_combat::fight` keeps its
            // attacks off by ([`acts_on`]).
            let target_at = places.get(&w.target.0).copied();
            if !acts_on(&w, target_at.is_some()) {
                continue;
            }
            let Some((tfeet, teyes)) = target_at else { continue };
            // What it uses: the weapon named, else the one drawn on it, else its fists or
            // a creature's own attack (module notes).
            let scale = global.affine().transform_vector3(Vec3::Z).length().max(0.1);
            let arm = match w.weapon.filter(|f| c.forms.weapons.contains_key(f)).or(weapons.get(&id).copied()) {
                Some(f) if c.forms.weapons.get(&f).is_some_and(|ws| is_gun(c, ws, f)) => Arm::Gun(f),
                Some(f) => Arm::Melee(Some(f)),
                None => match g.creature_attack(me) {
                    Some((damage, reach)) => Arm::Creature(damage as f32, creature_reach(reach, Some(&rig.skeleton), scale)),
                    None => Arm::Melee(None),
                },
            };
            let gun = match arm {
                Arm::Gun(gun) => gun,
                melee => {
                    still.push(id);
                    let x = MeleeCtx { c, me, name: &name, target_name: &target_name, global, hold, feet, eyes, tfeet, teyes, places: &places, dt, scale };
                    let room = gap(id, w.target.0, tfeet).zip(radii(id, w.target.0));
                    swing(g, s, &mut times, &x, &w, melee, room, (&mut strike, &mut anims));
                    continue;
                }
            };
            let Some(ws) = c.forms.weapons.get(&gun) else { continue };
            still.push(id);
            let fresh = s.by.get(&id).is_none_or(|x| x.seq != w.seq || x.weapon != gun);
            if fresh {
                let timing = Timing::of(ws, &ClipTiming::default(), &c.tuning);
                s.by.insert(id, Shooter { seq: w.seq, weapon: gun, cycle: Cycle::new(ws, timing, false, ws.clip_size.max(1)), pulled: false, wait: 0.0, held: None });
                s.stats.actors.insert(id, name.clone());
                match w.source {
                    Source::Script => s.stats.from_scripts += 1,
                    Source::Package(_) => s.stats.from_packages += 1,
                }
                let shots = match w.volleys {
                    Some(v) if w.shots_per_volley[0] == w.shots_per_volley[1] => format!("{} shots", v * w.shots_per_volley[0]),
                    Some(v) => format!("{v} volleys of {} to {} shots", w.shots_per_volley[0], w.shots_per_volley[1]),
                    None => format!("volleys of {} to {} shots, on and on", w.shots_per_volley[0], w.shots_per_volley[1]),
                };
                let flags: Vec<&str> = [(w.always_hit, "always hit"), (w.no_damage, "no damage"), (w.crouch_to_reload, "crouch to reload"), (w.hold_fire_when_blocked, "hold fire when blocked")].into_iter().filter(|f| f.0).map(|f| f.1).collect();
                let from = match w.source {
                    Source::Script => "a script".to_string(),
                    Source::Package(p) => format!("its package {}", g.data.name(p)),
                };
                let weapon_name = ws.edid.clone().unwrap_or_else(|| gun.to_string());
                info!(
                    "use weapon: {name} {me} uses {weapon_name} on {target_name} ({from}): {shots}{}{}",
                    if flags.is_empty() { String::new() } else { format!("; {}", flags.join(", ")) },
                    if w.weapon.is_some_and(|x| x != gun) { "; not the weapon named, which is no gun" } else { "" }
                );
            }
            let shooter = s.by.get_mut(&id).expect("inserted");
            shooter.wait = (shooter.wait - dt).max(0.0);
            let tchest = chests.get(&w.target.0).copied().unwrap_or([tfeet[0], tfeet[1], tfeet[2] + (teyes[2] - tfeet[2]) * CHEST]);
            // What holds its fire this frame.
            let restrained = g.extra.get(&me).is_some_and(|e| e.restrained);
            let away = w.target_here.and_then(|r| fnv_ai::goal::place_of_ref(g, r)).is_some_and(|p| (p.pos[0] - tfeet[0]).hypot(p.pos[1] - tfeet[1]) > TARGET_HERE_RADIUS);
            let blocker = if w.hold_fire_when_blocked {
                places
                    .iter()
                    .filter(|(k, _)| **k != id && **k != w.target.0)
                    .find(|(_, p)| tactics::in_line_of_fire(eyes, tchest, [p.0[0], p.0[1], p.0[2] + (p.1[2] - p.0[2]) * CHEST], tactics::LINE_OF_FIRE_WIDTH))
                    .map(|(k, _)| *k)
            } else {
                None
            };
            let why = if restrained {
                Some("restrained")
            } else if away {
                Some("its target is not where the use wants it")
            } else if blocker.is_some() {
                Some("someone stands in its line of fire")
            } else {
                None
            };
            if why != shooter.held {
                if let Some(why) = why {
                    match why {
                        "restrained" => s.stats.restrained += 1,
                        "someone stands in its line of fire" => s.stats.blocked += 1,
                        _ => s.stats.target_away += 1,
                    }
                    info!("use weapon: {name} {me} holds fire: {why}");
                }
                shooter.held = why;
            }
            let drawn = hold.is_none_or(|h| h.anim.stage == Stage::Drawn);
            let facing = facing_off(global, feet, tfeet) <= USE_FACING;
            let ready = why.is_none() && drawn && facing;
            // No pull while it reloads (a pull would close a round-by-round reload after a
            // round); an empty magazine is reloaded at once, as the combat AI's gunmen do.
            let reloading = shooter.cycle.reloading();
            let trigger = ready && !reloading && shooter.wait <= 0.0 && (ws.automatic || !shooter.pulled);
            shooter.pulled = trigger;
            let empty = shooter.cycle.magazine < shooter.cycle.ammo_use && shooter.cycle.regen.is_none();
            for e in shooter.cycle.step(&c.tuning, Input { trigger, reload: empty && !reloading, dt, carried: u32::MAX / 2 }) {
                match e {
                    Event::Fired { .. } => {
                        let muzzle = muzzle_at.get(&id).copied();
                        let to = vec::sub(tchest, eyes);
                        let aim = vec::scale(to, 1.0 / vec::len(to).max(1.0));
                        let origin = vec::add(eyes, vec::scale(aim, MUZZLE_AHEAD));
                        let cone = if w.always_hit {
                            0.0
                        } else {
                            let skill = ws.skill.as_deref().and_then(fnv_game::functions::av_by_name).map_or(50.0, |code| fnv_game::functions::actor_value_capped(g, me, code)) as f32;
                            let strength = fnv_game::functions::actor_value_capped(g, me, fnv_game::actor_values::STRENGTH) as f32;
                            let stance = Stance { aiming: false, moving: false, running: false, sneaking: false, airborne: false, skill, strength, aiming_move_divisor: 1.0, ..Stance::default() };
                            spread::cone_deg(spread::actor_spread(&c.rules, &c.tuning, ws, &stance), 0.0, ws, &[])
                        };
                        fire.write(ActorFire { shooter: me, weapon: gun, origin, aim, cone, muzzle, harmless: w.no_damage });
                        anims.write(PlayAttack { actor: id, weapon: Some(gun), melee: false, sights: false, kind: Default::default(), power: None, swing: None });
                        s.stats.shots += 1;
                        s.stats.sure += usize::from(w.always_hit);
                        s.stats.harmless += usize::from(w.no_damage);
                        let roll = next_unit(&mut s.rng);
                        let Some(u) = g.weapon_uses.get_mut(&me) else { break };
                        match u.shot(roll) {
                            After::More => {
                                if !ws.automatic {
                                    let [lo, hi] = ws.semi_auto_fire_delay;
                                    shooter.wait = lo + (hi - lo).max(0.0) * next_unit(&mut s.rng);
                                }
                            }
                            After::Pause(p) => {
                                shooter.wait = p;
                                // A volley is over: the trigger is let go.
                                shooter.pulled = true;
                            }
                            After::Done => {
                                info!("use weapon: {name} {me} has fired its {} shots at {target_name}", u.fired);
                                g.end_weapon_use(me);
                                s.stats.done += 1;
                                break;
                            }
                        }
                    }
                    Event::ReloadStarted => {
                        s.stats.reloads += 1;
                        if let Some(u) = g.weapon_uses.get_mut(&me).filter(|u| u.crouch_to_reload) {
                            u.crouched = true;
                            s.stats.crouched += 1;
                        }
                    }
                    Event::Reloaded { .. } => {
                        if let Some(u) = g.weapon_uses.get_mut(&me) {
                            u.crouched = false;
                        }
                    }
                    _ => {}
                }
            }
        }
        // Those no longer using a weapon stand up.
        s.by.retain(|id, _| still.contains(id));
        s.swingers.retain(|id, _| still.contains(id));
        for (a, u) in g.weapon_uses.iter_mut() {
            if !still.contains(&a.0) {
                u.crouched = false;
            }
        }
    });
}

/// What [`swing`] reads of the actor and its target this frame.
struct MeleeCtx<'a> {
    c: &'a Combat,
    me: FormId,
    name: &'a str,
    target_name: &'a str,
    global: &'a GlobalTransform,
    hold: Option<&'a WeaponHold>,
    feet: V3,
    eyes: V3,
    tfeet: V3,
    teyes: V3,
    places: &'a HashMap<u32, (V3, V3)>,
    dt: f32,
    scale: f32,
}

/// A melee weapon use this frame (module notes): its reach for the AI, its blows
/// landing, and the next blow when it may strike.
#[allow(clippy::too_many_arguments)]
fn swing(
    g: &mut fnv_game::game::Game<'_>,
    s: &mut Shooters,
    times: &mut AttackTimes,
    x: &MeleeCtx,
    w: &fnv_game::use_weapon::WeaponUse,
    arm: Arm,
    room: Option<(f32, f32)>,
    (strike, anims): (&mut MessageWriter<ActorStrike>, &mut MessageWriter<PlayAttack>),
) {
    let (c, me, id) = (x.c, x.me, x.me.0);
    let (weapon, creature_damage, reach) = match arm {
        Arm::Melee(wp) => (wp, None, wp.or(c.fists_id()).and_then(|f| c.forms.weapons.get(&f)).map_or(CREATURE_REACH, |ws| fnv_combat::damage::melee_reach(&c.rules, ws, x.scale))),
        Arm::Creature(d, r) => (None, Some(d), r),
        Arm::Gun(_) => return,
    };
    let away = (x.feet[0] - x.tfeet[0]).hypot(x.feet[1] - x.tfeet[1]);
    // How near it closes, and within what its blows land (`npc_combat`'s spacing).
    let (stop, within) = match (creature_damage, room) {
        (Some(_), room) => creature_spacing(room.map(|(r, _)| away - r), reach),
        (None, Some((r, radii))) => melee_spacing(away - r, radii, reach),
        (None, None) => engage(Some(reach), None),
    };
    if let Some(u) = g.weapon_uses.get_mut(&me) {
        u.reach = Some([stop, within]);
    }
    let near = away <= within;
    if s.swingers.get(&id).is_none_or(|x| x.seq != w.seq) {
        s.swingers.insert(id, Swinger { seq: w.seq, strike_in: 0.0, blows: Vec::new(), swing: None, held: None, spent: false });
        s.stats.actors.insert(id, x.name.to_string());
        match w.source {
            Source::Script => s.stats.from_scripts += 1,
            Source::Package(_) => s.stats.from_packages += 1,
        }
        let blows = match w.volleys {
            Some(v) if w.shots_per_volley[0] == w.shots_per_volley[1] => format!("{} blows", v * w.shots_per_volley[0]),
            Some(v) => format!("{v} volleys of {} to {} blows", w.shots_per_volley[0], w.shots_per_volley[1]),
            None => format!("volleys of {} to {} blows, on and on", w.shots_per_volley[0], w.shots_per_volley[1]),
        };
        let flags: Vec<&str> = [(w.always_hit, "always hit"), (w.no_damage, "no damage"), (w.hold_fire_when_blocked, "hold fire when blocked")].into_iter().filter(|f| f.0).map(|f| f.1).collect();
        let from = match w.source {
            Source::Script => "a script".to_string(),
            Source::Package(p) => format!("its package {}", g.data.name(p)),
        };
        let with = match (weapon, creature_damage) {
            (Some(f), _) => c.forms.weapons.get(&f).and_then(|ws| ws.edid.clone()).unwrap_or_else(|| f.to_string()),
            (None, Some(_)) => "its own attack".to_string(),
            (None, None) => "its fists".to_string(),
        };
        let flags = if flags.is_empty() { String::new() } else { format!("; {}", flags.join(", ")) };
        info!("use weapon: {} {me} uses {with} (melee, reach {within:.0}) on {} ({from}): {blows}{flags}", x.name, x.target_name);
    }
    let sw = s.swingers.get_mut(&id).expect("inserted");
    sw.strike_in = (sw.strike_in - x.dt).max(0.0);
    // The last blow's clip, once started: its blow lands at its own `Hit` key, and the
    // next may begin at its `a:` key (as `npc_combat`'s fighters).
    if let Some(sg) = sw.swing.as_mut() {
        sg.t += x.dt;
        if let Some(clip) = times.swings.remove(&id).filter(|_| sg.clip.is_none()) {
            sg.clip = Some(clip);
            if let Some(b) = sw.blows.last_mut() {
                b.0 = (clip.hit - sg.t + x.dt).max(0.0);
            }
            if let Some((at, _)) = clip.chain {
                sw.strike_in = sw.strike_in.min((at - sg.t).max(0.0));
            }
        }
        if sg.t > sg.clip.map_or(STRIKE_SECONDS * 2.0, |c| c.len) {
            sw.swing = None;
        }
    }
    let tchest = [x.tfeet[0], x.tfeet[1], x.tfeet[2] + (x.teyes[2] - x.tfeet[2]) * CHEST];
    // Blows whose clip reached its `Hit` key land: within reach, or always with Always
    // Hit.
    for b in &mut sw.blows {
        b.0 -= x.dt;
    }
    let (due, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut sw.blows).into_iter().partition(|b| b.0 <= 0.0);
    sw.blows = wait;
    for (_, b) in due {
        if !(near || w.always_hit) {
            s.stats.missed += 1;
            info!("use weapon: {}'s blow misses {}: out of reach ({away:.0} units)", x.name, x.target_name);
        } else if w.no_damage {
            s.stats.landed += 1;
            s.stats.harmless_blows += 1;
            info!("use weapon: {}'s harmless blow lands on {}", x.name, x.target_name);
        } else {
            s.stats.landed += 1;
            strike.write(ActorStrike { point: tchest, ..b });
        }
    }
    if sw.spent {
        if sw.blows.is_empty() {
            info!("use weapon: {} {me} has dealt its {} blows at {}", x.name, w.fired, x.target_name);
            s.swingers.remove(&id);
            g.end_weapon_use(me);
            s.stats.done += 1;
        }
        return;
    }
    // What holds its swings this frame (as it holds shots).
    let restrained = g.extra.get(&me).is_some_and(|e| e.restrained);
    let target_away = w.target_here.and_then(|r| fnv_ai::goal::place_of_ref(g, r)).is_some_and(|p| (p.pos[0] - x.tfeet[0]).hypot(p.pos[1] - x.tfeet[1]) > TARGET_HERE_RADIUS);
    let blocked = w.hold_fire_when_blocked
        && x.places
            .iter()
            .filter(|(k, _)| **k != id && **k != w.target.0)
            .any(|(_, p)| tactics::in_line_of_fire(x.eyes, tchest, [p.0[0], p.0[1], p.0[2] + (p.1[2] - p.0[2]) * CHEST], tactics::LINE_OF_FIRE_WIDTH));
    let why = if restrained {
        Some("restrained")
    } else if target_away {
        Some("its target is not where the use wants it")
    } else if blocked {
        Some("someone stands in its line of fire")
    } else {
        None
    };
    if why != sw.held {
        if let Some(why) = why {
            match why {
                "restrained" => s.stats.restrained += 1,
                "someone stands in its line of fire" => s.stats.blocked += 1,
                _ => s.stats.target_away += 1,
            }
            info!("use weapon: {} {me} holds its swings: {why}", x.name);
        }
        sw.held = why;
    }
    let drawn = x.hold.is_none_or(|h| h.anim.stage == Stage::Drawn);
    let facing = facing_off(x.global, x.feet, x.tfeet) <= USE_FACING;
    if why.is_some() || !drawn || !facing || !near || sw.strike_in > 0.0 || (x.feet[2] - x.tfeet[2]).abs() >= 128.0 {
        return;
    }
    // A blow: on a combo when the last clip's `a:` key has passed.
    let next = sw.swing.and_then(|sg| sg.next());
    let run = next.map_or(1, |_| sw.swing.map_or(0, |sg| sg.run) + 1);
    if next.is_some() {
        s.stats.chained += 1;
    }
    times.swings.remove(&id);
    sw.swing = Some(Swing { t: 0.0, clip: None, run });
    let delay = times.hit.get(&id).copied().unwrap_or(DEFAULT_HIT);
    let dir = vec::normalize(vec::sub(tchest, x.eyes));
    sw.blows.push((delay, ActorStrike { attacker: me, target: w.target, weapon, creature_damage, point: tchest, dir, anim: None }));
    let pick = SwingPick { next, rolls: [next_unit(&mut s.rng), next_unit(&mut s.rng)] };
    anims.write(PlayAttack { actor: id, weapon, melee: true, sights: false, kind: Move::Attack, power: None, swing: Some(pick) });
    s.stats.blows += 1;
    let speed = weapon.or(c.fists_id()).and_then(|f| c.forms.weapons.get(&f)).map_or(1.0, |ws| ws.attack_mult.max(0.25));
    sw.strike_in = STRIKE_SECONDS / speed;
    let roll = next_unit(&mut s.rng);
    let Some(u) = g.weapon_uses.get_mut(&me) else { return };
    let after = u.shot(roll);
    let combo = next.map_or(String::new(), |n| format!(", on its combo with {n:?}"));
    info!("use weapon: {} {me} swings at {} from {away:.0} units (blow {}{combo})", x.name, x.target_name, u.fired);
    match after {
        After::More => {}
        After::Pause(p) => sw.strike_in = sw.strike_in.max(p),
        After::Done => sw.spent = true,
    }
}

fn report(mut exit: MessageReader<AppExit>, shooters: Res<Shooters>, stats: Option<Res<CombatStats>>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = &shooters.stats;
    let harmless_hits = stats.map_or(0, |c| c.harmless_hits);
    info!(
        "use weapon summary: {} actors used a weapon ({}); {} uses ({} from scripts, {} from packages), {} over; {} shots ({} always hit, {} harmless; {harmless_hits} harmless hits), {} reloads ({} crouched); held fire: {} line of fire, {} target away, {} restrained; {} blows ({} landed, {} harmless, {} out of reach, {} went on a combo)",
        s.actors.len(),
        s.actors.values().cloned().collect::<Vec<_>>().join(", "),
        s.from_scripts + s.from_packages,
        s.from_scripts,
        s.from_packages,
        s.done,
        s.shots,
        s.sure,
        s.harmless,
        s.reloads,
        s.crouched,
        s.blocked,
        s.target_away,
        s.restrained,
        s.blows,
        s.landed,
        s.harmless_blows,
        s.missed,
        s.chained,
    );
}

#[cfg(test)]
mod tests {
    use super::acts_on;
    use fnv_game::use_weapon::{Source, WeaponUse};
    use fnv_sim::FormId;

    /// #2145 review: the shooter acts on a use only in place and with its target standing
    /// among the drawn people; a marker target (never among them), a target not drawn, or
    /// an actor still on its way get nothing from it, and the fight keeps the actor.
    #[test]
    fn the_shooter_acts_only_in_place_on_a_standing_target() {
        let walking = WeaponUse { source: Source::Package(FormId(0x9)), ..WeaponUse::of_script(None, None, FormId(0x7), 3, true, true, false, false, None) };
        let there = WeaponUse { in_place: true, ..walking.clone() };
        assert!(acts_on(&there, true), "in place, its target drawn and standing");
        assert!(!acts_on(&there, false), "a marker, or a target not drawn: nothing to fire at");
        assert!(!acts_on(&walking, true), "still on its way to its stand");
        assert!(!acts_on(&walking, false));
    }
}
