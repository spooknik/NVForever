//! Scripted weapon use: the `UseWeapon` function and the Use Weapon package
//! (docs/design/use-weapon.md, docs/spec-notes/use-weapon.md).
//!
//! Both give an actor a [`WeaponUse`]: a weapon, a place to stand, a target, how many
//! shots, and the behaviour flags (always hit, do no damage, crouch to reload, hold fire
//! when blocked). The layer that moves actors (`fnv_ai`) walks it to its place and turns
//! it to the target, then reports it is in place ([`Game::weapon_use_in_place`]); the
//! engine fires the shots, counting them here ([`WeaponUse::shot`]), and ends the use
//! when they are spent ([`Game::end_weapon_use`]).
//!
//! - **The function** [documented: GECK wiki UseWeapon (Function)]: weapon, where to
//!   stand, target and number of shots; crouch to reload and hold fire when blocked
//!   default to on, always hit and do no damage to off; the last argument is where the
//!   target has to be for the actor to fire ("default/NULL means anywhere"; the page's
//!   example passes a marker reference). The shots are one volley of that many
//!   [design]. A use from a script comes before the actor's package and its combat
//!   state while it lasts [hypothesis: the shipped scripts call StartCombat together
//!   with UseWeapon, e.g. a trigger that has an actor fire four shots and then fight
//!   the player, so the use must not wait for the fight; observed: FalloutNV.esm SCPT
//!   0x000FED52]; once spent it is gone and the actor goes back to its package (or its
//!   fight).
//! - **The package** [documented: xEdit PACK PKW3 field names; the meanings below are
//!   our reading of them]: location 1 is where to stand, target 1 the weapon (a weapon
//!   form, or an object type: then what the actor holds), target 2 whom to shoot. Fire
//!   count "Number of Bursts" fires that many volleys, "Repeat Fire" goes on for as
//!   long as the package runs; each volley is a random number of shots between Shots
//!   Per Volley min and max, and between volleys "Volley Fire" waits a random time
//!   between Pause Between Volleys min and max while "Auto Fire" goes straight on
//!   [hypothesis]. Spent, the package's goal is met (its end script runs) and the actor
//!   stays in it until it chooses again [design, as a Dialogue package that has
//!   talked]. A package's use waits while the actor is in combat, as other package
//!   doings do [design]; with the package flag Continue During Combat its other doings
//!   go on in combat (`Game::continues_during_combat`), and so does its fire, at the
//!   package's own target from its stand ("Actor will continue to fire and stand at the
//!   designated position, even during Combat" [documented: GECK wiki Use Weapon
//!   Package, Flags]; that the target stays the package's, not whoever attacks it, is
//!   our reading [design]). While the engine's shooter acts on such a use (in place, its
//!   target a drawn actor or the player) the combat AI's own attacks wait and the actor
//!   is drawn facing the use's target, not its enemy (#2120). Otherwise the ordinary
//!   attacks take over and it turns to its enemy ([`WeaponUse::taken`]); its package's
//!   walk to its stand goes on in combat all the same, so this is not the ordinary combat
//!   movement (docs/design/use-weapon.md §3).
//! - **Weapon**: the function equips the named weapon when the actor carries it; the
//!   page's "or grab one if one was nearby" is not built [design].
//! - **Melee** (docs/design/use-weapon.md §3.1): with a melee weapon, fists or a
//!   creature's own attack, each "shot" is a blow. Once in place, a script's use closes
//!   in on its target within the reach the engine sets ([`WeaponUse::reach`]) and swings
//!   there [hypothesis: the page says only "how many times to use the weapon"; a blow
//!   from where the actor stands would never land].
//! - Not part of the saved state: a use in progress is lost with a load [design].

use crate::game::Game;
use crate::package::{flags, PackageType, TargetKind};
use fnv_sim::FormId;
use serde::Serialize;

/// Where a weapon use came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Source {
    /// The `UseWeapon` function.
    Script,
    /// A Use Weapon package (the package).
    Package(FormId),
}

/// One actor's weapon use (module notes).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeaponUse {
    pub source: Source,
    /// A number of its own, so that a use given again is seen as new.
    pub seq: u64,
    /// The weapon to use (a base form); `None`: the one it holds.
    pub weapon: Option<FormId>,
    /// Where to stand (a reference); `None`: where the package's location says, or
    /// where it is.
    pub stand: Option<FormId>,
    pub target: FormId,
    /// The actor fires only while its target is within [`TARGET_HERE_RADIUS`] of this
    /// reference.
    pub target_here: Option<FormId>,
    /// Volleys to fire; `None`: on and on (Repeat Fire).
    pub volleys: Option<u32>,
    /// Shots per volley, at least and at most.
    pub shots_per_volley: [u32; 2],
    /// Seconds between volleys, at least and at most; `None`: none (Auto Fire).
    pub pause: Option<[f32; 2]>,
    pub always_hit: bool,
    pub no_damage: bool,
    pub crouch_to_reload: bool,
    pub hold_fire_when_blocked: bool,
    /// The actor is where it is to stand ([`Game::weapon_use_in_place`]).
    pub in_place: bool,
    /// Shots fired, volleys finished, and shots left in the volley being fired (`None`:
    /// none begun).
    pub fired: u32,
    pub volleys_done: u32,
    pub left_in_volley: Option<u32>,
    /// It crouches while it reloads (`crouch_to_reload`; `Game::sneaks`).
    pub crouched: bool,
    /// All its shots are fired.
    pub done: bool,
    /// A use with a melee weapon, fists or a creature's own attack: the distances
    /// (centre to centre, game units) within which it stops closing in on its target and
    /// beyond which it sets off again, set by the engine from the weapon's reach; `None`
    /// for a gun (module notes).
    pub reach: Option<[f32; 2]>,
    /// The fight has the actor this frame: in Combat, a package's use the engine's
    /// shooter has nothing to act on (a marker target, one not drawn, not yet in place),
    /// so the ordinary attacks take over and the actor turns to its enemy, not the use's
    /// target. Set each frame by the engine's fight (`Game::weapon_use_taken`), cleared
    /// at the start of each of its frames, on a new use and at its end; read only while
    /// the actor is in Combat, so one left set as it leaves Combat means nothing [design].
    pub taken: bool,
}

/// How near the "target should be here" reference the target has to be, game units
/// [design: the page gives no distance].
pub const TARGET_HERE_RADIUS: f32 = 256.0;

/// What a shot leaves of a weapon use ([`WeaponUse::shot`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum After {
    /// More shots in this volley.
    More,
    /// The volley is over: wait this many seconds before the next.
    Pause(f32),
    /// Every shot is fired.
    Done,
}

impl WeaponUse {
    /// The use of the `UseWeapon` function: `shots` shots in one volley.
    #[allow(clippy::too_many_arguments)]
    pub fn of_script(weapon: Option<FormId>, stand: Option<FormId>, target: FormId, shots: u32, crouch_to_reload: bool, hold_fire: bool, always_hit: bool, no_damage: bool, target_here: Option<FormId>) -> WeaponUse {
        WeaponUse {
            source: Source::Script,
            seq: 0,
            weapon,
            stand,
            target,
            target_here,
            volleys: Some(1),
            shots_per_volley: [shots, shots],
            pause: None,
            always_hit,
            no_damage,
            crouch_to_reload,
            hold_fire_when_blocked: hold_fire,
            in_place: false,
            fired: 0,
            volleys_done: 0,
            left_in_volley: None,
            crouched: false,
            done: shots == 0,
            reach: None,
            taken: false,
        }
    }

    /// The shots of the next volley, from a roll in [0, 1).
    fn volley_size(&self, roll: f32) -> u32 {
        let [lo, hi] = self.shots_per_volley;
        let hi = hi.max(lo);
        (lo + ((hi - lo + 1) as f32 * roll.clamp(0.0, 0.999_999)) as u32).max(1)
    }

    /// A shot was fired; `roll` (in [0, 1)) sizes a new volley and its pause.
    pub fn shot(&mut self, roll: f32) -> After {
        if self.done {
            return After::Done;
        }
        let left = match self.left_in_volley {
            Some(n) => n,
            None => self.volley_size(roll),
        };
        self.fired += 1;
        let left = left.saturating_sub(1);
        if left > 0 {
            self.left_in_volley = Some(left);
            return After::More;
        }
        self.left_in_volley = None;
        self.volleys_done += 1;
        if self.volleys.is_some_and(|v| self.volleys_done >= v) {
            self.done = true;
            return After::Done;
        }
        After::Pause(self.pause.map_or(0.0, |[lo, hi]| lo + (hi - lo).max(0.0) * roll))
    }
}

impl Game<'_> {
    /// Give `actor` a weapon use, in place of any it had.
    pub fn give_weapon_use(&mut self, actor: FormId, mut w: WeaponUse) {
        self.weapon_use_seq += 1;
        w.seq = self.weapon_use_seq;
        // A new use is the shooter's until the fight says otherwise.
        w.taken = false;
        self.weapon_uses.insert(actor, w);
    }

    /// The weapon use of an actor's Use Weapon package `package` (module notes); `None`
    /// when it names nobody to shoot.
    pub fn package_weapon_use(&self, actor: FormId, package: FormId) -> Option<WeaponUse> {
        let def = self.package_def(&package).filter(|d| d.ty == PackageType::UseWeapon)?;
        let linked = || self.world.placed.get(actor).and_then(|r| r.linked);
        let target = match def.target2.map(|t| t.kind) {
            Some(TargetKind::Reference(Some(r))) => r,
            Some(TargetKind::LinkedReference) => linked()?,
            _ => return None,
        };
        let weapon = match def.target.map(|t| t.kind) {
            Some(TargetKind::ObjectId(Some(w))) if self.data.sig(w).as_ref() == Some(b"WEAP") => Some(w),
            _ => None,
        };
        let p = def.use_weapon.unwrap_or_default();
        let lo = u32::from(p.shots_per_volley[0]).max(1);
        Some(WeaponUse {
            source: Source::Package(package),
            seq: 0,
            weapon,
            stand: None,
            target,
            target_here: None,
            volleys: (!p.repeat_fire).then_some(u32::from(p.bursts).max(1)),
            shots_per_volley: [lo, u32::from(p.shots_per_volley[1]).max(lo)],
            pause: p.volley_fire.then_some(p.pause_between_volleys),
            always_hit: p.always_hit,
            no_damage: p.do_no_damage,
            crouch_to_reload: p.crouch_to_reload,
            hold_fire_when_blocked: p.hold_fire_when_blocked,
            in_place: false,
            fired: 0,
            volleys_done: 0,
            left_in_volley: None,
            crouched: false,
            done: false,
            reach: None,
            taken: false,
        })
    }

    /// An actor's weapon use, while it has one.
    pub fn weapon_use(&self, actor: FormId) -> Option<&WeaponUse> {
        self.weapon_uses.get(&actor)
    }

    /// An actor's weapon use that is still to be carried out: not spent, and, from a
    /// package, not waiting for a fight to end, which a package with Continue During
    /// Combat does not do (module notes).
    pub fn active_weapon_use(&self, actor: FormId) -> Option<&WeaponUse> {
        self.weapon_uses.get(&actor).filter(|w| {
            !w.done
                && match w.source {
                    Source::Script => true,
                    Source::Package(p) => !self.awareness.state(actor).in_combat() || self.package_def(&p).is_some_and(|d| d.has(flags::CONTINUE_DURING_COMBAT)),
                }
        })
    }

    /// The actor is where its weapon use wants it.
    pub fn weapon_use_in_place(&mut self, actor: FormId) {
        if let Some(w) = self.weapon_uses.get_mut(&actor) {
            w.in_place = true;
        }
    }

    /// The fight took `actor`'s weapon use over this frame ([`WeaponUse::taken`]).
    pub fn weapon_use_taken(&mut self, actor: FormId) {
        if let Some(w) = self.weapon_uses.get_mut(&actor) {
            w.taken = true;
        }
    }

    /// A new frame of the fight: no use is taken over until it says so again.
    pub fn weapon_uses_untaken(&mut self) {
        for w in self.weapon_uses.values_mut() {
            w.taken = false;
        }
    }

    /// The weapon use's shots are spent (or it cannot go on): a script's use is gone,
    /// a package's goal is met (its end script runs) and the use stays, done, until
    /// the package changes.
    pub fn end_weapon_use(&mut self, actor: FormId) {
        let Some(w) = self.weapon_uses.get_mut(&actor) else { return };
        w.done = true;
        w.crouched = false;
        w.taken = false;
        match w.source {
            Source::Script => {
                self.weapon_uses.remove(&actor);
            }
            Source::Package(p) => {
                if self.extra.get(&actor).and_then(|e| e.current_package) == Some(p) {
                    self.package_goal_met(actor);
                }
            }
        }
    }

    /// The actor's weapon use is let go, whatever it was doing (ResetAI, death).
    pub fn drop_weapon_use(&mut self, actor: FormId) {
        self.weapon_uses.remove(&actor);
    }
}

/// `actor.UseWeapon weapon whereToStand target numberOfShots [crouchToReload holdFire
/// alwaysHit doNoDamage targetShouldBeHere]` (module notes).
pub fn use_weapon(g: &mut Game, ctx: &crate::functions::Ctx, a: &crate::functions::Args) -> Result<fnv_script::vm::Value, String> {
    let actor = ctx.subject()?;
    let weapon = a.opt_form(0);
    let stand = a.opt_form(1).filter(|r| *r != actor);
    let target = a.form(2)?;
    let shots = a.num(3)?.max(0.0) as u32;
    let flag = |i: usize, d: bool| a.num_or(i, if d { 1.0 } else { 0.0 }) != 0.0;
    let target_here = a.opt_form(8).filter(|r| r.0 != 0);
    // The weapon in hand, when it carries one.
    let inventory = |g: &mut Game| g.world.get(actor).ok().and_then(|s| s.inventory.as_ref()).map(|i| (weapon.is_some_and(|w| i.count(w) > 0), weapon.is_some_and(|w| i.equipped().contains(&w))));
    if let (Some(w), Some((true, false))) = (weapon, inventory(g)) {
        let _ = g.world.equip(actor, w);
    }
    let use_ = WeaponUse::of_script(weapon, stand, target, shots, flag(4, true), flag(5, true), flag(6, false), flag(7, false), target_here);
    g.give_weapon_use(actor, use_);
    crate::functions::none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_use_is_one_volley_of_its_shots() {
        let mut w = WeaponUse::of_script(None, None, FormId::PLAYER_REF, 3, true, true, false, false, None);
        assert_eq!(w.shot(0.7), After::More);
        assert_eq!(w.shot(0.1), After::More);
        assert_eq!(w.shot(0.5), After::Done);
        assert!(w.done);
        assert_eq!((w.fired, w.volleys_done), (3, 1));
        assert_eq!(w.shot(0.5), After::Done);
        assert_eq!(w.fired, 3);
        // No shots: nothing to do.
        assert!(WeaponUse::of_script(None, None, FormId::PLAYER_REF, 0, true, true, false, false, None).done);
    }

    #[test]
    fn volleys_take_their_size_from_the_roll_and_pause_between() {
        let mut w = WeaponUse::of_script(None, None, FormId::PLAYER_REF, 1, true, true, false, false, None);
        (w.volleys, w.shots_per_volley, w.pause, w.done) = (Some(2), [2, 4], Some([0.5, 1.5]), false);
        // A low roll: two shots, then a pause from the same roll.
        assert_eq!(w.shot(0.0), After::More);
        assert_eq!(w.shot(0.0), After::Pause(0.5));
        // A high roll: four shots; the second volley ends the use.
        assert_eq!(w.shot(0.99), After::More);
        assert_eq!(w.shot(0.2), After::More);
        assert_eq!(w.shot(0.2), After::More);
        assert_eq!(w.shot(0.2), After::Done);
        assert_eq!(w.fired, 6);
        // Repeat fire never ends; Auto Fire does not pause.
        let mut r = WeaponUse::of_script(None, None, FormId::PLAYER_REF, 1, true, true, false, false, None);
        r.volleys = None;
        for _ in 0..50 {
            assert_eq!(r.shot(0.3), After::Pause(0.0));
        }
        assert!(!r.done);
    }
}
