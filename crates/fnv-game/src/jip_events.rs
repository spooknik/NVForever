//! JIP LN's own event handlers (task J6; docs/spec-notes/jip-events.md): the
//! `SetOn…EventHandler` functions and SetGameMainLoopCallback. Beside the extender's
//! SetEventHandler (`Game::dispatch_event`), with JIP's own rules:
//! - several handlers for one event run **newest first** [observed: ev.order];
//! - a handler that declares fewer parameters than the event passes never runs
//!   [observed: ev.noparams, ev.oneparam]; NVForever logs why [decision];
//! - registrations are not saved: a load starts with none [documented for the ragdoll
//!   event; hypothesis for the others, jip-events.md §1].

use crate::game::{Game, RunCtx};
use fnv_script::vm::{Value, XVal};
use fnv_sim::FormId;
use std::sync::Arc;
use crate::jip_provenance::Registrar;

/// XMarker: the "no filter" form some events take, since 0 is refused [documented:
/// SetOnCriticalHitEventHandler].
const XMARKER: u32 = 0x0000_003B;

/// The JIP registrations of a session.
#[derive(Debug, Clone, Default)]
pub struct JipEvents {
    pub handlers: Vec<JipHandler>,
    pub main_loop: Vec<MainLoop>,
    seq: u64,
    /// Diagnostic metadata only, bounded by active registrations; no callable retention.
    registrars: std::collections::HashMap<u64, Arc<Registrar>>,
    /// Each actor's hit in progress, or its last one (task J7, jip-hit-info.md §5).
    pub hits: std::collections::HashMap<FormId, JipHit>,
    /// The keys and mouse buttons held last frame, for the key-down and key-up events.
    pub keys_held: std::collections::BTreeSet<u16>,
    /// ScriptWait: the waiting block of each script instance (one at a time
    /// [documented: ScriptWait]); not saved [hypothesis, as the registrations].
    pub waits: std::collections::HashMap<crate::game::InstKey, Waiting>,
    /// A ScriptWait call's cycles, for the block run that made it (`Exec::take_wait`).
    pub wait_request: Option<u32>,
    /// The script depth of the block run that may wait: ScriptWait elsewhere (a result
    /// script, a function, an effect) does not [documented: ScriptWait, "cannot be used
    /// in Effect-type scripts, UDF scripts, or result scripts"].
    pub wait_depth: Option<u32>,
    /// SetMusicState's requests for the engine: 0 stop, 1 pause, 2 resume [documented].
    pub music_states: Vec<u8>,
    /// What lasts the whole session, across loads (`Game::take_session`).
    pub kept: JipKept,
}

/// JIP's settings that last until the game is quit (`crate::nvse_ttw`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JipKept {
    /// MarkActivatorAshPile's activators [documented: "remains in effect for the duration
    /// of the current game session"].
    pub ash_pile_bases: std::collections::BTreeSet<FormId>,
    /// SetTerminalUIModel's menu art, by terminal form, a path under `meshes\`
    /// [documented]; kept for the session as MarkActivatorAshPile's marks [hypothesis:
    /// the page does not say].
    pub terminal_models: std::collections::BTreeMap<FormId, String>,
}

/// A block that waits (ScriptWait): which, the processing cycles still to wait and where
/// it goes on ([`fnv_script::vm::WaitPoint`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Waiting {
    pub script: FormId,
    pub block: usize,
    pub left: u32,
    pub at: fnv_script::vm::WaitPoint,
}

/// What JIP's GetHit… functions read [documented: jip-hit-info.md §1].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JipHit {
    pub attacker: Option<FormId>,
    pub weapon: Option<FormId>,
    pub health_damage: f64,
    pub limb_damage: f64,
    pub location: i32,
    /// GetHitExtendedFlag's bits: 1 critical, 2 fatal, 5 limb crippled (bit n = flag n).
    pub flags: u16,
}

/// One `SetOn<Event>EventHandler handler 1 filters…`.
#[derive(Debug, Clone, PartialEq)]
pub struct JipHandler {
    /// The name between `SetOn` and `EventHandler`, lower case (`healthdamage`).
    pub event: String,
    pub func: XVal,
    pub filters: Vec<XVal>,
    pub seq: u64,
}

/// Snapshot diagnostics with the handler before any callback can remove its registrar.
struct JipAttempt {
    handler: JipHandler,
    registrar: Option<Arc<Registrar>>,
}

impl JipEvents {
    fn attempt(&self, handler: &JipHandler) -> JipAttempt {
        JipAttempt { handler: handler.clone(), registrar: self.registrars.get(&handler.seq).cloned() }
    }
}

/// Bounded values for diagnostics: no array dereference or unbounded string copy.
fn diagnostic_value(value: &XVal) -> String {
    match value {
        XVal::Num(n) => format!("number:{n:?}"),
        XVal::Ref(id) => format!("ref:0x{id:08X}"),
        XVal::Array(id) => format!("array:{id}"),
        XVal::Str(text) => {
            let mut out = String::from("string:\"");
            let mut chars = text.chars();
            for c in chars.by_ref().take(32) {
                out.extend(c.escape_debug());
            }
            if chars.next().is_some() {
                out.push_str("<truncated>");
            }
            out.push('"');
            out
        }
    }
}

impl JipHandler {
    fn failure_context(&self, on: Option<FormId>, args: usize) -> String {
        let filters = self.filters.iter().take(4).map(diagnostic_value).collect::<Vec<_>>().join(", ");
        let omitted = if self.filters.len() > 4 { ", <truncated>" } else { "" };
        let receiver = on.map_or_else(|| "none".into(), |r| format!("0x{:08X}", r.0));
        format!(
            "registration={} callable={} receiver={} args={} filters(total={})=[{}{}]",
            self.seq, diagnostic_value(&self.func), receiver, args, self.filters.len(), filters, omitted,
        )
    }
}

/// One SetGameMainLoopCallback registration.
#[derive(Debug, Clone, PartialEq)]
pub struct MainLoop {
    pub func: XVal,
    /// The reference it was called on; the player when none [documented].
    pub on: Option<FormId>,
    /// Every this many frames [documented: frames, not time].
    pub delay: u64,
    /// 1 game mode, 2 menu mode, 8 removed on load [documented].
    pub flags: u32,
    pub due: u64,
}

/// What a filter at each position tests: the calling reference or an argument
/// [documented: the table of jip-events.md §2].
#[derive(Clone, Copy)]
enum Slot {
    On,
    Arg(usize),
    /// A required animation code: zero is a value, not an actor wildcard.
    ExactArg(usize),
}

fn slots(event: &str) -> &'static [Slot] {
    match event {
        "hit" | "healthdamage" | "crippledlimb" | "fireweapon" | "projectileimpact" => &[Slot::On],
        "animaction" | "playgroup" => &[Slot::On, Slot::ExactArg(0)],
        "criticalhit" => &[Slot::On, Slot::Arg(0), Slot::Arg(1)],
        "queststage" => &[Slot::Arg(0), Slot::Arg(1)],
        "useaiditem" | "dialogtopic" | "keydown" | "keyup" | "controldown" | "controlup" | "triggerdown" | "triggerup" | "menuopen" | "menuclose" | "mouseoverchange" => &[Slot::Arg(0)],
        _ => &[],
    }
}

/// Whether a SetOnMenuClickEventHandler tile path names this tile (see
/// [`Game::jip_menu_click`]).
pub fn click_path_matches(filter: &str, menu: &str, path: &str, id: f64) -> bool {
    if let Some((m, n)) = filter.split_once('#') {
        return m.trim().eq_ignore_ascii_case(menu) && n.trim().parse::<f64>().is_ok_and(|n| n == id && id >= 0.0);
    }
    let segs = |p: &str| p.split(['/', '\\']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_ascii_lowercase).collect::<Vec<_>>();
    let (want, have) = (segs(filter), segs(path));
    !want.is_empty() && want.len() <= have.len() && want.iter().zip(&have).all(|(a, b)| a == b)
}

impl Game<'_> {
    /// `SetOn<event>EventHandler func add filters…`; a second identical registration is
    /// kept once [decision, jip-events.md §2]. Returns whether anything changed.
    pub(crate) fn jip_set(&mut self, event: String, func: XVal, add: bool, filters: Vec<XVal>, registrar: Option<Arc<Registrar>>) -> bool {
        let same = |h: &JipHandler| h.event == event && h.func == func && h.filters == filters;
        if add {
            if self.jip.handlers.iter().any(same) {
                return true;
            }
            self.jip.seq += 1;
            let seq = self.jip.seq;
            self.jip.handlers.push(JipHandler { event, func, filters, seq });
            if let Some(registrar) = registrar { self.jip.registrars.insert(seq, registrar); }
            true
        } else {
            let before = self.jip.handlers.len();
            let registrars = &mut self.jip.registrars;
            self.jip.handlers.retain(|h| {
                if same(h) { registrars.remove(&h.seq); false } else { true }
            });
            self.jip.handlers.len() < before
        }
    }

    /// SetGameMainLoopCallback func add [delay] [flags].
    pub(crate) fn jip_main_loop(&mut self, func: XVal, add: bool, delay: u64, flags: u32, on: Option<FormId>) {
        self.jip.main_loop.retain(|m| !(m.func == func && m.on == on));
        if add {
            let delay = delay.max(1);
            self.jip.main_loop.push(MainLoop { func, on, delay, flags, due: self.frame + delay });
        }
    }

    /// Run the handlers of a JIP event, newest first; returns how many ran.
    pub fn jip_event(&mut self, event: &str, on: Option<FormId>, args: Vec<XVal>) -> usize {
        if self.jip.handlers.is_empty() {
            return 0;
        }
        let hs = self.jip.handlers.iter().filter(|h| h.event == event && self.jip_matches(h, on, &args)).map(|h| self.jip.attempt(h)).collect();
        self.jip_run(event, hs, on, args)
    }

    /// SetOnMenuClickEventHandler's event: a tile was clicked (task T8). `menu` is the
    /// menu's name, `path` the tile's full path, `id` its `<id>` (-1 without one), `name`
    /// the name passed. A handler's tile path matches when it is `Menu#<id>` of this tile,
    /// or the tile's path or a path above it [documented: SetOnMenuClickEventHandler,
    /// "a partial path … ANY tile beginning with that path"; hypothesis: compared by
    /// whole segments, without case, `/` and `\` alike]. The handler gets the menu's ID,
    /// the tile's id and its name in lower case [documented].
    pub fn jip_menu_click(&mut self, menu: &str, path: &str, menu_id: f64, id: f64, name: &str) -> usize {
        if self.jip.handlers.is_empty() {
            return 0;
        }
        let matches = |f: &XVal| match f {
            XVal::Str(f) => click_path_matches(f, menu, path, id),
            _ => false,
        };
        let hs = self.jip.handlers.iter().filter(|h| h.event == "menuclick" && h.filters.first().is_some_and(matches)).map(|h| self.jip.attempt(h)).collect();
        self.jip_run("menuclick", hs, None, vec![XVal::Num(menu_id), XVal::Num(id), XVal::Str(name.to_ascii_lowercase())])
    }

    fn jip_run(&mut self, event: &str, mut hs: Vec<JipAttempt>, on: Option<FormId>, args: Vec<XVal>) -> usize {
        hs.sort_by_key(|h| std::cmp::Reverse(h.handler.seq));
        let ctx = RunCtx { self_ref: on, ..RunCtx::default() };
        let mut ran = 0;
        for attempt in &hs {
            let h = &attempt.handler;
            if self.function_arity(&h.func).is_some_and(|n| n < args.len()) {
                tracing::info!("jip: a {event} handler takes fewer than the event's {} values and does not run", args.len());
                continue;
            }
            match self.run_function(&h.func, on.map(|f| Value::Ref(f.0)), args.clone(), None, ctx) {
                Ok(_) => ran += 1,
                Err(error) => self.handler_failed_with_registrar(format!("JIP {event} handler"), error,
                    || Some(h.failure_context(on, args.len())), attempt.registrar.as_deref()),
            }
        }
        ran
    }

    fn jip_matches(&self, h: &JipHandler, on: Option<FormId>, args: &[XVal]) -> bool {
        slots(&h.event).iter().zip(&h.filters).all(|(s, f)| {
            if !matches!(s, Slot::ExactArg(_)) && self.jip_wildcard(f) {
                return true;
            }
            let v = match s {
                Slot::On => on.map(|r| XVal::Ref(r.0)),
                Slot::Arg(i) | Slot::ExactArg(i) => args.get(*i).cloned(),
            };
            v.is_some_and(|v| self.filter_matches(f, &v, false))
        })
    }

    /// A filter that lets everything through: none (0), XMarker, or an empty form list
    /// [documented: SetOnAnimActionEventHandler, an empty list means any actor].
    fn jip_wildcard(&self, f: &XVal) -> bool {
        match f {
            XVal::Num(x) => *x == 0.0,
            XVal::Ref(0) | XVal::Ref(XMARKER) => true,
            XVal::Ref(r) => self.jip_empty_form_list(FormId(*r)),
            _ => false,
        }
    }

    /// Resolve a materialized clone's declared type, keeping its own list edits.
    /// Unmaterialized clones cannot use an absent copied list as a wildcard
    /// (docs/spec-notes/jip-cloned-list-wildcard.md).
    fn jip_empty_form_list(&self, list: FormId) -> bool {
        if self.clones.contains_key(&list) && !self.list_edits.contains_key(&list) && !self.form_lists.contains_key(&list) {
            return false;
        }
        let mut original = list;
        // At most one step per alias plus its terminal authored record. Public
        // TempCloneForm makes acyclic chains; malformed cycles fail closed here.
        for _ in 0..=self.clones.len() {
            match self.clones.get(&original) {
                Some(&next) => original = next,
                None => return self.data.sig(original) == Some(*b"FLST") && crate::functions::list_members(self, list).is_empty(),
            }
        }
        false
    }

    /// Whether a quest-stage registration that matches asked to skip the stage's result
    /// script (skipScript, the third filter) [documented: SetOnQuestStageEventHandler].
    pub(crate) fn jip_skips_stage(&self, quest: FormId, stage: i32) -> bool {
        let args = [XVal::Ref(quest.0), XVal::Num(f64::from(stage))];
        self.jip.handlers.iter().any(|h| h.event == "queststage" && h.filters.get(2).is_some_and(XVal::truthy) && self.jip_matches(h, None, &args))
    }

    /// A GetHit… value of `actor`'s current or last hit; `None` when it had none.
    pub(crate) fn hit_value(&self, actor: FormId, what: &str, k: i64) -> Option<XVal> {
        let h = self.jip.hits.get(&actor)?;
        Some(match what {
            "attacker" => XVal::Ref(h.attacker.map_or(0, |a| a.0)),
            "weapon" => XVal::Ref(h.weapon.map_or(0, |w| w.0)),
            "health" => XVal::Num(h.health_damage),
            "limb" => XVal::Num(h.limb_damage),
            "flag" => XVal::Num(f64::from(u8::from((0..16).contains(&k) && h.flags & (1 << k) != 0))),
            _ => return None,
        })
    }

    /// Each frame (`Game::tick`): the key-down and key-up events of the keys and mouse
    /// buttons pressed and let go since the last frame, by DirectX scancode (256 + n for
    /// the mouse) [documented: SetOnKeyDownEventHandler, SetOnKeyUpEventHandler; the
    /// handler gets the key]. The keys held are the player's whatever DisableKey says, as
    /// the events "still run even if the key has been disabled" [documented:
    /// SetOnKeyDownEventHandler].
    pub(crate) fn jip_keys(&mut self) {
        if self.jip.keys_held == self.input.held {
            return;
        }
        let now = self.input.held.clone();
        let before = std::mem::replace(&mut self.jip.keys_held, now.clone());
        if self.jip.handlers.iter().all(|h| h.event != "keydown" && h.event != "keyup") {
            return;
        }
        for &k in now.difference(&before) {
            self.jip_event("keydown", None, vec![XVal::Num(f64::from(k))]);
        }
        for &k in before.difference(&now) {
            self.jip_event("keyup", None, vec![XVal::Num(f64::from(k))]);
        }
    }

    /// An item was made in the recipe menu: SetOnCraftingEventHandler's event, with the
    /// item's base form and the quantity [documented: SetOnCraftingEventHandler]. Called
    /// once per output of the recipe, with all of that output the making gave
    /// [hypothesis: the page does not say how a recipe made several times, or one with
    /// two outputs, is reported].
    pub fn item_crafted(&mut self, item: FormId, quantity: i64) {
        self.jip_event("crafting", None, vec![XVal::Ref(item.0), XVal::Num(quantity as f64)]);
    }

    /// The main-loop callbacks due this frame (`Game::tick`).
    pub(crate) fn run_main_loop(&mut self) {
        if self.jip.main_loop.is_empty() {
            return;
        }
        let menu = self.menu.is_some() || self.dialogue.is_some();
        let want = if menu { 2 } else { 1 };
        let frame = self.frame;
        let due: Vec<MainLoop> = self.jip.main_loop.iter().filter(|m| m.due <= frame && m.flags & want != 0).cloned().collect();
        for m in due {
            for x in self.jip.main_loop.iter_mut().filter(|x| x.func == m.func && x.on == m.on) {
                x.due = frame + x.delay;
            }
            let on = m.on.unwrap_or(FormId::PLAYER_REF);
            let ctx = RunCtx { self_ref: Some(on), ..RunCtx::default() };
            if let Err(error) = self.run_function(&m.func, Some(Value::Ref(on.0)), Vec::new(), None, ctx) {
                self.handler_failed("JIP main-loop callback".into(), error);
            }
        }
    }
}
