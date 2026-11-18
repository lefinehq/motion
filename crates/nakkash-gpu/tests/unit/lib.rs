use super::*;

#[test]
fn a_4k_grid_fits_the_smallest_binding_an_adapter_may_have() {
    // WebGPU's floor for `max_storage_buffer_binding_size` is 128 MiB.
    assert!(grid_bytes(3840, 2160) <= 128 << 20);
    assert!(grid_bytes(7680, 4320) > 128 << 20);
}

#[test]
fn every_matte_mode_has_its_own_number() {
    let rows = [
        MatteMode::Alpha,
        MatteMode::AlphaInverted,
        MatteMode::Luma,
        MatteMode::LumaInverted,
    ]
    .map(matte_row);
    assert_eq!(rows, [0, 1, 2, 3]);
}
