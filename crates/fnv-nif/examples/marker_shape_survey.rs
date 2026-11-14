//! Diagnostic survey of editor-only geometry, for deciding which shapes the renderer
//! leaves out (docs/spec-notes/nif.md §6 "Helper geometry"):
//! - visible triangle shapes (not hidden, no hidden ancestor, not under an editor marker)
//!   with no property at all on themselves or an ancestor: by folder, by the name of the
//!   node holding them, and every such file outside landscape LOD;
//! - nodes with marker-like names (`MarkerSource`, `MarkerTarget`, `Capsule…`,
//!   `ChairBoundary`, any name holding "marker" or "boundary"): the visible shapes under
//!   them, with and without a shader property;
//! - BSXFlags bit 5 ("editor marker") and BSFurnitureMarker on the files involved.
//!
//! cargo run --release -p fnv-nif --example marker_shape_survey -- <schema dir> <archive.bsa>... [--list]

use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn top(title: &str, m: &BTreeMap<String, usize>, n: usize) {
    println!("\n{title}:");
    let mut v: Vec<_> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, c) in v.iter().take(n) {
        println!("  {c:7}  {k}");
    }
}

/// A node name without trailing digits, spaces, dots, colons or underscores, lower case.
fn stem(name: &str) -> String {
    name.trim_end_matches(|c: char| c.is_ascii_digit() || " ._:-".contains(c)).to_ascii_lowercase()
}

fn marker_like(name: &str) -> bool {
    let s = stem(name);
    s.contains("marker") || s.contains("boundary") || s.starts_with("capsule")
}

#[derive(Default)]
struct Count {
    shapes: usize,
    files: BTreeSet<String>,
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let list = a.iter().any(|x| x == "--list");
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let (mut files, mut shapes) = (0usize, 0usize);
    let mut bare = Count::default();
    let mut bare_lod = Count::default();
    let (mut bare_lod_uv, mut no_shader, mut no_shader_textured) = (0usize, 0usize, 0usize);
    let mut bare_lod_types: BTreeMap<String, usize> = BTreeMap::new();
    let mut helper = Count::default();
    let mut bare_by_folder: BTreeMap<String, usize> = BTreeMap::new();
    let mut bare_by_parent: BTreeMap<String, usize> = BTreeMap::new();
    let mut bare_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Marker-like node stem → (shapes with a shader, shapes without any property,
    // shapes with properties but no shader, files).
    let mut named: BTreeMap<String, (usize, usize, usize, BTreeSet<String>)> = BTreeMap::new();
    let mut named_shader_examples: Vec<String> = Vec::new();
    let (mut bsx5_files, mut bsx5_bare_files, mut furn_files, mut furn_bare_files) = (0usize, 0usize, 0usize, 0usize);
    let mut bare_bsx: BTreeMap<String, usize> = BTreeMap::new();
    // Files with property-less visible shapes, by whether the file holds any block
    // deriving from BSShaderProperty: (outside landscape LOD, landscape LOD).
    let mut shaded_file: BTreeMap<bool, (usize, usize)> = BTreeMap::new();
    for arc in &a[2..] {
        if arc.starts_with("--") {
            continue;
        }
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.ends_with(".nif")).collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            files += 1;
            let dec = Decoded::new(&file, &data, &schema);
            let objects = scene_objects(&file, &dec);
            let by_block: HashMap<usize, _> = objects.iter().map(|o| (o.block, o)).collect();
            let bsx = file
                .blocks
                .iter()
                .enumerate()
                .find(|(_, b)| b.type_name == "BSXFlags")
                .and_then(|(i, _)| dec.blocks[i].as_ref().ok())
                .and_then(|s| s.int("integer_data"));
            let bsx5 = bsx.is_some_and(|f| f & (1 << 5) != 0);
            let furn = file.blocks.iter().any(|b| b.type_name == "BSFurnitureMarker");
            bsx5_files += bsx5 as usize;
            furn_files += furn as usize;
            let lod = path.contains("landscape\\lod");
            let file_shaded = (0..file.blocks.len()).any(|i| dec.is_a(i, "BSShaderProperty"));
            let mut file_bare = Vec::new();
            let mut file_bare_lod = false;
            for o in objects.iter().filter(|o| !o.is_node) {
                let mut chain = vec![o.block];
                let mut cur = o.parent;
                while let Some(p) = cur {
                    if chain.len() > 512 {
                        break;
                    }
                    chain.push(p);
                    cur = by_block.get(&p).and_then(|x| x.parent);
                }
                let hidden = chain.iter().any(|&b| {
                    let s = dec.blocks[b].as_ref().ok();
                    let name = s.and_then(|s| s.text("name")).unwrap_or("");
                    s.and_then(|s| s.int("flags")).unwrap_or(0) & 1 != 0
                        || (name.len() >= 12 && name[..12].eq_ignore_ascii_case("editormarker"))
                });
                if hidden {
                    continue;
                }
                shapes += 1;
                let (mut any, mut shader, mut textured) = (false, false, false);
                for &b in &chain {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        if r >= file.blocks.len() {
                            continue;
                        }
                        any = true;
                        shader |= dec.is_a(r, "BSShaderProperty");
                        textured |= file.blocks[r].type_name == "NiTexturingProperty";
                    }
                }
                let parent_name = o.parent.and_then(|p| by_block.get(&p)).and_then(|p| p.name.clone()).unwrap_or_default();
                // The nearest marker-like name on the way up, the shape's own included.
                let marker = chain
                    .iter()
                    .filter_map(|b| by_block.get(b).and_then(|x| x.name.as_deref()))
                    .find(|n| marker_like(n))
                    .map(stem);
                if let Some(m) = marker {
                    let e = named.entry(m.clone()).or_default();
                    if shader {
                        e.0 += 1;
                        if named_shader_examples.len() < 40 {
                            named_shader_examples.push(format!("{path} :: {:?} under {m}", o.name));
                        }
                    } else if !any {
                        e.1 += 1;
                    } else {
                        e.2 += 1;
                    }
                    e.3.insert(path.clone());
                }
                if !shader {
                    no_shader += 1;
                    no_shader_textured += textured as usize;
                }
                if any && !shader && !textured {
                    helper.shapes += 1;
                    helper.files.insert(path.clone());
                }
                if any {
                    continue;
                }
                if lod {
                    if list && bare_lod.shapes < 10 {
                        println!("landscape LOD property-less: {path} :: {} {:?} block {} under {:?}", o.type_name, o.name, o.block, o.parent);
                    }
                    file_bare_lod = true;
                    let uv = dec.blocks[o.block]
                        .as_ref()
                        .ok()
                        .and_then(|s| s.block_ref("data"))
                        .and_then(|r| dec.get(r))
                        .is_some_and(|d| d.list("uv_sets").is_some_and(|l| !l.is_empty()));
                    bare_lod_uv += uv as usize;
                    *bare_lod_types.entry(o.type_name.clone()).or_default() += 1;
                    bare_lod.shapes += 1;
                    bare_lod.files.insert(path.clone());
                    continue;
                }
                bare.shapes += 1;
                bare.files.insert(path.clone());
                let folder = path.split('\\').take(3).collect::<Vec<_>>().join("\\");
                *bare_by_folder.entry(folder).or_default() += 1;
                *bare_by_parent.entry(stem(&parent_name)).or_default() += 1;
                let tris = dec.blocks[o.block]
                    .as_ref()
                    .ok()
                    .and_then(|s| s.block_ref("data"))
                    .and_then(|r| dec.get(r))
                    .map_or(0, fnv_nif::scene::triangle_count);
                file_bare.push(format!("{}<{parent_name}> {tris}t", o.name.as_deref().unwrap_or("?")));
            }
            if file_bare_lod {
                shaded_file.entry(file_shaded).or_default().1 += 1;
            }
            if !file_bare.is_empty() {
                shaded_file.entry(file_shaded).or_default().0 += 1;
                bsx5_bare_files += bsx5 as usize;
                furn_bare_files += furn as usize;
                *bare_bsx.entry(format!("bsx {bsx:?} bit5 {bsx5} furniture {furn}")).or_default() += 1;
                bare_files.insert(path.clone(), file_bare);
            }
        }
    }
    println!("files {files}, visible shapes {shapes}");
    println!(
        "no property at all: {} shapes in {} files outside landscape LOD, {} shapes in {} files of landscape LOD",
        bare.shapes,
        bare.files.len(),
        bare_lod.shapes,
        bare_lod.files.len()
    );
    println!("landscape LOD property-less shapes with texture coordinates: {bare_lod_uv}; by type: {bare_lod_types:?}");
    println!("without a shader property: {no_shader} shapes, {no_shader_textured} of them with a NiTexturingProperty");
    println!("properties but no shader and no texture (the helper rule): {} shapes in {} files", helper.shapes, helper.files.len());
    println!(
        "files with BSXFlags bit 5: {bsx5_files}, of them with property-less shapes: {bsx5_bare_files}; files with BSFurnitureMarker: {furn_files}, of them with property-less shapes: {furn_bare_files}"
    );
    for (shaded, (other, lod)) in &shaded_file {
        println!("files with property-less shapes, holding a BSShaderProperty block {shaded}: {other} outside landscape LOD, {lod} of landscape LOD");
    }
    top("property-less shapes (outside landscape LOD) by folder", &bare_by_folder, 40);
    top("property-less shapes (outside landscape LOD) by holding node's name stem", &bare_by_parent, 60);
    top("files with property-less shapes by BSXFlags / furniture marker", &bare_bsx, 40);
    println!("\nmarker-like node stems: shapes with a shader / without any property / properties without a shader, files:");
    let mut v: Vec<_> = named.iter().collect();
    v.sort_by(|a, b| (b.1 .0 + b.1 .1 + b.1 .2).cmp(&(a.1 .0 + a.1 .1 + a.1 .2)).then(a.0.cmp(b.0)));
    for (k, (s, b, h, f)) in v {
        println!("  {k:32} {s:6} {b:6} {h:6} {:6}", f.len());
    }
    println!("\nshaded shapes under marker-like nodes (first 40):");
    for e in &named_shader_examples {
        println!("  {e}");
    }
    if list {
        println!("\nfiles under each marker-like node stem:");
        for (k, (_, _, _, f)) in &named {
            println!("  {k}: {}", f.iter().cloned().collect::<Vec<_>>().join(", "));
        }
        println!("\nevery file with property-less shapes outside landscape LOD:");
        for (f, s) in &bare_files {
            println!("  {f}: {}", s.join(", "));
        }
    }
}
