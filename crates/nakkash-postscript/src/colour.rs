//! Colour spaces and the conversion of every colour to sRGB.
//!
//! Device grey and RGB are taken as sRGB. Device CMYK has no colour of its
//! own without a press profile, and no profile is assumed: the conversion
//! is the language's own rule, each additive primary the complement of its
//! subtractive one plus black, `r = 1 − min(1, c + k)` and so on. That rule
//! is fixed and documented, so the same file draws the same everywhere,
//! but it is darker than a press profile in rich blacks (see D71).
//!
//! Spaces built on others (indexed, separation, n-channel) are resolved
//! through their base or alternative space, running the program's tint
//! transform when they have one. CIE-based spaces are drawn as the device
//! space with the same number of components, and patterns are not drawn;
//! both are reported.

use std::rc::Rc;

use crate::interp::{Interp, PsError, R};
use crate::object::{Object, Value};
use crate::page::Rgb;

/// A colour space.
#[derive(Debug, Clone)]
pub(crate) enum Space {
    Gray,
    Rgb,
    Cmyk,
    Indexed {
        base: Rc<Space>,
        hival: usize,
        lookup: Lookup,
    },
    /// One colorant, or `None` that paints nothing, or `All` that paints on
    /// every one, through a tint transform into `alt`.
    Separation {
        alt: Rc<Space>,
        tint: Object,
        colorant: Colorant,
    },
    DeviceN {
        n: usize,
        alt: Rc<Space>,
        tint: Object,
    },
    /// A pattern space, drawn as nothing.
    Pattern {
        under: Option<Rc<Space>>,
    },
    /// A CIE-based space of `n` components, drawn as the device space of
    /// as many.
    Cie {
        n: usize,
    },
}

/// Where an indexed space finds its colours.
#[derive(Debug, Clone)]
pub(crate) enum Lookup {
    Table(Rc<[u8]>),
    Proc(Object),
}

/// Which colorant a separation names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Colorant {
    Named,
    All,
    None,
}

impl Space {
    /// How many components a colour in this space has.
    pub fn components(&self) -> usize {
        match self {
            Self::Gray | Self::Indexed { .. } | Self::Separation { .. } => 1,
            Self::Rgb => 3,
            Self::Cmyk => 4,
            Self::DeviceN { n, .. } | Self::Cie { n } => *n,
            Self::Pattern { under } => under.as_ref().map_or(0, |u| u.components()),
        }
    }

    /// The colour a space starts with when it is set.
    pub fn initial(&self) -> Vec<f64> {
        match self {
            Self::Cmyk => vec![0.0, 0.0, 0.0, 1.0],
            Self::Separation { .. } => vec![1.0],
            Self::DeviceN { n, .. } => vec![1.0; *n],
            Self::Pattern { .. } => Vec::new(),
            other => vec![0.0; other.components()],
        }
    }

    /// The range each sample of an image in this space decodes to, as a
    /// default `Decode` array gives it.
    pub fn default_decode(&self, bits: u32) -> Vec<f64> {
        match self {
            Self::Indexed { .. } => {
                let max = f64::from((1u32 << bits.min(16)) - 1);
                vec![0.0, max]
            }
            other => [0.0, 1.0].repeat(other.components()),
        }
    }
}

/// CMYK to sRGB by the language's own rule, without a press profile.
pub(crate) fn cmyk_to_rgb(c: f64, m: f64, y: f64, k: f64) -> Rgb {
    let channel = |v: f64| (1.0 - (v + k).min(1.0)) as f32;
    Rgb::new(channel(c), channel(m), channel(y))
}

/// HSB (hue, saturation, brightness, each 0–1) to RGB.
pub(crate) fn hsb_to_rgb(h: f64, s: f64, v: f64) -> [f64; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let sector = h.floor();
    let f = h - sector;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match sector as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// RGB to HSB, each 0–1.
pub(crate) fn rgb_to_hsb(r: f64, g: f64, b: f64) -> [f64; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let s = if max > 0.0 { delta / max } else { 0.0 };
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    [h, s, max]
}

impl Interp {
    /// The colour space a `setcolorspace` operand describes.
    pub fn colour_space(&mut self, object: Object) -> R<Space> {
        self.colour_space_depth(object, 0)
    }

    fn colour_space_depth(&mut self, object: Object, depth: u32) -> R<Space> {
        if depth > 8 {
            return Err(PsError::LimitCheck.into());
        }
        let (family, arr) = match object.value {
            Value::Name(name) => (self.vm.name_text(name), None),
            Value::Array(arr) | Value::Packed(arr) => {
                let first = self.vm.arr_get(arr, 0).ok_or(PsError::RangeCheck)?;
                let Value::Name(name) = first.value else {
                    return Err(PsError::TypeCheck.into());
                };
                (self.vm.name_text(name), Some(arr))
            }
            _ => return Err(PsError::TypeCheck.into()),
        };
        let element = |interp: &Self, index: usize| -> R<Object> {
            let arr = arr.ok_or(PsError::RangeCheck)?;
            interp
                .vm
                .arr_get(arr, index)
                .ok_or_else(|| PsError::RangeCheck.into())
        };
        Ok(match family.as_str() {
            "DeviceGray" | "G" => Space::Gray,
            "DeviceRGB" | "RGB" => Space::Rgb,
            "DeviceCMYK" | "CMYK" => Space::Cmyk,
            "Pattern" => {
                let under = match arr.map(|a| a.len) {
                    Some(len) if len > 1 => {
                        let base = element(self, 1)?;
                        Some(Rc::new(self.colour_space_depth(base, depth + 1)?))
                    }
                    _ => None,
                };
                Space::Pattern { under }
            }
            "Indexed" | "I" => {
                let base = element(self, 1)?;
                let base = Rc::new(self.colour_space_depth(base, depth + 1)?);
                let hival = element(self, 2)?.as_f64().ok_or(PsError::TypeCheck)?;
                let lookup = element(self, 3)?;
                let lookup = match lookup.value {
                    Value::String(s) if !lookup.exec => Lookup::Table(self.vm.str_bytes(s).into()),
                    _ => Lookup::Proc(lookup),
                };
                // A whole number the program wrote, clamped to the range.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let hival = hival.clamp(0.0, 4095.0) as usize;
                Space::Indexed {
                    base,
                    hival,
                    lookup,
                }
            }
            "Separation" => {
                let colorant = match element(self, 1)?.value {
                    Value::Name(name) => match self.vm.name_bytes(name) {
                        b"All" => Colorant::All,
                        b"None" => Colorant::None,
                        _ => Colorant::Named,
                    },
                    _ => Colorant::Named,
                };
                let alt = element(self, 2)?;
                let alt = Rc::new(self.colour_space_depth(alt, depth + 1)?);
                let tint = element(self, 3)?;
                Space::Separation {
                    alt,
                    tint,
                    colorant,
                }
            }
            "DeviceN" => {
                let names = element(self, 1)?.as_arr().ok_or(PsError::TypeCheck)?;
                let alt = element(self, 2)?;
                let alt = Rc::new(self.colour_space_depth(alt, depth + 1)?);
                let tint = element(self, 3)?;
                Space::DeviceN {
                    n: names.len as usize,
                    alt,
                    tint,
                }
            }
            "ICCBased" => {
                // A profile is never read; the component count picks the
                // device space, or the alternative space when there is one.
                let stream = element(self, 1)?;
                let mut space = Space::Rgb;
                if let Value::Dict(dict) = stream.value {
                    let alternate = self.vm.name(b"Alternate");
                    let n = self.vm.name(b"N");
                    let alt = self.vm.dict_get(dict, &crate::object::Key::Name(alternate));
                    let count = self
                        .vm
                        .dict_get(dict, &crate::object::Key::Name(n))
                        .and_then(Object::as_f64);
                    space = match (alt, count) {
                        (Some(alt), _) => self.colour_space_depth(alt, depth + 1)?,
                        (None, Some(n)) if n < 2.0 => Space::Gray,
                        (None, Some(n)) if n > 3.0 => Space::Cmyk,
                        _ => Space::Rgb,
                    };
                }
                space
            }
            "CIEBasedA" | "CalGray" => {
                self.unsupported("CIE-based colour, drawn as device colour");
                Space::Cie { n: 1 }
            }
            "CIEBasedABC" | "CIEBasedDEF" | "CalRGB" | "Lab" => {
                self.unsupported("CIE-based colour, drawn as device colour");
                Space::Cie { n: 3 }
            }
            "CIEBasedDEFG" => {
                self.unsupported("CIE-based colour, drawn as device colour");
                Space::Cie { n: 4 }
            }
            _ => return Err(PsError::Undefined.into()),
        })
    }

    /// The sRGB colour `components` give in `space`, or `None` when the
    /// colour paints nothing.
    pub fn resolve(&mut self, space: &Space, components: &[f64]) -> R<Option<Rgb>> {
        let at = |index: usize| {
            components
                .get(index)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 1.0)
        };
        Ok(match space {
            Space::Gray => {
                let g = at(0) as f32;
                Some(Rgb::new(g, g, g))
            }
            Space::Rgb => Some(Rgb::new(at(0) as f32, at(1) as f32, at(2) as f32)),
            Space::Cmyk => Some(cmyk_to_rgb(at(0), at(1), at(2), at(3))),
            Space::Cie { n } => match n {
                1 => Some(Rgb::new(at(0) as f32, at(0) as f32, at(0) as f32)),
                4 => Some(cmyk_to_rgb(at(0), at(1), at(2), at(3))),
                _ => Some(Rgb::new(at(0) as f32, at(1) as f32, at(2) as f32)),
            },
            Space::Indexed {
                base,
                hival,
                lookup,
            } => {
                let index = components.first().copied().unwrap_or(0.0).round();
                // Clamped into the table's range first.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let index = index.clamp(0.0, *hival as f64) as usize;
                let n = base.components();
                let values: Vec<f64> = match lookup {
                    Lookup::Table(table) => (0..n)
                        .map(|c| {
                            table
                                .get(index * n + c)
                                .map_or(0.0, |byte| f64::from(*byte) / 255.0)
                        })
                        .collect(),
                    Lookup::Proc(proc) => {
                        let index = i32::try_from(index).unwrap_or(0);
                        self.run_transform(*proc, &[f64::from(index)], n)?
                    }
                };
                self.resolve(base, &values)?
            }
            Space::Separation {
                alt,
                tint,
                colorant,
            } => match colorant {
                Colorant::None => None,
                Colorant::All => {
                    let g = 1.0 - at(0) as f32;
                    Some(Rgb::new(g, g, g))
                }
                Colorant::Named => {
                    let values = self.run_transform(
                        *tint,
                        &components[..1.min(components.len())],
                        alt.components(),
                    )?;
                    self.resolve(alt, &values)?
                }
            },
            Space::DeviceN { alt, tint, .. } => {
                let values = self.run_transform(*tint, components, alt.components())?;
                self.resolve(alt, &values)?
            }
            // Patterns are reported where they paint.
            Space::Pattern { .. } => None,
        })
    }

    /// Runs a tint transform or lookup procedure on `inputs` and takes
    /// `outputs` numbers back.
    fn run_transform(&mut self, proc: Object, inputs: &[f64], outputs: usize) -> R<Vec<f64>> {
        let depth = self.ostack.len();
        for value in inputs {
            self.push(Object::real(*value))?;
        }
        self.call(proc)?;
        if self.ostack.len() < depth + outputs {
            self.ostack.truncate(depth.min(self.ostack.len()));
            return Err(PsError::StackUnderflow.into());
        }
        let start = self.ostack.len() - outputs;
        let values = self.ostack[start..]
            .iter()
            .map(|o| o.as_f64().unwrap_or(0.0))
            .collect();
        self.ostack.truncate(depth);
        Ok(values)
    }

    /// Sets the colour space and its initial colour.
    pub fn set_space(&mut self, space: Space, object: Option<Object>) -> R {
        let colour = space.initial();
        let paint = self.resolve(&space, &colour)?;
        self.g.space = Rc::new(space);
        self.g.space_object = object;
        self.g.colour = colour;
        self.g.paint = paint;
        Ok(())
    }

    /// Sets the colour in the current space.
    pub fn set_colour(&mut self, colour: Vec<f64>) -> R {
        let space = Rc::clone(&self.g.space);
        self.g.paint = self.resolve(&space, &colour)?;
        self.g.colour = colour;
        Ok(())
    }
}
