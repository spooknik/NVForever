//! Prints a worldspace climate's weathers: cloud textures, cloud colours per time of day,
//! cloud speeds and the NAM0 sky colours, for the sky renderer (docs/spec-notes/world.md §6).
//!
//! With `--survey` instead of a climate: for every weather with six times of day, how
//! often the high-noon and midnight colours equal the day and night ones, or are black.
//! With `--fog`: every weather's FNAM fog distances, flagging zero or empty (far <= near)
//! ranges, and which climates list each such weather.
//! With `--sounds`: every weather with sounds, a precipitation model, lightning or a
//! classification flag: its SNAM sounds by kind, MODL, DATA fades and lightning, and
//! which climates and regions list it (docs/spec-notes/world.md §6 "Weather sound").
//! With `--day`: every weather's NAM0 size and day sky-upper, sky-lower and horizon.
//! With `--worlds`: every worldspace's climate (WRLD CNAM), its parent and its plugin.
//! With `--imad`: every image space adapter's cinematic tracks (IAD 0x11-0x14 multiply
//! and 0x51-0x54 add) as (time, value) keys, and every image space's cinematic values.
//!
//! cargo run --release -p fnv-world --example weather_dump -- <Data dir> <esm schema dir> [climate EDID | --survey | --fog | --imad | --sounds]
//!
//! `WEATHER_DUMP_MASTERS` (comma-separated plugin names or paths, in load order) replaces
//! the New Vegas masters, e.g. to read Tale of Two Wastelands' Capital Wasteland weathers.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::fields::Fields;
use fnv_world::sky::{climate, weather, COLOR_TYPES};
use std::path::Path;

const MASTERS: [&str; 10] = [
    "FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm",
    "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm", "CaravanPack.esm",
];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let schema = Schema::load_dir(Path::new(&a[2])).unwrap();
    let want = a.get(3).map_or("NVDefaultClimate", |s| s.as_str());
    let masters: Vec<String> = std::env::var("WEATHER_DUMP_MASTERS")
        .map(|v| v.split(',').map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect())
        .unwrap_or_else(|_| MASTERS.iter().map(|m| m.to_string()).collect());
    let plugins: Vec<Plugin> = masters.iter().map(|m| Plugin::open(data.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    if want == "--survey" {
        survey(&lo, &schema);
        return;
    }
    if want == "--fog" {
        fog_survey(&lo, &schema);
        return;
    }
    if want == "--sounds" {
        sound_survey(&lo, &schema);
        return;
    }
    if want == "--day" {
        day_survey(&lo, &schema, &masters);
        return;
    }
    if want == "--worlds" {
        world_survey(&lo, &schema, &masters);
        return;
    }
    if want == "--imad" {
        imad_survey(&lo, &schema);
        return;
    }
    let mut clim_id = None;
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"CLMT") {
            let f = Fields::new(r, pi, &schema).unwrap();
            if f.string(b"EDID").is_some_and(|e| e.eq_ignore_ascii_case(want)) {
                clim_id = lo.to_global(pi, r.header.form_id);
            }
        }
    }
    let c = climate(&lo, &schema, clim_id.expect("climate not found")).unwrap();
    println!("climate {:?} sun {:?} glare {:?} sunrise {:?} sunset {:?}", c.edid, c.sun_texture, c.sun_glare_texture, c.sunrise, c.sunset);
    if let Some(ws) = a.get(4).and_then(|w| u32::from_str_radix(w.trim_start_matches("0x"), 16).ok()) {
        println!("image space {ws:#010x}:");
        dump_floats(&lo, &schema, ws);
    }
    let names = ["sky-upper", "fog", "unused", "ambient", "sunlight", "sun", "stars", "sky-lower", "horizon", "effect"];
    for wc in &c.weathers {
        let Some(w) = weather(&lo, &schema, wc.weather) else { continue };
        println!("\nweather {:?} chance {} global {:?}", w.edid, wc.chance, wc.global.map(|g| format!("{g:#010x}")));
        println!("  max cloud layers {:?}, speeds {:?}", w.max_cloud_layers, w.cloud_speeds);
        for l in 0..4 {
            println!("  cloud layer {l}: {:?}  colours {:?}", w.cloud_textures[l], w.cloud_colors.get(l));
        }
        for (t, set) in w.colors.iter().enumerate().take(COLOR_TYPES) {
            println!("  {:10} {:?}", names[t], set);
        }
        println!("  fog {:?}", w.fog);
        if let Some((pi, r)) = fnv_world::fields::winner(&lo, wc.weather) {
            if let Ok(f) = Fields::new(r, pi, &schema) {
                for s in f.subs.iter().filter(|s| s.sig[0] < 0x20) {
                    println!("  raw {:?} {:02x?}", String::from_utf8_lossy(&s.sig), s.data);
                    // The links are IMAD form IDs [documented: xEdit wbDefinitionsFNV.pas WTHR].
                    if let Ok(b) = <[u8; 4]>::try_from(&s.data[..]) {
                        if let Some(id) = fnv_world::fields::global(&lo, pi, u32::from_le_bytes(b)) {
                            dump_floats(&lo, &schema, id);
                        }
                    }
                }
            }
        }
    }
}

fn survey(lo: &LoadOrder, schema: &Schema) {
    let names = ["sky-upper", "fog", "unused", "ambient", "sunlight", "sun", "stars", "sky-lower", "horizon", "effect"];
    let (mut six, mut four) = (0, 0);
    // Per colour type: [noon == day, noon black, midnight == night, midnight black].
    let mut counts = [[0usize; 4]; COLOR_TYPES];
    let mut black_mid_ambient = Vec::new();
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"WTHR") {
            let Some(id) = lo.to_global(pi, r.header.form_id) else { continue };
            let Some(w) = weather(lo, schema, id) else { continue };
            if w.colors.first().is_none_or(|c| c.len() < 6) {
                four += 1;
                continue;
            }
            six += 1;
            for (t, set) in w.colors.iter().enumerate().take(COLOR_TYPES) {
                let c = &mut counts[t];
                c[0] += (set[4] == set[1]) as usize;
                c[1] += (set[4] == [0, 0, 0]) as usize;
                c[2] += (set[5] == set[3]) as usize;
                c[3] += (set[5] == [0, 0, 0]) as usize;
            }
            if w.colors[3][5] == [0, 0, 0] && w.colors[3][3] != [0, 0, 0] {
                black_mid_ambient.push(w.edid.clone().unwrap_or_default());
            }
        }
    }
    println!("weathers: {six} with six times, {four} with four");
    println!("{:10} {:>9} {:>10} {:>14} {:>14}", "type", "noon=day", "noon black", "midnight=night", "midnight black");
    for (t, c) in counts.iter().enumerate() {
        println!("{:10} {:9} {:10} {:14} {:14}", names[t], c[0], c[1], c[2], c[3]);
    }
    println!("black midnight ambient but non-black night ambient: {} {:?}", black_mid_ambient.len(), &black_mid_ambient[..black_mid_ambient.len().min(12)]);
}

fn fog_survey(lo: &LoadOrder, schema: &Schema) {
    // Weather form ID -> climates listing it.
    let mut users: std::collections::HashMap<u32, Vec<String>> = Default::default();
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"CLMT") {
            let Some(c) = lo.to_global(pi, r.header.form_id).and_then(|id| climate(lo, schema, id)) else { continue };
            for w in &c.weathers {
                users.entry(w.weather).or_default().push(c.edid.clone().unwrap_or_default());
            }
        }
    }
    let (mut total, mut no_fnam, mut zero, mut empty) = (0, 0, 0, 0);
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"WTHR") {
            let Some(id) = lo.to_global(pi, r.header.form_id) else { continue };
            if fnv_world::fields::winner(lo, id).is_some_and(|(wpi, _)| wpi != pi) {
                continue; // overridden later; the winner is listed in its own plugin
            }
            let Some(w) = weather(lo, schema, id) else { continue };
            total += 1;
            let has = Fields::new(r, pi, schema).is_ok_and(|f| f.get(b"FNAM").is_some());
            let f = w.fog;
            let tag = if !has {
                no_fnam += 1;
                "NO-FNAM"
            } else if f[..4].iter().all(|v| *v == 0.0) {
                zero += 1;
                "ZERO"
            } else if f[1] <= f[0] || f[3] <= f[2] {
                empty += 1;
                "EMPTY"
            } else {
                ""
            };
            let fog_col = w.colors.get(1).map(|c| format!("{:?}", &c[..c.len().min(4)])).unwrap_or_default();
            println!(
                "{:7} {:18} {:#010x} {:32} day {:>9.0}..{:<9.0} night {:>9.0}..{:<9.0} power {:.2}/{:.2} fogcol {fog_col} climates {:?}",
                tag,
                MASTERS[pi],
                id,
                w.edid.clone().unwrap_or_default(),
                f[0], f[1], f[2], f[3], f[4], f[5],
                users.get(&id).cloned().unwrap_or_default()
            );
        }
    }
    println!("weathers {total}: no FNAM {no_fnam}, all-zero {zero}, far <= near {empty}");
}

/// Every subrecord of a record as little-endian f32 values (image-space records hold floats,
/// counts and time/value key pairs [documented: xEdit wbDefinitionsFNV.pas IMGS, IMAD]).
fn dump_floats(lo: &LoadOrder, schema: &Schema, id: u32) {
    let Some((pi, r)) = fnv_world::fields::winner(lo, id) else { return };
    let Ok(f) = Fields::new(r, pi, schema) else { return };
    println!("    {} {:#010x} {:?}", String::from_utf8_lossy(&r.header.sig), id, f.string(b"EDID"));
    for s in f.subs.iter().filter(|s| &s.sig != b"EDID" && !s.data.is_empty()) {
        let v: Vec<String> = s.data.as_chunks::<4>().0.iter().map(|c| format!("{:.4}", f32::from_le_bytes(*c))).collect();
        println!("      {:?} [{}]", String::from_utf8_lossy(&s.sig), v.join(", "));
    }
}

/// Cinematic tracks of every IMAD and cinematic values of every IMGS (raw keys, no
/// interpretation beyond xEdit's layout [documented: wbDefinitionsFNV.pas IMAD, IMGS]).
fn imad_survey(lo: &LoadOrder, schema: &Schema) {
    let floats = |d: &[u8]| d.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect::<Vec<_>>();
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"IMGS") {
            let Ok(f) = Fields::new(r, pi, schema) else { continue };
            let Some(d) = f.raw(b"DNAM") else { continue };
            let base = if d.data.len() >= 152 { 100 } else { 96 };
            let v = floats(d.data.get(base..base + 32).unwrap_or_default());
            println!("IMGS {:18} {:40} sat/avglum/contrast/bright/tint/amount {:?}", MASTERS[pi], f.string(b"EDID").unwrap_or_default(), v);
        }
    }
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"IMAD") {
            let Ok(f) = Fields::new(r, pi, schema) else { continue };
            let track = |i: u8| {
                f.subs
                    .iter()
                    .find(|s| s.sig == [i, b'I', b'A', b'D'])
                    .map(|s| floats(&s.data).chunks(2).map(|k| format!("{}:{}", k[0], k.get(1).copied().unwrap_or(f32::NAN))).collect::<Vec<_>>().join(" "))
                    .unwrap_or_default()
            };
            println!(
                "IMAD {:18} {:40} 11[{}]+[{}] 12[{}]+[{}] 13[{}]+[{}] 14[{}]+[{}]",
                MASTERS[pi],
                f.string(b"EDID").unwrap_or_default(),
                track(0x11), track(0x51), track(0x12), track(0x52), track(0x13), track(0x53), track(0x14), track(0x54)
            );
        }
    }
}

fn sound_survey(lo: &LoadOrder, schema: &Schema) {
    let mut users: std::collections::HashMap<u32, Vec<String>> = Default::default();
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"CLMT") {
            let Some(c) = lo.to_global(pi, r.header.form_id).and_then(|id| climate(lo, schema, id)) else { continue };
            for w in &c.weathers {
                users.entry(w.weather).or_default().push(format!("CLMT {}", c.edid.clone().unwrap_or_default()));
            }
        }
    }
    for rg in fnv_world::sky::region_weathers(lo, schema) {
        for w in &rg.weathers {
            users.entry(w.weather).or_default().push(format!("REGN {}", rg.edid.clone().unwrap_or_default()));
        }
    }
    let edid = |id: u32| {
        fnv_world::fields::winner(lo, id)
            .and_then(|(pi, r)| Fields::new(r, pi, schema).ok()?.string(b"EDID"))
            .unwrap_or_else(|| format!("{id:#010x}"))
    };
    let (mut total, mut shown) = (0, 0);
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"WTHR") {
            let Some(id) = lo.to_global(pi, r.header.form_id) else { continue };
            if fnv_world::fields::winner(lo, id).is_some_and(|(wpi, _)| wpi != pi) {
                continue;
            }
            let Some(w) = weather(lo, schema, id) else { continue };
            total += 1;
            if w.sounds.is_empty() && w.precipitation_model.is_none() && w.lightning_frequency == 0 && w.classification == 0 {
                continue;
            }
            shown += 1;
            let sounds: Vec<String> = w.sounds.iter().map(|s| format!("{:?}:{}", s.kind, edid(s.sound))).collect();
            println!(
                "{:18} {:#010x} {:28} class {:#04x} wind {:3} precip {:?} fade {:?} lightning freq {} fade {:?} colour {:?} sounds [{}] used by {:?}",
                MASTERS[pi],
                id,
                w.edid.clone().unwrap_or_default(),
                w.classification,
                w.wind_speed,
                w.precipitation_model,
                w.precipitation_fade,
                w.lightning_frequency,
                w.lightning_fade,
                w.lightning_color,
                sounds.join(", "),
                users.get(&id).cloned().unwrap_or_default()
            );
        }
    }
    println!("{shown} of {total} weathers have sounds, precipitation, lightning or a classification");
}

fn world_survey(lo: &LoadOrder, schema: &Schema, masters: &[String]) {
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"WRLD") {
            let Ok(f) = Fields::new(r, pi, schema) else { continue };
            let clim = f.formid(b"CNAM", lo);
            let name = clim.and_then(|c| climate(lo, schema, c)).and_then(|c| c.edid);
            println!(
                "{:?} {:#010x} parent {:?} climate {:?} {:?} in {}",
                f.string(b"EDID"),
                lo.to_global(pi, r.header.form_id).unwrap_or(0),
                f.formid(b"WNAM", lo).map(|w| format!("{w:#010x}")),
                clim.map(|c| format!("{c:#010x}")),
                name,
                masters[pi]
            );
        }
    }
}

fn day_survey(lo: &LoadOrder, schema: &Schema, masters: &[String]) {
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"WTHR") {
            let Some(id) = lo.to_global(pi, r.header.form_id) else { continue };
            let Some(w) = weather(lo, schema, id) else { continue };
            let nam0 = fnv_world::fields::winner(lo, id)
                .and_then(|(wp, wr)| Fields::new(wr, wp, schema).ok())
                .and_then(|f| f.subs.iter().find(|s| &s.sig == b"NAM0").map(|s| s.data.len()));
            let day = |t: usize| w.colors.get(t).and_then(|c| c.get(1).copied());
            println!("{:?} {id:#010x} NAM0 {nam0:?} upper {:?} lower {:?} horizon {:?} in {}", w.edid, day(0), day(7), day(8), masters[pi]);
        }
    }
}
