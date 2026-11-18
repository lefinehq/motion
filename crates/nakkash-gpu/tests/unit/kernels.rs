use super::*;
use nakkash_raster::BlendMode;

/// Every family's source as the device compiles it, by name.
fn sources() -> Vec<(&'static str, String, &'static FamilySpec)> {
    vec![
        ("pixel", with_blend_modes(PIXEL.source), &PIXEL),
        ("operate", with_operations(OPERATE.source), &OPERATE),
        ("amount", AMOUNT.source.to_owned(), &AMOUNT),
        ("unpack", UNPACK.source.to_owned(), &UNPACK),
        ("pack", PACK.source.to_owned(), &PACK),
        ("transform", TRANSFORM.source.to_owned(), &TRANSFORM),
        ("box_pass", BOX_PASS.source.to_owned(), &BOX_PASS),
        ("radial", RADIAL.source.to_owned(), &RADIAL),
        ("vector", VECTOR.source.to_owned(), &VECTOR),
        ("exact_box", EXACT_BOX.source.to_owned(), &EXACT_BOX),
    ]
}

#[test]
fn every_kernel_parses_and_validates() {
    for (name, source, spec) in sources() {
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&source)));
        for entry in spec.entries {
            assert!(
                module.entry_points.iter().any(|e| e.name == *entry),
                "{name} has the entry point {entry}"
            );
        }
        assert_eq!(
            module.entry_points.len(),
            spec.entries.len(),
            "{name} lists every entry point"
        );
    }
}

#[test]
fn every_blend_mode_has_a_constant_the_kernel_names() {
    let source = BLEND;
    for spec in BLEND_MODES {
        let name = format!("MODE_{}", spec.name.to_uppercase().replace('-', "_"));
        assert!(source.contains(&name), "the lay kernel names {name}");
    }
    assert_eq!(mode_row(BlendMode::Normal), 0);
}

#[test]
fn the_operations_read_luma_by_the_rasteriser_s_weights() {
    let [r, g, b] = nakkash_raster::LUMA;
    let weights = format!("p.r * {r:?} + p.g * {g:?} + p.b * {b:?}");
    assert!(OPERATION.contains(&weights), "{weights}");
}

#[test]
fn a_dispatch_past_the_limit_of_one_dimension_takes_two() {
    let ((x, y), row) = pixel_groups(3840 * 2160 * 4);
    assert!(x <= 65535);
    assert!(u64::from(x) * u64::from(y) * u64::from(PIXEL_GROUP) >= 3840 * 2160 * 4);
    assert_eq!(row, x * PIXEL_GROUP);
}

#[test]
fn parameters_are_laid_out_as_wgsl_lays_out_a_struct() {
    let bytes = Params::new().u32(1).f32(2.0).vec4([3.0; 4]).bytes();
    assert_eq!(bytes.len(), PARAMS_BYTES);
    assert_eq!(&bytes[0..4], &1u32.to_le_bytes());
    assert_eq!(&bytes[16..20], &3.0f32.to_le_bytes());
}
