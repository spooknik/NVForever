//! What the records say about each dialogue line's sound: the names its path is built
//! from (docs/spec-notes/voice.md §2) and who speaks with which voice type.

use crate::path::{voice_path, voice_stem, SOUND_EXTENSIONS};
use fnv_esm::raw::{group_type, record_flags, visit_records, Subrecord};
use fnv_esm::LoadOrder;
use std::collections::{BTreeMap, HashMap, HashSet};

/// One response of a line (INFO `TRDT` with the `NAM1` after it).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Response {
    /// `TRDT` response number: the last part of the file name [documented: xEdit
    /// "Response number", schema `gameplay.json`; observed as the file name's last
    /// number, voice.md §2].
    pub number: u8,
    /// `TRDT` sound: a SOUN the response names [documented: xEdit "Sound"; what the game
    /// does with it is not established, voice.md §6].
    pub sound: Option<u32>,
    /// Characters of the response text.
    pub chars: usize,
    /// Which consonant letter groups the text has (`lip::letter_groups`; used to check
    /// the `.lip` channels against the text, docs/spec-notes/lipsync.md §3.1).
    pub letters: u16,
}

/// A dialogue line (INFO), winning definition.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    /// `QSTI`.
    pub quest: Option<u32>,
    /// The DIAL whose "topic children" group holds the line.
    pub topic: Option<u32>,
    /// `ANAM`: the base form the line is restricted to.
    pub speaker: Option<u32>,
    /// In record order (the order of `fnv_game::data::InfoDef::response_chars`).
    pub responses: Vec<Response>,
}

/// The dialogue lines of a load order with what their sound paths are built from.
/// FormIDs are load-order ("global") ones.
#[derive(Debug, Clone, Default)]
pub struct VoiceIndex {
    /// Plugin file names in load order.
    pub plugins: Vec<String>,
    pub lines: HashMap<u32, Line>,
    /// Editor IDs of quests (QUST) and topics (DIAL).
    pub edids: HashMap<u32, String>,
    /// Voice types (VTYP) by FormID: the Editor ID is the folder name [observed: every
    /// voice folder of the owner's archives is a VTYP Editor ID, voice.md §2.1].
    pub voice_types: BTreeMap<u32, String>,
    /// Base form → its own voice type: NPC_ and CREA `VTCK`, TACT `VNAM` [documented:
    /// xEdit "Voice" / "Voice Type", schemas `gameplay.json`, `placed.json`].
    pub speakers: HashMap<u32, u32>,
    /// NPC_ and CREA `TPLT`, for those whose `ACBS` template flags have Traits set: the
    /// voice type is one of the traits the template then supplies [documented: xEdit
    /// hides race, sex, voice, height and weight when the flag is set,
    /// docs/spec-notes/actors.md §1].
    pub trait_templates: HashMap<u32, u32>,
    /// Every NPC_, CREA and TACT seen.
    pub speaker_forms: HashSet<u32>,
}

fn formid(lo: &LoadOrder, pi: usize, s: &Subrecord) -> Option<u32> {
    let local = u32::from_le_bytes(s.data.get(0..4)?.try_into().ok()?);
    if local == 0 {
        return None;
    }
    lo.to_global(pi, local)
}

fn zstring_len(data: &[u8]) -> usize {
    data.iter().position(|&b| b == 0).unwrap_or(data.len())
}

impl VoiceIndex {
    /// Read a load order: later plugins replace earlier definitions of the same record.
    pub fn build(lo: &LoadOrder) -> VoiceIndex {
        let mut ix = VoiceIndex { plugins: lo.plugins.iter().map(|p| p.name.clone()).collect(), ..VoiceIndex::default() };
        for (pi, p) in lo.plugins.iter().enumerate() {
            visit_records(p.nodes.get(1..).unwrap_or_default(), &mut |r, groups| {
                let sig = &r.header.sig;
                if !matches!(sig, b"INFO" | b"QUST" | b"DIAL" | b"VTYP" | b"NPC_" | b"CREA" | b"TACT") {
                    return;
                }
                let Some(id) = lo.to_global(pi, r.header.form_id) else { return };
                if r.header.flags & record_flags::DELETED != 0 {
                    ix.lines.remove(&id);
                    ix.speakers.remove(&id);
                    ix.trait_templates.remove(&id);
                    return;
                }
                let Ok(subs) = r.subrecords() else { return };
                let edid = || {
                    let s = subs.iter().find(|s| &s.sig == b"EDID")?;
                    Some(fnv_util::text::decode(&s.data[..zstring_len(&s.data)]))
                };
                match sig {
                    b"INFO" => {
                        let topic = groups
                            .iter()
                            .rev()
                            .find(|g| g.group_type == group_type::TOPIC_CHILDREN)
                            .and_then(|g| lo.to_global(pi, g.label_u32()));
                        let mut line = Line { topic, ..Line::default() };
                        for s in &subs {
                            match &s.sig {
                                b"QSTI" => line.quest = formid(lo, pi, s),
                                b"ANAM" => line.speaker = formid(lo, pi, s),
                                // Response number at byte 12, sound at 16 [documented:
                                // xEdit INFO TRDT, schema `gameplay.json`].
                                b"TRDT" if s.data.len() > 12 => {
                                    let sound = s.data.get(16..20).and_then(|b| {
                                        let local = u32::from_le_bytes(b.try_into().ok()?);
                                        (local != 0).then(|| lo.to_global(pi, local)).flatten()
                                    });
                                    line.responses.push(Response { number: s.data[12], sound, chars: 0, letters: 0 });
                                }
                                b"NAM1" => {
                                    if let Some(r) = line.responses.last_mut() {
                                        r.chars = zstring_len(&s.data);
                                        r.letters = crate::lip::letter_groups(&fnv_util::text::decode(&s.data[..r.chars]));
                                    }
                                }
                                _ => {}
                            }
                        }
                        ix.lines.insert(id, line);
                    }
                    b"QUST" | b"DIAL" => {
                        if let Some(e) = edid() {
                            ix.edids.insert(id, e);
                        }
                    }
                    b"VTYP" => {
                        if let Some(e) = edid() {
                            ix.voice_types.insert(id, e);
                        }
                    }
                    _ => {
                        let voice_sig: &[u8; 4] = if sig == b"TACT" { b"VNAM" } else { b"VTCK" };
                        match subs.iter().find(|s| &s.sig == voice_sig).and_then(|s| formid(lo, pi, s)) {
                            Some(v) => ix.speakers.insert(id, v),
                            None => ix.speakers.remove(&id),
                        };
                        ix.speaker_forms.insert(id);
                        // ACBS template flags: u16 at byte 22, bit 0 Traits [documented:
                        // xEdit NPC_/CREA ACBS, schema `gameplay.json`].
                        let traits = sig != b"TACT"
                            && subs.iter().find(|s| &s.sig == b"ACBS").and_then(|s| s.data.get(22)).is_some_and(|b| b & 1 != 0);
                        match subs.iter().find(|s| &s.sig == b"TPLT" && traits).and_then(|s| formid(lo, pi, s)) {
                            Some(t) => ix.trait_templates.insert(id, t),
                            None => ix.trait_templates.remove(&id),
                        };
                    }
                }
            });
        }
        ix
    }

    /// File name of the plugin that owns a FormID (its load-order byte).
    pub fn plugin_of(&self, form: u32) -> Option<&str> {
        self.plugins.get((form >> 24) as usize).map(String::as_str)
    }

    /// The voice type an actor base or talking activator speaks with: its template's
    /// when the template supplies its traits and is an actor record, else its own. A
    /// template that is a levelled list picks its actor at run time; the record's own
    /// voice type stands in then [hypothesis, voice.md §7]. A game resolves the list first
    /// and passes the picked entry (`fnv_game::Game::voice_base`), so this fallback is for
    /// tools that see only a base form.
    pub fn voice_of(&self, base: u32) -> Option<u32> {
        let mut b = base;
        for _ in 0..8 {
            match self.trait_templates.get(&b) {
                Some(t) if self.speaker_forms.contains(t) => b = *t,
                _ => break,
            }
        }
        self.speakers.get(&b).copied()
    }

    /// File name (no extension) of response `response` (index in record order) of a line.
    /// `None` when the line, the response, or the Editor ID of its quest or topic is
    /// missing.
    pub fn stem(&self, info: u32, response: usize) -> Option<String> {
        let line = self.lines.get(&info)?;
        let r = line.responses.get(response)?;
        Some(voice_stem(self.edids.get(&line.quest?)?, self.edids.get(&line.topic?)?, info, r.number))
    }

    /// The paths a response's sound can have for a voice type, one per extension of
    /// [`SOUND_EXTENSIONS`], in the plugin folder of the line's FormID [observed: the 38
    /// sounds of base-game lines that a DLC plugin overrides are in the base game's
    /// folder; hypothesis: the game never looks in the overriding plugin's, voice.md §3].
    pub fn paths(&self, info: u32, response: usize, voice_type: u32) -> Vec<String> {
        let (Some(line), Some(plugin), Some(voice)) = (self.lines.get(&info), self.plugin_of(info), self.voice_types.get(&voice_type)) else {
            return Vec::new();
        };
        let (Some(r), Some(quest), Some(topic)) =
            (line.responses.get(response), line.quest.and_then(|q| self.edids.get(&q)), line.topic.and_then(|t| self.edids.get(&t)))
        else {
            return Vec::new();
        };
        SOUND_EXTENSIONS.iter().map(|ext| voice_path(plugin, voice, quest, topic, info, r.number, ext)).collect()
    }
}
