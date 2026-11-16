//! A layer's `mask` as the rasteriser's masks ([D55]): each mask's outline,
//! mode, side and values at this frame.
//!
//! [D55]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d55-masks-are-items-that-cut-a-layers-content-before-its-effects

use motion_format::item::props::{MASK_EXPAND, MASK_FEATHER, MASK_OPACITY, MASK_PATH};
use motion_raster::kurbo::BezPath;
use motion_raster::{LayerMask, MaskMode};
use motion_scene::{ItemState, bezier};

/// The rasteriser's masks for `masks`, in order.
pub(super) fn masks(masks: &[ItemState]) -> Vec<LayerMask> {
    masks.iter().map(mask).collect()
}

/// One mask. Its outline closes, as a filled outline does.
fn mask(item: &ItemState) -> LayerMask {
    LayerMask {
        path: bezier(item.path(MASK_PATH)),
        mode: mode(item.setting("mode")),
        inverted: item.setting("keep") == "outside",
        opacity: item.number(MASK_OPACITY) as f32,
        feather: item.point(MASK_FEATHER).map(|f| f as f32),
        expand: item.number(MASK_EXPAND) as f32,
    }
}

/// The outline of the mask `item`, as an effect follows it: open where
/// its path is, whatever its feather and expansion.
pub(super) fn outline(item: &ItemState) -> BezPath {
    bezier(item.path(MASK_PATH))
}

/// The rasteriser's mode for the scene's mode called `name`.
fn mode(name: &str) -> MaskMode {
    match name {
        "subtract" => MaskMode::Subtract,
        "intersect" => MaskMode::Intersect,
        "lighten" => MaskMode::Lighten,
        "darken" => MaskMode::Darken,
        "difference" => MaskMode::Difference,
        "none" => MaskMode::None,
        _ => MaskMode::Add,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/render/mask.rs"]
mod tests;
