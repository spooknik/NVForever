//! The classic VATS style (docs/design/vats-classic.md): the original game's flow on
//! `crate::vats_mode`'s rules. V stops the world (or slows it, the settings page's
//! "V.A.T.S. time slowdown"), the view turns to a target, each of its body parts shows its
//! chance to hit, attacks are queued against the AP and Activate plays them.
//!
//! This module holds what the two styles do not share and what needs no world: which
//! style V starts ([`VatsStyle`]), how far the clock is slowed while targeting
//! ([`VatsSlowdown`], [`clock_scale`]), the order of the targets and the cycling through
//! them and through a target's parts ([`order`], [`cycle`]), the turn of the view
//! ([`ease_angle`]), and the two rows of the settings page ([`register`]). The frame's
//! work is `crate::vats_mode`'s, which asks here.
//!
//! Test aids: `NVF_PLAY_TEST_VATS_STYLE` (`classic` or `dead-eye`) and
//! `NVF_PLAY_TEST_VATS_SLOWDOWN` (0.5 to 1) set the two rows for an unattended run, which
//! reads no INI (`crate::settings`); every VATS scenario pins its style with the first.

use bevy::prelude::*;

/// What V starts: the settings page's "V.A.T.S. style" (`vats-style` in NVForever.ini).
/// Dead Eye is the default until the owner has ruled on docs/owner-review.md item 31a
/// [design: the owner's review of #2102, 2026-11-11; the fork's owner plays Classic].
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VatsStyle {
    /// The original's flow: the world stopped or slowed, a target and its parts chosen
    /// from a list (docs/design/vats-classic.md).
    Classic,
    /// NVForever's own slow-time mark and fire (docs/design/vats-mode.md).
    #[default]
    DeadEye,
}

impl VatsStyle {
    pub const ALL: [VatsStyle; 2] = [VatsStyle::Classic, VatsStyle::DeadEye];

    /// The style's name on the settings page and in NVForever.ini [design: NVForever's own
    /// words. The slow-time style's is not the name of the mechanic it is modelled on,
    /// which is another publisher's (LEGAL.md; the owner's review of #2102): the docs
    /// name the model, the page does not].
    pub fn name(self) -> &'static str {
        match self {
            VatsStyle::Classic => "Classic",
            VatsStyle::DeadEye => "Mark and Fire",
        }
    }

    /// The style of this name, any case; `dead-eye` and `deadeye` too (the test aid, and
    /// an NVForever.ini written before the style had its own name).
    pub fn by_name(name: &str) -> Option<VatsStyle> {
        let n: String = name.trim().chars().filter(|c| !matches!(c, ' ' | '-' | '_')).flat_map(char::to_lowercase).collect();
        match n.as_str() {
            "classic" => Some(VatsStyle::Classic),
            "markandfire" | "deadeye" => Some(VatsStyle::DeadEye),
            _ => None,
        }
    }
}

/// The least the slider slows the world: half speed [design: owner 2026-11-09, "from 50%
/// time slow to 100% time slow - full stop"].
/// How dark the picture is while the classic style chooses targets, as a part of its own
/// brightness; the selected target is kept in its own light (nvforever `vats_highlight`)
/// [design: the fork owner 2026-11-10, "try removing the color filter when using vats,
/// instead darken the game a bit but not the selected NPC"; the number by eye].
/// Tried at 0.6 and taken back the same day ("disable the darkening when using vats
/// completely"): 1 leaves the picture as it is, and the selected target gets no light of
/// its own then.
pub const DARKEN: f32 = 1.0;

pub const SLOWDOWN_MIN: f32 = 0.5;
/// The slider's step [design: owner 2026-11-09].
pub const SLOWDOWN_STEP: f32 = 0.05;

/// How far the world is slowed while the classic style's targets are chosen: the settings
/// page's "V.A.T.S. time slowdown" (`vats-slowdown` in NVForever.ini), from
/// [`SLOWDOWN_MIN`] to 1, a full stop, which is the original's and the default
/// [documented: fallout.wiki "Fallout: New Vegas V.A.T.S.", the game is paused while
/// targets are chosen; design: owner 2026-11-09, the range and the default].
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct VatsSlowdown(pub f32);

impl Default for VatsSlowdown {
    fn default() -> VatsSlowdown {
        VatsSlowdown(1.0)
    }
}

/// The clock's speed for a slowdown: 1 less it, so 0.75 runs the world at a quarter speed
/// and 1 stops it. Whole hundredths, so a full stop is exactly 0.
pub fn clock_scale(slowdown: f32) -> f32 {
    let s = slowdown.clamp(SLOWDOWN_MIN, 1.0);
    (((1.0 - s) * 100.0).round() / 100.0).max(0.0)
}

/// A slowdown as the settings page shows it.
fn slowdown_text(v: f32) -> String {
    if v >= 0.999 {
        "100% (full stop)".to_string()
    } else {
        format!("{:.0}%", v * 100.0)
    }
}

/// One actor the classic style may target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub actor: u32,
    /// Its bearing from the line the player looked along on entering, radians: negative
    /// to the left, positive to the right.
    pub bearing: f32,
    /// Its angle off that line, radians (up and down counted too).
    pub off: f32,
}

/// The actors with a hit volume a look for targets passed over, by the first rule that
/// left each out (vats-classic.md §3): what the log says when V is refused, so a
/// tester's report can be read.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Missed {
    /// How far the look reached, units.
    pub reach: f32,
    /// Further than that, and how far the nearest of them is.
    pub far: u32,
    pub nearest_far: f32,
    /// More than the front's angle off the entry line.
    pub aside: u32,
    /// Barred: dead, cloaked, never a target, No VATS Melee.
    pub barred: u32,
    /// No part of it shows from the eye.
    pub hidden: u32,
}

impl Missed {
    /// The log's words.
    pub fn text(&self, front_degrees: f32) -> String {
        let mut out = Vec::new();
        if self.far > 0 {
            out.push(format!("{} beyond the reach of {:.0} (the nearest {:.0} away)", self.far, self.reach, self.nearest_far));
        }
        if self.aside > 0 {
            out.push(format!("{} more than {front_degrees:.0} degrees off the view", self.aside));
        }
        if self.hidden > 0 {
            out.push(format!("{} with no part in sight", self.hidden));
        }
        if self.barred > 0 {
            out.push(format!("{} barred", self.barred));
        }
        if out.is_empty() {
            format!("no actor within {:.0}", self.reach)
        } else {
            out.join(", ")
        }
    }
}

/// A target's bearing and angle off the entry line: `dir` the unit line the player looked
/// along, `to` the line from the eye to the target (any length; game space, z up).
pub fn bearing_of(dir: [f32; 3], to: [f32; 3]) -> (f32, f32) {
    let len = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt().max(1e-6);
    let cos = ((dir[0] * to[0] + dir[1] * to[1] + dir[2] * to[2]) / len).clamp(-1.0, 1.0);
    // The headings across the ground (clockwise from +y, as `Player::heading`).
    let a = dir[0].atan2(dir[1]);
    let b = to[0].atan2(to[1]);
    let mut d = b - a;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    (d, cos.acos())
}

/// Put the candidates in the order the player cycles through them, left to right across
/// the view on entering, and name the first selected: the one nearest the entry line
/// [design: the sources say the target nearest the crosshair comes first; the order of
/// the others is ours].
pub fn order(c: &mut [Candidate]) -> Option<usize> {
    c.sort_by(|a, b| a.bearing.total_cmp(&b.bearing).then(a.actor.cmp(&b.actor)));
    (0..c.len()).min_by(|&i, &j| c[i].off.total_cmp(&c[j].off))
}

/// The place `step` on from `i` among `n`, round the ends.
pub fn cycle(i: usize, n: usize, step: i32) -> usize {
    if n == 0 {
        return 0;
    }
    (i as i64 + i64::from(step)).rem_euclid(n as i64) as usize
}

/// The view's turn onto a target, a share of what is left each second [design: the
/// original's camera swings onto the target in a fraction of a second; the fork owner
/// 2026-11-10, "slow down the VATS zoom in/out animation by 50%": it was 14, with the
/// zoom's time (`crate::dialogue_zoom::VATS_IN_SECONDS`)].
pub const TURN_RATE: f32 = 7.0;

/// `from` moved towards the angle `to` the short way round, `rate` of what is left a
/// second, over `dt` real seconds.
pub fn ease_angle(from: f32, to: f32, rate: f32, dt: f32) -> f32 {
    let mut d = (to - from) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    let k = 1.0 - (-rate.max(0.0) * dt.max(0.0)).exp();
    from + d * k
}

/// The test aids' style and slowdown (module notes).
fn from_env() -> (Option<VatsStyle>, Option<f32>) {
    let style = std::env::var("NVF_PLAY_TEST_VATS_STYLE").ok().and_then(|v| {
        let s = VatsStyle::by_name(&v);
        if s.is_none() {
            warn!("vats: NVF_PLAY_TEST_VATS_STYLE={v}: not classic or dead-eye");
        }
        s
    });
    let slow = std::env::var("NVF_PLAY_TEST_VATS_SLOWDOWN").ok().and_then(|v| {
        let s = v.trim().parse::<f32>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(SLOWDOWN_MIN, 1.0));
        if s.is_none() {
            warn!("vats: NVF_PLAY_TEST_VATS_SLOWDOWN={v}: not a number from 0.5 to 1");
        }
        s
    });
    (style, slow)
}

fn style_pick(w: &mut World, name: &str) -> bool {
    match VatsStyle::by_name(name) {
        Some(s) => {
            *w.resource_mut::<VatsStyle>() = s;
            true
        }
        None => false,
    }
}

/// The two resources and their rows of the settings page.
pub fn register(app: &mut App) {
    let (style, slow) = from_env();
    app.insert_resource(style.unwrap_or_default()).insert_resource(slow.map_or_else(VatsSlowdown::default, VatsSlowdown));
    if let Some(s) = style {
        info!("vats: style {} (NVF_PLAY_TEST_VATS_STYLE)", s.name());
    }
    if let Some(s) = slow {
        info!("vats: time slowdown {} (NVF_PLAY_TEST_VATS_SLOWDOWN)", slowdown_text(s));
    }
    crate::settings::add_choice_list(
        app,
        crate::settings::Setting {
            key: "vats-style",
            group: crate::pause::GROUP_MENUS,
            // "V.A.T.S." as the data spells it [observed: FalloutNV.esm MESG `HelpVATS`
            // (0000017D) and `HelpVATS2`, whose names are "V.A.T.S." and "V.A.T.S. --
            // Advanced" (`fnvtool esm inventory`); #1740, LEGAL.md]; "style" is ours.
            label: "V.A.T.S. style",
            note: "What the V.A.T.S. key starts. Classic: the world stops and you pick body parts to hit. Mark and Fire: time runs very slowly while you mark targets.",
            switches: "",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        crate::settings::ChoiceList {
            names: |_| VatsStyle::ALL.iter().map(|s| s.name().to_string()).collect(),
            chosen: |w| w.resource::<VatsStyle>().name().to_string(),
            pick: style_pick,
            save: |w| w.resource::<VatsStyle>().name().to_string(),
            load: style_pick,
            ask_keep: false,
        },
    );
    crate::settings::add_setting(
        app,
        crate::settings::Setting::slider(
            "vats-slowdown",
            crate::pause::GROUP_MENUS,
            // "V.A.T.S." as the data spells it (MESG `HelpVATS`, as the row above).
            "V.A.T.S. time slowdown",
            "How far the world slows while you choose targets in Classic: 100% stops it, 50% runs at half speed. Enemies attack at anything less than a stop.",
            crate::settings::Slider {
                get: |w| w.resource::<VatsSlowdown>().0,
                set: |w, v| w.resource_mut::<VatsSlowdown>().0 = v.clamp(SLOWDOWN_MIN, 1.0),
                min: SLOWDOWN_MIN,
                step: SLOWDOWN_STEP,
                shown: Some(slowdown_text),
            },
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_names_what_was_passed_over() {
        let none = Missed { reach: 5000.0, ..Missed::default() };
        assert_eq!(none.text(55.0), "no actor within 5000");
        let m = Missed { reach: 350.0, far: 2, nearest_far: 612.4, aside: 1, barred: 0, hidden: 3 };
        assert_eq!(m.text(55.0), "2 beyond the reach of 350 (the nearest 612 away), 1 more than 55 degrees off the view, 3 with no part in sight");
    }

    #[test]
    fn the_slowdown_sets_the_clock() {
        assert_eq!(clock_scale(1.0), 0.0, "a full stop is exactly 0");
        assert_eq!(clock_scale(0.75), 0.25);
        assert_eq!(clock_scale(0.5), 0.5);
        assert_eq!(clock_scale(0.95), 0.05, "Dead Eye's speed is within the range");
        // Out of range values are brought into it.
        assert_eq!(clock_scale(0.2), 0.5);
        assert_eq!(clock_scale(3.0), 0.0);
        assert_eq!(VatsSlowdown::default().0, 1.0, "the original's full stop is the default");
    }

    #[test]
    fn the_slowdown_reads_as_a_percentage() {
        assert_eq!(slowdown_text(0.5), "50%");
        assert_eq!(slowdown_text(0.85), "85%");
        assert_eq!(slowdown_text(1.0), "100% (full stop)");
    }

    #[test]
    fn styles_go_by_name() {
        assert_eq!(VatsStyle::default(), VatsStyle::DeadEye, "Dead Eye until owner-review 31a is answered");
        for s in VatsStyle::ALL {
            assert_eq!(VatsStyle::by_name(s.name()), Some(s));
        }
        assert_eq!(VatsStyle::by_name("dead-eye"), Some(VatsStyle::DeadEye));
        assert_eq!(VatsStyle::by_name(" CLASSIC "), Some(VatsStyle::Classic));
        assert_eq!(VatsStyle::by_name("fast"), None);
    }

    #[test]
    fn targets_run_left_to_right_and_start_nearest_the_crosshair() {
        let look = [0.0, 1.0, 0.0];
        let c = |actor, to: [f32; 3]| {
            let (bearing, off) = bearing_of(look, to);
            Candidate { actor, bearing, off }
        };
        let mut list = vec![c(1, [500.0, 500.0, 0.0]), c(2, [-50.0, 900.0, 0.0]), c(3, [-700.0, 400.0, 0.0])];
        let first = order(&mut list).unwrap();
        assert_eq!(list.iter().map(|c| c.actor).collect::<Vec<_>>(), [3, 2, 1]);
        assert_eq!(list[first].actor, 2);
        assert!(list[0].bearing < 0.0 && list[2].bearing > 0.0);
        assert_eq!(order(&mut []), None);
    }

    #[test]
    fn cycling_wraps_round() {
        assert_eq!(cycle(0, 3, 1), 1);
        assert_eq!(cycle(2, 3, 1), 0);
        assert_eq!(cycle(0, 3, -1), 2);
        assert_eq!(cycle(0, 0, 1), 0);
        assert_eq!(cycle(0, 1, -1), 0);
    }

    #[test]
    fn the_view_turns_the_short_way() {
        let pi = std::f32::consts::PI;
        // From just short of a half turn one way to just past it: a small step, not a
        // whole turn back.
        let next = ease_angle(pi - 0.1, -pi + 0.1, TURN_RATE, 0.016);
        assert!(next > pi - 0.1 && next < pi - 0.05, "{next}");
        // Long enough, it arrives.
        assert!((ease_angle(0.0, 1.0, TURN_RATE, 5.0) - 1.0).abs() < 1e-4);
        // No time, no turn.
        assert_eq!(ease_angle(0.3, 1.0, TURN_RATE, 0.0), 0.3);
    }
}
