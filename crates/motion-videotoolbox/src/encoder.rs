//! One compression session, safe to use: frames in, in order, packets out.

use std::ffi::c_void;
use std::fmt;
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::{Arc, Mutex, PoisonError};

use objc2_core_foundation::{
    CFBoolean, CFData, CFDictionary, CFNumber, CFRetained, CFString, CFType,
};
use objc2_core_media::{
    CMSampleBuffer, CMTime, CMVideoCodecType,
    kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms, kCMSampleAttachmentKey_NotSync,
    kCMTimeInvalid, kCMVideoCodecType_H264, kCMVideoCodecType_HEVC,
};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddressOfPlane,
    CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, kCVImageBufferColorPrimaries_ITU_R_709_2,
    kCVImageBufferTransferFunction_ITU_R_709_2, kCVImageBufferYCbCrMatrix_ITU_R_709_2,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
};
use objc2_video_toolbox::{
    VTCompressionSession, VTEncodeInfoFlags, VTSessionSetProperty,
    kVTCompressionPropertyKey_AllowFrameReordering, kVTCompressionPropertyKey_ColorPrimaries,
    kVTCompressionPropertyKey_ExpectedFrameRate, kVTCompressionPropertyKey_MaxKeyFrameInterval,
    kVTCompressionPropertyKey_ProfileLevel, kVTCompressionPropertyKey_Quality,
    kVTCompressionPropertyKey_RealTime, kVTCompressionPropertyKey_TransferFunction,
    kVTCompressionPropertyKey_YCbCrMatrix, kVTProfileLevel_H264_High_AutoLevel,
    kVTProfileLevel_HEVC_Main_AutoLevel,
};

/// The ticks of a frame: the timescale is the frame rate times this, so a
/// rate to a thousandth of a frame per second is exact.
const FRAME_TICKS: i64 = 1000;

/// The codecs of the system's encoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// H.264, High profile.
    H264,
    /// HEVC, Main profile.
    Hevc,
}

impl Codec {
    fn video_type(self) -> CMVideoCodecType {
        match self {
            Codec::H264 => kCMVideoCodecType_H264,
            Codec::Hevc => kCMVideoCodecType_HEVC,
        }
    }

    /// The name of the box that holds the decoder's configuration in MP4.
    fn record_box(self) -> &'static str {
        match self {
            Codec::H264 => "avcC",
            Codec::Hevc => "hvcC",
        }
    }
}

/// What a session encodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The codec.
    pub codec: Codec,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Frames per second, above zero.
    pub fps: f64,
    /// From 0 (smallest) to 1 (closest to the frames).
    pub quality: f32,
    /// A keyframe at least this often, in frames.
    pub keyframe_interval: u32,
}

/// One frame as BT.709 limited-range 4:2:0 planes, rows without padding:
/// chroma is half the luma's size both ways, rounded up.
#[derive(Debug, Clone, Copy)]
pub struct Planes<'a> {
    /// Luma.
    pub y: &'a [u8],
    /// Blue-difference chroma.
    pub cb: &'a [u8],
    /// Red-difference chroma.
    pub cr: &'a [u8],
}

/// One compressed frame: NAL units, each after its length in four bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    /// The NAL units.
    pub data: Vec<u8>,
    /// The number of the frame it shows, counting from 0.
    pub frame: u64,
    /// Whether it decodes without any frame before it.
    pub keyframe: bool,
}

/// A call into the system that failed, with the status it returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// What was being done.
    pub doing: &'static str,
    /// The system's status code.
    pub status: i32,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed with status {}", self.doing, self.status)
    }
}

impl std::error::Error for Error {}

/// `Ok` when `status` is the system's success, else the error of `doing`.
fn check(doing: &'static str, status: i32) -> Result<(), Error> {
    match status {
        0 => Ok(()),
        status => Err(Error { doing, status }),
    }
}

/// What the session's callback hands back, kept apart from the session so
/// the callback can reach it from the system's threads.
struct Output {
    codec: Codec,
    packets: Vec<Packet>,
    /// The decoder's configuration record, from the first frame.
    config: Option<Vec<u8>>,
    failure: Option<Error>,
}

/// One compression session.
pub struct Encoder {
    session: CFRetained<VTCompressionSession>,
    /// Shared with the callback, which holds a pointer to it until the
    /// session is invalidated in `drop`.
    output: Arc<Mutex<Output>>,
    settings: Settings,
    timescale: i32,
    frames: u64,
}

// SAFETY: a compression session may be called from any thread, one call at
// a time, which `&mut self` on every call that touches it ensures; the
// output it shares with the system's threads is behind a mutex.
unsafe impl Send for Encoder {}

impl Encoder {
    /// A session for `settings`, tagged BT.709, without reordered frames so
    /// packets come in the order they are shown.
    ///
    /// # Errors
    /// When the system has no encoder for the codec at this size, or refuses
    /// a setting.
    pub fn new(settings: Settings) -> Result<Encoder, Error> {
        let output = Arc::new(Mutex::new(Output {
            codec: settings.codec,
            packets: Vec::new(),
            config: None,
            failure: None,
        }));
        let refcon = Arc::as_ptr(&output).cast_mut().cast::<c_void>();
        let mut raw: *mut VTCompressionSession = ptr::null_mut();
        let width = i32::try_from(settings.width).unwrap_or(i32::MAX);
        let height = i32::try_from(settings.height).unwrap_or(i32::MAX);
        // SAFETY: no dictionaries are passed; `finished` has the callback's
        // signature; `refcon` points at the output, which outlives the
        // session (see `drop`); `raw` is a valid place for the result.
        let status = unsafe {
            VTCompressionSession::create(
                None,
                width,
                height,
                settings.codec.video_type(),
                None,
                None,
                None,
                Some(finished),
                refcon,
                NonNull::from(&mut raw),
            )
        };
        check("creating a compression session", status)?;
        let raw = NonNull::new(raw).ok_or(Error {
            doing: "creating a compression session",
            status: -1,
        })?;
        // SAFETY: a session from `create` is ours to release.
        let session = unsafe { CFRetained::from_raw(raw) };
        let encoder = Encoder {
            session,
            output,
            settings,
            timescale: (settings.fps * FRAME_TICKS as f64).round() as i32,
            frames: 0,
        };
        encoder.configure()?;
        Ok(encoder)
    }

    /// Sets the session's properties.
    fn configure(&self) -> Result<(), Error> {
        let profile = match self.settings.codec {
            // SAFETY: reading a constant of the framework.
            Codec::H264 => unsafe { kVTProfileLevel_H264_High_AutoLevel },
            // SAFETY: reading a constant of the framework.
            Codec::Hevc => unsafe { kVTProfileLevel_HEVC_Main_AutoLevel },
        };
        let quality = CFNumber::new_f32(self.settings.quality);
        let interval = CFNumber::new_i32(self.settings.keyframe_interval as i32);
        let rate = CFNumber::new_f64(self.settings.fps);
        // SAFETY: reading constants of the framework.
        let primaries = unsafe { kCVImageBufferColorPrimaries_ITU_R_709_2 };
        // SAFETY: reading constants of the framework.
        let transfer = unsafe { kCVImageBufferTransferFunction_ITU_R_709_2 };
        // SAFETY: reading constants of the framework.
        let matrix = unsafe { kCVImageBufferYCbCrMatrix_ITU_R_709_2 };
        // SAFETY: reading a constant of the framework.
        let real_time = unsafe { kVTCompressionPropertyKey_RealTime };
        // SAFETY: reading a constant of the framework.
        let reordering = unsafe { kVTCompressionPropertyKey_AllowFrameReordering };
        // SAFETY: reading a constant of the framework.
        let profile_key = unsafe { kVTCompressionPropertyKey_ProfileLevel };
        // SAFETY: reading a constant of the framework.
        let quality_key = unsafe { kVTCompressionPropertyKey_Quality };
        // SAFETY: reading a constant of the framework.
        let interval_key = unsafe { kVTCompressionPropertyKey_MaxKeyFrameInterval };
        // SAFETY: reading a constant of the framework.
        let rate_key = unsafe { kVTCompressionPropertyKey_ExpectedFrameRate };
        // SAFETY: reading a constant of the framework.
        let primaries_key = unsafe { kVTCompressionPropertyKey_ColorPrimaries };
        // SAFETY: reading a constant of the framework.
        let transfer_key = unsafe { kVTCompressionPropertyKey_TransferFunction };
        // SAFETY: reading a constant of the framework.
        let matrix_key = unsafe { kVTCompressionPropertyKey_YCbCrMatrix };
        let properties: [(&CFString, &CFType); 9] = [
            (real_time, CFBoolean::new(false)),
            (reordering, CFBoolean::new(false)),
            (profile_key, profile),
            (quality_key, &quality),
            (interval_key, &interval),
            (rate_key, &rate),
            (primaries_key, primaries),
            (transfer_key, transfer),
            (matrix_key, matrix),
        ];
        for (key, value) in properties {
            // SAFETY: each key is a compression property, and each value
            // the type the framework documents for it.
            let status = unsafe { VTSessionSetProperty(&self.session, key, Some(value)) };
            check("setting a compression property", status)?;
        }
        Ok(())
    }

    /// Takes the next frame, and returns the packets ready so far.
    ///
    /// # Errors
    /// When the planes are not of the session's size, or the system fails.
    pub fn encode(&mut self, planes: Planes<'_>) -> Result<Vec<Packet>, Error> {
        let buffer = self.pixel_buffer(planes)?;
        let frame = self.frames;
        // SAFETY: making a time value has no preconditions.
        let shown = unsafe { CMTime::new(frame as i64 * FRAME_TICKS, self.timescale) };
        // SAFETY: making a time value has no preconditions.
        let duration = unsafe { CMTime::new(FRAME_TICKS, self.timescale) };
        // SAFETY: `buffer` is an image buffer; no frame properties are
        // passed; the frame's reference is its number, which the callback
        // reads as a number and never follows; no flags are asked for.
        let status = unsafe {
            self.session.encode_frame(
                &buffer,
                shown,
                duration,
                None,
                frame as usize as *mut c_void,
                ptr::null_mut(),
            )
        };
        check("encoding a frame", status)?;
        self.frames += 1;
        self.take()
    }

    /// Encodes every frame still held and returns their packets.
    ///
    /// # Errors
    /// When the system fails.
    pub fn finish(&mut self) -> Result<Vec<Packet>, Error> {
        // SAFETY: reading a constant of the framework.
        let all = unsafe { kCMTimeInvalid };
        // SAFETY: an invalid time asks for every pending frame.
        let status = unsafe { self.session.complete_frames(all) };
        check("completing the frames", status)?;
        self.take()
    }

    /// The decoder's configuration record (`avcC` or `hvcC`), once a frame
    /// has come out.
    pub fn config(&self) -> Option<Vec<u8>> {
        self.output
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .config
            .clone()
    }

    /// The packets the callback has handed back, or its first failure.
    fn take(&self) -> Result<Vec<Packet>, Error> {
        let mut output = self.output.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(failure) = output.failure.take() {
            return Err(failure);
        }
        Ok(std::mem::take(&mut output.packets))
    }

    /// A pixel buffer holding `planes`, luma and interleaved chroma.
    fn pixel_buffer(&self, planes: Planes<'_>) -> Result<CFRetained<CVPixelBuffer>, Error> {
        let (width, height) = (self.settings.width as usize, self.settings.height as usize);
        let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
        if planes.y.len() != width * height
            || planes.cb.len() != chroma_width * chroma_height
            || planes.cr.len() != chroma_width * chroma_height
        {
            return Err(Error {
                doing: "taking planes of another size",
                status: -1,
            });
        }
        let mut raw: *mut CVPixelBuffer = ptr::null_mut();
        // SAFETY: no attributes are passed; `raw` is a valid place for the
        // result.
        let status = unsafe {
            CVPixelBufferCreate(
                None,
                width,
                height,
                kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                None,
                NonNull::from(&mut raw),
            )
        };
        check("making a pixel buffer", status)?;
        let raw = NonNull::new(raw).ok_or(Error {
            doing: "making a pixel buffer",
            status: -1,
        })?;
        // SAFETY: a buffer from `CVPixelBufferCreate` is ours to release.
        let buffer = unsafe { CFRetained::from_raw(raw) };
        let lock = CVPixelBufferLockFlags(0);
        // SAFETY: locking for writing a buffer we own.
        let status = unsafe { CVPixelBufferLockBaseAddress(&buffer, lock) };
        check("locking a pixel buffer", status)?;
        let written = match (plane(&buffer, 0), plane(&buffer, 1)) {
            (Some((luma, luma_stride)), Some((chroma, chroma_stride))) => {
                // SAFETY: the buffer is locked and ours alone until unlocked;
                // plane 0 holds `height` rows of `luma_stride` bytes.
                let luma =
                    unsafe { slice::from_raw_parts_mut(luma.as_ptr(), luma_stride * height) };
                // SAFETY: as above, plane 1 holds `chroma_height` rows of
                // `chroma_stride` bytes, apart from plane 0.
                let chroma = unsafe {
                    slice::from_raw_parts_mut(chroma.as_ptr(), chroma_stride * chroma_height)
                };
                for (row, line) in planes.y.chunks_exact(width).enumerate() {
                    luma[row * luma_stride..][..width].copy_from_slice(line);
                }
                let rows = planes
                    .cb
                    .chunks_exact(chroma_width)
                    .zip(planes.cr.chunks_exact(chroma_width));
                for (row, (cb, cr)) in rows.enumerate() {
                    let line = &mut chroma[row * chroma_stride..][..chroma_width * 2];
                    for (pair, (&b, &r)) in line.chunks_exact_mut(2).zip(cb.iter().zip(cr)) {
                        pair.copy_from_slice(&[b, r]);
                    }
                }
                true
            }
            _ => false,
        };
        // SAFETY: unlocking the lock taken above, with the same flags.
        let unlocked = unsafe { CVPixelBufferUnlockBaseAddress(&buffer, lock) };
        if !written {
            return Err(Error {
                doing: "reaching a pixel buffer's planes",
                status: -1,
            });
        }
        check("unlocking a pixel buffer", unlocked)?;
        Ok(buffer)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: tearing the session down before the output it calls back
        // into is dropped; no call reaches the callback after this returns.
        unsafe { self.session.invalidate() };
    }
}

/// Where plane `index` of a locked `buffer` starts, and the bytes a row
/// takes; none when the system gives no address.
fn plane(buffer: &CVPixelBuffer, index: usize) -> Option<(NonNull<u8>, usize)> {
    let base = CVPixelBufferGetBaseAddressOfPlane(buffer, index);
    let stride = CVPixelBufferGetBytesPerRowOfPlane(buffer, index);
    Some((NonNull::new(base.cast::<u8>())?, stride))
}

/// The session's callback: one compressed frame, or a failure, on one of the
/// system's threads.
unsafe extern "C-unwind" fn finished(
    output: *mut c_void,
    frame: *mut c_void,
    status: i32,
    _flags: VTEncodeInfoFlags,
    sample: *mut CMSampleBuffer,
) {
    // SAFETY: `output` is the pointer `Encoder::new` gave the session, to an
    // output that outlives it.
    let output = unsafe { &*output.cast_const().cast::<Mutex<Output>>() };
    let mut output = output.lock().unwrap_or_else(PoisonError::into_inner);
    if status != 0 {
        output.failure.get_or_insert(Error {
            doing: "encoding a frame",
            status,
        });
        return;
    }
    // A frame the encoder dropped has no sample.
    let Some(sample) = NonNull::new(sample) else {
        return;
    };
    // SAFETY: the system passes a valid sample for the length of the call.
    let sample = unsafe { sample.as_ref() };
    match read(sample, frame as usize as u64) {
        Ok(packet) => {
            if output.config.is_none() {
                output.config = config(sample, output.codec);
            }
            output.packets.push(packet);
        }
        Err(failure) => {
            output.failure.get_or_insert(failure);
        }
    }
}

/// A sample's bytes and whether it is a keyframe.
fn read(sample: &CMSampleBuffer, frame: u64) -> Result<Packet, Error> {
    let failure = Error {
        doing: "reading a compressed frame",
        status: -1,
    };
    // SAFETY: a valid sample.
    let block = unsafe { sample.data_buffer() }.ok_or(failure.clone())?;
    // SAFETY: a valid block buffer.
    let length = unsafe { block.data_length() };
    let mut data = vec![0u8; length];
    if length > 0 {
        let destination = NonNull::from(data.as_mut_slice()).cast::<c_void>();
        // SAFETY: `destination` holds `length` bytes.
        let status = unsafe { block.copy_data_bytes(0, length, destination) };
        check("reading a compressed frame", status)?;
    }
    Ok(Packet {
        data,
        frame,
        keyframe: is_keyframe(sample),
    })
}

/// Whether a sample decodes on its own: whether it lacks the "not sync"
/// attachment.
fn is_keyframe(sample: &CMSampleBuffer) -> bool {
    // SAFETY: a valid sample; nothing is created.
    let Some(attachments) = (unsafe { sample.sample_attachments_array(false) }) else {
        return true;
    };
    // SAFETY: the framework documents one dictionary per sample.
    let attachments = unsafe { attachments.cast_unchecked::<CFDictionary>() };
    let Some(first) = attachments.get(0) else {
        return true;
    };
    // SAFETY: attachment dictionaries are keyed by strings.
    let first = unsafe { first.cast_unchecked::<CFString, CFType>() };
    // SAFETY: reading a constant of the framework.
    let not_sync = unsafe { kCMSampleAttachmentKey_NotSync };
    !first
        .get(not_sync)
        .and_then(|value| value.downcast::<CFBoolean>().ok())
        .is_some_and(|value| value.as_bool())
}

/// The decoder's configuration record a sample's format carries.
fn config(sample: &CMSampleBuffer, codec: Codec) -> Option<Vec<u8>> {
    // SAFETY: a valid sample.
    let format = unsafe { sample.format_description() }?;
    // SAFETY: reading a constant of the framework.
    let key = unsafe { kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms };
    // SAFETY: a valid format description.
    let atoms = unsafe { format.extension(key) }?;
    let atoms = atoms.downcast::<CFDictionary>().ok()?;
    // SAFETY: the extension atoms are keyed by box name.
    let atoms = unsafe { atoms.cast_unchecked::<CFString, CFType>() };
    let name = CFString::from_static_str(codec.record_box());
    let record = atoms.get(&name)?.downcast::<CFData>().ok()?;
    Some(record.to_vec())
}

#[cfg(test)]
#[path = "../tests/unit/encoder.rs"]
mod tests;
