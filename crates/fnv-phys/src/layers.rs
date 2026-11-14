//! Havok collision layer numbers and names, as the REFR XTRI field enumerates them
//! [documented: xEdit wbDefinitionsFNV.pas REFR XTRI, via `schemas/fnv/world.json`].
//! Collision objects of meshes carry the same numbering [hypothesis until
//! docs/spec-notes/collision.md confirms it]. Which layers collide with which (the
//! original's layer matrix) is not known; [`character_ignores`] is a placeholder.

/// A set of collision layers (0–255).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct LayerSet([u64; 4]);

impl LayerSet {
    pub fn insert(&mut self, layer: u8) {
        self.0[(layer / 64) as usize] |= 1 << (layer % 64);
    }

    pub fn contains(&self, layer: u8) -> bool {
        self.0[(layer / 64) as usize] & (1 << (layer % 64)) != 0
    }

    pub fn from_layers(layers: &[u8]) -> LayerSet {
        let mut s = LayerSet::default();
        for &l in layers {
            s.insert(l);
        }
        s
    }

    /// Every layer in either set.
    pub fn union(mut self, other: LayerSet) -> LayerSet {
        for (a, b) in self.0.iter_mut().zip(other.0) {
            *a |= b;
        }
        self
    }

    pub fn layers(&self) -> Vec<u8> {
        (0..=255u8).filter(|&l| self.contains(l)).collect()
    }
}

pub const UNIDENTIFIED: u8 = 0;
pub const STATIC: u8 = 1;
pub const ANIM_STATIC: u8 = 2;
pub const TRANSPARENT: u8 = 3;
pub const CLUTTER: u8 = 4;
pub const WEAPON: u8 = 5;
pub const PROJECTILE: u8 = 6;
pub const SPELL: u8 = 7;
pub const BIPED: u8 = 8;
pub const TREES: u8 = 9;
pub const PROPS: u8 = 10;
pub const WATER: u8 = 11;
pub const TRIGGER: u8 = 12;
pub const TERRAIN: u8 = 13;
pub const TRAP: u8 = 14;
pub const NON_COLLIDABLE: u8 = 15;
pub const CLOUD_TRAP: u8 = 16;
pub const GROUND: u8 = 17;
pub const PORTAL: u8 = 18;
pub const DEBRIS_SMALL: u8 = 19;
pub const DEBRIS_LARGE: u8 = 20;
pub const ACOUSTIC_SPACE: u8 = 21;
pub const ACTOR_ZONE: u8 = 22;
pub const PROJECTILE_ZONE: u8 = 23;
pub const GAS_TRAP: u8 = 24;
pub const SHELL_CASING: u8 = 25;
pub const TRANSPARENT_SMALL: u8 = 26;
pub const INVISIBLE_WALL: u8 = 27;
pub const TRANSPARENT_SMALL_ANIM: u8 = 28;
pub const DEAD_BIP: u8 = 29;
pub const CHAR_CONTROLLER: u8 = 30;
pub const AVOID_BOX: u8 = 31;
pub const COLLISION_BOX: u8 = 32;
pub const CAMERA_SPHERE: u8 = 33;
pub const DOOR_DETECTION: u8 = 34;
pub const CAMERA_PICK: u8 = 35;
pub const ITEM_PICK: u8 = 36;
pub const LINE_OF_SIGHT: u8 = 37;
pub const PATH_PICK: u8 = 38;
pub const CUSTOM_PICK_1: u8 = 39;
pub const CUSTOM_PICK_2: u8 = 40;
pub const SPELL_EXPLOSION: u8 = 41;
pub const DROPPING_PICK: u8 = 42;

const NAMES: [&str; 43] = [
    "Unidentified",
    "Static",
    "AnimStatic",
    "Transparent",
    "Clutter",
    "Weapon",
    "Projectile",
    "Spell",
    "Biped",
    "Trees",
    "Props",
    "Water",
    "Trigger",
    "Terrain",
    "Trap",
    "Non Collidable",
    "Cloud Trap",
    "Ground",
    "Portal",
    "Debris Small",
    "Debris Large",
    "Acoustic Space",
    "Actor Zone",
    "Projectile Zone",
    "Gas Trap",
    "Shell Casing",
    "Transparent Small",
    "Invisible Wall",
    "Transparent Small Anim",
    "Dead Bip",
    "Char Controller",
    "Avoid Box",
    "Collision Box",
    "Camera Sphere",
    "Door Detection",
    "Camera Pick",
    "Item Pick",
    "Line Of Sight",
    "Path Pick",
    "Custom Pick 1",
    "Custom Pick 2",
    "Spell Explosion",
    "Dropping Pick",
];

/// The documented name of a layer number.
pub fn name(layer: u32) -> Option<&'static str> {
    NAMES.get(layer as usize).copied()
}

/// The layers of actor hit volumes (`crate::actors`): BIPED alive, DEAD_BIP dead. Rays
/// that look for floors, room or the camera's place skip them.
pub fn actor_volumes() -> LayerSet {
    LayerSet::from_layers(&[BIPED, DEAD_BIP])
}

/// Layers the character controller passes through by default: volumes that are not
/// physical obstacles by their names (triggers, zones, picks, water, the non-collidable
/// layer, small debris) [hypothesis: the original layer matrix is unknown, physics.md §4].
pub fn character_ignores() -> LayerSet {
    LayerSet::from_layers(&[
        WATER,
        TRIGGER,
        NON_COLLIDABLE,
        PORTAL,
        ACOUSTIC_SPACE,
        ACTOR_ZONE,
        PROJECTILE_ZONE,
        CLOUD_TRAP,
        GAS_TRAP,
        AVOID_BOX,
        CAMERA_SPHERE,
        DOOR_DETECTION,
        CAMERA_PICK,
        ITEM_PICK,
        LINE_OF_SIGHT,
        PATH_PICK,
        CUSTOM_PICK_1,
        CUSTOM_PICK_2,
        SPELL_EXPLOSION,
        DROPPING_PICK,
        DEBRIS_SMALL,
        SHELL_CASING,
    ])
}

/// Layers a melee swing passes: what the character passes ([`character_ignores`]) and the
/// invisible walls, which stop actors but not bullets, lasers, thrown weapons or blows
/// (the owner, issue #1545) [design]. Not the Transparent layer: the original's exception
/// for it is for projectiles only (see [`shot_ignores`]), so a blow stops on it.
pub fn melee_ignores() -> LayerSet {
    let mut s = character_ignores();
    s.insert(INVISIBLE_WALL);
    s
}

/// Layers a shot passes: what a melee swing passes ([`melee_ignores`]) and the plain
/// Transparent layer, which "nothing can pass through except projectiles" [documented:
/// GECK wiki "Collision layer", TRANSPARENT; issue #2340]. The Transparent Small layers
/// are not here: only a projectile flagged "Pass Through Small Transparent" passes them,
/// and fnv-combat's trace decides that per projectile [documented: same page,
/// TRANSPARENT SMALL].
pub fn shot_ignores() -> LayerSet {
    let mut s = melee_ignores();
    s.insert(TRANSPARENT);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shots_pass_invisible_walls_that_stop_characters() {
        assert!(!character_ignores().contains(INVISIBLE_WALL));
        assert!(shot_ignores().contains(INVISIBLE_WALL));
        assert!(character_ignores().layers().iter().all(|&l| shot_ignores().contains(l)));
        assert!(!shot_ignores().contains(STATIC) && !shot_ignores().contains(UNIDENTIFIED));
    }

    #[test]
    fn shots_pass_transparent_but_not_transparent_small() {
        // Transparent (3) stops actors and lets every projectile through (#2340).
        assert!(shot_ignores().contains(TRANSPARENT) && !character_ignores().contains(TRANSPARENT));
        // The small-transparent layers stay in the ray: the projectile's flag decides.
        assert!(!shot_ignores().contains(TRANSPARENT_SMALL) && !shot_ignores().contains(TRANSPARENT_SMALL_ANIM));
    }

    #[test]
    fn melee_stops_on_transparent_that_shots_pass() {
        // A blow passes what it passed before #2340 (the character's layers and invisible
        // walls) and nothing more: the Transparent layer stops it.
        assert!(!melee_ignores().contains(TRANSPARENT) && shot_ignores().contains(TRANSPARENT));
        assert!(melee_ignores().contains(INVISIBLE_WALL));
        assert!(character_ignores().layers().iter().all(|&l| melee_ignores().contains(l)));
        assert_eq!(shot_ignores().layers().len(), melee_ignores().layers().len() + 1);
    }

    #[test]
    fn names_match_numbers() {
        assert_eq!(name(INVISIBLE_WALL as u32), Some("Invisible Wall"));
        assert_eq!(name(DROPPING_PICK as u32), Some("Dropping Pick"));
        assert_eq!(name(TERRAIN as u32), Some("Terrain"));
        assert_eq!(name(43), None);
    }

    #[test]
    fn layer_sets() {
        let s = LayerSet::from_layers(&[0, 63, 64, 255]);
        assert_eq!(s.layers(), vec![0, 63, 64, 255]);
        assert!(!s.contains(1) && !s.contains(128));
    }

    #[test]
    fn names_agree_with_the_schema() {
        let text = include_str!("../../../schemas/fnv/world.json");
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        let values = &v["records"]["REFR"]["subrecords"]["XTRI"]["type"]["values"];
        for (i, n) in NAMES.iter().enumerate() {
            assert_eq!(values[i.to_string()].as_str(), Some(*n), "layer {i}");
        }
    }
}
