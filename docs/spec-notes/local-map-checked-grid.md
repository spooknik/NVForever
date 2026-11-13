# Checked local-map grid descriptor

[observed: source at f30fb730bc6f263df98dbc60d0fa9437dd94654c]
`pipboy/data/outline.rs` derives a world-north grid from window, scale, center,
floor height and interior/world. It applies margin 1.15, an exterior minimum
extent, fixed 28/96-unit sample spacing, quarter-extent center snapping, rounded
128-unit floor buckets and ceil-rounded grid dimensions. Ordinary callers supply
fixed bounded map-window/scale inputs. This is not a live variable-recipient bug
reproduction or a native minimap camera specification.

[design] Factor that arithmetic into a scalar-only `OutlineGrid` descriptor.
The existing infallible ordinary Outline API keeps its valid-input arithmetic
and allocation behavior. A new checked constructor for future admission uses the
same derivation, validates finite positive inputs and derived values, checks
integer conversions, pixel count and prospective payload bytes against explicit
caller limits before allocating anything. The checked descriptor has private
fields and records the complete effective sampling frame, including actual feet,
floor bucket, origin, spacing, grid dimensions and rounded effective span.

[design] WorldNorth is the only admitted basis. Arbitrary pixel resolution and
oriented sampling are not introduced. Requested extent and ceil-rounded effective
span are distinct. Negative anchors and exact halfpoints retain Rust f32::round
behavior (half away from zero); no f64 arithmetic replaces the existing f32
geometry. f64 is used only to compare representability bounds before conversion.

[design] Prospective bytes include `count * size_of::<Option<f32>>()` heights,
`count` alpha and `count * 4` RGBA, checked with integer operations. These are
requested payload sizes, not allocator overhead, final Vec capacities, total
service accounting or GPU residency. A future caller must still reserve/charge
actual capacities and bound finalization work. The descriptor constructor itself
contains no Vec, Image creation, ray/query preparation, file access or allocation.

[design] Owned tests use explicit asymmetric grid/frame expectations, negative
and positive halfpoints, independent width/height/scale changes, same-effective-
extent controls, floor and exterior-minimum cases, and invalid/overflow/budget
rejection. Compare descriptor output to the actual legacy Outline frame on valid
cases as an equivalence control, not an alleged old failure. Existing three-field
OutlineKey remains unchanged; its extent-key improvement is a separate candidate.

[design] This prerequisite does not register a recipient, select a geometry
provider, publish a runtime image, bind a normal/projected painter or establish
native/GPU parity. Future recipient admission is deliberately not implemented.
