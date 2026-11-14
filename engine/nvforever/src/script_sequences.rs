//! Script-requested player animation layers. Separate from cinematic IDLE slots:
//! an animated wrist display must leave walking and mouse input with the player.

use bevy::prelude::*;
use fnv_actor::idle_play::IdleKeys;
use fnv_game::nvse_anim::ScriptSequence;
use fnv_game::nvse_knvse::SequenceInfo;
use fnv_sim::FormId;
use nvf_assets::actor::{ActorCtx, ActorParts};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::ModelCtx;
use nvf_play::script::ScriptState;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Sample {
    pub request: ScriptSequence,
    pub clip: Arc<Clip>,
    pub time: f32,
    pub weight: f32,
    pub fade_in: f32,
}

#[derive(Resource, Default)]
pub(crate) struct PlayerSequences {
    pub first: Vec<Sample>,
    pub third: Vec<Sample>,
}

pub(crate) fn prepare(
    mut script: Option<ResMut<ScriptState>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut samples: ResMut<PlayerSequences>,
    mut told: Local<HashSet<u32>>,
) {
    samples.first.clear();
    samples.third.clear();
    let Some(script) = script.as_deref_mut() else { return; };
    let Some(runtime) = script.rt.as_mut() else { return; };
    let g = &mut runtime.game;
    let requests: Vec<_> = g.anim_paths.sequences.iter()
        .filter(|((actor, _, _), _)| *actor == FormId::PLAYER_REF)
        .map(|(_, request)| request.clone()).collect();
    let active: HashSet<_> = requests.iter().map(|r|r.serial).collect();
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for request in requests {
        let path = if request.path.to_ascii_lowercase().starts_with("meshes\\") {
            request.path.clone()
        } else { format!("meshes\\{}", request.path) };
        if !ctx.clip_ready(&path) { continue; }
        let Some(clip) = ctx.clip(&path) else {
            if told.insert(request.serial) { warn!("script sequence: {:?} could not load; not rendered", request.path); }
            g.anim_paths.sequences.remove(&(FormId::PLAYER_REF, request.first, request.path.replace('/', "\\").to_ascii_lowercase()));
            if let Some(paths) = g.anim_paths.played.get_mut(&FormId::PLAYER_REF) {
                paths.retain(|p| !p.eq_ignore_ascii_case(&request.path));
            }
            continue;
        };
        g.set_sequence_info(&request.path, SequenceInfo {
            name:clip.name.clone(), accum_root:clip.accum_root.clone().unwrap_or_default(),
            frequency:f64::from(clip.frequency), begin:f64::from(clip.start), end:f64::from(clip.stop),
            cycle:clip.cycle_type, text_keys:clip.text_keys.iter().map(|(t,s)| (f64::from(*t),s.clone())).collect(),
        });
        let clock = g.script_sequence_time_for(FormId::PLAYER_REF, &request.path, request.first);
        let weight = f64::from(request.weight);
        let phase = (clock * f64::from(clip.frequency)) as f32;
        if !phase.is_finite() || !weight.is_finite() { continue; }
        let keys = IdleKeys::read(clip.start, clip.stop, &clip.text_keys);
        let sample = Sample { time:phase,
            weight:weight.clamp(0.0,1.0) as f32, fade_in:request.ease_in.unwrap_or(keys.blend_in),
            request, clip };
        if told.insert(sample.request.serial) {
            info!("script sequence: player {} {:?}, priority {}, weight {}, {} tracks", if sample.request.first {"first"} else {"third"}, sample.request.path, sample.request.priority, sample.weight, sample.clip.tracks.len());
        }
        if sample.request.first { samples.first.push(sample); } else { samples.third.push(sample); }
    }
    samples.first.sort_by_key(|s|s.request.serial);
    samples.third.sort_by_key(|s|s.request.serial);
    told.retain(|serial|active.contains(serial));
}
