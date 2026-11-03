//! JIP's live-projectile functions over the shots in flight: GetProjectileRef… and
//! SetProjectileRef… speed multiplier, damage and source, the lifetime, distance and
//! weapon [documented: their pages; observed: proj1, the peer's capture of 2026-10-27].
//!
//! The engine fires and flies the shots (`nvf_play::combat`); for each one it asks the rules
//! for a temporary reference of the shot's PROJ base ([`Game::projectile_fired`]), which a
//! script finds with GetFirstRef 51 in the frame the shot is fired, says each frame how far
//! it has gone, takes back what a script set, and removes the reference when the shot has
//! landed or expired.
//!
//! Known difference: a script's FireWeapon shot is fired at the engine's next frame
//! (`CombatRequest::Fire`), so a walk finds it one frame later than the original did
//! [observed: proj1, GetFirstRef 51 found it in FireWeapon's own frame].

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use std::collections::HashMap;

/// A shot in flight, by its temporary reference.
#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    pub source: FormId,
    pub weapon: FormId,
    /// The base damage it deals on impact: the weapon's own at first [documented:
    /// GetProjectileRefDamage, "By default, this equals the base damage of the source
    /// weapon"; observed: proj1, 22 for the 10mm pistol].
    pub damage: f32,
    pub speed_mult: f32,
    /// Seconds since it was fired.
    pub age: f32,
    /// Seconds left of a flight the engine found over at once (a hitscan shot that met
    /// nothing): the reference stays that long, as the bullet would fly.
    pub left: Option<f32>,
    /// What it hit, once it has: the reference (0 for the terrain) and the Havok material.
    pub impact: Option<(FormId, Option<u32>)>,
    /// Game units flown.
    pub travelled: f32,
    /// The ammunition it was fired with: the one the shooter's weapon uses when it is the
    /// weapon fired [hypothesis: the engine does not say which round it fired]
    /// (GetProjectileRefAmmo, `crate::nvse_one111`).
    pub ammo: Option<FormId>,
}

/// This file's session state (not saved: shots do not outlive a save's load).
#[derive(Debug, Default)]
pub struct ProjState {
    pub shots: HashMap<FormId, Shot>,
    /// Shots over: their references are no longer valid forms (IsFormValid 0).
    pub gone: std::collections::HashSet<FormId>,
}

impl Game<'_> {
    /// A shot was fired: its temporary reference of the PROJ base `proj` at `at`, in the
    /// shooter's cell; `None` when none could be placed.
    pub fn projectile_fired(&mut self, proj: FormId, source: FormId, weapon: FormId, damage: f32, at: [f32; 3]) -> Option<FormId> {
        let cell = self.cell_of(source).or_else(|| self.cell_of(FormId::PLAYER_REF));
        let r = self.world.place(proj, cell, at, [0.0; 3], 1).ok()?;
        let ammo = if crate::functions::items::equipped_weapon(self, source) == Some(weapon) { self.ammo_in_use(source) } else { None };
        self.gaps.projs.shots.insert(r, Shot { source, weapon, damage, speed_mult: 1.0, age: 0.0, travelled: 0.0, left: None, impact: None, ammo });
        Some(r)
    }

    /// The engine's frame for a shot in flight: its place, its age and how far it went.
    pub fn projectile_moved(&mut self, r: FormId, at: [f32; 3], age: f32, travelled: f32) {
        if let Some(s) = self.gaps.projs.shots.get_mut(&r) {
            s.age = age;
            s.travelled = travelled;
            if let Ok(st) = self.world.get(r) {
                st.pos = at;
            }
        }
    }

    /// A shot whose flight the engine found over at once: its reference stays `seconds`
    /// more, ageing, then goes ([`Game::projectiles_tick`]).
    pub fn projectile_flies_on(&mut self, r: FormId, seconds: f32) {
        if let Some(s) = self.gaps.projs.shots.get_mut(&r) {
            s.left = Some(seconds.max(0.0));
        }
    }

    /// One frame of `dt` seconds for the shots flying on ([`Game::projectile_flies_on`]).
    pub fn projectiles_tick(&mut self, dt: f32) {
        let mut done = Vec::new();
        for (r, s) in self.gaps.projs.shots.iter_mut() {
            if let Some(l) = s.left.as_mut() {
                s.age += dt;
                *l -= dt;
                if *l <= 0.0 {
                    done.push(*r);
                }
            }
        }
        for r in done {
            self.projectile_gone(r);
        }
    }

    /// The shot hit `what` (0: the terrain or nothing placed) of Havok material `material`:
    /// what the scripts set on it, for the hit; the reference stays one frame more with the
    /// impact on it (GetProjectileRefImpactRef) [hypothesis: the original keeps a shot
    /// that hit for its impact's frame].
    pub fn projectile_impact(&mut self, r: FormId, what: FormId, material: Option<u32>) -> Option<Shot> {
        let s = self.gaps.projs.shots.get_mut(&r)?;
        s.impact = Some((what, material));
        s.left = Some(self.cfg.frame_seconds as f32);
        Some(s.clone())
    }

    /// The shot is over: its reference goes [observed: proj1, IsFormValid 0 on the kept
    /// reference 30 frames on]. What the scripts left set on it, for the hit.
    pub fn projectile_gone(&mut self, r: FormId) -> Option<Shot> {
        let s = self.gaps.projs.shots.remove(&r)?;
        let _ = self.world.set_deleted(r, true);
        self.gaps.projs.gone.insert(r);
        Some(s)
    }
}

fn num(args: &[XVal], k: usize) -> Option<f64> {
    match args.get(k)? {
        XVal::Num(n) => Some(*n),
        _ => None,
    }
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(r) if *r != 0 => Some(FormId(*r)),
        _ => None,
    }
}

/// The functions of this file; `None` for any other name. Called on anything but a shot
/// in flight, the getters read 0 and the setters do nothing [hypothesis].
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let shot = subject.and_then(|r| g.gaps.projs.shots.get_mut(&r));
    let v = match (name, shot) {
        ("getprojectilerefspeedmult", Some(s)) => XVal::Num(f64::from(s.speed_mult)),
        ("getprojectilerefdamage", Some(s)) => XVal::Num(f64::from(s.damage)),
        ("getprojectilereflifetime", Some(s)) => XVal::Num(f64::from(s.age)),
        ("getprojectilerefdistancetraveled", Some(s)) => XVal::Num(f64::from(s.travelled)),
        ("getprojectilerefsource", Some(s)) => XVal::Ref(s.source.0),
        ("getprojectilerefweapon", Some(s)) => XVal::Ref(s.weapon.0),
        // What the shot hit and its material, once it has hit [documented:
        // GetProjectileRefImpactRef, GetProjectileRefImpactMaterial; hypothesis: before a
        // hit 0 and -1].
        ("getprojectilerefimpactref", Some(s)) => XVal::Ref(s.impact.map_or(0, |(r, _)| r.0)),
        ("getprojectilerefimpactmaterial", Some(s)) => XVal::Num(s.impact.and_then(|(_, m)| m).map_or(-1.0, f64::from)),
        // The setters return 0 and hold from then on [observed: proj1, speed 0.5, damage
        // 5 and the source NPC read back that frame and the frames after].
        ("setprojectilerefspeedmult", Some(s)) => {
            if let Some(m) = num(args, 0) {
                s.speed_mult = m as f32;
            }
            zero
        }
        ("setprojectilerefdamage", Some(s)) => {
            if let Some(d) = num(args, 0) {
                s.damage = d as f32;
            }
            zero
        }
        ("setprojectilerefsource", Some(s)) => {
            if let Some(a) = form(args, 0) {
                s.source = a;
            }
            zero
        }
        ("getprojectilerefspeedmult" | "getprojectilerefdamage" | "getprojectilereflifetime" | "getprojectilerefdistancetraveled", None) => zero,
        ("getprojectilerefsource" | "getprojectilerefweapon" | "getprojectilerefimpactref", None) => XVal::Ref(0),
        ("getprojectilerefimpactmaterial", None) => XVal::Num(-1.0),
        ("setprojectilerefspeedmult" | "setprojectilerefdamage" | "setprojectilerefsource", None) => zero,
        _ => return None,
    };
    Some(Ok(v))
}
