//! A grid of float pixels in the GPU's memory: what a [`Resident`] holds
//! for this crate.

use nakkash_raster::Resident;

use crate::pool::Pooled;

/// `width` by `height` premultiplied RGBA pixels, four 32-bit floats each,
/// row by row from the top: the layout of the rasteriser's `Pixels`.
pub(crate) struct Grid {
    pub(crate) pixels: Pooled,
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl Grid {
    /// How many pixels it holds.
    pub(crate) fn count(&self) -> u32 {
        u32::from(self.width) * u32::from(self.height)
    }

    /// The buffer that holds them.
    pub(crate) fn buffer(&self) -> &wgpu::Buffer {
        self.pixels.buffer()
    }

    /// The grid as pixels the painter moves between calls.
    pub(crate) fn into_resident(self) -> Resident {
        let (width, height) = (self.width, self.height);
        Resident::new(width, height, Box::new(self))
    }
}

/// The grid a [`Resident`] holds, if this crate made it.
pub(crate) fn grid(resident: &Resident) -> &Grid {
    resident
        .held::<Grid>()
        .expect("pixels held by this GPU are its grids")
}

/// The grid a [`Resident`] holds, to change, if this crate made it.
pub(crate) fn grid_mut(resident: &mut Resident) -> &mut Grid {
    resident
        .held_mut::<Grid>()
        .expect("pixels held by this GPU are its grids")
}
