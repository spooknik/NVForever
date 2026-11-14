//! The menu engine's tile tree (docs/design/ui-tile-tree.md): the game's menu files
//! (`menus\*.xml`, read from the owner's archives at run time, never shipped) and the
//! XML mods inject, read into a tree of tiles, each with traits, found by path
//! (`HUDMainMenu/HitPoints/width`), and worked out by the traits' expressions. The UI
//! script functions (GetUIFloat, SetUIFloat, SetUIString, ModUIFloat, InjectUIXML,
//! InjectUIComponent …) act on it through [`Ui`]; the engine draws menus from it.
//!
//! - [`xml`]: a forgiving reader for the files' XML (docs/spec-notes/menu-xml.md §1).
//! - [`value`]: a trait's value.
//! - [`expr`]: the operators that work a trait out.
//! - [`tree`]: tiles, traits, loading with includes and templates, paths, updates.
//! - [`input`]: the pointer and keys on tiles: hit-tests, `mouseover`, `clicked`, the
//!   `_PCButton_*` keys and the controller's `x…` traits (task T8).
//! - [`strings`]: the game strings table for `&-sName;`.
//!
//! No Bevy here: the tree is plain data, tested without the game's files.

pub mod expr;
pub mod index;
pub mod input;
pub mod strings;
pub mod tree;
pub mod uio;
pub mod value;
pub mod xml;

pub use tree::{Count, MemSource, Origin, Source, Tile, TileId, Trait, Ui};
pub use index::MenuIndex;
pub use strings::GameStrings;
pub use value::Value;

#[cfg(test)]
mod tests;
