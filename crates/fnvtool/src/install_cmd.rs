//! `fnvtool setup` / `fnvtool paths`: NVForever's own home, kept apart from the game.

use crate::HomeArg;
use anyhow::{bail, Context};
use fnv_install::{check_separation, original_user_dir, Config, EngineHome, GameInstall};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn home_from(args: &HomeArg) -> anyhow::Result<EngineHome> {
    match &args.home {
        Some(h) => Ok(EngineHome::new(h)),
        None => EngineHome::user().context("cannot determine a home directory; pass --home or set NVFOREVER_HOME"),
    }
}

fn path_str(p: &std::path::Path) -> Value {
    json!(p.to_string_lossy())
}

fn report(home: &EngineHome, game: Option<&GameInstall>) -> Value {
    let exe = std::env::current_exe().ok();
    let mut v = json!({
        "schema": "nvforever.paths/1",
        "home": path_str(&home.root),
        "config_file": path_str(&home.config_file()),
        "plugins_file": path_str(&home.plugins_file()),
        "saves": path_str(&home.saves()),
        "logs": path_str(&home.logs()),
        "cache": path_str(&home.cache()),
        "mods": path_str(&home.mods()),
        "executable": exe.as_deref().map(path_str),
        "original_game_user_dir_never_touched": original_user_dir().as_deref().map(path_str),
    });
    match game {
        Some(g) => {
            let viol: Vec<String> = check_separation(home, g, exe.as_deref()).into_iter().map(|x| x.0).collect();
            v["game_install"] = path_str(&g.root);
            v["game_data_read_only"] = path_str(&g.data);
            v["missing_game_files"] = json!(g.missing_files());
            v["addon_masters"] = json!(g.addon_masters());
            v["separated"] = json!(viol.is_empty());
            v["violations"] = json!(viol);
        }
        None => {
            v["game_install"] = Value::Null;
            v["separated"] = Value::Null;
        }
    }
    v
}

pub fn paths(args: &HomeArg) -> anyhow::Result<Value> {
    let home = home_from(args)?;
    let cfg = Config::load(&home)?;
    let game = match &cfg.game_dir {
        Some(d) => Some(GameInstall::open(d)?),
        None => None,
    };
    Ok(report(&home, game.as_ref()))
}

pub fn setup(game_dir: &Path, args: &HomeArg) -> anyhow::Result<Value> {
    let home = home_from(args)?;
    let game = GameInstall::open(game_dir)?;
    game.check_files()?;
    let viol = check_separation(&home, &game, std::env::current_exe().ok().as_deref());
    if !viol.is_empty() {
        bail!(fnv_install::InstallError::Overlap(viol));
    }
    home.create()?;
    Config { game_dir: Some(PathBuf::from(&game.root)), ..Config::default() }.save(&home, Some(&game))?;
    Ok(report(&home, Some(&game)))
}
