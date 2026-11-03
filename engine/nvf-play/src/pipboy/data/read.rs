//! What the DATA page reads from the plugins: notes (`NOTE`), map markers (references
//! with `XMRK`), the worldspace's map (`WRLD ICON`, `MNAM`), regions' map names
//! (`REGN RDMP`) and the names of places (docs/spec-notes/pipboy.md §5.3).
//!
//! Subrecords are read from their raw bytes as xEdit lays them out [documented: xEdit
//! wbDefinitionsFNV.pas, as `schemas/fnv` records them] unless marked otherwise.

use super::model::{game_text, WorldMap, CELL};
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::{FormId, RefTable};
use fnv_world::fields::{global, num, winner, Fields};
use fnv_world::WorldIndex;

/// The `STAT` every map marker reference places [observed: `NAME 0x00000010` on the
/// Goodsprings, Goodsprings Cemetery and Vault 22 markers].
pub const MAP_MARKER: u32 = 0x10;

fn fields<'a>(lo: &LoadOrder, schema: &'a Schema, form: u32) -> Option<Fields<'a>> {
    let (pi, r) = winner(lo, form)?;
    Fields::new(r, pi, schema).ok()
}

fn text(f: &Fields, sig: &[u8; 4]) -> Option<String> {
    f.raw(sig).map(|s| game_text(&s.data)).filter(|s| !s.trim().is_empty())
}

fn byte(f: &Fields, sig: &[u8; 4]) -> Option<u8> {
    f.raw(sig).and_then(|s| s.data.first().copied())
}

/// A note: its name, its type (`DATA`: 0 sound, 1 text, 2 image, 3 voice), what it shows
/// and the quest it belongs to (`ONAM`).
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub form: u32,
    pub name: String,
    pub kind: u8,
    /// A text note's text: `TNAM`, a NUL-ended string [observed: VMQ01CourierNote's
    /// `TNAM` is the text the owner's screenshot shows, "INSTRUCTIONS", a blank line,
    /// "Deliver the package …"; the schema leaves `TNAM` of `NOTE` unread].
    pub text: Option<String>,
    /// An image note's picture (`XNAM`).
    pub image: Option<String>,
    pub quest: Option<u32>,
}

pub fn note(lo: &LoadOrder, schema: &Schema, form: u32) -> Option<Note> {
    let f = fields(lo, schema, form)?;
    if &f.sig != b"NOTE" {
        return None;
    }
    let kind = byte(&f, b"DATA").unwrap_or(1);
    let quest = f.raw(b"ONAM").and_then(|s| s.data.get(..4)).and_then(|b| global(lo, f.plugin, u32::from_le_bytes([b[0], b[1], b[2], b[3]])));
    Some(Note {
        form,
        name: text(&f, b"FULL").unwrap_or_else(|| format!("{}", FormId(form))),
        kind,
        text: if kind == 1 { f.raw(b"TNAM").map(|s| game_text(&s.data)) } else { None },
        image: if kind == 2 { text(&f, b"XNAM") } else { None },
        quest,
    })
}

/// A map marker: the reference, its worldspace and place, name, type (`TNAM`), flags
/// (`FNAM`: 1 visible, 2 can travel to), the radius the player finds it in (`XRDS`) and
/// its reputation (`WMI1`, a REPU: "the reputation that is shown when the player hovers
/// over the icon on the world map" [documented: GECK wiki Reference, Map Marker]).
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub form: u32,
    pub world: u32,
    pub pos: [f32; 3],
    pub name: String,
    pub kind: u8,
    pub flags: u8,
    pub radius: f32,
    /// The reputation (REPU FormID) and its name (`FULL`).
    pub reputation: Option<(u32, String)>,
}

/// Every placed map marker that has a name, by worldspace.
pub fn markers(lo: &LoadOrder, schema: &Schema, refs: &RefTable) -> Vec<Marker> {
    let mut out = Vec::new();
    for r in refs.refs.values().filter(|r| r.base == FormId(MAP_MARKER)) {
        let Some(world) = r.cell.and_then(|c| refs.cell_world.get(&c)) else { continue };
        let Some(f) = fields(lo, schema, r.id.0) else { continue };
        if f.raw(b"XMRK").is_none() {
            continue;
        }
        let Some(name) = text(&f, b"FULL") else { continue };
        let radius = f.raw(b"XRDS").and_then(|s| s.data.get(..4)).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let reputation = f
            .raw(b"WMI1")
            .and_then(|s| s.data.get(..4))
            .and_then(|b| global(lo, f.plugin, u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
            .filter(|&rep| rep != 0)
            .and_then(|rep| Some((rep, fields(lo, schema, rep).and_then(|rf| text(&rf, b"FULL"))?)));
        out.push(Marker { form: r.id.0, world: world.0, pos: r.pos, name, kind: byte(&f, b"TNAM").unwrap_or(0), flags: byte(&f, b"FNAM").unwrap_or(0), radius, reputation });
    }
    out.sort_by_key(|m| m.form);
    out
}

/// The map of worldspace `world`, or of the worldspace it takes its map from: the first
/// parent with a map picture [hypothesis].
pub fn world_map(lo: &LoadOrder, schema: &Schema, index: &WorldIndex, world: u32) -> Option<WorldMap> {
    let mut w = Some(world);
    for _ in 0..4 {
        let id = w?;
        if let Some(f) = fields(lo, schema, id) {
            let mnam = f.raw(b"MNAM").map(|s| s.data.clone()).filter(|d| d.len() >= 16);
            if let (Some(texture), Some(m)) = (text(&f, b"ICON"), mnam) {
                let i32_at = |o: usize| i32::from_le_bytes([m[o], m[o + 1], m[o + 2], m[o + 3]]);
                let i16_at = |o: usize| i16::from_le_bytes([m[o], m[o + 1]]) as i32;
                return Some(WorldMap { texture, usable: [i32_at(0) as f32, i32_at(4) as f32], nw: [i16_at(8), i16_at(10)], se: [i16_at(12), i16_at(14)] });
            }
        }
        w = index.worldspaces.get(&id).and_then(|ws| ws.parent);
    }
    None
}

/// A worldspace's `DATA` flags (bit 1: Can't Fast Travel [documented: xEdit]); 0 when
/// it has none.
pub fn world_flags(lo: &LoadOrder, schema: &Schema, world: u32) -> u8 {
    fields(lo, schema, world).filter(|f| &f.sig == b"WRLD").and_then(|f| byte(&f, b"DATA")).unwrap_or(0)
}

/// A region's map name (`RDMP`) and the priority of the map entry it belongs to (the
/// `RDAT` before it).
fn region_name(lo: &LoadOrder, schema: &Schema, region: u32) -> Option<(String, f64)> {
    let f = fields(lo, schema, region)?;
    let mut priority = 0.0;
    for s in &f.subs {
        match &s.sig {
            b"RDAT" => priority = f.decode(s).and_then(|v| num(&v, "priority")).unwrap_or(0.0),
            b"RDMP" => return Some((game_text(&s.data), priority)).filter(|(n, _)| !n.trim().is_empty()),
            _ => {}
        }
    }
    None
}

/// The name of a place: an interior's own name (`FULL`, else its Editor ID); outdoors the
/// map name of the regions the cell is in, the highest priority first, else the
/// worldspace's name [hypothesis: the owner's screenshots, taken in Goodsprings, show
/// "Goodsprings", the map name of `VMapGoodspringsRegion` (observed); an interior's own
/// name is NVForever's reading].
pub fn place_name(lo: &LoadOrder, schema: &Schema, index: &WorldIndex, world: u32, interior: Option<u32>, at: [f32; 3]) -> String {
    if let Some(c) = interior.and_then(|i| index.cells.get(&i)) {
        return c.name.clone().or_else(|| c.edid.clone()).unwrap_or_default();
    }
    let grid = ((at[0] / CELL).floor() as i32, (at[1] / CELL).floor() as i32);
    let cell = index.worldspaces.get(&world).and_then(|w| w.grid.get(&grid)).copied();
    let best = cell
        .into_iter()
        .flat_map(|c| fnv_world::audio::cell_regions(lo, schema, c))
        .filter_map(|r| region_name(lo, schema, r))
        .max_by(|a, b| a.1.total_cmp(&b.1));
    match best {
        Some((n, _)) => n,
        None => index.worldspaces.get(&world).and_then(|w| w.name.clone().or_else(|| w.edid.clone())).unwrap_or_default(),
    }
}

/// A door that leads elsewhere, near where the local map looks: where it stands and the
/// name of the place it leads to.
#[derive(Debug, Clone, PartialEq)]
pub struct Door {
    pub form: u32,
    pub pos: [f32; 3],
    pub name: String,
}

/// REFR header flag "Hidden From Local Map" (bit 6): "The reference will not be displayed
/// on the local map" [documented: GECK wiki Reference, Reference Data Flags; xEdit
/// wbDefinitionsFNV.pas REFR flags; schemas/fnv/world.json].
pub const HIDDEN_FROM_LOCAL_MAP: u32 = 0x40;

/// DOOR `FNAM` flag "Hidden" (bit 2) [documented: xEdit wbDefinitionsFNV.pas DOOR;
/// schemas/fnv/objects.json]. What it does is not documented; NVForever leaves such a
/// door off the local map [hypothesis: the three doors flagged in FalloutNV.esm are
/// secret ones, `CG04SecretDoor`, `NVLucky38RmLoWallNoShadowDoor` and
/// `SewerLidExit01NoMinUse`, observed; docs/owner-review.md].
pub const DOOR_HIDDEN: u8 = 0x04;

/// Whether a door is left off the local map: its reference is flagged Hidden From Local
/// Map, or its base door is flagged Hidden (`door_flags`, DOOR `FNAM`).
pub fn door_hidden(ref_flags: u32, door_flags: u8) -> bool {
    ref_flags & HIDDEN_FROM_LOCAL_MAP != 0 || door_flags & DOOR_HIDDEN != 0
}

/// The doors leading elsewhere among the references of `cells`, named by the place their
/// destination door stands in [observed: the owner's Local Map screenshot labels the door
/// icon "Goodsprings Gas Station", the name of the cell `GSGasStation`]. Doors left off
/// the local map ([`door_hidden`]) are not listed; whether a door is enabled now is the
/// caller's to ask (it changes while the list is kept).
pub fn doors(lo: &LoadOrder, schema: &Schema, index: &WorldIndex, refs: &RefTable, cells: &[u32]) -> Vec<Door> {
    let mut out = Vec::new();
    for c in cells {
        for id in refs.by_cell.get(&FormId(*c)).into_iter().flatten() {
            let Some(r) = refs.get(*id) else { continue };
            let Some(t) = r.teleport else { continue };
            let door_flags = fields(lo, schema, r.base.0).filter(|f| &f.sig == b"DOOR").and_then(|f| byte(&f, b"FNAM")).unwrap_or(0);
            if door_hidden(r.flags, door_flags) {
                continue;
            }
            let Some(dest) = refs.get(t.door) else { continue };
            let Some(dest_cell) = dest.cell else { continue };
            let name = match refs.cell_world.get(&dest_cell) {
                Some(w) => place_name(lo, schema, index, w.0, None, dest.pos),
                None => place_name(lo, schema, index, 0, Some(dest_cell.0), dest.pos),
            };
            if !name.trim().is_empty() {
                out.push(Door { form: id.0, pos: r.pos, name });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doors_hidden_from_the_local_map() {
        assert!(!door_hidden(0, 0));
        // Hidden From Local Map on the reference (the Dead Money doors 0x0100BFB1 …).
        assert!(door_hidden(HIDDEN_FROM_LOCAL_MAP | 0x400, 0));
        // Hidden on the base door (`CG04SecretDoor`, FNAM 28: Hidden, Minimal Use,
        // Sliding Door).
        assert!(door_hidden(0, 28));
        // Automatic Door and Minimal Use alone keep it on the map.
        assert!(!door_hidden(0x400, 0x02 | 0x08));
    }
}
