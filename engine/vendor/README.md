# Patched dependencies

Crates copied from crates.io with a small change, used through `[patch.crates-io]` in
`engine/Cargo.toml`. Each keeps its own licence files. When the crate is upgraded, copy
the new version here and apply the change again (or drop the patch if upstream fixed it).

## gpu-allocator 0.28.0 (MIT OR Apache-2.0, Traverse-Research/gpu-allocator)

Used by wgpu for Vulkan memory. Change, in `src/vulkan/mod.rs` `MemoryType::free`: an
emptied general memory block is kept for reuse while fewer than
`KEEP_EMPTY_GENERAL_BLOCKS` (2) other empty general blocks of that memory type exist;
upstream destroys every emptied block except the last.

Still needed with Bevy 0.20.1 (#934): its wgpu-hal 30 uses gpu-allocator 0.28, and 0.28.0
is still the latest release, with the same free policy.

Why (#897): late in long sessions the block wgpu keeps fills with long-lived data, so each
frame's short-lived staging memory spills into a new block, which empties when the frame
is done and was destroyed at once. An Nsight Systems capture of the owner's game showed
about 285 blocks of 128 MiB made and freed a second, each zeroed and paged in by Windows,
and the render thread stalling 100-800 ms in the driver meanwhile.

Second change (#1429), in `src/vulkan/mod.rs` `MemoryBlock::new` and `src/lib.rs`: a
`vkAllocateMemory` that takes 1 ms or more logs `gpu memory: vkAllocateMemory of <n> MiB
(memory type <t>, host-visible|device-local, a general block|dedicated) took <ms> ms`, so a
single slow frame charged to a render system that only writes a buffer
(`upload_light_probes`, `write_binned_instance_buffers`) can be matched to a new memory
block. `std` is linked for the clock (wgpu-hal turns the crate's `std` feature off).

## Dropped

- bevy_pbr 0.19.1 (#903, PR #905): the bindless index-table comparator in
  `MaterialBindlessSlab::get_bindless_index_table`, reversed in 0.19.1, which kept freed
  `ExtendedMaterial`s' textures in the slabs. Fixed upstream (bevyengine/bevy#25848);
  Bevy 0.20 has the fix, in `bevy_render::material_bind_groups` (#934).
