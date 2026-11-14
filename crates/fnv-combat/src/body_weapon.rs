//! Which of a creature's body weapons it fights with (docs/design/combat-ai.md §19).
//!
//! A creature's "Embedded Weapon" records ("Checked if the weapon is a built in part of
//! the creature" [documented: GECK wiki Weapons]) are its arms: Mister Handy's buzzsaw
//! and flamer, Mister Gutsy's plasma gun and flamer, a protectron's hand and head lasers.
//! A creature with its own bite or swipe has that as one more arm (a fire gecko's bite
//! beside its flame). How the original picks among them is not documented; the rule here
//! is ours [design], built on the weapons' own ranges: "Min Range: Tells combat AI they
//! don't need to be closer than this value", "Max Range: … they don't want to be any
//! further away than this value" [documented: GECK wiki Weapons].
//!
//! - A melee arm is for a target within [`CLOSE`].
//! - A gun suits a target between its Min Range and its Max Range (0: no limit), and
//!   beyond its own explosion's radius ([`Arm::blast`]): `npc_combat` holds fire at a
//!   target within its blast, so a launcher taken up there would never fire [design;
//!   before, a Hardened Sentry Bot took up its missile launcher at 3 and 420 units, held
//!   fire within its 1000-unit blast and never took up its minigun again, observed
//!   2026-11-05, #1621]. A fresh choice wants the target beyond the blast by
//!   [`KEEP_SLACK`]; the arm in use is kept down to the blast itself, never within it.
//! - The arm in use is kept while the target stays within its band widened by
//!   [`KEEP_SLACK`], so a target on the edge does not make it switch back and forth.
//! - Otherwise: a melee arm when one suits; else the gun that suits with the most damage
//!   per second; else, the target beyond every gun's reach, the gun that reaches furthest
//!   (it closes in with it); else, the target nearer than every gun's Min Range, the
//!   melee arm if there is one, else the gun with the smallest Min Range (its blast
//!   counted as one).
//! - An arm that cannot be used (its limb crippled to 0, the caller's rule) is left out;
//!   with none left there is no choice. A combat style's Ranged Only leaves the melee
//!   arms out while a gun can be used ([`restrict_ranged`]).

/// One arm as the choice sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arm {
    /// A blow (a melee weapon, or the creature's own attack) rather than a shot.
    pub melee: bool,
    /// WEAP DNAM Min Range and Max Range, units (0: no limit); unused for melee.
    pub min_range: f32,
    pub max_range: f32,
    /// Its projectile's explosion radius, units: it suits no target nearer (module
    /// notes); 0 without one, or when its combat style has Ignore Damaging Self (as
    /// `npc_combat`'s blast hold has it).
    pub blast: f32,
    /// Damage per second ([`damage_per_second`]), to rank guns that both suit.
    pub dps: f32,
    pub usable: bool,
}

/// A melee arm is for a target within this, units [design: about three body widths of a
/// person, a step and a blow].
pub const CLOSE: f32 = 192.0;

/// The arm in use keeps to its band widened by this share [design].
pub const KEEP_SLACK: f32 = 0.15;

/// Seconds a creature keeps an arm before it may switch again [design], so a target
/// moving about the edge of two bands does not make it switch every frame (each switch
/// plays the new arm's equip clip).
pub const SWITCH_SECONDS: f32 = 1.5;

/// A weapon's damage per second for the ranking: base damage times projectiles per
/// shot times shots per second (DNAM Attack Shots/Sec, else Fire Rate, else one).
pub fn damage_per_second(base_damage: i32, projectiles: u32, shots_per_sec: f32, fire_rate: f32) -> f32 {
    let rate = if shots_per_sec > 0.0 {
        shots_per_sec
    } else if fire_rate > 0.0 {
        fire_rate
    } else {
        1.0
    };
    base_damage.max(0) as f32 * projectiles.max(1) as f32 * rate
}

/// Whether arm `a` suits a target `d` units away, its band widened by `slack`.
pub fn suits(a: &Arm, d: f32, slack: f32) -> bool {
    if a.melee {
        d <= CLOSE * (1.0 + slack)
    } else {
        d >= a.min_range * (1.0 - slack) && d >= blast_floor(a, slack) && (a.max_range <= 0.0 || d <= a.max_range * (1.0 + slack))
    }
}

/// The nearest target a gun's blast lets it suit: the radius widened outward by
/// [`KEEP_SLACK`] less `slack`, so a fresh choice (no slack) wants the target clear of
/// the blast and the arm in use keeps to the radius itself (module notes).
fn blast_floor(a: &Arm, slack: f32) -> f32 {
    a.blast * (1.0 + (KEEP_SLACK - slack).max(0.0))
}

/// The nearest a gun suits: its Min Range or its blast, the further.
fn nearest(a: &Arm) -> f32 {
    a.min_range.max(blast_floor(a, 0.0))
}

/// A combat style's Ranged Only: "the combatant will always use ranged weapons as their
/// primary weapon" [documented: GECK wiki Combat Style]. Its melee arms are left out
/// (made unusable) while one of its guns can be used; with no gun left it still fights
/// with them [design, #2351: ranged as the primary weapon, not the only one].
pub fn restrict_ranged(arms: &mut [Arm]) {
    if arms.iter().any(|a| a.usable && !a.melee) {
        for a in arms.iter_mut().filter(|a| a.melee) {
            a.usable = false;
        }
    }
}

/// The arm (index into `arms`) to fight a target `d` units away with, `current` the one
/// in use (module notes).
pub fn choose(arms: &[Arm], d: f32, current: Option<usize>) -> Option<usize> {
    let usable: Vec<usize> = (0..arms.len()).filter(|&i| arms[i].usable).collect();
    if usable.is_empty() {
        return None;
    }
    if let Some(c) = current.filter(|c| usable.contains(c) && suits(&arms[*c], d, KEEP_SLACK)) {
        return Some(c);
    }
    let melee = usable.iter().copied().find(|&i| arms[i].melee);
    if let Some(m) = melee.filter(|&m| suits(&arms[m], d, 0.0)) {
        return Some(m);
    }
    let guns: Vec<usize> = usable.iter().copied().filter(|&i| !arms[i].melee).collect();
    let by = |key: &dyn Fn(&Arm) -> f32, among: &mut dyn Iterator<Item = usize>| among.max_by(|&a, &b| key(&arms[a]).total_cmp(&key(&arms[b])));
    if let Some(g) = by(&|a| a.dps, &mut guns.iter().copied().filter(|&i| suits(&arms[i], d, 0.0))) {
        return Some(g);
    }
    let reach = |a: &Arm| if a.max_range <= 0.0 { f32::INFINITY } else { a.max_range };
    let beyond_all = guns.iter().all(|&i| d > reach(&arms[i]));
    if beyond_all && !guns.is_empty() {
        return by(&reach, &mut guns.iter().copied());
    }
    melee.or_else(|| by(&|a| -nearest(a), &mut guns.iter().copied()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gun(min: f32, max: f32, dps: f32) -> Arm {
        Arm { melee: false, min_range: min, max_range: max, blast: 0.0, dps, usable: true }
    }
    fn blade() -> Arm {
        Arm { melee: true, min_range: 0.0, max_range: 0.0, blast: 0.0, dps: 0.0, usable: true }
    }

    /// Mister Handy (CrMisterHandy): MisterHandyBuzzsaw (melee) and MisterHandyFlamer2HL
    /// (Min Range 128, Max Range 600, 2 damage at 8 shots a second) [observed:
    /// FalloutNV.esm].
    fn handy() -> Vec<Arm> {
        vec![blade(), gun(128.0, 600.0, damage_per_second(2, 1, 8.0, 8.0))]
    }

    /// Mister Gutsy (CrMisterGutsy): MisterGutsyPlasma1HP (512 to 1024, 60 damage at 2.5
    /// a second) and MisterGutsyFlamer2HL (128 to 600, 6 damage at 8 a second) [observed:
    /// FalloutNV.esm].
    fn gutsy() -> Vec<Arm> {
        vec![gun(512.0, 1024.0, damage_per_second(60, 1, 2.5, 1.0)), gun(128.0, 600.0, damage_per_second(6, 1, 8.0, 8.0))]
    }

    #[test]
    fn mister_handy_saws_up_close_and_burns_further_out() {
        let a = handy();
        assert_eq!(choose(&a, 80.0, None), Some(0));
        assert_eq!(choose(&a, 350.0, None), Some(1));
        // Beyond the flamer's reach it closes in with the flamer, its only gun.
        assert_eq!(choose(&a, 2000.0, None), Some(1));
        // Between the saw's reach and the flamer's Min Range: the flamer suits from 128.
        assert_eq!(choose(&a, 150.0, None), Some(0));
        assert_eq!(choose(&a, 200.0, None), Some(1));
    }

    #[test]
    fn mister_gutsy_shoots_plasma_far_and_burns_near() {
        let a = gutsy();
        assert_eq!(choose(&a, 900.0, None), Some(0));
        assert_eq!(choose(&a, 300.0, None), Some(1));
        // Both suit: the plasma gun deals more per second.
        assert_eq!(choose(&a, 560.0, None), Some(0));
        // Too far for both: the plasma gun reaches further.
        assert_eq!(choose(&a, 3000.0, None), Some(0));
        // Nearer than both Min Ranges: the flamer's is the smaller.
        assert_eq!(choose(&a, 60.0, None), Some(1));
    }

    #[test]
    fn the_arm_in_use_is_kept_near_the_edge_of_its_band() {
        let a = gutsy();
        // The flamer at 650 (Max 600, kept to 690).
        assert_eq!(choose(&a, 650.0, Some(1)), Some(1));
        assert_eq!(choose(&a, 700.0, Some(1)), Some(0));
        // The plasma gun at 470 (Min 512, kept down to 435).
        assert_eq!(choose(&a, 470.0, Some(0)), Some(0));
        assert_eq!(choose(&a, 420.0, Some(0)), Some(1));
        let h = handy();
        assert_eq!(choose(&h, 215.0, Some(0)), Some(0));
        assert_eq!(choose(&h, 230.0, Some(0)), Some(1));
    }

    /// Hardened Sentry Bot (CrSentryBotMGHard): SentryBotMinigun2HR (500 to 2000, 9
    /// damage at 15 a second) and SentryBotMissileLauncher2HL (500 to 2000, 20 damage at
    /// 1.58 a second; MissileExplosion radius 1000) [observed: FalloutNV.esm, `fnvtool
    /// combat weapon`].
    fn sentry() -> Vec<Arm> {
        let mut launcher = gun(500.0, 2000.0, damage_per_second(20, 1, 1.578_947_4, 1.0));
        launcher.blast = 1000.0;
        vec![gun(500.0, 2000.0, damage_per_second(9, 1, 15.0, 15.0)), launcher]
    }

    #[test]
    fn a_sentry_bot_keeps_its_minigun_inside_its_launchers_blast() {
        let a = sentry();
        // Nearer than both Min Ranges: the minigun's 500 is nearer than the blast.
        for d in [3.0, 300.0, 420.0, 561.0, 900.0, 1100.0] {
            assert_eq!(choose(&a, d, None), Some(0), "at {d}");
            assert_eq!(choose(&a, d, Some(0)), Some(0), "kept at {d}");
            // The launcher in use is given up within its blast (kept beyond it).
            assert_eq!(choose(&a, d, Some(1)), if d < 1000.0 { Some(0) } else { Some(1) }, "launcher at {d}");
        }
        // Without the minigun: the launcher only beyond its blast, kept down to it.
        let mut a = sentry();
        a[0].usable = false;
        assert_eq!(choose(&a, 1200.0, None), Some(1));
        assert_eq!(choose(&a, 1050.0, Some(1)), Some(1));
        assert!(!suits(&a[1], 990.0, KEEP_SLACK));
        assert!(!suits(&a[1], 1100.0, 0.0));
        // Its only gun: taken up anyway (`npc_combat` keeps clear of the blast).
        assert_eq!(choose(&a, 400.0, None), Some(1));
    }

    #[test]
    fn a_blast_ignored_by_the_style_does_not_bar_the_launcher() {
        let mut a = sentry();
        a[1].blast = 0.0;
        a[1].dps = 500.0;
        assert_eq!(choose(&a, 600.0, None), Some(1));
    }

    #[test]
    fn a_crippled_arm_is_left_out() {
        let mut a = gutsy();
        a[1].usable = false;
        assert_eq!(choose(&a, 300.0, None), Some(0));
        assert_eq!(choose(&a, 300.0, Some(1)), Some(0));
        a[0].usable = false;
        assert_eq!(choose(&a, 300.0, None), None);
    }

    #[test]
    fn a_creature_with_a_bite_bites_near_and_breathes_fire_further() {
        let a = vec![blade(), gun(0.0, 512.0, 30.0)];
        assert_eq!(choose(&a, 100.0, None), Some(0));
        assert_eq!(choose(&a, 400.0, None), Some(1));
    }

    /// #2351: a combat style's Ranged Only keeps a creature to its guns while one can be
    /// used; with none left its melee arm comes back.
    #[test]
    fn ranged_only_leaves_the_melee_arms_out_while_a_gun_can_be_used() {
        let mut a = handy();
        restrict_ranged(&mut a);
        assert!(!a[0].usable && a[1].usable);
        // At its feet it keeps the flamer (the smallest Min Range), not the saw.
        assert_eq!(choose(&a, 80.0, None), Some(1));
        assert_eq!(choose(&a, 80.0, Some(0)), Some(1));
        // The flamer lost to a crippled limb: the saw again.
        let mut a = handy();
        a[1].usable = false;
        restrict_ranged(&mut a);
        assert!(a[0].usable);
        assert_eq!(choose(&a, 80.0, None), Some(0));
        // Guns alone: nothing changes.
        let mut g = gutsy();
        restrict_ranged(&mut g);
        assert_eq!(g, gutsy());
    }

    #[test]
    fn damage_per_second_falls_back_to_the_fire_rate_then_one() {
        assert_eq!(damage_per_second(10, 2, 0.0, 3.0), 60.0);
        assert_eq!(damage_per_second(10, 0, 0.0, 0.0), 10.0);
    }
}
