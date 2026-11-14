//! Commands JIP LN 57.30 registers that have no wiki page, called by the Blue Moon and
//! High and Dry mod lists' mods (#2275 follow-up): lights on a node, the equipment lock,
//! teammate ammo, position strings, the centre of the bounds, ListToArray, KillActorAlt,
//! and the Companions Command & Control mod's own functions. Their argument shapes come
//! from those mods' script source, what they do from probes in the original game (sets
//! cmd1-cmd3 of tools/capture/jip_probes.py); in our words in
//! docs/spec-notes/ext-unlisted-commands.md, which every comment here cites.

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

/// What these commands keep for the session.
#[derive(Debug, Default)]
pub struct UnlistedState {
    /// LockEquipment 1's actors (a script's EquipItem and UnequipItem still work, §2.2).
    pub locked_equipment: BTreeSet<FormId>,
    /// SetTeammateUsingAmmo 0's actors (every actor reads 1 until set).
    pub no_ammo: BTreeSet<FormId>,
    /// ToggleDetectionFix, ToggleIgnoreLockedDoors and DisableScriptedActivate's
    /// switches, by reference (kept for the session; no effect is built, §4).
    pub detection_fix: BTreeSet<FormId>,
    pub ignore_locked_doors: BTreeSet<FormId>,
    pub no_scripted_activate: BTreeSet<FormId>,
    /// AttachLight's lights, by reference and lower-case node: the light and the node
    /// the mirror holds it as.
    pub lights: BTreeMap<(FormId, String), (FormId, String)>,
}

/// GetConditionDamagePenalty's answer: the same at every difficulty, with nothing that
/// sets it [observed: cmd1.condpenalty, cmd3.condpenalty.0-4, §3.6].
pub const CONDITION_DAMAGE_PENALTY: f64 = 0.67;

/// The name of the node AttachLight adds for a light [observed: cmd2.light.children, the
/// child of "Bip01 Head" after attaching LIGH 00004477; §2.1].
pub fn light_node_name(light: FormId) -> String {
    format!("PtLight {:X}", light.0)
}

fn num(args: &[XVal], k: usize) -> Option<f64> {
    match args.get(k)? {
        XVal::Num(n) if n.is_finite() => Some(*n),
        _ => None,
    }
}

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k)? {
        XVal::Str(s) => Some(s.clone()),
        _ => None,
    }
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

/// RefToPosStr's text: the interior cell's FormID, else the worldspace's, in hex without
/// leading zeros, then X and Y cut to whole units toward zero and Z's whole part plus 1
/// [observed: cmd4.posstr.player, 500.145 -5789.315 7552.582 read "109038 500 -5789
/// 7553"; posstr.doc 619.839 -5883.541 7552.355 read "619 -5883 7553"; outdoors
/// "DA726 37000 -117159 6058" for 6057.049; cmd3.crate.bare.pos 5001 for a crate put at
/// z 5000]; empty with no cell [observed: cmd3.crate.bare.pos in the gas station's save]
/// (§3.2).
pub fn pos_str(g: &mut Game, r: FormId) -> String {
    let Some(cell) = g.cell_of(r) else { return String::new() };
    let place = if crate::nvse_jg5::interior(g, cell) { Some(cell) } else { g.world.placed.world_of(cell) };
    let (Some(place), Some((p, _))) = (place, g.ref_pos_rot(r)) else { return String::new() };
    format!("{:X} {} {} {}", place.0, p[0] as i32, p[1] as i32, p[2] as i32 + 1)
}

/// MoveToPosStr's text read back: a cell or worldspace FormID in hex (either case,
/// leading zeros or not) and three numbers, read as whole numbers cut toward zero; words
/// after them are ignored; another form or fewer numbers is refused [observed:
/// cmd4.movetopos.frac "10.7 -20.2 30.9" put the crate at 10 -20 30, movetopos.neg at
/// -10 -20 -30; movetopos.lower, zeros, extra 1; cmd3.crate.badform "14 0 0 0" and
/// crate.short "109038 1 2" 0] (§3.2).
fn parse_pos(g: &Game, s: &str) -> Option<(FormId, [f32; 3])> {
    let mut it = s.split_whitespace();
    let place = FormId(u32::from_str_radix(it.next()?, 16).ok()?);
    let mut pos = [0.0f32; 3];
    for v in &mut pos {
        *v = it.next()?.parse::<f64>().ok()?.trunc() as f32;
    }
    matches!(g.data.sig(place), Some(s) if &s == b"CELL" || &s == b"WRLD").then_some((place, pos))
}

/// The centre of a reference's bounds (its base's OBND, turned by its heading) in the
/// world [observed: cmd1.centerpos.box, a footlocker with OBND z 0-23 reads 11.5 above
/// its position, x and y 0; hypothesis: the heading turns the x/y offset] (§3.3).
fn center_pos(g: &mut Game, r: FormId) -> Option<[f32; 3]> {
    let (p, rot) = g.ref_pos_rot(r)?;
    let b = crate::functions::base_of(g, r).and_then(|b| g.data.object_bounds.get(&b).copied());
    let Some(b) = b else { return Some(p) };
    let c = [0, 1, 2].map(|i| (f32::from(b[i]) + f32::from(b[i + 3])) / 2.0);
    let (s, co) = rot[2].sin_cos();
    Some([p[0] + c[0] * co + c[1] * s, p[1] - c[0] * s + c[1] * co, p[2] + c[2]])
}

/// CCCLocationName's text, in our own words (§5.3): indoors the cell's name, outdoors the
/// map name of the cell's regions, each with a word before it [observed: cmd1.locname.*
/// in the gas station's save, cmd1.locname.doc "Searchlight Airport" and
/// cmd3.crate.goodsprings.name "Goodsprings" in the airport's]; outdoors in a cell no map
/// region names, the nearest map marker of the worldspace [observed: cmd3.crate.bare.name
/// gave a compass direction and Legate's Camp; NVForever names the marker only]; with no
/// cell a phrase of its own [observed: cmd3.crate.*.name in the gas station's save].
fn location_text(g: &mut Game, r: FormId) -> String {
    const NOWHERE: &str = "somewhere unknown";
    let Some(cell) = g.cell_of(r) else { return NOWHERE.to_string() };
    if crate::nvse_jg5::interior(g, cell) {
        let name = crate::nvse_jg5::location_name(g, r);
        return if name.is_empty() { NOWHERE.to_string() } else { format!("inside {name}") };
    }
    if let Some(n) = crate::nvse_jg5::region_map_name(g, cell) {
        return format!("around {n}");
    }
    match nearest_marker(g, r, cell) {
        Some(n) => format!("near {n}"),
        None => NOWHERE.to_string(),
    }
}

/// The name of the map marker nearest the reference in its worldspace (a script's
/// SetMapMarkerName first, `crate::nvse_gaps`).
fn nearest_marker(g: &mut Game, r: FormId, cell: FormId) -> Option<String> {
    let world = g.world.placed.world_of(cell)?;
    let (p, _) = g.ref_pos_rot(r)?;
    let placed = g.world.placed;
    let best = placed
        .refs
        .iter()
        .filter(|(_, m)| m.map_marker.is_some() && m.cell.and_then(|c| placed.world_of(c)) == Some(world))
        .map(|(&id, m)| (id, (m.pos[0] - p[0]).hypot(m.pos[1] - p[1])))
        .filter(|(id, _)| g.gaps.marker_names.contains_key(id) || g.data.full_names.contains_key(id))
        .min_by(|a, b| a.1.total_cmp(&b.1))?
        .0;
    g.gaps.marker_names.get(&best).cloned().or_else(|| g.data.full_names.get(&best).cloned())
}

/// CCCGetReputation: the reputation of the first of the base's factions that counts
/// towards one [observed: cmd2.rep.doc and rep.pete, both of GoodspringsFaction, give
/// RepNVGoodsprings 00104C22; the player and a crate 0] (§5.4).
fn reputation_of(g: &Game, r: FormId) -> FormId {
    crate::functions::actor::base_factions(g, r).iter().find_map(|(f, _)| g.data.faction_reputations.get(f).copied()).unwrap_or(FormId(0))
}

/// CCCInFaction: in the faction by a rank a script gave the reference; with the second
/// argument non-zero, by the base's factions as well [observed: cmd1.infac.*, cmd2.infac.*:
/// Doc reads 0 for Goodsprings, which his NPC_ lists at rank 0, and 1 with the argument 1;
/// NCR given by SetFactionRank 0 or 3 reads 1 either way, and 0 once at -1] (§5.2).
fn in_faction(g: &Game, r: FormId, f: FormId, with_base: bool) -> bool {
    match g.extra.get(&r).and_then(|e| e.factions.get(&f)) {
        Some(&rank) => rank >= 0,
        None => with_base && crate::functions::actor::faction_rank(g, r, f) >= 0,
    }
}

impl Game<'_> {
    /// Whether LockEquipment 1 holds for the actor. A script's EquipItem and UnequipItem
    /// still work then [observed: cmd4.equip.locked.after, unequip.locked.after]; what it
    /// holds back is not known, so nothing else reads it yet (§2.2).
    pub fn equipment_locked(&self, actor: FormId) -> bool {
        self.other.unlisted.locked_equipment.contains(&actor)
    }

    /// GetTeammateUsingAmmo's answer. NPCs fire without ammunition in NVForever whatever
    /// it says (engine npc_combat, owner 2026-10-17), so nothing else reads it (§2.3).
    pub fn teammate_uses_ammo(&self, actor: FormId) -> bool {
        !self.other.unlisted.no_ammo.contains(&actor)
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- lights (§2.1) ----
        // `node light x y z`: 1 with a LIGH on a node the reference's drawn tree has, a
        // node of its own under that one at the offset; again on the same node also 1
        // [observed: cmd1.light.attach, attach2; no such node 0, a weapon form 0;
        // hypothesis: the newer light takes the place of the older]. RemoveLight takes
        // the node's light away and answers 0 either way [observed: cmd1.light.remove,
        // remove2; cmd2.light.children.after].
        "attachlight" => {
            let (Some(r), Some(node), Some(light)) = (subject, text(args, 0), form(args, 1)) else { return Some(Ok(zero)) };
            if g.data.sig(light) != Some(*b"LIGH") {
                return Some(Ok(zero));
            }
            let offset = [2, 3, 4].map(|k| num(args, k).unwrap_or(0.0) as f32);
            let key = (r, node.to_ascii_lowercase());
            if let Some((_, old)) = g.other.unlisted.lights.remove(&key) {
                g.remove_node_now(r, &old);
            }
            let lname = light_node_name(light);
            let ok = g.attach_light_now(r, &node, light, &lname, offset);
            if ok {
                g.other.unlisted.lights.insert(key, (light, lname));
            }
            yes(ok)
        }
        "removelight" => {
            if let (Some(r), Some(node)) = (subject, text(args, 0)) {
                if let Some((_, old)) = g.other.unlisted.lights.remove(&(r, node.to_ascii_lowercase())) {
                    g.remove_node_now(r, &old);
                }
            }
            zero
        }
        // ---- actors (§2.2-§2.6) ----
        // Answers 0 [observed: cmd1.lockequip.on, off]; kept per actor (§2.2).
        "lockequipment" => {
            if let Some(r) = subject {
                if num(args, 0).unwrap_or(0.0) != 0.0 {
                    g.other.unlisted.locked_equipment.insert(r);
                } else {
                    g.other.unlisted.locked_equipment.remove(&r);
                }
            }
            zero
        }
        // 1 until set; the setter answers 0 [observed: cmd1.teamammo.*, Doc and the player
        // 1, after 1 then 0 they read 1 then 0].
        "getteammateusingammo" => yes(subject.is_some_and(|r| g.teammate_uses_ammo(r))),
        "setteammateusingammo" => {
            if let Some(r) = subject {
                if num(args, 0).unwrap_or(0.0) != 0.0 {
                    g.other.unlisted.no_ammo.remove(&r);
                } else {
                    g.other.unlisted.no_ammo.insert(r);
                }
            }
            zero
        }
        // Switches kept, answers 0 [observed: cmd1.detfix.*, lockeddoors.*, scriptedact.*];
        // what each changes is not built (§4).
        "toggledetectionfix" | "toggleignorelockeddoors" | "disablescriptedactivate" => {
            if let Some(r) = subject {
                let set = match name {
                    "toggledetectionfix" => &mut g.other.unlisted.detection_fix,
                    "toggleignorelockeddoors" => &mut g.other.unlisted.ignore_locked_doors,
                    _ => &mut g.other.unlisted.no_scripted_activate,
                };
                if num(args, 0).unwrap_or(0.0) != 0.0 {
                    set.insert(r);
                } else {
                    set.remove(&r);
                }
            }
            zero
        }
        // Answers 0; Doc came no farther away in 4 s [observed: cmd1.moveaway,
        // cmd3.moveaway.*: 206 then 125 and 134 units as his package walked him]: no move
        // is built (§2.5).
        "moveawayfromplayer" | "refreshanimdata" => zero,
        // `[killer]`: dead, the killer recorded, answers 0 [observed: cmd1.kill.alt, kill.dead
        // 1, kill.killer the player] (§2.4).
        "killactoralt" => {
            if let Some(r) = subject {
                g.kill(r, form(args, 0));
            }
            zero
        }
        // The node of the last hit; "NULL" with none [observed: cmd1.hitnode.doc]. A hit's
        // node is its body part's (BPNN) in the creature's body-part data [hypothesis:
        // not probed with a hit; NPCs' parts are not looked up] (§2.6).
        "gethitnode" => {
            let hit = subject.and_then(|r| g.extra.get(&r).and_then(|e| e.last_hit).map(|h| (r, h.location)));
            let node = hit.and_then(|(r, loc)| {
                let base = crate::functions::base_of(g, r)?;
                let bptd = match crate::nvse::record_value(g, base, "PNAM", "") {
                    Some(XVal::Ref(b)) if g.data.sig(base) == Some(*b"CREA") => FormId(b),
                    _ => return None,
                };
                g.body_part_text(bptd, i64::from(loc), "BPNN").filter(|s| !s.is_empty())
            });
            XVal::Str(node.unwrap_or_else(|| "NULL".to_string()))
        }
        // ---- references (§3) ----
        "reftoposstr" => XVal::Str(subject.map(|r| pos_str(g, r)).unwrap_or_default()),
        // 1 once moved [observed: cmd2.movetopos.literal, the crate at 100 200 300 of the
        // cell given] (§3.2).
        "movetoposstr" => {
            let (Some(r), Some(s)) = (subject, text(args, 0)) else { return Some(Ok(zero)) };
            let Some((place, pos)) = parse_pos(g, &s) else { return Some(Ok(zero)) };
            let rot = g.ref_pos_rot(r).map_or([0.0; 3], |x| x.1);
            yes(g.place_ref(r, Some(place), pos, rot))
        }
        "getcenterpos" => {
            let axis = match args.first() {
                Some(XVal::Str(s)) => s.trim().to_ascii_lowercase(),
                Some(XVal::Num(n)) => match *n as i64 {
                    88 | 120 => "x".to_string(),
                    89 | 121 => "y".to_string(),
                    _ => "z".to_string(),
                },
                _ => "z".to_string(),
            };
            let i = match axis.as_str() {
                "x" => 0,
                "y" => 1,
                _ => 2,
            };
            XVal::Num(subject.and_then(|r| center_pos(g, r)).map_or(0.0, |c| f64::from(c[i])))
        }
        "getconditiondamagepenalty" => XVal::Num(CONDITION_DAMAGE_PENALTY),
        // `list`: its forms in list order as an array; no array for another form
        // [observed: cmd2.listtoarray 18 of 18, first and last as ListGetNthForm;
        // listtoarray.notlist ar_Size -1] (§3.5).
        "listtoarray" => match form(args, 0).filter(|&f| g.data.sig(f) == Some(*b"FLST")) {
            Some(f) => {
                let entries = crate::functions::list_members(g, f).into_iter().enumerate().map(|(i, m)| (XKey::Num(i as f64), XVal::Ref(m.0))).collect();
                XVal::Array(g.ext.new_array(ArrayKind::List, entries))
            }
            None => XVal::Array(0),
        },
        // ---- Companions Command & Control (§5) ----
        "cccinfaction" => {
            let (Some(r), Some(f)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            yes(in_faction(g, r, f, num(args, 1).unwrap_or(0.0) != 0.0))
        }
        // `target mode`: 4 the height between the two, any other mode the distance across
        // the ground [observed: cmd2.crates.modes, two crates 300 400 100 apart: modes 0-3
        // and 5-7 read 500, mode 4 104 (the crates had settled), the same from either end;
        // cmd2.doc.modes] (§5.1).
        "cccgetdistance" => {
            let (Some(r), Some(t)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let (Some((a, _)), Some((b, _))) = (g.ref_pos_rot(r), g.ref_pos_rot(t)) else { return Some(Ok(zero)) };
            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            XVal::Num(if num(args, 1).unwrap_or(0.0) as i64 == 4 { f64::from(d[2].abs()) } else { f64::from(d[0].hypot(d[1])) })
        }
        "ccclocationname" => XVal::Str(subject.map(|r| location_text(g, r)).unwrap_or_default()),
        "cccgetreputation" => XVal::Ref(subject.map_or(0, |r| reputation_of(g, r).0)),
        // Forms the CCC plugin keeps; none without it [observed: cmd1.savedform.0-9 and
        // cmdload, with the plugin not loaded] (§5.5).
        "cccsavedform" => XVal::Ref(0),
        // The CCC mod's HUD and orders: answer 0 [observed: cmd2.setfloat … runresult];
        // what they change needs the CCC plugin loaded in the original, which the probes
        // could not (§5.5).
        "cccsetfloat" | "cccsetstring" | "cccsettrait" | "cccsetequipped" | "cccsetnccs" | "cccsms" | "cccsetfollowstate" | "cccsayto"
        | "cccrunresultscripts" | "cccloadnccs" | "ccconload" => zero,
        _ => return None,
    };
    Some(Ok(v))
}
