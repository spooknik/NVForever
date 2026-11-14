//! Owned behavioral fixture for the existing idle, body and player systems.
//! The original two Script cases use interfaces present on 9cb7cf70; they do not
//! call the new hold predicates or write the per-Playing policy flag.
//! The Activation case uses IdleSource::Activate from upstream 805454db.

use super::*;
use bevy::ecs::system::RunSystemOnce;
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_sim::{FormStore, RefTable, SimConfig};
use nvf_play::furniture::PlayerSeat;
use nvf_play::phys::LoadedCell;
use nvf_play::phys::cells::PhysCell;
use nvf_play::player::{MoveSpeeds, PlayerPlugin};
use nvf_play::{PlayData, PlaySet, PlayState};
use std::time::Duration;

const MOVEMENT: u32 = 0xA01;
const CINEMATIC: u32 = 0xA02;
const THIRD_PERSON: u32 = 0xA03;

fn words(out: &mut Vec<u8>, values: &[u32]) { out.extend(values.iter().flat_map(|v| v.to_le_bytes())); }
fn floats(out: &mut Vec<u8>, values: &[f32]) { out.extend(values.iter().flat_map(|v| v.to_le_bytes())); }

// Owned byte builders follow schemas/nif and the existing outfits test fixture.
fn nif(blocks: &[(&str, Vec<u8>)], strings: &[&str]) -> Vec<u8> {
    fn string(out: &mut Vec<u8>, s: &str) { words(out, &[s.len() as u32]); out.extend(s.as_bytes()); }
    let mut out = b"Gamebryo File Format, Version 20.2.0.7\n".to_vec();
    words(&mut out, &[0x1402_0007]);
    out.push(1);
    words(&mut out, &[11, blocks.len() as u32, 34]);
    out.extend([1, 0, 1, 0, 1, 0]);
    let mut types = Vec::new();
    for (kind, _) in blocks { if !types.contains(kind) { types.push(*kind); } }
    out.extend((types.len() as u16).to_le_bytes());
    for kind in &types { string(&mut out, kind); }
    for (kind, _) in blocks { out.extend((types.iter().position(|t| t == kind).unwrap() as u16).to_le_bytes()); }
    for (_, block) in blocks { words(&mut out, &[block.len() as u32]); }
    words(&mut out, &[strings.len() as u32, strings.iter().map(|s| s.len()).max().unwrap_or(0) as u32]);
    for s in strings { string(&mut out, s); }
    words(&mut out, &[0]);
    for (_, block) in blocks { out.extend(block); }
    words(&mut out, &[1, 0]);
    out
}

fn node(name: u32, children: &[u32], z: f32) -> Vec<u8> {
    let mut out = Vec::new();
    words(&mut out, &[name, 0, u32::MAX, 14]);
    floats(&mut out, &[0.0, 0.0, z, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
    words(&mut out, &[0, u32::MAX, children.len() as u32]);
    words(&mut out, children);
    words(&mut out, &[0]);
    out
}

fn clip(target: &str) -> Vec<u8> {
    let mut sequence = Vec::new();
    words(&mut sequence, &[0, 1, 1, 1, u32::MAX]); // name, count, growth, interpolator, controller
    sequence.push(50); // priority
    words(&mut sequence, &[1, u32::MAX, u32::MAX, u32::MAX, u32::MAX]); // controlled target and unused strings
    floats(&mut sequence, &[1.0]);
    words(&mut sequence, &[u32::MAX, 0]); // no text keys, looping clip
    floats(&mut sequence, &[1.0, 0.0, 0.7]);
    words(&mut sequence, &[u32::MAX, u32::MAX]); // manager, accumulation root
    sequence.extend(0u16.to_le_bytes());
    let mut interpolator = Vec::new();
    floats(&mut interpolator, &[0.0, 0.0, 70.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    words(&mut interpolator, &[u32::MAX]); // static transform, no key data
    nif(&[("NiControllerSequence", sequence), ("NiTransformInterpolator", interpolator)], &["OwnedOverlay", target])
}

fn plugin() -> Vec<u8> {
    fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord { Subrecord { sig: *sig, data, extended: false } }
    fn record(sig: &[u8; 4], id: u32, subs: &[Subrecord]) -> Vec<u8> {
        let body = write_subrecords(subs);
        [&sig[..], &(body.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &id.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &body].concat()
    }
    fn group(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
        [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &sig[..], &0i32.to_le_bytes(), &[0; 8], body].concat()
    }
    let head = record(b"TES4", 0, &[sub(b"HEDR", [1.34f32.to_le_bytes(), 0u32.to_le_bytes(), 0xB00u32.to_le_bytes()].concat())]);
    let player = record(b"NPC_", 7, &[sub(b"EDID", b"OwnedPlayer\0".to_vec())]);
    let idles: Vec<u8> = [(MOVEMENT, 1, "OwnedMovement", "OwnedCamera.kf"), (CINEMATIC, 7, "OwnedCinematic", "OwnedCamera.kf"), (THIRD_PERSON, 1, "OwnedThird", "OwnedThird.kf")]
        .into_iter().flat_map(|(id, section, name, path)| {
            record(b"IDLE", id, &[
                sub(b"EDID", [name.as_bytes(), &[0]].concat()), sub(b"MODL", [path.as_bytes(), &[0]].concat()),
                sub(b"ANAM", vec![0; 8]), sub(b"DATA", vec![section, 255, 255, 0, 0, 0, 0, 0]),
            ])
        }).collect();
    [head, group(b"NPC_", &player), group(b"IDLE", &idles)].concat()
}

struct Fixture {
    app: App,
    _files: crate::outfits::tests::TestFiles,
}

impl Fixture {
    fn new() -> Self {
        use crate::outfits::tests::{asset_fixture, empty_world_data, OTHER};
        use crate::residents::{Clock, Residents};
        let (mut app, files) = asset_fixture();
        let old = app.world().resource::<nvf_assets::GameData>().clone();
        let dir = old.vfs.data_dir(); // task-owned TEMP directory from asset_fixture
        let skeleton = nif(&[("NiNode", node(0, &[1, 2], 0.0)), ("NiNode", node(1, &[], 0.0)), ("NiNode", node(2, &[], 70.0))], &["Root", "Weapon", "Camera1st"]);
        std::fs::write(dir.join("meshes/skeleton.nif"), skeleton).unwrap();
        std::fs::write(dir.join("meshes/OwnedCamera.kf"), clip("Camera1st")).unwrap();
        std::fs::write(dir.join("meshes/OwnedThird.kf"), clip("Root")).unwrap();
        app.insert_resource(nvf_assets::GameData { vfs: Arc::new(fnv_vfs::Vfs::open(dir, &[]).unwrap()), schema: old.schema.clone() });

        let plugins = Box::leak(vec![fnv_esm::Plugin::parse(&plugin(), "OwnedCamera.esm".into()).unwrap()].into_boxed_slice());
        let mut data = empty_world_data();
        data.lo = Box::leak(Box::new(fnv_esm::LoadOrder::new(plugins).unwrap()));
        data.index = Box::leak(Box::new(fnv_world::WorldIndex::build(data.lo, data.schema)));
        let mut residents = Residents::build(&data, &[], Clock { hour: 12.0, days_passed: 0.0, rate: 0.0 }, 80.0, false).unwrap();
        let g = residents.own_game().unwrap();
        assert_eq!(g.data.idles.get(&FormId(MOVEMENT)).unwrap().section, 1);
        assert_eq!(g.data.idles.get(&FormId(CINEMATIC)).unwrap().section, 7);
        let forms: &'static FormStore = Box::leak(Box::new(FormStore::build(data.lo, data.schema)));
        let refs: &'static RefTable = Box::leak(Box::new(RefTable::build(data.lo, data.schema)));
        app.insert_resource(residents).insert_resource(PlayWorld(PlayData {
            lo: data.lo, schema: data.schema, index: data.index, world: 0, interior: None,
            eye: [1000.0, 1000.0, 130.0], heading_deg: 0.0, pitch_deg: 0.0, auto_walk: 0.0,
            start_at_cell_spot: false, packages: false,
        })).insert_resource(nvf_play::sim::SimState { forms, refs, world: fnv_sim::World::new(forms, refs, SimConfig::default()) });
        app.add_plugins(PlayerPlugin)
            .configure_sets(Update, PlaySet::Input.run_if(|| false))
            .insert_resource(State::new(PlayState::Playing))
            .insert_resource(Time::<()>::default())
            .insert_resource(MoveSpeeds { walk: 80.0, run: 160.0 })
            .add_message::<nvf_play::Travel>().add_message::<nvf_play::falls::Fell>()
            .init_resource::<PlayerBody>().init_resource::<PlayerIdles>()
            .init_resource::<crate::script_sequences::PlayerSequences>()
            .init_resource::<crate::idles::Idles>().init_resource::<crate::idles::IdleHolds>()
            .init_resource::<crate::furniture::BodyOverride>().init_resource::<PlayerSeat>()
            .init_resource::<nvf_play::pipboy::PipBoy>().init_resource::<WeaponAnim>()
            .init_resource::<crate::death_cam::DeathCam>().init_resource::<crate::player_knockdown::PlayerKnockdown>()
            .init_resource::<crate::weapon_lower::Ahead>().init_resource::<nvf_play::phys_load::PhysLoad>();
        let mut phys = PhysState::default();
        let heights = vec![0.0; fnv_world::land::SIDE * fnv_world::land::SIDE];
        phys.world.add_terrain((0, 0), &heights);
        phys.cells.insert(PhysCell::Exterior(0, 0), LoadedCell { refs: Vec::new(), heights: Some(heights) });
        assert!(phys.place_player([1000.0, 1000.0, 0.0], 50.0, 100.0).is_some());
        let feet = phys.player.as_ref().unwrap().feet;
        app.insert_resource(phys).insert_resource(Player { feet, heading: 0.0, pitch: 0.0, eye_height: 70.0, body_eye: None, body_view: None, placed: None });
        let parent = app.world_mut().spawn((Transform::IDENTITY, Visibility::Inherited)).id();
        let body = app.world_mut().run_system_once(move |mut commands: Commands, world: Res<ActorWorld>, mut parts: ResMut<ActorParts>, mut models: ModelCtx| {
            let a = world.resolve(OTHER.0, FormId::PLAYER_REF.0).unwrap();
            let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
            let camera = ctx.clip("meshes/OwnedCamera.kf").unwrap();
            assert!(camera.tracks.iter().any(|(name, _)| name == "Camera1st"));
            assert!(ctx.clip("meshes/OwnedThird.kf").is_some());
            Body {
                worn: Vec::new(), look: None, change: Default::default(), female: false, scale: 1.0,
                third: build_rig(&mut commands, parent, &world, &mut ctx, &a, View::Third, None).unwrap(),
                first: Some(build_rig(&mut commands, parent, &world, &mut ctx, &a, View::First, None).unwrap()),
            }
        }).unwrap();
        app.world_mut().resource_mut::<PlayerBody>().body = Some(body);
        assert!(!app.world().resource::<nvf_play::player::NoClip>().0, "the fixture uses collision and the walking step path");
        Self { app, _files: files }
    }

    fn ask(&mut self, id: u32) {
        self.app.world_mut().resource_mut::<crate::residents::Residents>().own_game().unwrap()
            .ask_idle(FormId::PLAYER_REF, FormId(id), fnv_game::idles::IdleSource::Script);
    }

    fn frame(&mut self, seconds: f32) {
        let w = self.app.world_mut();
        w.resource_mut::<Time>().advance_by(Duration::from_secs_f32(seconds));
        // Actual systems, not copies of their hold policy. The first two produce the
        // seat constraint, then PlayerPlugin's normal Update runs its private step.
        // Startup is deliberately not run: all owned resources/rigs are ready above.
        w.run_system_once(crate::idles::drive).unwrap();
        w.run_system_once(drive_body).unwrap();
        w.run_schedule(Update);
    }

    fn held(&self) -> bool { self.app.world().resource::<PlayerSeat>().hold.is_some() }
    fn feet(&self) -> [f32; 3] { self.app.world().resource::<PhysState>().player.as_ref().unwrap().feet }
    fn forward(&mut self) { self.app.world_mut().resource_mut::<MoveIntent>().forward = 1.0; }
    fn moves(&self) -> u64 { self.app.world().resource::<nvf_play::phys_load::PhysLoad>().moves }
}

#[test]
fn movement_camera_ecs_keeps_a_looping_overlay_free_to_step() {
    let mut f = Fixture::new();
    f.forward();
    f.ask(THIRD_PERSON);
    f.frame(0.0);
    assert!(!f.held(), "a third-person idle does not acquire the camera seat hold");
    f.ask(MOVEMENT);
    f.frame(0.0);
    assert!(!f.held(), "the actual idle/body systems must not acquire the seat hold for Movement");
    let before = f.feet();
    let steps = f.app.world().resource::<PhysState>().world.steps();
    for _ in 0..20 { f.frame(0.05); }
    assert!(f.app.world().resource::<PlayerIdles>().first.playing().is_some(), "the 0.7-second clip still loops after a second");
    assert!(!f.held());
    assert!(f.app.world().resource::<PhysState>().world.steps() > steps);
    assert!(f.moves() > 0, "the real player character-controller path ran");
    assert!(f.feet()[1] - before[1] > 40.0, "forward input moves the physical character: {:?} -> {:?}", before, f.feet());
    assert!(f.app.world().resource::<PhysState>().player.as_ref().unwrap().grounded);
}

#[test]
fn movement_camera_ecs_preserves_then_releases_a_cinematic_fade() {
    let mut f = Fixture::new();
    f.forward();
    f.ask(CINEMATIC);
    f.frame(0.0);
    assert!(f.held(), "a cinematic holds on its zero-weight first frame");
    let held = f.feet();
    for _ in 0..6 { f.frame(0.05); }
    assert_eq!(f.feet(), held);
    assert_eq!(f.moves(), 0, "the actual held step path does not move the player controller");
    f.ask(CINEMATIC);
    f.frame(0.0);
    assert!(f.held(), "a repeated cinematic request keeps the hold");
    f.ask(MOVEMENT);
    f.frame(0.0);
    assert!(f.held(), "the visible outgoing cinematic still holds at replacement");
    f.frame(0.05);
    assert!(f.held(), "the outgoing cinematic has positive blend weight");
    assert_eq!(f.feet(), held);
    for _ in 0..6 { f.frame(0.05); }
    assert!(!f.held(), "the actual release path clears PlayerSeat.hold once the cinematic fades away");
    assert!(f.moves() > 0);
    assert!(f.feet()[1] > held[1] + 5.0, "input resumes physical movement after release");
    f.app.world_mut().resource_mut::<crate::residents::Residents>().own_game().unwrap().gaps.idle_stops.push(FormId::PLAYER_REF);
    for _ in 0..6 { f.frame(0.05); }
    assert!(f.app.world().resource::<PlayerIdles>().first.is_empty());
    assert!(!f.held());
}

#[test]
fn activation_camera_ecs_keeps_a_cinematic_section_free_to_step() {
    let mut f = Fixture::new();
    f.forward();
    // Enter the real request/start consumer at the public queue boundary. The
    // water interaction and activation-tree picker are deliberately not claimed.
    let asked = f.app.world_mut().resource_mut::<crate::residents::Residents>().own_game().unwrap()
        .ask_idle(FormId::PLAYER_REF, FormId(CINEMATIC), fnv_game::idles::IdleSource::Activate);
    assert!(asked);
    f.frame(0.0);
    {
        let idles = f.app.world().resource::<PlayerIdles>();
        let playing = idles.first.playing().expect("the Camera1st request reached the first-person slot");
        assert_eq!(playing.idle, CINEMATIC);
        assert_eq!(playing.source, fnv_game::idles::IdleSource::Activate);
        assert!(!playing.holds, "Activate must not mark a cinematic-section camera slot as holding");
        assert!(idles.held.is_none(), "Activate must not capture a player position");
    }
    assert!(!f.held(), "the actual Activation idle/body systems must not acquire a seat hold");
    let before = f.feet();
    let steps = f.app.world().resource::<PhysState>().world.steps();
    for _ in 0..20 { f.frame(0.05); }
    let idles = f.app.world().resource::<PlayerIdles>();
    let playing = idles.first.playing().expect("the owned looping Activation clip remains active");
    assert_eq!(playing.source, fnv_game::idles::IdleSource::Activate);
    assert!(!playing.holds);
    assert!(idles.held.is_none());
    assert!(!f.held());
    assert!(f.app.world().resource::<PhysState>().world.steps() > steps);
    assert!(f.moves() > 0, "the real player character-controller path ran");
    assert!(f.feet()[1] - before[1] > 40.0, "Activation leaves physical movement free: {:?} -> {:?}", before, f.feet());
    assert!(f.app.world().resource::<PhysState>().player.as_ref().unwrap().grounded);
}
