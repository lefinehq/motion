//! Where a channel's speaker stands, and how it folds down to stereo.

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_8};

/// Where a channel's speaker stands, by the names WAV's channel mask gives
/// them, in the mask's bit order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    /// Front left.
    FrontLeft,
    /// Front right.
    FrontRight,
    /// Front centre.
    FrontCenter,
    /// The low-frequency effects channel.
    LowFrequency,
    /// Back left.
    BackLeft,
    /// Back right.
    BackRight,
    /// Between front left and front centre.
    FrontLeftOfCenter,
    /// Between front centre and front right.
    FrontRightOfCenter,
    /// Back centre.
    BackCenter,
    /// Side left.
    SideLeft,
    /// Side right.
    SideRight,
    /// Above the listener.
    TopCenter,
    /// Above, front left.
    TopFrontLeft,
    /// Above, front centre.
    TopFrontCenter,
    /// Above, front right.
    TopFrontRight,
    /// Above, back left.
    TopBackLeft,
    /// Above, back centre.
    TopBackCenter,
    /// Above, back right.
    TopBackRight,
    /// A speaker with no place a file says.
    Other,
}

/// The speakers in the bit order of WAV's channel mask.
const BY_MASK_BIT: [Speaker; 18] = [
    Speaker::FrontLeft,
    Speaker::FrontRight,
    Speaker::FrontCenter,
    Speaker::LowFrequency,
    Speaker::BackLeft,
    Speaker::BackRight,
    Speaker::FrontLeftOfCenter,
    Speaker::FrontRightOfCenter,
    Speaker::BackCenter,
    Speaker::SideLeft,
    Speaker::SideRight,
    Speaker::TopCenter,
    Speaker::TopFrontLeft,
    Speaker::TopFrontCenter,
    Speaker::TopFrontRight,
    Speaker::TopBackLeft,
    Speaker::TopBackCenter,
    Speaker::TopBackRight,
];

impl Speaker {
    /// Where `count` channels stand when a file does not say: the layouts
    /// WAV and FLAC share, from mono to 7.1, and no place beyond.
    pub fn default_layout(count: usize) -> Vec<Speaker> {
        use Speaker::{
            BackCenter, BackLeft, BackRight, FrontCenter, FrontLeft, FrontRight, LowFrequency,
            Other, SideLeft, SideRight,
        };
        match count {
            1 => vec![FrontCenter],
            2 => vec![FrontLeft, FrontRight],
            3 => vec![FrontLeft, FrontRight, FrontCenter],
            4 => vec![FrontLeft, FrontRight, BackLeft, BackRight],
            5 => vec![FrontLeft, FrontRight, FrontCenter, BackLeft, BackRight],
            6 => vec![
                FrontLeft,
                FrontRight,
                FrontCenter,
                LowFrequency,
                BackLeft,
                BackRight,
            ],
            7 => vec![
                FrontLeft,
                FrontRight,
                FrontCenter,
                LowFrequency,
                BackCenter,
                SideLeft,
                SideRight,
            ],
            8 => vec![
                FrontLeft,
                FrontRight,
                FrontCenter,
                LowFrequency,
                BackLeft,
                BackRight,
                SideLeft,
                SideRight,
            ],
            n => vec![Other; n],
        }
    }

    /// Where `count` channels stand by WAV's channel `mask`: the speakers of
    /// its set bits, lowest first, then [`Speaker::Other`] for channels
    /// beyond them. A mask of 0 says nothing, so the default layout holds.
    pub fn from_mask(mask: u32, count: usize) -> Vec<Speaker> {
        if mask == 0 {
            return Speaker::default_layout(count);
        }
        BY_MASK_BIT
            .iter()
            .enumerate()
            .filter(|(bit, _)| mask & (1 << bit) != 0)
            .map(|(_, speaker)| *speaker)
            .chain(std::iter::repeat(Speaker::Other))
            .take(count)
            .collect()
    }

    /// Its bit in WAV's channel mask, if it has a place.
    pub fn mask_bit(self) -> Option<u32> {
        BY_MASK_BIT
            .iter()
            .position(|s| *s == self)
            .and_then(|bit| u32::try_from(bit).ok())
    }

    /// Its gains into left and right when folding down to stereo, by the
    /// ITU-R BS.775 down-mix: a centre speaker goes to both sides 3 dB
    /// down, a surround one to its side 3 dB down, the low-frequency
    /// channel is left out, a speaker between two is panned between them
    /// at constant power, and one with no place goes to both 6 dB down.
    pub fn fold(self) -> (f32, f32) {
        let half = FRAC_1_SQRT_2;
        // Constant-power pan an eighth of a turn off centre.
        let (near, far) = (FRAC_PI_8.cos(), FRAC_PI_8.sin());
        match self {
            Speaker::FrontLeft | Speaker::TopFrontLeft => (1.0, 0.0),
            Speaker::FrontRight | Speaker::TopFrontRight => (0.0, 1.0),
            Speaker::FrontCenter | Speaker::TopCenter | Speaker::TopFrontCenter => (half, half),
            Speaker::LowFrequency => (0.0, 0.0),
            Speaker::BackLeft | Speaker::SideLeft | Speaker::TopBackLeft => (half, 0.0),
            Speaker::BackRight | Speaker::SideRight | Speaker::TopBackRight => (0.0, half),
            Speaker::FrontLeftOfCenter => (near, far),
            Speaker::FrontRightOfCenter => (far, near),
            Speaker::BackCenter | Speaker::TopBackCenter => (0.5, 0.5),
            Speaker::Other => (0.5, 0.5),
        }
    }
}
