//! `--mo2-profile <profile folder>`: a Mod Organizer 2 profile as a command's load order
//! (issue #2275). Before the arguments are parsed, the option is taken out and the
//! profile's active plugins are put at the end of the arguments as full paths, in load
//! order (`fnv_install::mo2::profile_plugins`), so every command that takes `<DATA_DIR>
//! <PLUGINS>...` reads the mod list as it is, without MO2 running and without a merged
//! Data folder (a full path joined to the data folder stays itself). The game's Data
//! folder that the base plugins come from is the instance's `gamePath` in
//! `ModOrganizer.ini` (two folders above the profile) plus `Data`. Only plugins are
//! resolved: a mod's loose files and archives are not seen. Plugins no mod has are
//! named on stderr and left out.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

const FLAG: &str = "--mo2-profile";

/// The arguments with `--mo2-profile <dir>` (or `--mo2-profile=<dir>`) replaced by the
/// profile's plugins at the end; unchanged when the option is not given.
pub fn expand(args: Vec<OsString>) -> anyhow::Result<Vec<OsString>> {
    let mut out = Vec::with_capacity(args.len());
    let mut profile: Option<PathBuf> = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let s = a.to_string_lossy();
        if s == FLAG {
            let v = it.next().ok_or_else(|| anyhow::anyhow!("{FLAG} needs a profile folder"))?;
            profile = Some(PathBuf::from(v));
        } else if let Some(v) = s.strip_prefix("--mo2-profile=") {
            profile = Some(PathBuf::from(v));
        } else {
            out.push(a);
        }
    }
    let Some(profile) = profile else { return Ok(out) };
    let data = instance_data(&profile)?;
    let got = fnv_install::mo2::profile_plugins(&profile, &data)?;
    for m in &got.missing {
        eprintln!("warning: {FLAG}: {m} is in the profile's plugins.txt but in no active mod nor in {}", data.display());
    }
    if got.found.is_empty() {
        anyhow::bail!("{FLAG}: no plugins found for {}", profile.display());
    }
    out.extend(got.found.into_iter().map(OsString::from));
    Ok(out)
}

/// `<gamePath>\Data` of the instance that holds `profile` (`<instance>\profiles\<name>`).
fn instance_data(profile: &Path) -> anyhow::Result<PathBuf> {
    let instance = profile.parent().and_then(Path::parent).ok_or_else(|| anyhow::anyhow!("{FLAG}: {} is not in an instance's profiles folder", profile.display()))?;
    let ini_path = instance.join("ModOrganizer.ini");
    let ini = std::fs::read_to_string(&ini_path).map_err(|e| anyhow::anyhow!("{FLAG}: {}: {e}", ini_path.display()))?;
    let game = fnv_install::mo2::ini_game_path(&ini).ok_or_else(|| anyhow::anyhow!("{FLAG}: {} names no gamePath", ini_path.display()))?;
    Ok(game.join("Data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_option_leaves_the_arguments_alone() {
        let a: Vec<OsString> = ["fnvtool", "ext", "report", "D:\\Data", "FalloutNV.esm"].map(OsString::from).to_vec();
        assert_eq!(expand(a.clone()).unwrap(), a);
    }

    #[test]
    fn a_profile_without_its_value_is_an_error() {
        let a: Vec<OsString> = ["fnvtool", "ext", "report", FLAG].map(OsString::from).to_vec();
        assert!(expand(a).is_err());
    }
}
