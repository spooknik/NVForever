# Content-based UI true-color TGA decoding

[documented] Truevision's TGA 2.0 specification describes type 2 true-color data
and type 10 packets containing raw or repeated pixels. The identifier precedes
pixel data; descriptor bits 4 and 5 describe image origin. The optional footer
and extension areas need not be interpreted. Source: Truevision Technical Manual
2.2, January 1991, [format specification](https://swarm.workshop.perforce.com/files/guest/perforce_software/utils/image-plugins/src/TGAPlugins/TGA_File_Format_Spec.html).

[observed] Installed UI files can contain complete 32-bit TGA RLE streams despite
having DDS suffixes. Seven privately retained files decoded to the same complete
pixel buffers using an independent software decoder. The observed RGB values were
white, so those files do not establish colored channel order or native opacity.
No installed files, decoded pictures or game-derived byte fixtures are included.

[hypothesis: compatibility policy] Sniff supported true-color TGA content at the
existing UI texture-read boundary, independent of filename. Accept 24/32-bit type
2/10, with no color map or interleaving, nonzero dimensions and at most 4,000,000
pixels. Validate identifier, payload and packet bounds before access. Return a
failure for malformed recognized content rather than manufacture a placeholder.
The bound is an engineering resource limit, not a native-engine measurement.

[hypothesis: compatibility policy] Honor both origin bits and ignore absolute
canvas placement metadata for a texture. Preserve the fourth byte of 32-bit data
with either zero or eight declared attribute bits; 24-bit data is opaque. This
raw-byte policy is not evidence of native alpha interpretation. Optional trailing
metadata is not interpreted. Runs may cross scan lines within the checked image
extent; this is a permissive compatibility choice, whereas the specification
recommends ending run packets on scan-line boundaries. No strict TGA validator
claim follows from accepting these streams.

[documented: repository implementation] DDS magic retains the existing selected
DDS decoder, corruption errors, compressed format and top-mip behavior. Unknown
non-TGA content also retains that decoder. Supported TGA becomes one sRGB RGBA8
mip through the existing checked-image builder, selected VFS reader, image assets
and Pictures cache. No dependency, Cargo feature or script API is added. This does
not extend the DDS parser or change the font decoder itself.

[observed: qualified private CPU validation] Six exact codec unit controls passed.
The same authored public Pictures/VFS fixture failed on the original interface's
missing-image assertion and passed with the candidate; two additional DDS/error
consumer controls passed. That comparison used an exact-source private host with
real cached engine dependencies and only unrelated layout-test registration
omitted equally on both sides. It was not execution of a complete canonical
Cargo target or the current public whole tree. Public-source compilation, hosted
checks and rendered UI/native acceptance remain pending for this port. A future
TGA paperdoll anchor also needs separately qualified source-density handling;
the DDS-only CND metadata cache is outside this change.
