//! Light and noise gauges while sneaking (owner request 2026-10-24, after the stealth
//! games that show how lit and how loud the player is) [design: not the original's; the
//! original shows only the sneak word]. Off by default; the settings page's "Gameplay"
//! switch `sneak-gauges` turns them on, kept in the INI like every switch.
//!
//! Two segmented bars under the sneak word, top middle, while the player sneaks and the
//! HUD is up:
//!
//! - **Light**: the light level at the player, 0–100, as detection measures it
//!   (nvforever `detection.rs`: ambient, the sun unless shaded, the lights that reach
//!   the player, the Pip-Boy light), before fSneakLightMult.
//! - **Noise**: the sound the player makes as detection counts it before the distance
//!   falloff (`fnv_game::detection::Settings::noise`: footsteps by body armour weight,
//!   × 1.5 running unless Silent Running, shots by their sound level), shown against
//!   [`NOISE_FULL`]. A shot fills it at once and it falls back over [`NOISE_FALL`]
//!   seconds, so a moment's sound can be seen.
//!
//! The values are written by the detection pass (four times a second); the bars ease
//! towards them. They show what the player gives off, not whether anyone notices: the
//! sneak word says that.

use crate::hud::{HudSet, HudView};
use crate::interface::{amber, u};
use crate::settings::{add_setting, Setting};
use crate::PlaySet;
use bevy::prelude::*;

/// The noise that fills the bar: a walk in light armour is about a third, a run
/// about a half, a Normal shot (50 × 2 × 1.6 = 160) over full [design].
pub const NOISE_FULL: f32 = 100.0;
/// Seconds the noise bar takes to fall from full to empty [design].
pub const NOISE_FALL: f32 = 1.5;
/// Seconds the light bar takes to follow a change across the whole bar [design].
const LIGHT_FOLLOW: f32 = 0.5;
/// Segments of each bar [design: as the stealth games' bars].
const SEGMENTS: usize = 10;
/// The piece: width, a bar's height, the row's height; its top, under the sneak word.
const WIDTH: f32 = 220.0;
const BAR: f32 = 10.0;
const ROW: f32 = 22.0;
const TOP: f32 = 64.0;
const LABEL: f32 = 60.0;

/// What the gauges show, written by the detection pass.
#[derive(Resource, Debug, Default, Clone)]
pub struct SneakGauges {
    /// The setting.
    pub on: bool,
    /// The light level at the player, 0–100.
    pub light: f32,
    /// The player's noise, in the sound term's units (0 standing still).
    pub noise: f32,
    /// What the bars show now, 0–1 each.
    shown: [f32; 2],
    /// Seconds shown, and the most light and noise seen, for the exit line.
    secs: f32,
    most: [f32; 2],
}

impl SneakGauges {
    /// Where the bars go next from `shown` towards the values, `dt` seconds on: light
    /// eases both ways; noise jumps up and falls at [`NOISE_FALL`].
    pub fn step(shown: [f32; 2], light: f32, noise: f32, dt: f32) -> [f32; 2] {
        let l = (light / 100.0).clamp(0.0, 1.0);
        let n = (noise / NOISE_FULL).clamp(0.0, 1.0);
        let follow = dt / LIGHT_FOLLOW;
        let light = shown[0] + (l - shown[0]).clamp(-follow, follow);
        let noise = if n >= shown[1] { n } else { (shown[1] - dt / NOISE_FALL).max(n) };
        [light, noise]
    }
}

pub struct SneakGaugesPlugin;

impl Plugin for SneakGaugesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SneakGauges>()
            .add_systems(Update, (spawn, draw).chain().after(HudSet).in_set(PlaySet::Ui))
            .add_systems(Last, report);
        add_setting(
            app,
            Setting {
                key: "sneak-gauges",
                group: crate::pause::GROUP_MENUS,
                label: "Light and noise gauges while sneaking",
                note: "On: two bars under the sneak word show how lit you are and how much noise you make, as detection counts them. Not in the original.",
                switches: "",
                get: |w| w.resource::<SneakGauges>().on,
                set: |w, on| w.resource_mut::<SneakGauges>().on = on,
                slider: None,
            },
        );
    }
}

#[derive(Component)]
struct GaugeRoot;

/// Segment `k` of bar `bar` (0 light, 1 noise).
#[derive(Component)]
struct Segment {
    bar: usize,
    k: usize,
}

fn spawn(mut commands: Commands, root: Query<(), With<GaugeRoot>>) {
    // The HUD's lengths at the UI scale (`crate::interface::hud_units`, #1631).
    let _hud = crate::interface::hud_units();
    if !root.is_empty() {
        return;
    }
    let node = Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(50.0),
        margin: UiRect::left(u(-WIDTH / 2.0)),
        top: u(TOP),
        width: u(WIDTH),
        height: u(ROW * 2.0),
        flex_direction: FlexDirection::Column,
        ..default()
    };
    commands.spawn((GaugeRoot, node, Visibility::Hidden, GlobalZIndex(5))).with_children(|p| {
        for (bar, name) in ["LIGHT", "NOISE"].into_iter().enumerate() {
            p.spawn(Node { height: u(ROW), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                row.spawn((Node { width: u(LABEL), ..default() }, Text::new(name), TextFont { font_size: crate::interface::font_units(13.0), ..default() }, TextColor(amber(255.0))));
                row.spawn(Node { width: u(WIDTH - LABEL), height: u(BAR), column_gap: u(3.0), ..default() }).with_children(|segs| {
                    for k in 0..SEGMENTS {
                        segs.spawn((Segment { bar, k }, Node { flex_grow: 1.0, height: Val::Percent(100.0), ..default() }, BackgroundColor(amber(60.0))));
                    }
                });
            });
        }
    });
}

fn draw(
    time: Res<Time>,
    view: Res<HudView>,
    mut gauges: ResMut<SneakGauges>,
    mut root: Query<&mut Visibility, With<GaugeRoot>>,
    mut segments: Query<(&Segment, &mut BackgroundColor)>,
) {
    let show = gauges.on && view.shown && view.sneak.is_some();
    let Ok(mut vis) = root.single_mut() else { return };
    let want = if show { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
        if show {
            info!("sneak gauges shown: light {:.0}, noise {:.0}", gauges.light, gauges.noise);
        }
    }
    if !show {
        return;
    }
    let dt = time.delta_secs();
    let g = &mut *gauges;
    g.shown = SneakGauges::step(g.shown, g.light, g.noise, dt);
    g.secs += dt;
    g.most = [g.most[0].max(g.light), g.most[1].max(g.noise)];
    for (s, mut c) in &mut segments {
        let lit = (s.k as f32 + 0.5) / SEGMENTS as f32 <= g.shown[s.bar];
        c.0 = amber(if lit { 255.0 } else { 60.0 });
    }
}

fn report(mut exit: MessageReader<AppExit>, gauges: Res<SneakGauges>) {
    if exit.read().next().is_none() {
        return;
    }
    info!("sneak gauges summary: on {}, shown {:.1} s, most light {:.0}, most noise {:.0}", gauges.on, gauges.secs, gauges.most[0], gauges.most[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_jumps_up_and_falls_slowly_light_eases() {
        let up = SneakGauges::step([0.0, 0.0], 100.0, 160.0, 0.1);
        assert!((up[0] - 0.2).abs() < 1e-5, "light moves 0.1 / 0.5 of the bar");
        assert_eq!(up[1], 1.0, "a shot fills the noise bar at once");
        let down = SneakGauges::step([1.0, 1.0], 100.0, 0.0, 0.15);
        assert!((down[1] - 0.9).abs() < 1e-5, "and it falls a tenth in 0.15 s");
        assert_eq!(down[0], 1.0);
        let walk = SneakGauges::step([0.0, 0.0], 0.0, 31.2, 0.1);
        assert!((walk[1] - 0.312).abs() < 1e-5);
    }
}
