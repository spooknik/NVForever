//! [design: player-script-sequence-pose-aid.md] Observer only: reads the real
//! post-propagation rigs. It never writes requests, clips, layers or transforms.
use super::*;
use nvf_play::script::ScriptState;
use serde_json::Value;

const PHASES: [&str; 5] = [
    "before",
    "active-quarter",
    "active-three-quarter",
    "fade",
    "released",
];

pub(super) struct Aid {
    input: Value,
    next: usize,
    serial: Option<u32>,
    nodes: Option<[Entity; 2]>,
    failed: bool,
    baseline: Option<[Vec3; 4]>,
    active: Option<[Vec3; 2]>,
}

fn vector(value: &Value) -> Option<Vec3> {
    let a = value.as_array()?;
    if a.len() != 3 {
        return None;
    }
    let n = [a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?];
    if n.iter()
        .any(|n| !n.is_finite() || n.abs() > f64::from(f32::MAX))
    {
        return None;
    }
    Some(Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32))
}

fn load() -> Result<Aid, String> {
    let path = std::env::var("NVF_PLAY_TEST_SEQUENCE_POSE").map_err(|e| e.to_string())?;
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(16385)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16384 {
        return Err("manifest exceeds 16KiB".into());
    }
    let input: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if input["schema"] != "nvf.sequence-pose-observer/1"
        || input["path"] != "qa\\sequence2395\\owned_pose.kf"
        || input["bone"]
            .as_str()
            .is_none_or(|s| s.is_empty() || s.len() > 128)
    {
        return Err("schema, owned path or bone invalid".into());
    }
    let tolerance = input["tolerance"].as_f64().ok_or("missing tolerance")?;
    if !tolerance.is_finite() || !(0.0..=0.01).contains(&tolerance) {
        return Err("tolerance invalid".into());
    }
    for field in ["base", "clip_start", "clip_end"] {
        if vector(&input[field]).is_none() {
            return Err(format!("invalid {field}"));
        }
    }
    let base = vector(&input["base"]).expect("checked vector");
    let from = vector(&input["clip_start"]).expect("checked vector");
    let to = vector(&input["clip_end"]).expect("checked vector");
    let separation = (tolerance as f32 * 10.0).max(0.1);
    if from.distance(to) <= separation
        || base.distance(from) <= separation
        || base.distance(to) <= separation
    {
        return Err("fixture must discriminate base and changing active poses".into());
    }
    Ok(Aid {
        input,
        next: 0,
        serial: None,
        nodes: None,
        failed: false,
        baseline: None,
        active: None,
    })
}

// None means the aid is disabled, including no file I/O or environment changes.
// The local is initialized once. An invalid enabled manifest reports failure and
// can never produce the scenario's five completion lines.
pub(super) fn observe(
    body: Res<PlayerBody>,
    script: Option<Res<ScriptState>>,
    samples: Res<crate::script_sequences::PlayerSequences>,
    transforms: Query<(&Transform, &GlobalTransform, Option<&ChildOf>)>,
    globals: Query<&GlobalTransform>,
    mut state: Local<Option<Option<Aid>>>,
) {
    if state.is_none() {
        *state = Some(
            if std::env::var_os("NVF_PLAY_TEST_SEQUENCE_POSE").is_some() {
                match load() {
                    Ok(aid) => Some(aid),
                    Err(e) => {
                        error!("sequence pose aid: FAIL manifest {e}");
                        None
                    }
                }
            } else {
                None
            },
        );
    }
    let Some(aid) = state.as_mut().and_then(Option::as_mut) else {
        return;
    };
    if aid.failed || aid.next == PHASES.len() {
        return;
    }
    let Some(runtime) = script.as_deref().and_then(|s| s.rt.as_ref()) else {
        return;
    };
    let game = &runtime.game;
    let Some(body) = body.body.as_ref() else {
        return;
    };
    let Some(first) = body.first.as_ref() else {
        return;
    };
    let bone = aid.input["bone"].as_str().expect("validated bone");
    let node = |rig: &Rig| {
        rig.animator
            .skel
            .by_name
            .get(bone)
            .and_then(|i| rig.animator.nodes.get(*i))
            .copied()
    };
    let Some(nodes) = node(first).zip(node(&body.third)).map(|(a, b)| [a, b]) else {
        return;
    };
    if aid.nodes.is_some_and(|before| before != nodes) {
        error!("sequence pose aid: FAIL rig identity changed");
        aid.failed = true;
        return;
    }
    let key = (
        FormId::PLAYER_REF,
        true,
        "qa\\sequence2395\\owned_pose.kf".to_string(),
    );
    let request = game.anim_paths.sequences.get(&key);
    if let Some(request) = request {
        if aid.serial.is_some_and(|before| before != request.serial) {
            error!("sequence pose aid: FAIL request replaced");
            aid.failed = true;
            return;
        }
        aid.serial = Some(request.serial);
    }
    let low_path = "qa\\sequence2395\\owned_underlying.kf".to_string();
    let low_first = game
        .anim_paths
        .sequences
        .get(&(FormId::PLAYER_REF, true, low_path.clone()));
    let low_third = game
        .anim_paths
        .sequences
        .get(&(FormId::PLAYER_REF, false, low_path));
    if !low_first
        .zip(low_third)
        .is_some_and(|(a, b)| a.stopped.is_none() && b.stopped.is_none())
    {
        return;
    }
    let low_tags = low_first
        .zip(low_third)
        .map(|(a, b)| [0x4000_0000 | a.serial, 0x4000_0000 | b.serial])
        .expect("checked requests");
    if !first.animator.layers.iter().any(|l| l.tag == low_tags[0])
        || !body
            .third
            .animator
            .layers
            .iter()
            .any(|l| l.tag == low_tags[1])
    {
        return;
    }
    if let Some(request) = request {
        if !samples.first.iter().any(|s| {
            s.request.serial == request.serial
                && s.clip.cycle_type == 2
                && s.clip.start == 0.0
                && s.clip.stop == 1.0
                && s.clip.frequency == 1.0
        }) {
            return;
        }
    }
    let clock = game.script_sequence_time_for(FormId::PLAYER_REF, &key.2, true);
    let ready = match aid.next {
        0 => request.is_none() && aid.serial.is_none(),
        1 => request.is_some_and(|r| r.stopped.is_none()) && (0.25..0.5).contains(&clock),
        2 => request.is_some_and(|r| r.stopped.is_none()) && (0.75..1.0).contains(&clock),
        3 => request.is_some_and(|r| r.stopped.is_some_and(|at| game.time - at >= 0.125)),
        4 => aid.serial.is_some() && request.is_none(),
        _ => false,
    };
    if !ready {
        return;
    }
    let Ok((first_local, first_world, first_parent)) = transforms.get(nodes[0]) else {
        return;
    };
    let Ok((third_local, third_world, third_parent)) = transforms.get(nodes[1]) else {
        return;
    };
    let actual = [
        first_local.translation,
        first_world.translation(),
        third_local.translation,
        third_world.translation(),
    ];
    let phase = PHASES[aid.next];

    let tolerance = aid.input["tolerance"]
        .as_f64()
        .expect("validated tolerance") as f32;
    let tag = aid.serial.map(|serial| 0x4000_0000 | serial);
    let in_first =
        tag.is_some_and(|tag| first.animator.layers.iter().any(|layer| layer.tag == tag));
    let in_third = tag.is_some_and(|tag| {
        body.third
            .animator
            .layers
            .iter()
            .any(|layer| layer.tag == tag)
    });
    let expected_layer = (1..=3).contains(&aid.next);
    let base = vector(&aid.input["base"]).expect("validated base");
    let from = vector(&aid.input["clip_start"]).expect("validated clip_start");
    let to = vector(&aid.input["clip_end"]).expect("validated clip_end");
    let sampled = from.lerp(to, clock.clamp(0.0, 1.0) as f32);
    // The observer runs on render frames, not the core's fixed tick. The CPU
    // fixture independently checks the fade clock. Here read the actual retained
    // layer's share and require a genuine intermediate fade, then check its ECS
    // output; never assume that two clocks land on the same frame boundary.
    let layer = tag.and_then(|tag| first.animator.layers.iter().find(|l| l.tag == tag));
    let share = if aid.next == 3 {
        let Some(layer) = layer else {
            return;
        };
        if !layer.ease.is_finite() || !(layer.ease > 0.0 && layer.ease < 1.0) {
            return;
        }
        layer.ease
    } else if expected_layer {
        1.0
    } else {
        0.0
    };
    let expected = base.lerp(sampled, share);

    let Some(first_parent) = first_parent.and_then(|p| globals.get(p.parent()).ok()) else {
        return;
    };
    let Some(third_parent) = third_parent.and_then(|p| globals.get(p.parent()).ok()) else {
        return;
    };
    let desired = [
        expected,
        first_parent.affine().transform_point3(expected),
        base,
        third_parent.affine().transform_point3(base),
    ];
    let stable = aid.baseline.is_none_or(|before| {
        before[2].abs_diff_eq(actual[2], tolerance) && before[3].abs_diff_eq(actual[3], tolerance)
    });
    let distinct = aid.next != 2
        || aid.active.is_some_and(|before| {
            before[0].distance(actual[0]) > tolerance * 10.0
                && before[1].distance(actual[1]) > tolerance * 10.0
        });
    let ok = distinct
        && stable
        && actual.iter().zip(desired).all(|(actual, expected)| {
            actual.is_finite() && actual.abs_diff_eq(expected, tolerance)
        })
        && in_first == expected_layer
        && !in_third;
    info!(
        "sequence pose aid: measured phase={phase} clock={clock} time={} nodes={nodes:?} local_world={actual:?} first_tag={in_first} third_tag={in_third} measured_share={share}",
        game.time
    );
    if !ok {
        error!("sequence pose aid: FAIL phase={phase}");
        aid.failed = true;
        return;
    }
    if aid.next == 1 {
        aid.active = Some([actual[0], actual[1]]);
    }
    aid.nodes = Some(nodes);
    aid.baseline.get_or_insert(actual);
    info!("sequence pose aid: PASS phase={phase}");
    aid.next += 1;
}
