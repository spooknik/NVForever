//! A reader of the original game's save files (`.fos`), for the extender lab only: its
//! probe sets ran from the owner's save, so `fnvtool ext queries --save` starts the run
//! from the state this reads (issue #1023). The game never loads a `.fos`.
//!
//! The layout is in docs/spec-notes/save-files.md; every part read here is tagged there.
//! Only what the lab needs is decoded: the header, the plugin list, the global variables,
//! the player's place, quests (flags, stages, objectives, variables), the player's
//! SPECIAL, tag skills, skill modifiers and inventory. Anything else is kept as raw bytes
//! per changed form.

use std::fmt;

/// A parse failure: where and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FosError(pub String);

impl fmt::Display for FosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FosError {}

fn err<T>(msg: impl Into<String>) -> Result<T, FosError> {
    Err(FosError(msg.into()))
}

/// The separator byte after nearly every field (save-files.md §2).
const BAR: u8 = 0x7C;
const MAGIC: &[u8] = b"FO3SAVEGAME";

/// Changed-form record types (save-files.md §6.1).
pub const TYPE_ACHR: u8 = 1;
pub const TYPE_QUST: u8 = 9;
pub const TYPE_NPC: u8 = 10;
pub const TYPE_CLAS: u8 = 33;

/// The player's reference and base form.
pub const PLAYER_REF: u32 = 0x14;
pub const PLAYER_BASE: u32 = 0x7;
/// The player's class, whose record holds the tag skills (FalloutNV.esm PlayerClass; the
/// one CLAS the saves change, save-files.md §6.1) [observed].
pub const PLAYER_CLASS: u32 = 0x0005_7E6A;

/// A cursor over the file's bytes.
#[derive(Clone)]
struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn new(b: &'a [u8], p: usize) -> Self {
        Cur { b, p }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], FosError> {
        let end = self.p.checked_add(n).filter(|&e| e <= self.b.len()).ok_or_else(|| FosError(format!("{n} bytes past the end at {}", self.p)))?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    /// A byte that must have this value: a fixed marker of the layout.
    fn expect_u8(&mut self, want: u8) -> Result<(), FosError> {
        match self.u8()? {
            b if b == want => Ok(()),
            b => err(format!("expected {want:#04x} at {}, found {b:#04x}", self.p - 1)),
        }
    }
    fn u8(&mut self) -> Result<u8, FosError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, FosError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, FosError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, FosError> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<f32, FosError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64, FosError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn bar(&mut self) -> Result<(), FosError> {
        match self.u8()? {
            BAR => Ok(()),
            b => err(format!("expected | at {}, found {b:#04x}", self.p - 1)),
        }
    }
    /// A variable-size count: the low 2 bits of the first byte give the width (1, 2 or
    /// 4 bytes), the value is the little-endian number shifted right by 2 (§5.4).
    fn vsval(&mut self) -> Result<u32, FosError> {
        let width = match self.b.get(self.p).copied().unwrap_or(0) & 3 {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => return err(format!("bad vsval at {}", self.p)),
        };
        let mut v = 0u32;
        for (i, x) in self.take(width)?.iter().enumerate() {
            v |= (*x as u32) << (8 * i);
        }
        Ok(v >> 2)
    }
    /// A 3-byte reference, big-endian (§5.4).
    fn ref3(&mut self) -> Result<[u8; 3], FosError> {
        Ok(self.take(3)?.try_into().unwrap())
    }
    /// A string: u16 length, `|`, the bytes (§3.1), in this run's code page
    /// (fnv_util::text, #1439).
    fn string(&mut self) -> Result<String, FosError> {
        let n = self.u16()? as usize;
        self.bar()?;
        Ok(fnv_util::text::decode(self.take(n)?))
    }
    fn at_end(&self) -> bool {
        self.p >= self.b.len()
    }
}

/// The header fields (§3.1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FosHeader {
    pub save_number: u32,
    pub player_name: String,
    pub karma_title: String,
    pub level: u32,
    pub location: String,
    pub play_time: String,
}

/// One changed form (§6): its form ID (resolved through the form ID array), change
/// flags, record type and data.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangedForm {
    pub form: u32,
    pub flags: u32,
    pub kind: u8,
    pub data: Vec<u8>,
}

impl ChangedForm {
    fn has(&self, bit: u32) -> bool {
        self.flags & (1 << bit) != 0
    }
}

/// A script variable's saved value (§6.2): a number, or a reference when the index's top
/// bit is set (`None`: no reference).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VarValue {
    Num(f64),
    Ref(Option<u32>),
    /// A reference the form ID array does not hold.
    Unresolved,
}

/// A quest's saved changes (§6.2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FosQuest {
    pub form: u32,
    /// The flags byte (change flag 1): bit 0 running, bit 1 completed [hypothesis].
    pub flags: Option<u8>,
    /// SetQuestDelay's value (change flag 2) [hypothesis].
    pub delay: Option<f32>,
    /// (stage index, done) for each stage the save lists (change flag 31).
    pub stages: Vec<(u8, bool)>,
    /// Script variables by index (change flag 30).
    pub vars: Vec<(u32, VarValue)>,
    /// (objective index, state) (change flag 29); bit 0 displayed, bit 1 completed
    /// [hypothesis: states 1 and 3 seen].
    pub objectives: Vec<(u32, u32)>,
}

impl FosQuest {
    pub fn running(&self) -> Option<bool> {
        self.flags.map(|f| f & 1 != 0)
    }
    pub fn completed(&self) -> Option<bool> {
        self.flags.map(|f| f & 2 != 0)
    }
}

/// An inventory entry of the player (§6.2): the item and its count change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FosItem {
    /// `None`: a reference the form ID array does not hold.
    pub item: Option<u32>,
    /// The change against the base form's inventory [hypothesis: the Pip-Boy, which the
    /// player's base carries, is listed worn with 0].
    pub count: i32,
}

/// The player's state that the lab reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FosPlayer {
    /// The worldspace (outdoors) or interior cell, position and rotation (radians).
    pub place: Option<(u32, [f32; 3], [f32; 3])>,
    /// Strength … Luck from the player's base form (NPC_ 0x7, change flag 2).
    pub special: Option<[u8; 7]>,
    /// Tag skills, as actor value codes (PlayerClass, change flag 1).
    pub tag_skills: Vec<u32>,
    /// The first array of actor-value modifiers by code (§6.2), non-zero entries
    /// [hypothesis: the permanent modifiers; skill points spent are here].
    pub av_modifiers: Vec<(u32, f32)>,
    /// The inventory changes, `None` when the record's layout was not understood.
    pub inventory: Option<Vec<FosItem>>,
}

/// A parsed `.fos`.
#[derive(Debug, Clone, Default)]
pub struct Fos {
    pub header: FosHeader,
    /// The load order the save was made with; a form ID's top byte indexes it (§4).
    pub plugins: Vec<String>,
    pub form_ids: Vec<u32>,
    pub worlds: Vec<u32>,
    /// Global data entries of both tables: (type, bytes) (§5).
    pub globals_data: Vec<(u32, Vec<u8>)>,
    pub changed: Vec<ChangedForm>,
}

/// The number of actor values in each modifier array of the player's record.
const AV_COUNT: usize = 77;
/// f32 entries between the place and the rest of the player's record: three arrays of
/// [`AV_COUNT`] and one more [observed: 232 in every save looked at].
const AV_ENTRIES: usize = 232;

impl Fos {
    pub fn parse(b: &[u8]) -> Result<Fos, FosError> {
        if !b.starts_with(MAGIC) {
            return err("not a FO3SAVEGAME file");
        }
        let mut c = Cur::new(b, MAGIC.len());
        let header_size = c.u32()? as usize;
        let header_start = c.p;
        let mut h = FosHeader::default();
        c.u32()?;
        c.bar()?;
        c.take(64)?;
        c.bar()?;
        let width = c.u32()? as usize;
        c.bar()?;
        let height = c.u32()? as usize;
        c.bar()?;
        h.save_number = c.u32()?;
        c.bar()?;
        h.player_name = c.string()?;
        c.bar()?;
        h.karma_title = c.string()?;
        c.bar()?;
        h.level = c.u32()?;
        c.bar()?;
        h.location = c.string()?;
        c.bar()?;
        h.play_time = c.string()?;
        // The screenshot follows the header (§3.2).
        let shot = width.checked_mul(height).and_then(|n| n.checked_mul(3));
        c.p = shot.and_then(|n| header_start.checked_add(header_size)?.checked_add(n)).ok_or_else(|| FosError("a header or screenshot size past any file".into()))?;
        let _form_version = c.u8()?;
        let plugin_block = c.u32()? as usize;
        let plugins_end = c.p.checked_add(plugin_block).ok_or_else(|| FosError("a plugin block size past any file".into()))?;
        let n = c.u8()?;
        c.bar()?;
        let mut plugins = Vec::new();
        for _ in 0..n {
            plugins.push(c.string()?);
            c.bar()?;
        }
        c.p = plugins_end;
        let mut t = [0u32; 8];
        for v in &mut t {
            *v = c.u32()?;
        }
        let [fid_off, _tail, g1_off, cf_off, g2_off, g1_count, g2_count, cf_count] = t.map(|v| v as usize);
        let mut a = Cur::new(b, fid_off);
        let nf = a.u32()? as usize;
        let form_ids = (0..nf).map(|_| a.u32()).collect::<Result<Vec<_>, _>>()?;
        let nw = a.u32()? as usize;
        let worlds = (0..nw).map(|_| a.u32()).collect::<Result<Vec<_>, _>>()?;
        let mut globals_data = Vec::new();
        for (off, count) in [(g1_off, g1_count), (g2_off, g2_count)] {
            let mut g = Cur::new(b, off);
            for _ in 0..count {
                let ty = g.u32()?;
                let len = g.u32()? as usize;
                globals_data.push((ty, g.take(len)?.to_vec()));
            }
        }
        let mut fos = Fos { header: h, plugins, form_ids, worlds, globals_data, changed: Vec::new() };
        let mut r = Cur::new(b, cf_off);
        for _ in 0..cf_count {
            let rf = r.ref3()?;
            let flags = r.u32()?;
            let ty = r.u8()?;
            let _version = r.u8()?;
            let len = match ty >> 6 {
                0 => r.u8()? as usize,
                1 => r.u16()? as usize,
                2 => r.u32()? as usize,
                _ => return err(format!("bad length width at {}", r.p)),
            };
            let data = r.take(len)?.to_vec();
            fos.changed.push(ChangedForm { form: fos.resolve(rf).unwrap_or(0), flags, kind: ty & 0x3F, data });
        }
        if r.p != g2_off {
            return err(format!("the changed forms end at {}, not at table 2 ({g2_off})", r.p));
        }
        Ok(fos)
    }

    /// A reference's form ID (§5.4): kind 0 indexes the form ID array (1-based, 0 = none),
    /// kind 1 is a form ID of the first master, kind 2 a created form.
    /// `None` for no reference and for one the save does not hold ([`Fos::reference`]
    /// tells them apart).
    pub fn resolve(&self, r: [u8; 3]) -> Option<u32> {
        self.reference(r).ok().flatten()
    }

    /// [`Fos::resolve`], with `Err` for an index past the form ID array or an unknown kind,
    /// and `Ok(None)` for "no reference".
    pub fn reference(&self, r: [u8; 3]) -> Result<Option<u32>, FosError> {
        let v = u32::from_be_bytes([0, r[0], r[1], r[2]]);
        let value = v & 0x3F_FFFF;
        match v >> 22 {
            0 if value == 0 => Ok(None),
            0 => self.form_ids.get(value as usize - 1).copied().map(Some).ok_or_else(|| FosError(format!("reference {value} past the form ID array"))),
            1 => Ok(Some(value)),
            2 => Ok(Some(0xFF00_0000 | value)),
            k => err(format!("reference of unknown kind {k}")),
        }
    }

    fn global(&self, ty: u32) -> Option<&[u8]> {
        self.globals_data.iter().find(|(t, _)| *t == ty).map(|(_, d)| d.as_slice())
    }

    /// The global variables (global table type 3): (form ID, value).
    pub fn global_variables(&self) -> Result<Vec<(u32, f32)>, FosError> {
        let Some(d) = self.global(3) else { return Ok(Vec::new()) };
        let mut c = Cur::new(d, 0);
        let n = c.vsval()?;
        c.bar()?;
        let mut out = Vec::new();
        for _ in 0..n {
            let r = c.ref3()?;
            c.bar()?;
            let v = c.f32()?;
            c.bar()?;
            if let Some(id) = self.resolve(r) {
                out.push((id, v));
            }
        }
        Ok(out)
    }

    /// The quest that global table 1's type 1 entry names (§5.1) [hypothesis: the active
    /// quest].
    pub fn active_quest(&self) -> Option<u32> {
        let d = self.global(1)?;
        // next created ID |, world |, grid x |, grid y |, cell or world |, 3 f32 |, quest.
        let mut c = Cur::new(d, 0);
        c.u32().ok()?;
        c.bar().ok()?;
        c.ref3().ok()?;
        c.bar().ok()?;
        c.i32().ok()?;
        c.bar().ok()?;
        c.i32().ok()?;
        c.bar().ok()?;
        c.ref3().ok()?;
        c.bar().ok()?;
        c.take(12).ok()?;
        c.bar().ok()?;
        self.resolve(c.ref3().ok()?)
    }

    fn records(&self, kind: u8) -> impl Iterator<Item = &ChangedForm> {
        self.changed.iter().filter(move |f| f.kind == kind)
    }

    fn record(&self, form: u32, kind: u8) -> Option<&ChangedForm> {
        self.records(kind).find(|f| f.form == form)
    }

    /// Every quest record's changes (§6.2), each on its own: a record the reader does not
    /// understand is an `Err` and leaves the others.
    pub fn quests(&self) -> Vec<Result<FosQuest, FosError>> {
        self.records(TYPE_QUST).map(|f| self.quest(f).map_err(|e| FosError(format!("quest {:08X}: {e}", f.form)))).collect()
    }

    fn quest(&self, f: &ChangedForm) -> Result<FosQuest, FosError> {
        let mut q = FosQuest { form: f.form, ..FosQuest::default() };
        let mut c = Cur::new(&f.data, 0);
        if f.has(1) {
            q.flags = Some(c.u8()?);
            c.bar()?;
        }
        if f.has(2) {
            q.delay = Some(c.f32()?);
            c.bar()?;
        }
        if f.has(31) {
            let n = c.vsval()?;
            c.bar()?;
            for _ in 0..n {
                let stage = c.u8()?;
                c.bar()?;
                let done = c.u8()? != 0;
                c.bar()?;
                q.stages.push((stage, done));
                let entries = c.vsval()?;
                c.bar()?;
                for _ in 0..entries {
                    c.u8()?;
                    c.bar()?;
                    let done = c.u8()? != 0;
                    c.bar()?;
                    if done {
                        // A date stamp (§6.2).
                        c.take(4)?;
                        c.bar()?;
                    }
                }
            }
        }
        if f.has(30) {
            let n = c.vsval()?;
            c.bar()?;
            for _ in 0..n {
                let index = c.u32()?;
                c.bar()?;
                let value = if index & 0x8000_0000 != 0 {
                    self.reference(c.ref3()?).map_or(VarValue::Unresolved, VarValue::Ref)
                } else {
                    VarValue::Num(c.f64()?)
                };
                c.bar()?;
                q.vars.push((index & 0x7FFF_FFFF, value));
            }
            // Two more lists, empty in every quest looked at.
            for _ in 0..2 {
                if c.vsval()? != 0 {
                    return err("a non-empty list after the variables");
                }
                c.bar()?;
            }
        }
        if f.has(29) {
            let n = c.vsval()?;
            c.bar()?;
            for _ in 0..n {
                let index = c.u32()?;
                c.bar()?;
                let state = c.u32()?;
                c.bar()?;
                q.objectives.push((index, state));
            }
        }
        if !c.at_end() {
            return err(format!("{} bytes left over", f.data.len() - c.p));
        }
        Ok(q)
    }

    /// The player's place, SPECIAL, tags, modifiers and inventory.
    pub fn player(&self) -> Result<FosPlayer, FosError> {
        let mut p = FosPlayer::default();
        if let Some(npc) = self.record(PLAYER_BASE, TYPE_NPC).filter(|f| f.has(2)) {
            p.special = Some(npc.data.get(..7).ok_or_else(|| FosError("short SPECIAL".into()))?.try_into().unwrap());
        }
        if let Some(clas) = self.record(PLAYER_CLASS, TYPE_CLAS).filter(|f| f.has(1)) {
            let mut c = Cur::new(&clas.data, 0);
            for _ in 0..4 {
                let v = c.u32()?;
                c.bar()?;
                if v != u32::MAX {
                    p.tag_skills.push(v);
                }
            }
        }
        let Some(r) = self.record(PLAYER_REF, TYPE_ACHR) else { return Ok(p) };
        let mut c = Cur::new(&r.data, 0);
        let at = self.resolve(c.ref3()?);
        let mut v = [0f32; 6];
        for x in &mut v {
            *x = c.f32()?;
        }
        c.bar()?;
        if let Some(at) = at {
            p.place = Some((at, [v[0], v[1], v[2]], [v[3], v[4], v[5]]));
        }
        for i in 0..AV_ENTRIES {
            let x = c.f32()?;
            c.bar()?;
            if i < AV_COUNT && x != 0.0 {
                p.av_modifiers.push((i as u32, x));
            }
        }
        p.inventory = self.player_inventory(r, c).ok();
        Ok(p)
    }

    /// The player's inventory list, after the parts between the modifiers and it that are
    /// not understood but have the same shape in every save looked at (save-files.md §6.2).
    /// Its fixed markers (0x00, 1.0, 0x5E, 0x18, 0x74) are checked, so a record of another
    /// shape gives no inventory rather than items read out of other data.
    fn player_inventory(&self, r: &ChangedForm, mut c: Cur<'_>) -> Result<Vec<FosItem>, FosError> {
        c.expect_u8(0)?;
        c.bar()?;
        if c.f32()? != 1.0 {
            return err("not 1.0 after the actor values");
        }
        c.bar()?;
        c.vsval()?;
        c.bar()?;
        c.expect_u8(0x5E)?;
        c.bar()?;
        let n = c.vsval()?;
        c.bar()?;
        for _ in 0..n {
            c.ref3()?;
            c.bar()?;
            c.u8()?;
            c.bar()?;
        }
        c.expect_u8(0x18)?;
        c.bar()?;
        c.ref3()?;
        c.bar()?;
        c.take(12)?;
        c.bar()?;
        c.f32()?;
        c.bar()?;
        if r.has(29) {
            c.expect_u8(0x74)?;
            c.bar()?;
            c.ref3()?;
            c.bar()?;
        }
        let n = c.vsval()?;
        c.bar()?;
        let mut items = Vec::new();
        for _ in 0..n {
            let item = self.reference(c.ref3()?).ok().flatten();
            c.bar()?;
            let count = c.i32()?;
            c.bar()?;
            let lists = c.vsval()?;
            c.bar()?;
            for _ in 0..lists {
                let extras = c.vsval()?;
                c.bar()?;
                for _ in 0..extras {
                    let size = match c.u8()? {
                        // worn
                        0x16 => 0,
                        // a reference (an owner)
                        0x21 => 3,
                        // condition (f32)
                        0x25 => 4,
                        t => return err(format!("extra data type {t:#04x} of unknown size")),
                    };
                    c.bar()?;
                    if size > 0 {
                        c.take(size)?;
                        c.bar()?;
                    }
                }
            }
            items.push(FosItem { item, count });
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes built the way the reader expects, for a save with one plugin.
    struct Build {
        b: Vec<u8>,
    }

    impl Build {
        fn bar(&mut self) -> &mut Self {
            self.b.push(BAR);
            self
        }
        fn u8(&mut self, v: u8) -> &mut Self {
            self.b.push(v);
            self
        }
        fn u32(&mut self, v: u32) -> &mut Self {
            self.b.extend(v.to_le_bytes());
            self
        }
        fn f32(&mut self, v: f32) -> &mut Self {
            self.b.extend(v.to_le_bytes());
            self
        }
        fn f64(&mut self, v: f64) -> &mut Self {
            self.b.extend(v.to_le_bytes());
            self
        }
        fn s(&mut self, v: &str) -> &mut Self {
            self.b.extend((v.len() as u16).to_le_bytes());
            self.bar();
            self.b.extend(v.as_bytes());
            self
        }
        /// A kind-0 reference to the form ID array's 1-based `i`.
        fn r(&mut self, i: u32) -> &mut Self {
            self.b.extend(&i.to_be_bytes()[1..]);
            self
        }
        fn vs(&mut self, n: u32) -> &mut Self {
            assert!(n < 64);
            self.u8((n << 2) as u8)
        }
    }

    fn record(out: &mut Vec<u8>, r: u32, flags: u32, kind: u8, data: &[u8]) {
        out.extend(&r.to_be_bytes()[1..]);
        out.extend(flags.to_le_bytes());
        out.push(kind | 0x40);
        out.push(27);
        out.extend((data.len() as u16).to_le_bytes());
        out.extend(data);
    }

    const PLAYER_FLAGS: u32 = 1 << 1 | 1 << 4 | 1 << 5;

    /// The player record's data: the place, the actor values, the unknown part (with
    /// `marker` where 0x5E stands), the flag-29 block when `bit29`, and two items, the
    /// second with a worn extra and one of type `extra` (0x25: an f32).
    fn player_data(bit29: bool, marker: u8, extra: u8) -> Vec<u8> {
        let mut p = Build { b: Vec::new() };
        p.r(3);
        for v in [37000.0f32, -117159.0, 6057.0, 0.0, 0.0, 1.309] {
            p.f32(v);
        }
        p.bar();
        for i in 0..AV_ENTRIES {
            p.f32(if i == 38 { 2.0 } else { 0.0 }).bar();
        }
        p.u8(0).bar().f32(1.0).bar().vs(3).bar().u8(marker).bar().vs(1).bar().r(4).bar().u8(1).bar();
        p.u8(0x18).bar().r(2).bar().f32(1.0).f32(2.0).f32(3.0).bar().f32(3.5).bar();
        if bit29 {
            p.u8(0x74).bar().r(8).bar();
        }
        p.vs(2).bar();
        p.r(4).bar().u32(9).bar().vs(0).bar();
        p.r(4).bar().u32(1).bar().vs(1).bar().vs(2).bar().u8(0x16).bar().u8(extra).bar().f32(41.25).bar();
        p.b
    }

    /// A save: form IDs [quest 0x1234, global 0x5678, world 0xDA726, item 0x15169,
    /// player 0x14, player base 7, class 0x57E6A, ref var target 0x4242].
    fn save() -> Vec<u8> {
        let fids = [0x1234u32, 0x5678, 0xDA726, 0x15169, 0x14, 0x7, 0x57E6A, 0x4242];
        let mut h = Build { b: Vec::new() };
        h.u32(0x30).bar();
        h.b.extend([0u8; 64]);
        h.bar().u32(2).bar().u32(1).bar().u32(15).bar().s("Courier").bar().s("Drifter").bar().u32(3).bar().s("Goodsprings").bar().s("000.11.50");
        let mut b = MAGIC.to_vec();
        b.extend((h.b.len() as u32).to_le_bytes());
        b.extend(&h.b);
        b.extend([0u8; 6]); // 2 x 1 screenshot
        let mut pl = Build { b: Vec::new() };
        pl.u8(1).bar().s("FalloutNV.esm").bar();
        b.push(27);
        b.extend((pl.b.len() as u32).to_le_bytes());
        b.extend(&pl.b);
        let table_at = b.len();
        b.extend([0u8; 32 + 78]);
        // Global table 1: type 1 (place) and type 3 (globals).
        let g1 = b.len();
        let mut t1 = Build { b: Vec::new() };
        t1.u32(0xFF00_1000).bar().r(3).bar().u32(9).bar().u32(-29i32 as u32).bar().r(3).bar().f32(1.0).f32(2.0).f32(3.0).bar().r(1).bar();
        b.extend(1u32.to_le_bytes());
        b.extend((t1.b.len() as u32).to_le_bytes());
        b.extend(&t1.b);
        let mut t3 = Build { b: Vec::new() };
        t3.vs(1).bar().r(2).bar().f32(12.5).bar();
        b.extend(3u32.to_le_bytes());
        b.extend((t3.b.len() as u32).to_le_bytes());
        b.extend(&t3.b);
        // Changed forms.
        let cf = b.len();
        let mut q = Build { b: Vec::new() };
        q.u8(0x31).bar();
        q.vs(1).bar().u8(10).bar().u8(1).bar().vs(1).bar().u8(0).bar().u8(1).bar().u32(0x08E9_0123).bar();
        q.vs(2).bar().u32(3).bar().f64(1.5).bar().u32(0x8000_0004).bar().r(8).bar().vs(0).bar().vs(0).bar();
        q.vs(1).bar().u32(10).bar().u32(3).bar();
        record(&mut b, 1, 1 << 1 | 1 << 31 | 1 << 30 | 1 << 29, TYPE_QUST, &q.b);
        let mut npc = Build { b: Vec::new() };
        npc.b.extend([5, 6, 5, 5, 5, 5, 10]);
        npc.bar();
        record(&mut b, 6, 1 << 2, TYPE_NPC, &npc.b);
        let mut clas = Build { b: Vec::new() };
        clas.u32(38).bar().u32(43).bar().u32(44).bar().u32(u32::MAX).bar();
        record(&mut b, 7, 1 << 1, TYPE_CLAS, &clas.b);
        record(&mut b, 5, PLAYER_FLAGS, TYPE_ACHR, &player_data(false, 0x5E, 0x25));
        let g2 = b.len();
        b.extend(1000u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        let fid = b.len();
        b.extend((fids.len() as u32).to_le_bytes());
        for f in fids {
            b.extend(f.to_le_bytes());
        }
        b.extend(1u32.to_le_bytes());
        b.extend(0xDA726u32.to_le_bytes());
        let tail = b.len();
        b.extend([5, 0, 0, 0, 0, 0, 0, 0, 0x7C]);
        let table = [fid, tail, g1, cf, g2, 2, 1, 4].map(|v| v as u32);
        for (i, v) in table.iter().enumerate() {
            b[table_at + 4 * i..table_at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn reads_header_plugins_and_globals() {
        let f = Fos::parse(&save()).unwrap();
        assert_eq!(f.header.player_name, "Courier");
        assert_eq!(f.header.level, 3);
        assert_eq!(f.header.location, "Goodsprings");
        assert_eq!(f.plugins, ["FalloutNV.esm"]);
        assert_eq!(f.worlds, [0xDA726]);
        assert_eq!(f.changed.len(), 4);
        assert_eq!(f.global_variables().unwrap(), [(0x5678, 12.5)]);
        assert_eq!(f.active_quest(), Some(0x1234));
    }

    #[test]
    fn reads_quest_stages_variables_and_objectives() {
        let f = Fos::parse(&save()).unwrap();
        let q = f.quests().remove(0).unwrap();
        assert_eq!(q.form, 0x1234);
        assert_eq!((q.running(), q.completed()), (Some(true), Some(false)));
        assert_eq!(q.stages, [(10, true)]);
        assert_eq!(q.vars, [(3, VarValue::Num(1.5)), (4, VarValue::Ref(Some(0x4242)))]);
        assert_eq!(q.objectives, [(10, 3)]);
    }

    #[test]
    fn reads_the_player() {
        let f = Fos::parse(&save()).unwrap();
        let p = f.player().unwrap();
        let (at, pos, rot) = p.place.unwrap();
        assert_eq!((at, pos[0], rot[2]), (0xDA726, 37000.0, 1.309));
        assert_eq!(p.special, Some([5, 6, 5, 5, 5, 5, 10]));
        assert_eq!(p.tag_skills, [38, 43, 44]);
        assert_eq!(p.av_modifiers, [(38, 2.0)]);
        assert_eq!(p.inventory.unwrap(), [FosItem { item: Some(0x15169), count: 9 }, FosItem { item: Some(0x15169), count: 1 }]);
    }

    /// The player record with other data in place of the test save's.
    fn with_player(data: Vec<u8>, flags: u32) -> FosPlayer {
        let mut f = Fos::parse(&save()).unwrap();
        let r = f.changed.iter_mut().find(|c| c.form == PLAYER_REF).unwrap();
        (r.data, r.flags) = (data, flags);
        f.player().unwrap()
    }

    #[test]
    fn the_inventory_after_the_flag_29_block_and_none_for_another_shape() {
        // Flag 29: `74 |` and a ref before the list.
        let p = with_player(player_data(true, 0x5E, 0x25), PLAYER_FLAGS | 1 << 29);
        assert_eq!(p.inventory.map(|i| i.len()), Some(2));
        // A marker that differs: no inventory, not items read out of other data.
        assert_eq!(with_player(player_data(false, 0x5F, 0x25), PLAYER_FLAGS).inventory, None);
        // The flag set but no block: none either.
        assert_eq!(with_player(player_data(false, 0x5E, 0x25), PLAYER_FLAGS | 1 << 29).inventory, None);
        // An extra data type of unknown size: none.
        let p = with_player(player_data(false, 0x5E, 0x0D), PLAYER_FLAGS);
        assert_eq!(p.inventory, None);
        assert_eq!(p.tag_skills, [38, 43, 44], "the rest is read");
    }

    #[test]
    fn an_odd_quest_leaves_the_others_and_a_reference_past_the_array_is_unresolved() {
        let mut f = Fos::parse(&save()).unwrap();
        let good = f.changed.iter().find(|c| c.kind == TYPE_QUST).unwrap().clone();
        // Bytes left over after the flags byte.
        f.changed.push(ChangedForm { form: 0x9999, flags: 1 << 1, kind: TYPE_QUST, data: vec![1, BAR, 0] });
        // A reference variable naming form 99 of an 8-entry array.
        let mut v = Build { b: Vec::new() };
        v.vs(1).bar().u32(0x8000_0002).bar().r(99).bar().vs(0).bar().vs(0).bar();
        f.changed.push(ChangedForm { form: 0x8888, flags: 1 << 30, kind: TYPE_QUST, data: v.b });
        let qs = f.quests();
        assert_eq!(qs.len(), 3);
        assert_eq!(qs[0].as_ref().unwrap().form, good.form);
        assert!(qs[1].is_err());
        assert_eq!(qs[2].as_ref().unwrap().vars, [(2, VarValue::Unresolved)]);
    }

    #[test]
    fn tag_skills_only_from_the_player_class() {
        let mut f = Fos::parse(&save()).unwrap();
        f.changed.iter_mut().find(|c| c.kind == TYPE_CLAS).unwrap().form = 0x1234_5678;
        assert_eq!(f.player().unwrap().tag_skills, Vec::<u32>::new());
    }

    #[test]
    fn sizes_past_any_file_are_errors_not_overflows() {
        // The screenshot's width and height (after the u32, the language and their bars).
        let mut b = save();
        let at = MAGIC.len() + 4 + 5 + 65;
        b[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        b[at + 5..at + 9].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Fos::parse(&b).is_err());
        // The header size.
        let mut b = save();
        b[MAGIC.len()..MAGIC.len() + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Fos::parse(&b).is_err());
    }

    #[test]
    fn refuses_other_files_and_short_ones() {
        assert!(Fos::parse(b"NVFSAVE\0").is_err());
        let b = save();
        assert!(Fos::parse(&b[..b.len() / 2]).is_err());
    }
}
