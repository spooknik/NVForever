//! What the load order's scripts ask about plugins: IsPluginInstalled, GetPluginVersion,
//! IsDLLLoaded (an extender plugin by name or file), IsModLoaded and GetModIndex (a
//! plugin of the load order). A mod that reads "not there" for something it needs
//! switches itself off or shows its "requires ..." message, so the answers say why a
//! mod does nothing.
//!
//! A test aid [design]: kept only when the engine asks for it
//! (`Game::plugin_questions`, the engine's `NVF_PLAY_TEST_MOD_LOG`); nothing of it is
//! saved or traced.

use std::collections::BTreeSet;

/// The functions whose answers are kept, lower case.
pub const FUNCTIONS: [&str; 5] = ["isplugininstalled", "getpluginversion", "isdllloaded", "ismodloaded", "getmodindex"];

/// One question, the first time a plugin's scripts asked it.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    /// The function, lower case (one of [`FUNCTIONS`]).
    pub function: String,
    /// The name asked for, as the script spelled it.
    pub name: String,
    pub answer: f64,
    /// The asking unit's key and its plugin (lower case; empty for a console line).
    pub unit: String,
    pub plugin: String,
}

impl Question {
    /// Whether the answer says "not there": 0 for the yes/no functions, -1 for
    /// GetPluginVersion [documented: GetPluginVersion], 255 for GetModIndex [documented:
    /// GetModIndex].
    pub fn absent(&self) -> bool {
        match self.function.as_str() {
            "getpluginversion" => self.answer < 0.0,
            "getmodindex" => self.answer == 255.0,
            _ => self.answer == 0.0,
        }
    }
}

/// The questions asked so far, each (plugin, function, name) once.
#[derive(Debug, Default)]
pub struct Questions {
    seen: BTreeSet<(String, String, String)>,
    fresh: Vec<Question>,
}

impl Questions {
    /// Keep a question unless the same plugin asked it before.
    pub fn record(&mut self, function: &str, name: &str, answer: f64, unit: String, plugin: String) {
        if self.seen.insert((plugin.clone(), function.to_string(), name.to_ascii_lowercase())) {
            self.fresh.push(Question { function: function.to_string(), name: name.to_string(), answer, unit, plugin });
        }
    }

    /// The questions kept since the last call, in the order asked.
    pub fn take(&mut self) -> Vec<Question> {
        std::mem::take(&mut self.fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_is_kept_once_per_plugin() {
        let mut q = Questions::default();
        q.record("isplugininstalled", "JIP NVSE Plugin", 1.0, "a.esp|quest 1".into(), "a.esp".into());
        q.record("isplugininstalled", "jip nvse plugin", 1.0, "a.esp|quest 2".into(), "a.esp".into());
        q.record("isplugininstalled", "JIP NVSE Plugin", 1.0, "b.esp|quest 1".into(), "b.esp".into());
        q.record("getpluginversion", "yUI", -1.0, "a.esp|quest 1".into(), "a.esp".into());
        let got = q.take();
        assert_eq!(got.iter().map(|x| (x.plugin.as_str(), x.function.as_str())).collect::<Vec<_>>(), [("a.esp", "isplugininstalled"), ("b.esp", "isplugininstalled"), ("a.esp", "getpluginversion")]);
        assert_eq!(got.iter().map(Question::absent).collect::<Vec<_>>(), [false, false, true]);
        assert!(q.take().is_empty());
        // Asked again later: still the one line.
        q.record("getpluginversion", "yUI", -1.0, "a.esp|quest 1".into(), "a.esp".into());
        assert!(q.take().is_empty());
    }

    #[test]
    fn absent_reads_each_functions_own_no() {
        let q = |function: &str, answer: f64| Question { function: function.into(), name: String::new(), answer, unit: String::new(), plugin: String::new() };
        assert!(q("getmodindex", 255.0).absent() && !q("getmodindex", 0.0).absent());
        assert!(q("ismodloaded", 0.0).absent() && !q("ismodloaded", 1.0).absent());
        assert!(q("isdllloaded", 0.0).absent() && q("getpluginversion", -1.0).absent() && !q("getpluginversion", 0.0).absent());
    }
}
