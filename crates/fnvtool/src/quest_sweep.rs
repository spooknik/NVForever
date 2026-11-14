//! `fnvtool quest sweep`: the shapes in a load order's data that can soft-lock a player
//! (issue #2365 item 5; spec: docs/design/quest-sweep.md). Read-only and static: no script
//! runs. Four checks, each a list of findings:
//!
//! 1. `conversations`: lines whose choices never lead to a way out of the conversation,
//!    and speakers whose top level offers no Goodbye (#1595's shape).
//! 2. `controls`: scripts that take the player's movement away (DisablePlayerControls,
//!    `player.SetRestrained 1`) and where the controls come back, followed through the
//!    variables, stages, packages and lines the taking script sets going.
//! 3. `stages`: quest stages that something tests but whose only setters are a package's
//!    script, a trigger with no navmesh near it, an activator that starts disabled, or
//!    nothing at all (a hypothesis list).
//! 4. `doors`: locked doors that need a key and have none, and what opens them instead.
//!
//! The checks read [`Facts`], a small model drawn from `GameData`, the placed references
//! and the navmeshes ([`extract`]); the unit tests build `Facts` by hand.

use fnv_game::cond::{Condition, RunOn};
use fnv_game::data::topic_type;
use fnv_game::GameData;
use fnv_script::unit::{PackageEvent, RefSlot, UnitOwner};
use fnv_script::vm::{ArgValue, Value as VmValue};
use fnv_script::{Arg, Expr, Program, ScriptUnit, Stmt, Token, VarSlot};
use fnv_sim::{FormId, RefTable};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

pub const PLAYER: u32 = 0x14;

// ------------------------------------------------------------------------- the model

/// Who a call runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum On {
    /// No reference: the script's own (its owner, or the player for most result scripts).
    Implicit,
    Form(u32),
    /// A local `ref` variable.
    Var(String),
    Other,
}

/// One call parameter, as far as the checks read it.
#[derive(Debug, Clone, PartialEq)]
pub enum P {
    Num(f64),
    Form(u32),
    Other,
}

/// A function call statement in a unit.
#[derive(Debug, Clone, PartialEq)]
pub struct CallFact {
    /// Function name, lower case.
    pub func: String,
    pub on: On,
    pub args: Vec<P>,
    /// The `Begin` block's event name, lower case (`None`: a result script's top level).
    pub block: Option<String>,
    /// Event parameters of the block (`OnTrigger player` → `[Form(0x14)]`).
    pub block_args: Vec<P>,
    /// What the `If` conditions around the call read: function names (lower case),
    /// `var:<name>` for a local, `var:0x<form>.<name>` for another script's variable.
    pub guards: Vec<String>,
}

/// A `set <form>.<var> to …` in a unit: another script's variable written.
#[derive(Debug, Clone, PartialEq)]
pub struct Write {
    pub form: u32,
    pub var: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnitFacts {
    pub key: String,
    pub owner: UnitOwner,
    pub calls: Vec<CallFact>,
    pub writes: Vec<Write>,
    /// Locals counted with GetSecondsPassed (timers).
    pub timers: BTreeSet<String>,
    /// Quest stages the unit's conditions test (GetStage, GetStageDone).
    pub stage_tests: Vec<(u32, Option<i32>)>,
}

/// One condition atom as far as a speaker decides it.
#[derive(Debug, Clone, PartialEq)]
pub enum Atom {
    /// GetIsID: the speaker's base is (or is not) one of these.
    Id(Vec<u32>, bool),
    /// GetIsVoiceType: the voice is (or is not) one of these.
    Voice(Vec<u32>, bool),
    Faction(u32, bool),
    Female(bool),
    Race(u32, bool),
    /// Anything else: may pass.
    Unknown,
}

/// A dialogue line (INFO).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LineFacts {
    pub topic: u32,
    pub quest: Option<u32>,
    pub goodbye: bool,
    pub info_type: u8,
    /// TCLT.
    pub choices: Vec<u32>,
    /// TCLF.
    pub link_from: Vec<u32>,
    /// The conditions on the speaker, as OR groups joined by AND (ANAM is a group of one).
    pub who: Vec<Vec<Atom>>,
    pub prompt: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TopicFacts {
    pub edid: Option<String>,
    pub topic_type: u8,
    pub lines: Vec<u32>,
    /// A top-level Topic (offered in the topic list).
    pub top_level: bool,
    /// The topics whose lines name this one in their Link From.
    pub linked_from: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActorFacts {
    pub edid: Option<String>,
    pub voice: Option<u32>,
    pub female: bool,
    pub race: Option<u32>,
    pub factions: Vec<u32>,
    /// The factions come from a template (not known here).
    pub factions_templated: bool,
    pub placed: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RefFacts {
    pub edid: Option<String>,
    pub base: u32,
    pub base_sig: String,
    pub cell: Option<u32>,
    pub pos: [f32; 3],
    pub disabled: bool,
    pub enable_parent: Option<u32>,
    /// (level, key): level 255 needs the key.
    pub lock: Option<(u8, Option<u32>)>,
    pub teleport: bool,
    /// Half the largest extent of the trigger box, when the reference has one.
    pub radius: Option<f32>,
    /// Whether a navmesh lies under or beside it (`None`: not looked at).
    pub nav_near: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PackageFacts {
    pub edid: Option<String>,
    /// The topic a Dialogue package says.
    pub topic: Option<u32>,
    /// Begin, end, change result scripts (unit indices).
    pub units: Vec<usize>,
}

/// Everything the checks read.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub units: Vec<UnitFacts>,
    pub lines: BTreeMap<u32, LineFacts>,
    pub topics: BTreeMap<u32, TopicFacts>,
    pub greeting: Option<u32>,
    pub goodbye: Option<u32>,
    pub actors: BTreeMap<u32, ActorFacts>,
    pub refs: BTreeMap<u32, RefFacts>,
    pub packages: BTreeMap<u32, PackageFacts>,
    /// Actor base → its AI packages.
    pub actor_packages: BTreeMap<u32, Vec<u32>>,
    /// Quest → its stages (defined in the record).
    pub quest_stages: BTreeMap<u32, BTreeSet<i32>>,
    /// Stage result scripts.
    pub stage_units: BTreeMap<(u32, i32), Vec<usize>>,
    /// Line → its begin and end result scripts.
    pub line_units: BTreeMap<u32, Vec<usize>>,
    /// SCPT → its unit.
    pub script_unit: BTreeMap<u32, usize>,
    /// Base form or quest → its SCPT.
    pub form_script: BTreeMap<u32, u32>,
    /// Form lists (GetIsVoiceType and GetIsID take them).
    pub form_lists: BTreeMap<u32, Vec<u32>>,
    pub edids: BTreeMap<u32, String>,
    pub sigs: BTreeMap<u32, String>,
    /// Terminal item texts by (terminal, item).
    pub terminal_items: BTreeMap<(u32, u32), String>,
    /// Stages tested by conditions outside scripts (lines, packages, quests, stages).
    pub condition_stage_tests: BTreeSet<(u32, Option<i32>)>,
}

impl Facts {
    fn name(&self, f: u32) -> String {
        match self.edids.get(&f) {
            Some(e) => format!("{e} 0x{f:08X}"),
            None => format!("0x{f:08X}"),
        }
    }

    /// The SCPT unit of a reference's base, a base form or a quest.
    fn script_of(&self, form: u32) -> Option<usize> {
        let base = self.refs.get(&form).map_or(form, |r| r.base);
        self.form_script.get(&base).and_then(|s| self.script_unit.get(s)).copied()
    }

    /// The placed references whose base runs SCPT `script`, and the quests and bases that do.
    fn users_of_script(&self, script: u32) -> (Vec<u32>, Vec<u32>) {
        let owners: Vec<u32> = self.form_script.iter().filter(|(_, s)| **s == script).map(|(f, _)| *f).collect();
        let refs: Vec<u32> = self.refs.iter().filter(|(_, r)| owners.contains(&r.base)).map(|(id, _)| *id).collect();
        (refs, owners)
    }

    fn quest_script_units(&self) -> BTreeSet<usize> {
        self.quest_stages.keys().filter_map(|q| self.script_of(*q)).collect()
    }
}

// ------------------------------------------------------------------------- extraction

fn lower_name(data: &GameData, op: u16) -> String {
    data.table.get(op).map_or_else(|| format!("op_{op:04x}"), |f| f.name.to_ascii_lowercase())
}

fn slot_form(u: &ScriptUnit, r: fnv_script::ir::RefIndex) -> Option<u32> {
    match u.slot(r)? {
        RefSlot::Form(f) => Some(f),
        _ => None,
    }
}

fn on_of(u: &ScriptUnit, r: Option<fnv_script::ir::RefIndex>) -> On {
    let Some(r) = r else { return On::Implicit };
    match u.slot(r) {
        Some(RefSlot::Form(f)) => On::Form(f),
        Some(RefSlot::Local(i)) => On::Var(u.var(i).map_or_else(|| format!("#{i}"), |v| v.name.to_ascii_lowercase())),
        _ => On::Other,
    }
}

fn arg_of(u: &ScriptUnit, a: &Arg) -> P {
    match a {
        Arg::Int(i) => P::Num(*i as f64),
        Arg::Int32(i) => P::Num(*i as f64),
        Arg::Float(f) => P::Num(*f),
        Arg::Ref(r) => slot_form(u, *r).map_or(P::Other, P::Form),
        _ => P::Other,
    }
}

fn var_label(u: &ScriptUnit, v: &fnv_script::ir::VarRef) -> String {
    match (v.on, v.var) {
        (None, VarSlot::Local { index, .. }) => format!("var:{}", u.var(index as u32).map_or_else(|| format!("#{index}"), |d| d.name.to_ascii_lowercase())),
        (Some(r), VarSlot::Local { index, .. }) => format!("var:{}.#{index}", slot_form(u, r).map_or_else(|| "?".into(), |f| format!("0x{f:08X}"))),
        (_, VarSlot::Global { global }) => format!("global:{}", slot_form(u, global).map_or_else(|| "?".into(), |f| format!("0x{f:08X}"))),
    }
}

struct Walk<'a> {
    data: &'a GameData,
    ref_base: &'a HashMap<u32, u32>,
    unit: &'a ScriptUnit,
    out: UnitFacts,
}

impl Walk<'_> {
    fn expr_reads(&mut self, e: &Expr, into: &mut Vec<String>) {
        for t in &e.0 {
            match t {
                Token::Call { call } => {
                    let name = lower_name(self.data, call.opcode);
                    if name == "getstage" || name == "getstagedone" {
                        if let Some(q) = call.args.first().and_then(|a| match arg_of(self.unit, a) {
                            P::Form(f) => Some(f),
                            _ => None,
                        }) {
                            let st = call.args.get(1).and_then(|a| match arg_of(self.unit, a) {
                                P::Num(n) => Some(n as i32),
                                _ => None,
                            });
                            self.out.stage_tests.push((q, st));
                        }
                    }
                    into.push(name);
                }
                Token::Var { var } => into.push(var_label(self.unit, var)),
                _ => {}
            }
        }
    }

    fn stmts(&mut self, body: &[Stmt], block: Option<&str>, block_args: &[P], guards: &[String]) {
        for s in body {
            match s {
                Stmt::Call { call } => {
                    let func = lower_name(self.data, call.opcode);
                    let args = call.args.iter().map(|a| arg_of(self.unit, a)).collect();
                    self.out.calls.push(CallFact { func, on: on_of(self.unit, call.on), args, block: block.map(str::to_string), block_args: block_args.to_vec(), guards: guards.to_vec() });
                }
                Stmt::Set { target, expr } => {
                    let mut reads = Vec::new();
                    self.expr_reads(expr, &mut reads);
                    if let (Some(r), VarSlot::Local { index, .. }) = (target.on, target.var) {
                        if let Some(f) = slot_form(self.unit, r) {
                            let var = script_var_name(self.data, self.ref_base, f, index).unwrap_or_else(|| format!("#{index}"));
                            self.out.writes.push(Write { form: f, var });
                        }
                    }
                    if reads.iter().any(|r| r == "getsecondspassed") {
                        let l = var_label(self.unit, target);
                        self.out.timers.insert(l);
                    }
                    // Calls inside the expression run too (`set x to SomeRef.GetAV …`): not
                    // statements that matter here.
                }
                Stmt::If { arms, otherwise } => {
                    let mut seen: Vec<String> = Vec::new();
                    for a in arms {
                        let mut g = guards.to_vec();
                        let mut reads = Vec::new();
                        self.expr_reads(&a.cond, &mut reads);
                        g.extend(seen.iter().cloned());
                        g.extend(reads.iter().cloned());
                        seen.extend(reads);
                        self.stmts(&a.body, block, block_args, &g);
                    }
                    if let Some(o) = otherwise {
                        let mut g = guards.to_vec();
                        g.extend(seen);
                        self.stmts(o, block, block_args, &g);
                    }
                }
                Stmt::While { body, .. } | Stmt::ForEach { body, .. } => self.stmts(body, block, block_args, guards),
                _ => {}
            }
        }
    }
}

/// The facts of one compiled unit.
pub fn unit_facts(data: &GameData, ref_base: &HashMap<u32, u32>, unit: &ScriptUnit, program: &Program) -> UnitFacts {
    let mut w = Walk { data, ref_base, unit, out: UnitFacts { key: unit.key(), owner: unit.owner.clone(), calls: Vec::new(), writes: Vec::new(), timers: BTreeSet::new(), stage_tests: Vec::new() } };
    w.stmts(&program.top, None, &[], &[]);
    for b in &program.blocks {
        let name = data.table.block_name(b.event).map_or_else(|| format!("event_{}", b.event), str::to_ascii_lowercase);
        let bargs: Vec<P> = b.params.iter().flatten().map(|a| arg_of(unit, a)).collect();
        w.stmts(&b.body, Some(&name), &bargs, &[]);
    }
    let mut out = w.out;
    // Dedupe the guard lists (long If chains repeat names).
    for c in &mut out.calls {
        let mut seen = BTreeSet::new();
        c.guards.retain(|g| seen.insert(g.clone()));
    }
    out
}

/// The name of variable `index` of the script `form` runs (a quest, a reference's base).
fn script_var_name(data: &GameData, ref_base: &HashMap<u32, u32>, form: u32, index: u16) -> Option<String> {
    let base = ref_base.get(&form).copied().unwrap_or(form);
    let script = data.form_script.get(&FormId(base))?;
    let u = data.scripts.get(script)?;
    data.units[*u].unit.var(index as u32).map(|v| v.name.to_ascii_lowercase())
}

fn cond_form(c: &Condition, i: usize) -> Option<u32> {
    match c.params.get(i)? {
        ArgValue::Value(VmValue::Ref(f)) => Some(*f),
        _ => None,
    }
}

fn cond_num(c: &Condition, i: usize) -> Option<f64> {
    match c.params.get(i)?.value()? {
        VmValue::Num(n) => Some(n),
        _ => None,
    }
}

/// Whether the comparison wants the function true (1) or false (0); `None` when it is
/// neither a plain test of 1 nor of 0.
fn wants(c: &Condition) -> Option<bool> {
    let fnv_game::cond::Operand::Value(v) = c.rhs else { return None };
    let t1 = c.cmp.test(1.0, v);
    let t0 = c.cmp.test(0.0, v);
    match (t1, t0) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

fn atom_of(data: &GameData, c: &Condition) -> Atom {
    if c.run_on != RunOn::Subject {
        return Atom::Unknown;
    }
    let name = lower_name(data, c.opcode());
    let Some(w) = wants(c) else { return Atom::Unknown };
    let list = |f: u32| data.form_lists.get(&FormId(f)).map_or_else(|| vec![f], |l| l.iter().map(|x| x.0).collect());
    match name.as_str() {
        "getisid" => cond_form(c, 0).map_or(Atom::Unknown, |f| Atom::Id(list(f), w)),
        "getisvoicetype" => cond_form(c, 0).map_or(Atom::Unknown, |f| Atom::Voice(list(f), w)),
        "getinfaction" => cond_form(c, 0).map_or(Atom::Unknown, |f| Atom::Faction(f, w)),
        "getisrace" => cond_form(c, 0).map_or(Atom::Unknown, |f| Atom::Race(f, w)),
        // GetIsSex Female == 1: param is the sex code (0 male, 1 female) [documented: GECK wiki GetIsSex].
        "getissex" => cond_num(c, 0).map_or(Atom::Unknown, |s| Atom::Female((s == 1.0) == w)),
        _ => Atom::Unknown,
    }
}

/// OR groups joined by AND [documented: GECK wiki Conditions, "OR" binds the condition
/// to the next one; scripts.md §6].
fn groups(data: &GameData, conds: &[Condition]) -> Vec<Vec<Atom>> {
    let mut out: Vec<Vec<Atom>> = Vec::new();
    let mut cur: Vec<Atom> = Vec::new();
    for c in conds {
        cur.push(atom_of(data, c));
        if !c.or {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn stage_tests_of(data: &GameData, conds: &[Condition], into: &mut BTreeSet<(u32, Option<i32>)>) {
    for c in conds {
        let n = lower_name(data, c.opcode());
        if n == "getstage" || n == "getstagedone" {
            if let Some(q) = cond_form(c, 0) {
                into.insert((q, cond_num(c, 1).map(|s| s as i32)));
            }
        }
    }
}

/// Draw the model from the game data, the placed references and (for the trigger check)
/// the navmeshes.
pub fn extract(data: &GameData, refs: &RefTable, nav: Option<&fnv_nav::NavRecords>) -> Facts {
    let ref_base: HashMap<u32, u32> = refs.refs.iter().map(|(id, r)| (id.0, r.base.0)).collect();
    let mut f = Facts::default();
    use rayon::prelude::*;
    f.units = data.units.par_iter().map(|c| unit_facts(data, &ref_base, &c.unit, &c.program)).collect();
    for (i, u) in f.units.iter().enumerate() {
        match &u.owner {
            UnitOwner::Script { form } => {
                f.script_unit.insert(*form, i);
            }
            UnitOwner::QuestStage { form, stage, .. } => f.stage_units.entry((*form, *stage as i32)).or_default().push(i),
            _ => {}
        }
    }
    // Keep the units GameData's maps point to (the winning records'); the script map wins.
    for (s, u) in &data.scripts {
        f.script_unit.insert(s.0, *u);
    }
    for (form, s) in &data.form_script {
        f.form_script.insert(form.0, s.0);
    }
    for (id, l) in &data.form_lists {
        f.form_lists.insert(id.0, l.iter().map(|x| x.0).collect());
    }
    for (id, e) in &data.edid_of {
        f.edids.insert(id.0, e.clone());
    }
    for (id, s) in &data.sigs {
        f.sigs.insert(id.0, String::from_utf8_lossy(s).into_owned());
    }
    f.greeting = data.greeting.map(|x| x.0);
    f.goodbye = data.goodbye.map(|x| x.0);
    let top: BTreeSet<u32> = data.top_level.iter().map(|x| x.0).collect();
    for (t, def) in &data.topic_defs {
        f.topics.insert(
            t.0,
            TopicFacts {
                edid: def.edid.clone(),
                topic_type: def.topic_type,
                // A topic with shared lines (INFC / INFX) says them too: the "Let's talk about
                // something else." topics have nothing else.
                lines: data.shared_lines.get(t).or_else(|| data.topics.get(t)).map(|v| v.iter().map(|x| x.0).collect()).unwrap_or_default(),
                top_level: top.contains(&t.0),
                linked_from: data.linked_from.get(t).map(|v| v.iter().map(|x| x.0).collect()).unwrap_or_default(),
            },
        );
    }
    for (id, d) in &data.infos {
        let mut who = groups(data, &d.conditions);
        if let Some(s) = d.speaker {
            who.push(vec![Atom::Id(vec![s.0], true)]);
        }
        // The quest's dialogue conditions hold for every line of it [documented: GECK wiki
        // Quest Data Tab; docs/spec-notes/dialogue.md §4].
        if let Some(q) = d.quest.and_then(|q| data.quests.get(&q)) {
            who.extend(groups(data, &q.conditions));
        }
        f.lines.insert(
            id.0,
            LineFacts {
                topic: d.topic.map_or(0, |t| t.0),
                quest: d.quest.map(|q| q.0),
                goodbye: d.goodbye,
                info_type: d.info_type,
                choices: d.choices.iter().map(|x| x.0).collect(),
                link_from: d.link_from.iter().map(|x| x.0).collect(),
                who,
                prompt: d.prompt.clone(),
                text: d.responses.first().map(|r| r.text.clone()),
            },
        );
        let units: Vec<usize> = [d.begin, d.end].into_iter().flatten().collect();
        if !units.is_empty() {
            f.line_units.insert(id.0, units);
        }
        stage_tests_of(data, &d.conditions, &mut f.condition_stage_tests);
    }
    for (id, a) in &data.actors {
        f.actors.insert(
            id.0,
            ActorFacts {
                edid: data.edid_of.get(id).cloned(),
                voice: a.voice.map(|x| x.0),
                female: a.female,
                race: a.race.map(|x| x.0),
                factions: a.factions.iter().filter(|(_, r)| *r >= 0).map(|(x, _)| x.0).collect(),
                factions_templated: a.template.is_some() && a.template_factions,
                placed: false,
            },
        );
        f.actor_packages.insert(id.0, a.packages.iter().map(|x| x.0).collect());
    }
    for (id, p) in &data.package_defs {
        let units: Vec<usize> = data.packages.get(id).map(|u| u.iter().flatten().copied().collect()).unwrap_or_default();
        f.packages.insert(id.0, PackageFacts { edid: data.edid_of.get(id).cloned(), topic: p.dialogue.and_then(|d| d.topic).map(|x| x.0), units });
        stage_tests_of(data, &p.conditions, &mut f.condition_stage_tests);
    }
    for (q, def) in &data.quests {
        f.quest_stages.insert(q.0, def.stages.keys().copied().collect());
        stage_tests_of(data, &def.conditions, &mut f.condition_stage_tests);
        for entries in def.stages.values() {
            for e in entries {
                stage_tests_of(data, &e.conditions, &mut f.condition_stage_tests);
            }
        }
        for o in def.objectives.values() {
            for t in &o.targets {
                stage_tests_of(data, &t.conditions, &mut f.condition_stage_tests);
            }
        }
    }
    for (t, def) in &data.terminals {
        for (i, it) in def.items.iter().enumerate() {
            f.terminal_items.insert((t.0, i as u32), it.text.clone());
            stage_tests_of(data, &it.conditions, &mut f.condition_stage_tests);
        }
    }
    // Placed references: doors, actors, and the references of scripted bases.
    let scripted: BTreeSet<u32> = f.form_script.keys().copied().collect();
    for (id, r) in &refs.refs {
        let base = r.base.0;
        let sig = f.sigs.get(&base).cloned().unwrap_or_default();
        if let Some(a) = f.actors.get_mut(&base) {
            a.placed = true;
        }
        if !(sig == "DOOR" || scripted.contains(&base) || r.primitive.is_some()) {
            continue;
        }
        f.refs.insert(
            id.0,
            RefFacts {
                edid: r.edid.clone(),
                base,
                base_sig: sig,
                cell: r.cell.map(|c| c.0),
                pos: r.pos,
                disabled: r.initially_disabled(),
                enable_parent: r.enable_parent.map(|e| e.reference.0),
                lock: r.lock.map(|l| (l.level, l.key.map(|k| k.0))),
                teleport: r.teleport.is_some(),
                radius: r.primitive.map(|p| if p.sphere { p.bounds[0] } else { p.bounds[0].max(p.bounds[1]) }),
                nav_near: None,
            },
        );
    }
    // Navmesh near the triggers whose scripts have OnTrigger blocks.
    if let Some(nav) = nav {
        let trigger_scripts: BTreeSet<u32> = f
            .script_unit
            .iter()
            .filter(|(_, u)| f.units[**u].calls.iter().any(|c| c.block.as_deref().is_some_and(is_trigger_event)))
            .map(|(s, _)| *s)
            .collect();
        let near = NavNear::new(nav, refs);
        let fs = f.form_script.clone();
        for r in f.refs.values_mut() {
            if fs.get(&r.base).is_some_and(|s| trigger_scripts.contains(s)) {
                r.nav_near = Some(near.near(r.cell, r.pos, r.radius.unwrap_or(64.0)));
            }
        }
    }
    f
}

fn is_trigger_event(e: &str) -> bool {
    matches!(e, "ontrigger" | "ontriggerenter" | "ontriggerleave")
}

fn is_package_event(e: &str) -> bool {
    matches!(e, "onpackagestart" | "onpackageend" | "onpackagedone" | "onpackagechange")
}

/// Navmesh triangles by cell, for "is there a navmesh near this point".
struct NavNear<'a> {
    nav: &'a fnv_nav::NavRecords,
    by_cell: HashMap<u32, Vec<u32>>,
    /// Exterior cell → (worldspace, grid).
    grid_of: HashMap<u32, (u32, (i32, i32))>,
    grid: HashMap<(u32, (i32, i32)), u32>,
}

impl<'a> NavNear<'a> {
    fn new(nav: &'a fnv_nav::NavRecords, refs: &RefTable) -> Self {
        let mut by_cell: HashMap<u32, Vec<u32>> = HashMap::new();
        for (id, m) in &nav.navmeshes {
            if let Some(c) = m.cell.or_else(|| nav.stored_in.get(id).copied()) {
                by_cell.entry(c).or_default().push(*id);
            }
        }
        let mut grid_of = HashMap::new();
        let mut grid = HashMap::new();
        for (w, cells) in &refs.world_grid {
            for (xy, c) in cells {
                grid_of.insert(c.0, (w.0, *xy));
                grid.insert((w.0, *xy), c.0);
            }
        }
        NavNear { nav, by_cell, grid_of, grid }
    }

    fn near(&self, cell: Option<u32>, p: [f32; 3], radius: f32) -> bool {
        let Some(cell) = cell else { return false };
        let mut cells = vec![cell];
        if let Some((w, _)) = self.grid_of.get(&cell) {
            let gx = (p[0] / 4096.0).floor() as i32;
            let gy = (p[1] / 4096.0).floor() as i32;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(c) = self.grid.get(&(*w, (gx + dx, gy + dy))) {
                        cells.push(*c);
                    }
                }
            }
        }
        let reach = radius + 128.0;
        for c in cells {
            for m in self.by_cell.get(&c).into_iter().flatten() {
                let m = &self.nav.navmeshes[m];
                for t in &m.triangles {
                    let v = t.v.map(|i| m.vertices.get(i as usize).copied().unwrap_or([f32::MAX; 3]));
                    let zmin = v.iter().map(|x| x[2]).fold(f32::MAX, f32::min);
                    let zmax = v.iter().map(|x| x[2]).fold(f32::MIN, f32::max);
                    if p[2] + reach + 256.0 < zmin || p[2] - reach - 256.0 > zmax {
                        continue;
                    }
                    if dist2d_triangle(p, &v) <= reach {
                        return true;
                    }
                }
            }
        }
        false
    }
}

fn dist2d_triangle(p: [f32; 3], v: &[[f32; 3]; 3]) -> f32 {
    let s = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let d1 = s(v[0], v[1], p);
    let d2 = s(v[1], v[2], p);
    let d3 = s(v[2], v[0], p);
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    if !(neg && pos) {
        return 0.0;
    }
    let seg = |a: [f32; 3], b: [f32; 3]| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx * dx + dy * dy;
        let t = if l == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l).clamp(0.0, 1.0) };
        let (x, y) = (a[0] + t * dx - p[0], a[1] + t * dy - p[1]);
        (x * x + y * y).sqrt()
    };
    seg(v[0], v[1]).min(seg(v[1], v[2])).min(seg(v[2], v[0]))
}

// ------------------------------------------------------------------------- speakers

/// Whether an atom can pass for `actor` (`None`: no actor known, everything may pass).
fn atom_may(f: &Facts, a: &Atom, actor: Option<u32>) -> bool {
    let Some(id) = actor else { return true };
    let act = f.actors.get(&id);
    match a {
        Atom::Id(ids, w) => ids.contains(&id) == *w,
        Atom::Voice(vs, w) => act.and_then(|x| x.voice).is_none_or(|v| vs.contains(&v) == *w),
        Atom::Faction(fa, w) => act.is_none_or(|x| x.factions_templated || x.factions.contains(fa) == *w),
        Atom::Female(fe) => act.is_none_or(|x| x.female == *fe),
        Atom::Race(r, w) => act.and_then(|x| x.race).is_none_or(|rr| (rr == *r) == *w),
        Atom::Unknown => true,
    }
}

/// Whether `actor` may say the line, every condition the sweep cannot decide passing.
fn may_say(f: &Facts, l: &LineFacts, actor: Option<u32>) -> bool {
    l.who.iter().all(|g| g.iter().any(|a| atom_may(f, a, actor)))
}

/// [`may_say`] with every condition the sweep cannot decide failing.
fn may_say_strict(f: &Facts, l: &LineFacts, actor: u32) -> bool {
    l.who.iter().all(|g| g.iter().any(|a| !matches!(a, Atom::Unknown) && atom_may(f, a, Some(actor))))
}

/// The speakers a line is written for: a group that is only GetIsID == 1 tests.
fn named_speakers(l: &LineFacts) -> Option<Vec<u32>> {
    for g in &l.who {
        if !g.is_empty() && g.iter().all(|a| matches!(a, Atom::Id(_, true))) {
            let mut v: Vec<u32> = g.iter().flat_map(|a| if let Atom::Id(ids, _) = a { ids.clone() } else { vec![] }).collect();
            v.sort_unstable();
            v.dedup();
            return Some(v);
        }
    }
    None
}

// ------------------------------------------------------------------------- 1. conversations

/// The lines a line can lead to for the speaker: its choices' lines, then the lines that
/// name its topic in their Link From (`Game::links_after`).
fn next_lines(f: &Facts, l: &LineFacts, speaker: Option<u32>) -> Vec<u32> {
    let mut out = Vec::new();
    for t in &l.choices {
        for i in f.topics.get(t).map(|t| t.lines.as_slice()).unwrap_or_default() {
            if f.lines.get(i).is_some_and(|x| may_say(f, x, speaker)) {
                out.push(*i);
            }
        }
    }
    for t in f.topics.get(&l.topic).map(|t| t.linked_from.as_slice()).unwrap_or_default() {
        if l.choices.contains(t) {
            continue;
        }
        for i in f.topics.get(t).map(|t| t.lines.as_slice()).unwrap_or_default() {
            if f.lines.get(i).is_some_and(|x| x.link_from.contains(&l.topic) && may_say(f, x, speaker)) {
                out.push(*i);
            }
        }
    }
    out
}

fn is_player_line(f: &Facts, l: &LineFacts) -> bool {
    l.info_type == topic_type::TOPIC || Some(l.topic) == f.greeting
}

/// Check 1a: lines with choices from which no sequence of choices reaches a line that
/// ends the conversation (Goodbye) or hands back to the topic list (a line with nothing
/// to choose). Every condition the sweep cannot decide counts as passing, so a hit is a
/// conversation with no way out even when every line is available.
pub fn conversation_traps(f: &Facts) -> Vec<Value> {
    let mut out = Vec::new();
    for (id, l) in &f.lines {
        if !is_player_line(f, l) || l.goodbye || (l.choices.is_empty() && f.topics.get(&l.topic).is_none_or(|t| t.linked_from.is_empty())) {
            continue;
        }
        let speakers = named_speakers(l).unwrap_or_default();
        let speaker = speakers.first().copied();
        if next_lines(f, l, speaker).is_empty() {
            continue;
        }
        let mut seen = BTreeSet::from([*id]);
        let mut q = VecDeque::from([*id]);
        let mut exit = false;
        while let Some(x) = q.pop_front() {
            let lx = &f.lines[&x];
            if lx.goodbye {
                exit = true;
                break;
            }
            let n = next_lines(f, lx, speaker);
            if n.is_empty() {
                exit = true;
                break;
            }
            for y in n {
                if seen.insert(y) {
                    q.push_back(y);
                }
            }
        }
        if !exit {
            out.push(json!({
                "line": format!("0x{id:08X}"),
                "topic": f.name(l.topic),
                "quest": l.quest.map(|q| f.name(q)),
                "speakers": speakers.iter().map(|s| f.name(*s)).collect::<Vec<_>>(),
                "text": l.text,
                "lines_reachable": seen.len(),
            }));
        }
    }
    out
}

/// Check 1b: actors with lines of their own at the top level or in GREETING but no
/// Goodbye they may say there: neither a GOODBYE line nor a top-level line with the
/// Goodbye flag (#1595). NVForever lets the player leave a topic list anyway (Tab).
pub fn goodbye_less_top_levels(f: &Facts) -> Vec<Value> {
    // Lines naming each actor, top level or GREETING.
    let mut own: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut exits: Vec<u32> = Vec::new();
    for (id, l) in &f.lines {
        let t = f.topics.get(&l.topic);
        let top = t.is_some_and(|t| t.top_level && t.topic_type == topic_type::TOPIC);
        let greet = Some(l.topic) == f.greeting;
        if Some(l.topic) == f.goodbye || (top && l.goodbye) {
            exits.push(*id);
        }
        if top || greet {
            for s in named_speakers(l).unwrap_or_default() {
                own.entry(s).or_default().push(*id);
            }
        }
    }
    let mut out = Vec::new();
    for (actor, lines) in own {
        if f.actors.get(&actor).is_none_or(|a| !a.placed) {
            continue;
        }
        // An actor whose every greeting is a Goodbye line never opens the menu.
        let greets: Vec<&u32> = lines.iter().filter(|i| Some(f.lines[i].topic) == f.greeting).collect();
        let tops = lines.len() - greets.len();
        if tops == 0 {
            continue;
        }
        // An exit counts when the sweep can show the actor may say it: every condition
        // it cannot decide fails here. Those it may say only if such a condition passes
        // are listed with the hit.
        if exits.iter().any(|e| may_say_strict(f, &f.lines[e], actor)) {
            continue;
        }
        let maybe: Vec<String> = exits.iter().filter(|e| may_say(f, &f.lines[e], Some(actor))).take(3).map(|e| format!("0x{e:08X}")).collect();
        out.push(json!({
            "actor": f.name(actor),
            "top_level_lines": tops,
            "greeting_lines": greets.len(),
            "exits_behind_undecided_conditions": maybe,
        }));
    }
    out
}

// ------------------------------------------------------------------------- 2. controls

fn is_true_num(p: Option<&P>) -> Option<bool> {
    match p {
        Some(P::Num(n)) => Some(*n != 0.0),
        _ => None,
    }
}

/// Whether a call takes the player's movement away.
pub fn takes_movement(c: &CallFact) -> bool {
    match c.func.as_str() {
        // No parameters: everything; else the first is movement [documented: GECK wiki
        // DisablePlayerControls, "movement" the first flag, all default to 1 when omitted].
        "disableplayercontrols" => c.args.is_empty() || is_true_num(c.args.first()).unwrap_or(true),
        "setrestrained" => c.on == On::Form(PLAYER) && is_true_num(c.args.first()).unwrap_or(false),
        _ => false,
    }
}

/// Whether a call gives movement back.
pub fn gives_movement(c: &CallFact) -> bool {
    match c.func.as_str() {
        "enableplayercontrols" => c.args.is_empty() || is_true_num(c.args.first()).unwrap_or(true),
        "setrestrained" => c.on == On::Form(PLAYER) && is_true_num(c.args.first()) == Some(false),
        _ => false,
    }
}

/// What a release (or a setter) waits for: the owner kind, the block, the conditions.
fn wait_tags(f: &Facts, ui: usize, c: &CallFact) -> BTreeSet<&'static str> {
    let u = &f.units[ui];
    let mut t = BTreeSet::new();
    match &u.owner {
        UnitOwner::Package { .. } => {
            t.insert("package");
        }
        UnitOwner::Info { .. } => {
            t.insert("dialogue line");
        }
        UnitOwner::Terminal { .. } => {
            t.insert("terminal");
        }
        UnitOwner::QuestStage { .. } => {
            t.insert("stage");
        }
        _ => {}
    }
    if let Some(b) = c.block.as_deref() {
        if is_trigger_event(b) {
            t.insert("trigger");
        } else if is_package_event(b) {
            t.insert("package");
        } else if b == "onactivate" {
            t.insert("activation");
        } else if matches!(b, "ondeath" | "ondying" | "onhit" | "oncombatend" | "onstartcombat") {
            t.insert("actor event");
        } else if b == "ontriggermob" || b == "onload" {
            t.insert("load");
        }
    }
    for g in &c.guards {
        match g.as_str() {
            "getcurrentaipackage" | "getiscurrentpackage" | "getcurrentaiprocedure" | "getisprocedure" => {
                t.insert("package");
            }
            "istalking" | "gettalkedtopc" | "gettalkedtopcparam" | "sayto" | "isinconversation" => {
                t.insert("dialogue");
            }
            "getdistance" | "getinsamecell" | "getincell" | "getinworldspace" | "isincelloradjacent" | "getinzone" | "isintrigger" => {
                t.insert("position");
            }
            "getstage" | "getstagedone" | "getquestrunning" | "getquestcompleted" | "getobjectivecompleted" => {
                t.insert("stage");
            }
            "getbuttonpressed" => {
                t.insert("message box");
            }
            "isactionref" => {
                t.insert("who triggers");
            }
            "getdead" | "getdisabled" | "getunconscious" | "isincombat" | "getknockedstate" => {
                t.insert("actor state");
            }
            "gamedayspassed" | "getcurrenttime" | "gamehour" => {
                t.insert("time of day");
            }
            _ => {
                if u.timers.contains(g) {
                    t.insert("timer");
                } else if g.starts_with("var:0x") {
                    t.insert("another script's variable");
                }
            }
        }
    }
    t
}

/// The tags that mean "released only when something else happens" (#2365 item 5).
const RISKY: &[&str] = &["package", "dialogue line", "dialogue", "trigger", "timer", "position", "actor state", "time of day", "who triggers"];

/// The units a unit sets going, with how: stages it sets, scripts whose variables it
/// writes, packages it changes, topics it has said, a line's choices.
fn successors(f: &Facts, ui: usize) -> Vec<(usize, String)> {
    let u = &f.units[ui];
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut push = |i: usize, how: String| {
        if i != ui && !out.iter().any(|(j, _)| *j == i) {
            out.push((i, how));
        }
    };
    for w in &u.writes {
        if let Some(s) = f.script_of(w.form) {
            push(s, format!("sets {}.{}", f.name(w.form), w.var));
        }
    }
    // The result scripts of the lines of topic `t` that `speaker` (a reference or a base)
    // may say: a package or SayTo on GREETING reaches only the actor's own lines.
    let lines_of_by = |t: u32, speaker: Option<u32>| -> Vec<usize> {
        let base = speaker.map(|r| f.refs.get(&r).map_or(r, |x| x.base));
        f.topics
            .get(&t)
            .map(|t| {
                t.lines
                    .iter()
                    .filter(|i| base.is_none() || f.lines.get(i).is_some_and(|l| named_speakers(l).is_none_or(|s| s.contains(&base.unwrap_or(0))) && may_say(f, l, base)))
                    .flat_map(|i| f.line_units.get(i).cloned().unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default()
    };
    let lines_of = |t: u32| lines_of_by(t, None);
    let speaker_of = |on: &On| if let On::Form(r) = on { Some(*r) } else { None };
    for c in &u.calls {
        let form0 = c.args.iter().find_map(|a| if let P::Form(x) = a { Some(*x) } else { None });
        match c.func.as_str() {
            "setstage" => {
                if let (Some(P::Form(q)), Some(P::Num(n))) = (c.args.first(), c.args.get(1)) {
                    for s in f.stage_units.get(&(*q, *n as i32)).into_iter().flatten() {
                        push(*s, format!("sets stage {} {}", f.name(*q), n));
                    }
                    if let Some(s) = f.script_of(*q) {
                        push(s, format!("sets stage {} {} (quest script)", f.name(*q), n));
                    }
                }
            }
            "startquest" => {
                if let Some(q) = form0 {
                    if let Some(s) = f.script_of(q) {
                        push(s, format!("starts {}", f.name(q)));
                    }
                }
            }
            "addscriptpackage" => {
                if let Some(p) = form0.and_then(|p| f.packages.get(&p).map(|x| (p, x))) {
                    for s in &p.1.units {
                        push(*s, format!("gives package {}", f.name(p.0)));
                    }
                    if let Some(t) = p.1.topic {
                        for s in lines_of_by(t, speaker_of(&c.on)) {
                            push(s, format!("gives package {} (says {})", f.name(p.0), f.name(t)));
                        }
                    }
                }
            }
            "evaluatepackage" => {
                if let On::Form(r) = &c.on {
                    let base = f.refs.get(r).map_or(*r, |x| x.base);
                    for p in f.actor_packages.get(&base).into_iter().flatten() {
                        if let Some(pf) = f.packages.get(p) {
                            for s in &pf.units {
                                push(*s, format!("re-evaluates {}'s package {}", f.name(*r), f.name(*p)));
                            }
                            if let Some(t) = pf.topic {
                                for s in lines_of_by(t, Some(*r)) {
                                    push(s, format!("re-evaluates {} (package {} says {})", f.name(*r), f.name(*p), f.name(t)));
                                }
                            }
                        }
                    }
                }
            }
            "sayto" | "say" | "startconversation" => {
                for a in &c.args {
                    if let P::Form(t) = a {
                        if f.topics.contains_key(t) {
                            for s in lines_of_by(*t, speaker_of(&c.on)) {
                                push(s, format!("{} {}", c.func, f.name(*t)));
                            }
                        }
                    }
                }
            }
            "activate" | "enable" | "moveto" => {
                if let On::Form(r) = &c.on {
                    if let Some(s) = f.script_of(*r) {
                        push(s, format!("{} {}", c.func, f.name(*r)));
                    }
                }
            }
            _ => {}
        }
    }
    if let UnitOwner::Info { form, .. } = &u.owner {
        if let Some(l) = f.lines.get(form) {
            for t in &l.choices {
                for s in lines_of(*t) {
                    push(s, format!("choice {}", f.name(*t)));
                }
            }
        }
    }
    if let UnitOwner::QuestStage { form, .. } = &u.owner {
        if let Some(s) = f.script_of(*form) {
            push(s, format!("quest script of {}", f.name(*form)));
        }
    }
    out
}

/// Describe a unit for a person: what kind of thing it is and what runs it.
fn describe(f: &Facts, ui: usize) -> Value {
    let u = &f.units[ui];
    match &u.owner {
        UnitOwner::Info { form, end } => {
            let l = f.lines.get(form);
            let speakers = l.and_then(named_speakers).unwrap_or_default();
            json!({"kind": "dialogue line", "line": format!("0x{form:08X}"), "at": if *end { "end" } else { "begin" },
                "topic": l.map(|l| f.name(l.topic)), "speakers": speakers.iter().map(|s| f.name(*s)).collect::<Vec<_>>(),
                "talking_activator": speakers.iter().any(|s| f.sigs.get(s).is_some_and(|x| x == "TACT"))})
        }
        UnitOwner::Terminal { form, item } => json!({"kind": "terminal", "terminal": f.name(*form), "item": item, "text": f.terminal_items.get(&(*form, *item))}),
        UnitOwner::QuestStage { form, stage, .. } => json!({"kind": "quest stage", "quest": f.name(*form), "stage": stage}),
        UnitOwner::Package { form, event } => json!({"kind": "package", "package": f.name(*form), "event": match event { PackageEvent::Begin => "begin", PackageEvent::End => "end", PackageEvent::Change => "change" }}),
        UnitOwner::Script { form } => {
            let (refs, owners) = f.users_of_script(*form);
            let quests: Vec<String> = owners.iter().filter(|o| f.quest_stages.contains_key(o)).map(|o| f.name(*o)).collect();
            let kind = if !quests.is_empty() { "quest script" } else { "object script" };
            json!({"kind": kind, "script": f.name(*form), "quests": quests,
                "bases": owners.iter().filter(|o| !f.quest_stages.contains_key(o)).take(4).map(|o| format!("{} {}", f.sigs.get(o).map_or("", String::as_str), f.name(*o))).collect::<Vec<_>>(),
                "placed": refs.len()})
        }
        o => json!({"kind": o.sig(), "unit": u.key}),
    }
}

/// Check 2: every call that takes movement away, and the releases reached from it.
pub fn control_holds(f: &Facts, depth: usize) -> (Vec<Value>, BTreeMap<String, usize>) {
    let mut out = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (ui, u) in f.units.iter().enumerate() {
        let takes: Vec<&CallFact> = u.calls.iter().filter(|c| takes_movement(c)).collect();
        if takes.is_empty() {
            continue;
        }
        // Breadth-first over the units this one sets going.
        // In the order reached: the nearest releases first.
        let mut seen: Vec<(usize, Vec<String>)> = vec![(ui, Vec::new())];
        let mut known: BTreeSet<usize> = BTreeSet::from([ui]);
        let mut at = 0;
        while at < seen.len() {
            let (x, path) = seen[at].clone();
            at += 1;
            if path.len() >= depth {
                continue;
            }
            for (y, how) in successors(f, x) {
                if known.insert(y) {
                    let mut p = path.clone();
                    p.push(how);
                    seen.push((y, p));
                }
            }
        }
        let mut releases = Vec::new();
        let mut verdict = "unreleased";
        for (y, path) in &seen {
            for c in f.units[*y].calls.iter().filter(|c| gives_movement(c)) {
                let mut tags = wait_tags(f, *y, c);
                if *y != ui {
                    tags.extend(wait_tags_of_path(path));
                }
                let risky: Vec<&str> = tags.iter().copied().filter(|t| RISKY.contains(t)).collect();
                let same_block = *y == ui && takes.iter().any(|t| t.block == c.block);
                let v = if same_block && risky.is_empty() { "in place" } else if risky.is_empty() { "released" } else { "waits" };
                verdict = match (verdict, v) {
                    (_, "in place") | ("in place", _) => "in place",
                    (_, "released") | ("released", _) => "released",
                    _ => "waits",
                };
                let r = json!({"unit": f.units[*y].key, "by": describe(f, *y), "block": c.block, "path": path, "waits_for": tags.iter().collect::<Vec<_>>()});
                if !releases.contains(&r) {
                    releases.push(r);
                }
            }
        }
        *counts.entry(verdict.to_string()).or_default() += 1;
        if verdict == "in place" {
            continue;
        }
        releases.truncate(6);
        out.push(json!({
            "unit": u.key,
            "by": describe(f, ui),
            "takes": takes.iter().map(|c| json!({"call": c.func, "block": c.block, "guards": c.guards})).collect::<Vec<_>>(),
            "verdict": verdict,
            "releases": releases,
        }));
    }
    (out, counts)
}

fn wait_tags_of_path(path: &[String]) -> BTreeSet<&'static str> {
    let mut t = BTreeSet::new();
    for p in path {
        if p.starts_with("gives package") || p.starts_with("re-evaluates") {
            t.insert("package");
        }
        if p.starts_with("sayto") || p.starts_with("say ") || p.starts_with("startconversation") || p.starts_with("choice") || p.contains(" says ") {
            t.insert("dialogue");
        }
    }
    t
}

// ------------------------------------------------------------------------- 3. stages

/// Every SetStage with a quest and a stage written out: (quest, stage) → (unit, call).
/// (quest, stage) → the (unit, call) pairs that set it.
type Setters = BTreeMap<(u32, i32), Vec<(usize, usize)>>;

fn stage_setters(f: &Facts) -> (Setters, usize) {
    let mut out: Setters = BTreeMap::new();
    let mut dynamic = 0;
    for (ui, u) in f.units.iter().enumerate() {
        for (ci, c) in u.calls.iter().enumerate() {
            if c.func != "setstage" {
                continue;
            }
            match (c.args.first(), c.args.get(1)) {
                (Some(P::Form(q)), Some(P::Num(n))) => out.entry((*q, *n as i32)).or_default().push((ui, ci)),
                _ => dynamic += 1,
            }
        }
    }
    (out, dynamic)
}

/// What kind of setter a SetStage call is, for the stage check.
fn setter_kind(f: &Facts, ui: usize, c: &CallFact, enablers: &BTreeMap<u32, Vec<usize>>) -> (String, Value) {
    let u = &f.units[ui];
    if matches!(u.owner, UnitOwner::Package { .. }) || c.block.as_deref().is_some_and(is_package_event) {
        return ("package".into(), describe(f, ui));
    }
    if let UnitOwner::Script { form } = u.owner {
        let (refs, owners) = f.users_of_script(form);
        let quest = owners.iter().any(|o| f.quest_stages.contains_key(o));
        if !quest {
            if c.block.as_deref().is_some_and(is_trigger_event) {
                if refs.is_empty() {
                    return ("trigger never placed".into(), describe(f, ui));
                }
                let walkable: Vec<&u32> = refs.iter().filter(|r| f.refs[r].nav_near != Some(false)).collect();
                if walkable.is_empty() {
                    let mut d = describe(f, ui);
                    d["triggers"] = json!(refs.iter().take(4).map(|r| f.name(*r)).collect::<Vec<_>>());
                    return ("trigger without navmesh".into(), d);
                }
            } else if c.block.as_deref() == Some("onactivate") && !refs.is_empty() && refs.iter().all(|r| f.refs[r].disabled) {
                let mut d = describe(f, ui);
                d["activators"] = json!(refs.iter().take(4).map(|r| {
                    let en: Vec<String> = enablers.get(r).into_iter().flatten().take(3).map(|e| f.units[*e].key.clone()).collect();
                    json!({"ref": f.name(*r), "enable_parent": f.refs[r].enable_parent.map(|p| f.name(p)), "enabled_by": en})
                }).collect::<Vec<_>>());
                return ("disabled activator".into(), d);
            }
        }
    }
    let k = match &u.owner {
        UnitOwner::Info { .. } => "dialogue line",
        UnitOwner::Terminal { .. } => "terminal",
        UnitOwner::QuestStage { .. } => "quest stage",
        UnitOwner::Script { .. } => {
            if f.quest_script_units().contains(&ui) {
                "quest script"
            } else {
                "object script"
            }
        }
        _ => "other",
    };
    (k.into(), describe(f, ui))
}

const HARD_SETTERS: &[&str] = &["package", "trigger without navmesh", "trigger never placed", "disabled activator"];

/// Check 3: stages something tests whose only setters are hard for a player to bring
/// about, and stages tested but never set by any script. A hypothesis list.
pub fn stage_reach(f: &Facts) -> (Vec<Value>, Value) {
    let (setters, dynamic) = stage_setters(f);
    let mut tested: BTreeSet<(u32, Option<i32>)> = f.condition_stage_tests.clone();
    for u in &f.units {
        tested.extend(u.stage_tests.iter().copied());
    }
    let tested_stage = |q: u32, s: i32| tested.contains(&(q, Some(s))) || tested.contains(&(q, None));
    // Quests a SetStage with a stage not written out may set.
    let mut enablers: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (ui, u) in f.units.iter().enumerate() {
        for c in &u.calls {
            if c.func == "enable" {
                if let On::Form(r) = &c.on {
                    enablers.entry(*r).or_default().push(ui);
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut checked = 0;
    for ((q, s), list) in &setters {
        if !tested_stage(*q, *s) {
            continue;
        }
        checked += 1;
        let kinds: Vec<(String, Value)> = list.iter().map(|(ui, ci)| setter_kind(f, *ui, &f.units[*ui].calls[*ci], &enablers)).collect();
        if kinds.iter().all(|(k, _)| HARD_SETTERS.contains(&k.as_str())) {
            let mut ks: Vec<&str> = kinds.iter().map(|(k, _)| k.as_str()).collect();
            ks.sort_unstable();
            ks.dedup();
            for k in &ks {
                *by_kind.entry(k.to_string()).or_default() += 1;
            }
            out.push(json!({"quest": f.name(*q), "stage": s, "only_set_by": ks, "setters": kinds.iter().take(4).map(|(_, d)| d.clone()).collect::<Vec<_>>()}));
        }
    }
    // Tested, defined in the quest, and set by nothing.
    let mut never = 0;
    for (q, stages) in &f.quest_stages {
        for s in stages {
            if tested.contains(&(*q, Some(*s))) && !setters.contains_key(&(*q, *s)) {
                never += 1;
                *by_kind.entry("no setter".into()).or_default() += 1;
                out.push(json!({"quest": f.name(*q), "stage": s, "only_set_by": ["no setter"], "setters": []}));
            }
        }
    }
    (out, json!({"stages_set_and_tested": checked, "setstage_with_computed_arguments": dynamic, "tested_never_set": never, "by_kind": by_kind}))
}

// ------------------------------------------------------------------------- 4. doors

/// Level 255: only the key opens the lock [documented: GECK wiki GetLockLevel;
/// fnv_game::lockpick::KEY_ONLY].
pub const KEY_ONLY: u8 = 255;

/// Check 4: doors locked at Requires Key with no key, and the scripts that open them.
pub fn script_doors(f: &Facts) -> (Vec<Value>, Value) {
    let mut opens: BTreeMap<u32, Vec<(usize, usize)>> = BTreeMap::new();
    for (ui, u) in f.units.iter().enumerate() {
        for (ci, c) in u.calls.iter().enumerate() {
            let opener = c.func == "unlock" || (c.func == "lock" && matches!(c.args.first(), Some(P::Num(n)) if *n == 0.0)) || (c.func == "setopenstate" && matches!(c.args.first(), Some(P::Num(n)) if *n != 0.0));
            if !opener {
                continue;
            }
            match &c.on {
                On::Form(r) => opens.entry(*r).or_default().push((ui, ci)),
                On::Implicit => {
                    if let UnitOwner::Script { form } = u.owner {
                        for r in f.users_of_script(form).0 {
                            opens.entry(r).or_default().push((ui, ci));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    let (mut keyless, mut by_script, mut never) = (0, 0, 0);
    for (id, r) in &f.refs {
        if r.base_sig != "DOOR" || r.lock.is_none_or(|(lv, key)| lv != KEY_ONLY || key.is_some()) {
            continue;
        }
        keyless += 1;
        let Some(list) = opens.get(id) else {
            never += 1;
            continue;
        };
        by_script += 1;
        out.push(json!({
            "door": f.name(*id),
            "cell": r.cell.map(|c| f.name(c)),
            "load_door": r.teleport,
            "opened_by": list.iter().take(6).map(|(ui, ci)| {
                let c = &f.units[*ui].calls[*ci];
                let mut d = describe(f, *ui);
                d["call"] = json!(c.func);
                d["block"] = json!(c.block);
                d
            }).collect::<Vec<_>>(),
        }));
    }
    (out, json!({"key_only_without_key": keyless, "opened_by_script": by_script, "never_opened": never}))
}

// ------------------------------------------------------------------------- the command

pub fn sweep(f: &Facts, depth: usize) -> Value {
    let traps = conversation_traps(f);
    let goodbye = goodbye_less_top_levels(f);
    let (controls, control_counts) = control_holds(f, depth);
    let (stages, stage_summary) = stage_reach(f);
    let (doors, door_summary) = script_doors(f);
    json!({
        "summary": {
            "conversation_traps": traps.len(),
            "goodbye_less_top_levels": goodbye.len(),
            "control_holds": control_counts.values().sum::<usize>(),
            "control_verdicts": control_counts,
            "controls_waiting": controls.iter().filter(|c| c["verdict"] == "waits").count(),
            "controls_unreleased": controls.iter().filter(|c| c["verdict"] == "unreleased").count(),
            // A fade's timer alone is the commonest wait: these wait on a package, a line,
            // a trigger or an actor as well.
            "controls_waiting_beyond_a_timer": controls.iter().filter(|c| c["verdict"] == "waits" && c["releases"].as_array().is_some_and(|rs| rs.iter().all(|r| r["waits_for"].as_array().is_some_and(|t| t.iter().any(|t| t != "timer" && RISKY.contains(&t.as_str().unwrap_or(""))))))).count(),
            "stages_hard_to_reach": stages.len(),
            "stages": stage_summary,
            "doors_opened_by_script": doors.len(),
            "doors": door_summary,
        },
        "conversations": {"traps": traps, "goodbye_less_top_levels": goodbye},
        "controls": controls,
        "stages": stages,
        "doors": doors,
    })
}

// ------------------------------------------------------------------------- CLI

#[derive(clap::Subcommand)]
pub enum QuestCmd {
    /// The shapes in the data that can soft-lock a player: conversations with no way
    /// out, controls taken by one script and given back only by something else, stages
    /// only hard-to-reach things set, keyless doors only scripts open
    /// (fnv.quest.sweep/1; docs/design/quest-sweep.md).
    Sweep {
        #[command(flatten)]
        a: crate::script_cmd::ScriptArgs,
        /// How many steps (stages set, variables written, packages given, lines said) a
        /// control check follows from the script that takes the controls.
        #[arg(long, default_value_t = 4)]
        depth: usize,
        /// Skip reading the navmeshes (the trigger part of the stage check then treats
        /// every trigger as reachable).
        #[arg(long)]
        no_nav: bool,
    },
}

pub fn run(cmd: QuestCmd) -> anyhow::Result<Value> {
    use anyhow::Context;
    match cmd {
        QuestCmd::Sweep { a, depth, no_nav } => {
            let t0 = std::time::Instant::now();
            let tp = crate::script_cmd::table_path(&a);
            let ftext = std::fs::read_to_string(&tp).with_context(|| tp.display().to_string())?;
            let fjson: Value = serde_json::from_str(&ftext).with_context(|| tp.display().to_string())?;
            let table = fnv_script::FunctionTable::load(&tp).with_context(|| tp.display().to_string())?;
            let plugins = crate::esm_cmd::open_load_order(&a.lo)?;
            let lo = fnv_esm::LoadOrder::new(&plugins)?;
            let schema = fnv_esm::Schema::load_dir(&a.schema)?;
            let refs = RefTable::build(&lo, &schema);
            let data = GameData::build(&lo, &schema, table, &fjson, &refs);
            let nav = if no_nav { None } else { Some(fnv_nav::NavRecords::read(&lo)) };
            let facts = extract(&data, &refs, nav.as_ref());
            let mut v = sweep(&facts, depth);
            v["schema"] = json!("fnv.quest.sweep/1");
            v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
            v["summary"]["units"] = json!(facts.units.len());
            v["summary"]["lines"] = json!(facts.lines.len());
            v["summary"]["navmeshes_read"] = json!(!no_nav);
            v["seconds"] = json!((t0.elapsed().as_secs_f64() * 10.0).round() / 10.0);
            Ok(v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(func: &str, on: On, args: Vec<P>) -> CallFact {
        CallFact { func: func.into(), on, args, block: None, block_args: vec![], guards: vec![] }
    }

    fn unit(key: &str, owner: UnitOwner, calls: Vec<CallFact>) -> UnitFacts {
        UnitFacts { key: key.into(), owner, calls, writes: vec![], timers: BTreeSet::new(), stage_tests: vec![] }
    }

    fn line(topic: u32, goodbye: bool, choices: Vec<u32>) -> LineFacts {
        LineFacts { topic, goodbye, choices, info_type: topic_type::TOPIC, ..Default::default() }
    }

    fn topic(lines: Vec<u32>, top: bool) -> TopicFacts {
        TopicFacts { lines, top_level: top, topic_type: topic_type::TOPIC, ..Default::default() }
    }

    #[test]
    fn a_loop_of_choices_with_no_goodbye_is_a_trap() {
        let mut f = Facts::default();
        // Topic 10's line offers 11; 11's line offers 10 back. Neither ends.
        f.topics.insert(10, topic(vec![100], false));
        f.topics.insert(11, topic(vec![110], false));
        f.lines.insert(100, line(10, false, vec![11]));
        f.lines.insert(110, line(11, false, vec![10]));
        // Topic 20 offers 21, whose line says goodbye: fine.
        f.topics.insert(20, topic(vec![200], false));
        f.topics.insert(21, topic(vec![210], false));
        f.lines.insert(200, line(20, false, vec![21]));
        f.lines.insert(210, line(21, true, vec![]));
        let t = conversation_traps(&f);
        assert_eq!(t.len(), 2, "{t:?}");
        assert_eq!(t[0]["line"], "0x00000064");
    }

    #[test]
    fn a_choice_without_choices_hands_back_the_topic_list() {
        let mut f = Facts::default();
        f.topics.insert(10, topic(vec![100], false));
        f.topics.insert(11, topic(vec![110], false));
        f.lines.insert(100, line(10, false, vec![11]));
        f.lines.insert(110, line(11, false, vec![]));
        assert!(conversation_traps(&f).is_empty());
    }

    #[test]
    fn an_exit_another_speaker_says_does_not_count() {
        let mut f = Facts::default();
        f.actors.insert(1, ActorFacts { placed: true, ..Default::default() });
        f.topics.insert(10, topic(vec![100], false));
        f.topics.insert(11, topic(vec![110, 111], false));
        let mut l = line(10, false, vec![11]);
        l.who = vec![vec![Atom::Id(vec![1], true)]];
        f.lines.insert(100, l);
        // 110 loops back for actor 1; 111 is a goodbye for actor 2 only.
        let mut a = line(11, false, vec![10]);
        a.who = vec![vec![Atom::Id(vec![1], true)]];
        f.lines.insert(110, a);
        let mut b = line(11, true, vec![]);
        b.who = vec![vec![Atom::Id(vec![2], true)]];
        f.lines.insert(111, b);
        let t = conversation_traps(&f);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0]["speakers"][0], "0x00000001");
    }

    #[test]
    fn julie_shape_a_top_level_without_goodbye() {
        let mut f = Facts { goodbye: Some(5), ..Default::default() };
        f.actors.insert(1, ActorFacts { placed: true, voice: Some(50), ..Default::default() });
        f.actors.insert(2, ActorFacts { placed: true, voice: Some(51), ..Default::default() });
        f.topics.insert(5, TopicFacts { lines: vec![500], topic_type: topic_type::TOPIC, ..Default::default() });
        // The only Goodbye is for voice 51.
        let mut g = line(5, true, vec![]);
        g.who = vec![vec![Atom::Voice(vec![51], true)]];
        f.lines.insert(500, g);
        f.topics.insert(10, topic(vec![100, 101], true));
        for (i, a) in [(100, 1), (101, 2)] {
            let mut l = line(10, false, vec![]);
            l.who = vec![vec![Atom::Id(vec![a], true)]];
            f.lines.insert(i, l);
        }
        let v = goodbye_less_top_levels(&f);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["actor"], "0x00000001");
    }

    #[test]
    fn controls_released_in_the_same_block_are_not_listed() {
        let mut f = Facts::default();
        f.units.push(unit("a", UnitOwner::Script { form: 1 }, vec![call("disableplayercontrols", On::Implicit, vec![]), call("enableplayercontrols", On::Implicit, vec![])]));
        let (v, c) = control_holds(&f, 4);
        assert!(v.is_empty());
        assert_eq!(c["in place"], 1);
    }

    #[test]
    fn movement_flag_zero_takes_nothing() {
        assert!(!takes_movement(&call("disableplayercontrols", On::Implicit, vec![P::Num(0.0), P::Num(1.0)])));
        assert!(takes_movement(&call("disableplayercontrols", On::Implicit, vec![P::Num(1.0)])));
        assert!(takes_movement(&call("setrestrained", On::Form(PLAYER), vec![P::Num(1.0)])));
        assert!(!takes_movement(&call("setrestrained", On::Form(0x999), vec![P::Num(1.0)])));
    }

    /// Ramos' shape (#2453): a line's end script takes the controls and sets a variable
    /// on a trigger; the trigger's OnTrigger block, behind that variable and a timer,
    /// gives them back.
    #[test]
    fn a_release_behind_a_trigger_and_a_timer_waits() {
        let mut f = Facts::default();
        f.lines.insert(0x500, line(0x50, true, vec![]));
        let mut take = unit("line", UnitOwner::Info { form: 0x500, end: true }, vec![call("disableplayercontrols", On::Implicit, vec![])]);
        take.writes.push(Write { form: 0x900, var: "dofade".into() });
        f.units.push(take);
        let mut rel = call("enableplayercontrols", On::Implicit, vec![]);
        rel.block = Some("ontrigger".into());
        rel.guards = vec!["var:dofade".into(), "var:timer".into()];
        let mut trig = unit("trig", UnitOwner::Script { form: 0x800 }, vec![rel]);
        trig.timers.insert("var:timer".into());
        f.units.push(trig);
        f.script_unit.insert(0x800, 1);
        f.refs.insert(0x900, RefFacts { base: 0x700, base_sig: "ACTI".into(), ..Default::default() });
        f.form_script.insert(0x700, 0x800);
        let (v, c) = control_holds(&f, 4);
        assert_eq!(c["waits"], 1, "{v:?}");
        let tags = &v[0]["releases"][0]["waits_for"];
        assert!(tags.as_array().unwrap().iter().any(|t| t == "trigger"));
        assert!(tags.as_array().unwrap().iter().any(|t| t == "timer"));
        assert_eq!(v[0]["releases"][0]["path"][0], "sets 0x00000900.dofade");
    }

    /// Hardin's shape (#2309): released only by a line of the topic a package the quest
    /// script gives says.
    #[test]
    fn a_release_in_a_packages_topic_waits_on_the_package() {
        let mut f = Facts::default();
        f.quest_stages.insert(0x40, BTreeSet::new());
        f.form_script.insert(0x40, 0x41);
        f.script_unit.insert(0x41, 1);
        let mut take = unit("line", UnitOwner::Info { form: 0x500, end: true }, vec![call("setrestrained", On::Form(PLAYER), vec![P::Num(1.0)])]);
        take.writes.push(Write { form: 0x40, var: "training".into() });
        f.units.push(take);
        f.units.push(unit("qs", UnitOwner::Script { form: 0x41 }, vec![call("addscriptpackage", On::Form(0x77), vec![P::Form(0x60)])]));
        f.packages.insert(0x60, PackageFacts { topic: Some(0x61), ..Default::default() });
        f.topics.insert(0x61, topic(vec![0x610], false));
        f.lines.insert(0x610, line(0x61, false, vec![]));
        f.line_units.insert(0x610, vec![2]);
        f.units.push(unit("rel", UnitOwner::Info { form: 0x610, end: false }, vec![call("setrestrained", On::Form(PLAYER), vec![P::Num(0.0)])]));
        let (v, _) = control_holds(&f, 4);
        assert_eq!(v[0]["verdict"], "waits");
        let tags: Vec<&str> = v[0]["releases"][0]["waits_for"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        assert!(tags.contains(&"package") && tags.contains(&"dialogue line"), "{tags:?}");
    }

    #[test]
    fn a_hold_nothing_releases_is_unreleased() {
        let mut f = Facts::default();
        f.units.push(unit("a", UnitOwner::QuestStage { form: 1, stage: 10, entry: 0 }, vec![call("disableplayercontrols", On::Implicit, vec![])]));
        let (v, _) = control_holds(&f, 4);
        assert_eq!(v[0]["verdict"], "unreleased");
    }

    fn stage_facts() -> Facts {
        let mut f = Facts::default();
        f.quest_stages.insert(0x30, BTreeSet::from([10, 20, 30, 40]));
        f.condition_stage_tests.extend([(0x30, Some(10)), (0x30, Some(20)), (0x30, Some(30)), (0x30, Some(40))]);
        f
    }

    #[test]
    fn stages_only_a_package_sets_are_listed() {
        let mut f = stage_facts();
        let set = |s: f64| call("setstage", On::Implicit, vec![P::Form(0x30), P::Num(s)]);
        f.units.push(unit("pkg", UnitOwner::Package { form: 0x60, event: PackageEvent::End }, vec![set(10.0)]));
        f.units.push(unit("line", UnitOwner::Info { form: 0x500, end: true }, vec![set(20.0), set(30.0)]));
        // 30 is also set by a package: one ordinary setter is enough.
        f.units.push(unit("pkg2", UnitOwner::Package { form: 0x61, event: PackageEvent::End }, vec![set(30.0)]));
        let (v, s) = stage_reach(&f);
        let stages: Vec<i64> = v.iter().map(|x| x["stage"].as_i64().unwrap()).collect();
        assert_eq!(stages, vec![10, 40], "{v:?}");
        assert_eq!(v[0]["only_set_by"][0], "package");
        assert_eq!(v[1]["only_set_by"][0], "no setter");
        assert_eq!(s["stages_set_and_tested"], 3);
    }

    #[test]
    fn a_trigger_off_the_navmesh_and_a_disabled_activator() {
        let mut f = stage_facts();
        let mut on_trig = call("setstage", On::Implicit, vec![P::Form(0x30), P::Num(10.0)]);
        on_trig.block = Some("ontriggerenter".into());
        let mut on_act = call("setstage", On::Implicit, vec![P::Form(0x30), P::Num(20.0)]);
        on_act.block = Some("onactivate".into());
        f.units.push(unit("trig", UnitOwner::Script { form: 0x80 }, vec![on_trig]));
        f.units.push(unit("act", UnitOwner::Script { form: 0x81 }, vec![on_act]));
        f.units.push(unit("en", UnitOwner::Info { form: 0x5, end: true }, vec![call("enable", On::Form(0x91), vec![])]));
        f.form_script.insert(0x70, 0x80);
        f.form_script.insert(0x71, 0x81);
        f.refs.insert(0x90, RefFacts { base: 0x70, nav_near: Some(false), ..Default::default() });
        f.refs.insert(0x91, RefFacts { base: 0x71, disabled: true, ..Default::default() });
        let (v, _) = stage_reach(&f);
        assert_eq!(v[0]["only_set_by"][0], "trigger without navmesh");
        assert_eq!(v[1]["only_set_by"][0], "disabled activator");
        assert_eq!(v[1]["setters"][0]["activators"][0]["enabled_by"][0], "en");
        // With a navmesh by the trigger, stage 10 is ordinary.
        f.refs.get_mut(&0x90).unwrap().nav_near = Some(true);
        let (v, _) = stage_reach(&f);
        assert!(v.iter().all(|x| x["stage"] != 10));
    }

    /// REPCONN's shape: a Requires Key door without a key, unlocked by a line's end script
    /// (INFO 0x000CD87E → REPCONMidToTopDoor).
    #[test]
    fn a_keyless_door_a_line_unlocks() {
        let mut f = Facts::default();
        f.sigs.insert(0x7, "TACT".into());
        f.refs.insert(0x82FF9, RefFacts { base: 0x50, base_sig: "DOOR".into(), lock: Some((255, None)), ..Default::default() });
        f.refs.insert(0x82FFA, RefFacts { base: 0x50, base_sig: "DOOR".into(), lock: Some((255, None)), ..Default::default() });
        f.refs.insert(0x82FFB, RefFacts { base: 0x50, base_sig: "DOOR".into(), lock: Some((255, Some(0x1))), ..Default::default() });
        f.refs.insert(0x82FFC, RefFacts { base: 0x50, base_sig: "DOOR".into(), lock: Some((50, None)), ..Default::default() });
        let mut l = line(0x60, false, vec![]);
        l.who = vec![vec![Atom::Id(vec![0x7], true)]];
        f.lines.insert(0xCD87E, l);
        f.units.push(unit("intercom", UnitOwner::Info { form: 0xCD87E, end: true }, vec![call("unlock", On::Form(0x82FF9), vec![])]));
        let (v, s) = script_doors(&f);
        assert_eq!(s["key_only_without_key"], 2);
        assert_eq!(s["opened_by_script"], 1);
        assert_eq!(s["never_opened"], 1);
        assert_eq!(v[0]["opened_by"][0]["kind"], "dialogue line");
        assert_eq!(v[0]["opened_by"][0]["talking_activator"], true);
    }

    #[test]
    fn or_groups_join_by_and() {
        let mut f = Facts::default();
        f.actors.insert(1, ActorFacts { voice: Some(9), ..Default::default() });
        let l = LineFacts { who: vec![vec![Atom::Voice(vec![8], true), Atom::Voice(vec![9], true)], vec![Atom::Unknown]], ..Default::default() };
        assert!(may_say(&f, &l, Some(1)));
        let l = LineFacts { who: vec![vec![Atom::Voice(vec![8], true)], vec![Atom::Voice(vec![9], true)]], ..Default::default() };
        assert!(!may_say(&f, &l, Some(1)));
    }

    #[test]
    fn triangle_distance() {
        let t = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]];
        assert_eq!(dist2d_triangle([10.0, 10.0, 0.0], &t), 0.0);
        assert!((dist2d_triangle([-30.0, 0.0, 0.0], &t) - 30.0).abs() < 1e-3);
    }
}
