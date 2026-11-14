//! xNVSE's console functions (`con_…`): a console command run from a script
//! [documented: their pages, "Con …" in the wiki export]. Each runs NVForever's console
//! command of the same name (`crate::functions::console`); what a command prints reaches
//! the console only while the console echo is on (SetConsoleEcho), as the pages of
//! Con_Inv, Con_ShowVars and Con_SQV say ("Requires Console Echo to be toggled on").

use crate::functions::console::{self as con, ConsoleCtx};
use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k)? {
        XVal::Str(s) => Some(s.clone()),
        XVal::Num(n) => Some(format!("{n}")),
        XVal::Ref(r) => Some(format!("{r:08X}")),
        _ => None,
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let ctx = ConsoleCtx { on: subject, active_quest: g.ui.active_quest };
    let words: Vec<String> = (0..args.len()).filter_map(|k| text(args, k)).collect();
    let run: Result<Vec<String>, String> = match name {
        // LU's `Console "line"`: the line as the console runs it, a console-only command
        // or any function by name or alias [documented: Console; observed: anim2,
        // `Console "tgm"` turned god mode on and off; reload1, damage 0 with it on].
        "console" => {
            let line = text(args, 0).unwrap_or_default();
            match con::split_call(&line) {
                Some((on_name, func, words)) => {
                    let on = match &on_name {
                        Some(n) => crate::runner::resolve(g, n).ok(),
                        None => subject,
                    };
                    let ctx = ConsoleCtx { on, active_quest: g.ui.active_quest };
                    let compile_ctx = crate::game::RunCtx { self_ref: on, ..Default::default() };
                    if on_name.is_none() && func.eq_ignore_ascii_case("set") {
                        // A parenthesized or compact expression is still one word.
                        // Keep literal assignments on the selected-reference path.
                        let literal = words.len() == 3 && (words[2].parse::<f64>().is_ok() || !con::has_expression_operator(&words[2]));
                        if literal { con::set_statement(g, &ctx, &words) } else { g.run_script_console_source(&line, compile_ctx) }
                    } else {
                        match con::lookup(&func) {
                            Some(f) => f(g, &ctx, &words),
                            // Choose the parser before executing any host function.
                            // A runtime failure must never retry a side effect.
                            None if con::call_has_expression(g, &line, &func, &words) || g.data.table.unnumbered.contains_key(&func.to_ascii_lowercase()) => g.run_script_console_source(&line, compile_ctx),
                            None => con::call_words(g, on, &func, &words, "Console").map(|_| Vec::new()),
                        }
                    }
                }
                None => Ok(Vec::new()),
            }
        }
        "con_closeallmenus" => con::close_all_menus(g, &ctx, &[]),
        "con_quitgame" => con::quit_game(g, &ctx, &[]),
        // `name [dump]`: the dump's text file is not written [decision: NVForever writes
        // nothing beside the game].
        "con_save" => con::save_game(g, &ctx, &words[..words.len().min(1)]),
        "con_loadgame" => con::load_game(g, &ctx, &words[..words.len().min(1)]),
        "con_setgamesetting" => con::set_game_setting(g, &ctx, &words),
        "con_inv" => con::show_inventory(g, &ctx, &[]),
        "con_showvars" => con::show_vars(g, &ctx, &words),
        "con_sqv" => con::show_quest_vars(g, &ctx, &words),
        // The camera and toggles run the console's own commands.
        "con_setcamerafov" => con::call_words(g, subject, "SetCameraFOV", &words, "con_SetCameraFOV").map(|_| Vec::new()),
        "con_setufocamspeedmult" => con::call_words(g, subject, "SetUFOCamSpeedMult", &words, "Con_SetUFOCamSpeedMult").map(|_| Vec::new()),
        "con_tcl" => con::call_words(g, subject, "ToggleCollision", &[], "Con_TCL").map(|_| Vec::new()),
        "con_tfc" => con::call_words(g, subject, "ToggleFlyCam", &words, "Con_TFC").map(|_| Vec::new()),
        "con_togglemenus" => con::call_words(g, subject, "ToggleMenus", &[], "Con_ToggleMenus").map(|_| Vec::new()),
        // INI settings: the peer's store (`nvse_settings`); GetINISetting prints the
        // value. RefreshINI and SaveINI do nothing [decision: NVForever never writes the
        // game's INI files].
        // GetINISetting prints `name = value` [design: the console's line is not
        // documented].
        "con_getinisetting" => {
            let n = text(args, 0).unwrap_or_default();
            match crate::nvse_settings::call(g, "getstringinisetting", args) {
                Some(Ok(XVal::Str(v))) if !v.is_empty() => Ok(vec![format!("{n} = {v}")]),
                Some(Ok(XVal::Num(v))) => Ok(vec![format!("{n} = {v}")]),
                _ => Ok(vec![format!("No INI setting {n}")]),
            }
        }
        "con_setinisetting" => {
            // A number goes in as one; any other value as text.
            let numeric = matches!(args.get(1), Some(XVal::Num(_))) || text(args, 1).is_some_and(|t| t.trim().parse::<f64>().is_ok());
            let f = if numeric { "setnumericinisetting" } else { "setstringinisetting" };
            let vals: Vec<XVal> = match (numeric, text(args, 1)) {
                (true, Some(t)) => vec![args[0].clone(), XVal::Num(t.trim().parse().unwrap_or(0.0))],
                _ => args.to_vec(),
            };
            let _ = crate::nvse_settings::call(g, f, &vals);
            Ok(Vec::new())
        }
        // RefreshINI reads the files again, so the session's changes go [documented:
        // Con RefreshINI, "Refresh INI settings from files"]; SaveINI writes nothing [decision: NVForever
        // never writes the game's INI files].
        "con_refreshini" => {
            g.ini.reload();
            Ok(Vec::new())
        }
        "con_saveini" => Ok(Vec::new()),
        // [documented: Con_SCOF] console output to a file: kept as the name only (GetConsoleOutputFilename)
        // [decision: NVForever writes nothing beside the game].
        "con_scof" => {
            g.other.scof = text(args, 0).filter(|s| !s.is_empty());
            Ok(Vec::new())
        }
        // ToggleFootIK: the engine switches the feet's placing as the settings page does.
        "con_tfik" => {
            g.toggles.asked.push(crate::functions::toggles::Toggle::FootIk);
            Ok(Vec::new())
        }
        // ToggleDebugText and SetVel have no NVForever command [hypothesis: no debug
        // text to switch; SetVel's physics hook is not built].
        "con_tdt" | "con_setvel" => Ok(Vec::new()),
        _ => return None,
    };
    match run {
        Ok(lines) => {
            if g.other.echo {
                g.console_out.extend(lines);
            }
            Some(Ok(XVal::Num(0.0)))
        }
        // A console command's refusal is the script's error, and the script goes on.
        Err(e) => {
            g.console_out.push(format!("Script error: {name}: {e}"));
            Some(Ok(XVal::Num(0.0)))
        }
    }
}
