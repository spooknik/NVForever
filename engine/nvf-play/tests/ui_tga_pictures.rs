//! Authored actual public Pictures/VFS/GameData/image-store regression; no renderer.
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use nvf_assets::GameData;
use nvf_play::interface::Pictures;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

fn tga(kind: u8, width: u16, height: u16, bits: u8, descriptor: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0, 0, kind, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes.extend([bits, descriptor]);
    bytes.extend(payload);
    bytes
}

struct OwnedFiles {
    root: PathBuf,
    files: Vec<PathBuf>,
}

impl OwnedFiles {
    fn new(entries: &[(&str, Vec<u8>)]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nvf-owned-ui-tga-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("textures")).unwrap();
        let mut files = Vec::new();
        for (name, bytes) in entries {
            assert!(!name.contains(['/', '\\']) && !name.is_empty());
            let path = root.join("textures").join(name);
            std::fs::write(&path, bytes).unwrap();
            files.push(path);
        }
        Self { root, files }
    }

    fn data(&self) -> GameData {
        let vfs = fnv_vfs::Vfs::open(&self.root, &[]).unwrap();
        let schema_root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif");
        let schema = fnv_nif::Schema::load_dir(&schema_root).unwrap();
        GameData {
            vfs: Arc::new(vfs),
            schema: Arc::new(schema),
        }
    }
}

impl Drop for OwnedFiles {
    fn drop(&mut self) {
        // Only individually tracked authored inputs and the two owned directories;
        // no recursive delete, path enumeration, game folder or external assets.
        for path in &self.files {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_dir(self.root.join("textures"));
        let _ = std::fs::remove_dir(&self.root);
    }
}

#[test]
fn pictures_use_real_vfs_content_under_dds_names_and_cache_colored_rgba_images() {
    let rgba = tga(10, 2, 1, 32, 32, &[1, 9, 17, 31, 0, 10, 18, 32, 204]);
    let rgb = tga(2, 2, 1, 24, 32, &[9, 17, 31, 10, 18, 32]);
    let files = OwnedFiles::new(&[
        ("owned_rgba.dds", rgba.clone()),
        ("owned_rgb.dds", rgb),
        ("owned_content.blob", rgba.clone()),
    ]);
    let data = files.data();
    assert_eq!(data.read("textures\\owned_rgba.dds"), Some(rgba));
    let mut images = Assets::<Image>::default();
    let mut pictures = Pictures::load(Some(&data));
    let (handle, rect, size) = pictures
        .picture_sized("owned_rgba.dds", None, Some(&data), &mut images, false)
        .unwrap();
    assert!(rect.is_none());
    assert_eq!(size, Vec2::new(2.0, 1.0));
    let image = images.get(&handle).unwrap();
    assert_eq!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8UnormSrgb
    );
    assert_eq!(image.texture_descriptor.mip_level_count, 1);
    assert_eq!(
        image.data.as_deref(),
        Some([31, 17, 9, 0, 32, 18, 10, 204].as_slice())
    );
    let again = pictures
        .own_sized("owned_rgba.dds", Some(&data), &mut images, false)
        .unwrap();
    assert_eq!(again.0.id(), handle.id());
    assert_eq!(images.len(), 1);
    let (rgb_handle, _, _) = pictures
        .own_sized("owned_rgb.dds", Some(&data), &mut images, false)
        .unwrap();
    assert_eq!(
        images.get(&rgb_handle).unwrap().data.as_deref(),
        Some([31, 17, 9, 255, 32, 18, 10, 255].as_slice())
    );
    let (other, _) = pictures
        .file("textures\\owned_content.blob", &data, &mut images, false)
        .unwrap();
    assert_eq!(
        images.get(&other).unwrap().data.as_deref(),
        Some([31, 17, 9, 0, 32, 18, 10, 204].as_slice())
    );
    assert_eq!(images.len(), 3);
}
