//! Diagnostic: parse every .dds under a folder of loose files (a mod's `Textures`) and
//! print the ones `Dds::parse` refuses, with why.
//!
//! cargo run --release -p fnv-dds --example scan_dir -- <folder>

fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dds")) {
            out.push(p);
        }
    }
}

fn main() {
    let root = std::env::args().nth(1).expect("a folder");
    let mut files = Vec::new();
    walk(std::path::Path::new(&root), &mut files);
    files.sort();
    let (mut refused, mut over) = (0, 0);
    for f in &files {
        let Ok(data) = std::fs::read(f) else { continue };
        match fnv_dds::Dds::parse(&data) {
            Ok(d) => {
                let extra = data.len() - fnv_dds::HEADER_LEN - d.header.expected_size(d.format);
                if extra > 0 {
                    over += 1;
                }
            }
            Err(e) => {
                refused += 1;
                println!("{}: {e}", f.display());
            }
        }
    }
    println!("{} files, {refused} refused, {over} with data after the last level", files.len());
}
