//! ResurrectActor: a dead actor alive again (docs/spec-notes/resurrect.md).
//!
//! - **The rules** [observed: function probes fp5, 2026-10-17: after `KillActor player 0
//!   1` then `ResurrectActor 0` on Ringo, GetDead 0, GetCauseofDeath −1,
//!   GetHealthPercentage 1.0]: not dead or unconscious, its killer and cause of death
//!   forgotten, unharmed (every actor value's damage gone, the limbs' too [hypothesis:
//!   the owner's "generated again from its record", below]). With Anim 0 its severed and
//!   exploded limbs come back ([`RefExtra::limbs_gone`](crate::game::RefExtra::limbs_gone)
//!   cleared, so the engine draws it whole); with 1 they stay gone [documented: "body data
//!   will not be updated"].
//! - **Generated again** [observed: owner, 2026-10-17: "resurrect rerolls any level list
//!   or random gen on the actor record so if it comes from a template with an item list
//!   or its a generic that can be male female all this will be re rolled on the
//!   resurrected actor"]: a dead actor's random choices are rolled again
//!   ([`RefExtra::rolls`](crate::game::RefExtra::rolls), the seed
//!   `fnv_sim::leveled::roll_seed`): the entry a leveled actor list stands for (which can
//!   be a man or a woman), and, when its carried items come through such a pick, the
//!   template they come from and the items themselves (`fnv_sim::World::reset_inventory`).
//!   An actor with its own item list keeps what it carries, looted or given [observed:
//!   capture rs1, 2026-10-17: Ringo given three Stimpaks, two taken from his corpse, one
//!   left after ResurrectActor 0]. What scripts gave it otherwise stays (factions,
//!   packages, actor values set, essential, AI off, dispositions) [hypothesis: the quests
//!   that resurrect companions and Yes Man rely on it].
//! - **Where**: where its body lies [observed: rs1: Ringo's reference followed his
//!   ragdoll from (685, -5602) to (730, -5609) and he stood up there, at once and still
//!   after 180 frames], which is where it died or where a script moved the corpse
//!   [observed: the shipped scripts move the corpse first, e.g. Yes Man's `MoveTo` to a
//!   marker and `ResurrectActor` after it, VMQYesManSupport, and the robot squads'
//!   `bot01.MoveTo` then `bot01.ResurrectActor` a frame later, decompiled 2026-10-17].
//!   The engine tells where the body came to rest ([`Game::stand_where_the_body_lies`]).
//!   It stands there in its default standing pose [observed: owner, 2026-10-17: "they
//!   respawn in their default standing position"; rs1: standing in the first frame after
//!   ResurrectActor 0, GetKnockedState 0 throughout].
//! - **The animate flag** (0 just standing, 1 standing up [documented: GECK wiki
//!   ResurrectActor]) is kept with the resurrection for the engine; both stand at once
//!   [design: the owner, 2026-10-17, "we shouldnt need a getup animation for
//!   resurrect"]. (With 1 the original showed a torn body in the air for a second or two,
//!   GetKnockedState 0, then the actor walking on [observed: rs1 screenshots]; not
//!   copied.)
//! - **A living actor** (scripts call it from OnHit when Health is below 0, decompiled)
//!   is only healed: nothing is rolled again [hypothesis: generating a living actor again
//!   would strip a companion of its gear].
//! - **Not loaded**: the rules are the same; it stays where it was and is drawn alive
//!   when its cell is [observed: rs1: Chet killed and raised in the unloaded general
//!   store, GetDead 0, position unchanged, standing when the player arrived].
//! - It leaves combat (`Game::stop_combat`) [hypothesis: a new actor starts calm].

use crate::functions::base_of;
use crate::game::Game;
use fnv_sim::leveled::roll_seed;
use fnv_sim::FormId;
use serde::Serialize;

/// One ResurrectActor for the engine (`Game::take_resurrections`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Resurrection {
    pub actor: FormId,
    /// The animate flag (1: stand up; module notes).
    pub animate: bool,
    /// It was dead and has been generated again (its look may have changed): the engine
    /// draws it anew.
    pub regenerated: bool,
    /// Its rolls after this one (`RefExtra::rolls`).
    pub rolls: u32,
}

impl Game<'_> {
    /// `actor.ResurrectActor animate` (module notes).
    pub fn resurrect(&mut self, r: FormId, animate: bool) {
        let regenerated = self.is_dead(r);
        let e = self.extra.entry(r).or_default();
        e.dead = false;
        e.unconscious = false;
        e.knocked_out = None;
        e.killer = None;
        e.death_cause = None;
        e.av_damage.clear();
        e.last_hit = None;
        e.killing_hit = None;
        e.player_damage = 0.0;
        e.total_damage = 0.0;
        let was_culled = crate::functions::critical_stage_culls(e.critical_stage);
        e.critical_stage = 0;
        e.furniture = None;
        e.weapon_out = false;
        // With 1 "body data will not be updated" [documented: GECK wiki ResurrectActor]: a
        // severed limb stays gone; with 0 the actor comes back whole [hypothesis: read
        // from that note].
        if !animate {
            e.limbs_gone.clear();
        }
        if regenerated {
            e.rolls += 1;
        }
        let rolls = e.rolls;
        if was_culled {
            self.script_looks.critical_changed.push(r);
        }
        self.remove_ash_pile(r);
        if regenerated {
            self.roll_again(r, rolls);
            self.stop_combat(r);
        }
        self.resurrections.push(Resurrection { actor: r, animate, regenerated, rolls });
    }

    /// Roll the carried items of `r` again when they come through a leveled pick (its
    /// base or a template in its Inventory chain is a leveled actor list); an actor with
    /// its own item list keeps what it carries (module notes).
    fn roll_again(&mut self, r: FormId, rolls: u32) {
        let Some(base) = base_of(self, r) else { return };
        if self.data.template_sources_any_pick(base, |d| d.template_inventory).len() < 2 {
            return;
        }
        let source = self.data.template_source_at(base, roll_seed(r, rolls), self.leveled_at(r), |d| d.template_inventory).map(|(s, _)| s).filter(|&s| s != base);
        self.world.set_inventory_source(r, source);
        // A source with no carried items: it carries nothing (but a planted explosive's
        // live unit, `Extra::live`, #1640).
        if !self.world.reset_inventory(r).unwrap_or(true) {
            let _ = self.world.clear_inventory_but_live(r);
        }
    }

    /// The engine found the body of resurrected `r` lying at `pos` (game units): the
    /// reference is put there and the actor stands up from there (module notes), its
    /// walker started again (`Game::teleported`).
    pub fn stand_where_the_body_lies(&mut self, r: FormId, pos: [f32; 3]) {
        if let Ok(s) = self.world.get(r) {
            s.pos = pos;
        }
        self.teleported.insert(r);
    }

    /// How many times ResurrectActor rolled the actor's random choices again.
    pub fn rolls(&self, r: FormId) -> u32 {
        self.extra.get(&r).map_or(0, |e| e.rolls)
    }

    /// The resurrections since the last call (the engine stands the actors up).
    pub fn take_resurrections(&mut self) -> Vec<Resurrection> {
        std::mem::take(&mut self.resurrections)
    }
}
