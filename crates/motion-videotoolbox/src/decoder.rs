//! One decompression session, safe to use: samples in, in decode order,
//! one picture out for each.

use std::ffi::c_void;
use std::fmt;
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::{Arc, Mutex, PoisonError};

use objc2_core_foundation::{
    CFArray, CFData, CFDictionary, CFNumber, CFRetained, CFString, CFType,
};
use objc2_core_media::{
    CMBlockBuffer, CMSampleBuffer, CMTime, CMVideoCodecType, CMVideoFormatDescription,
    CMVideoFormatDescriptionCreate, kCMBlockBufferAssureMemoryNowFlag,
    kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms, kCMVideoCodecType_H264,
    kCMVideoCodecType_HEVC,
};
use objc2_core_video::{
    CVImageBuffer, CVPixelBuffer, CVPixelBufferGetBaseAddressOfPlane,
    CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane,
    CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount, CVPixelBufferGetWidth,
    CVPixelBufferGetWidthOfPlane, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
};
use objc2_video_toolbox::{
    VTDecodeFrameFlags, VTDecodeInfoFlags, VTDecompressionOutputCallbackRecord,
    VTDecompressionSession,
};

use crate::Error;

/// A stream's decoder configuration record, which also says the codec: the
/// body of its sample description's `avcC` or `hvcC` box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Record<'a> {
    /// H.264: the `avcC` box's body, with the stream's parameter sets.
    Avc(&'a [u8]),
    /// HEVC: the `hvcC` box's body, with the stream's parameter sets.
    Hevc(&'a [u8]),
}

impl Record<'_> {
    fn video_type(self) -> CMVideoCodecType {
        match self {
            Record::Avc(_) => kCMVideoCodecType_H264,
            Record::Hevc(_) => kCMVideoCodecType_HEVC,
        }
    }

    /// The box's name and body.
    fn parts(&self) -> (&'static str, &[u8]) {
        match *self {
            Record::Avc(body) => ("avcC", body),
            Record::Hevc(body) => ("hvcC", body),
        }
    }
}

/// The pixel formats a session hands pictures back in, and whether each
/// is ten bits a sample: 4:2:0 in two planes, at eight or ten bits, in
/// limited or full range. A decoder whose own output is in another one has
/// the system convert it.
const FORMATS: [(u32, bool); 4] = [
    (kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, false),
    (kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, false),
    (kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange, true),
    (kCVPixelFormatType_420YpCbCr10BiPlanarFullRange, true),
];

/// One plane of luma and one of chroma, 4:2:0: the chroma plane is half
/// the luma's size both ways, rounded up, each sample a blue-difference
/// and red-difference pair. Rows may be longer than the pixels they hold.
#[derive(Debug, Clone, Copy)]
pub struct Biplanar<'a, T> {
    /// Width of the picture in pixels.
    pub width: usize,
    /// Height of the picture in pixels.
    pub height: usize,
    /// Luma, `height` rows of `luma_stride` samples.
    pub luma: &'a [T],
    /// Samples from one luma row to the next.
    pub luma_stride: usize,
    /// Chroma pairs, Cb then Cr, `height` halved rows of `chroma_stride`
    /// samples.
    pub chroma: &'a [T],
    /// Samples from one chroma row to the next.
    pub chroma_stride: usize,
}

/// The samples of one picture, as coded: the caller applies its stream's
/// matrix and range.
#[derive(Debug, Clone, Copy)]
pub enum Samples<'a> {
    /// Eight bits a sample.
    Eight(Biplanar<'a, u8>),
    /// Ten bits a sample, in the top ten bits of sixteen.
    Ten(Biplanar<'a, u16>),
}

/// One decoded picture, held from the system's buffers until dropped.
pub struct Picture {
    buffer: CFRetained<CVPixelBuffer>,
}

// SAFETY: a pixel buffer is a Core Foundation object, retained and
// released atomically from any thread; once the session has handed it
// back, the decoder only reads it (as a reference picture), as `read` does.
unsafe impl Send for Picture {}

impl fmt::Debug for Picture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Picture")
            .field("size", &self.size())
            .finish()
    }
}

impl Picture {
    /// Its size in pixels.
    pub fn size(&self) -> (u32, u32) {
        let width = CVPixelBufferGetWidth(&self.buffer);
        let height = CVPixelBufferGetHeight(&self.buffer);
        (
            u32::try_from(width).unwrap_or(u32::MAX),
            u32::try_from(height).unwrap_or(u32::MAX),
        )
    }

    /// Hands `read` the picture's samples while they are locked for
    /// reading.
    ///
    /// # Errors
    /// When the picture is in a pixel format the session was not asked for,
    /// or the system cannot lock it.
    pub fn read<R>(&self, read: impl FnOnce(Samples<'_>) -> R) -> Result<R, Error> {
        let format = CVPixelBufferGetPixelFormatType(&self.buffer);
        let Some(&(_, ten)) = FORMATS.iter().find(|(f, _)| *f == format) else {
            return Err(Error {
                doing: format!(
                    "reading a picture in the pixel format `{}`",
                    String::from_utf8_lossy(&format.to_be_bytes())
                ),
                status: -1,
            });
        };
        let lock = CVPixelBufferLockFlags::ReadOnly;
        // SAFETY: locking a valid buffer for reading; unlocked below with
        // the same flags.
        let status = unsafe { CVPixelBufferLockBaseAddress(&self.buffer, lock) };
        check("locking a picture", status)?;
        let result = match ten {
            false => biplanar::<u8>(&self.buffer).map(|planes| read(Samples::Eight(planes))),
            true => biplanar::<u16>(&self.buffer).map(|planes| read(Samples::Ten(planes))),
        };
        // SAFETY: unlocking the lock taken above, with the same flags; the
        // planes lent to `read` do not outlive it.
        let unlocked = unsafe { CVPixelBufferUnlockBaseAddress(&self.buffer, lock) };
        let result = result.ok_or(Error {
            doing: "reaching a picture's planes".into(),
            status: -1,
        })?;
        check("unlocking a picture", unlocked)?;
        Ok(result)
    }
}

/// The two planes of a locked bi-planar `buffer`, as samples of `T`; none
/// when the buffer is not two planes of whole samples the system gives an
/// address for.
fn biplanar<T>(buffer: &CVPixelBuffer) -> Option<Biplanar<'_, T>> {
    if CVPixelBufferGetPlaneCount(buffer) != 2 {
        return None;
    }
    let size = size_of::<T>();
    let plane = |index: usize| -> Option<(&[T], usize, usize, usize)> {
        let base = CVPixelBufferGetBaseAddressOfPlane(buffer, index);
        let stride = CVPixelBufferGetBytesPerRowOfPlane(buffer, index);
        let width = CVPixelBufferGetWidthOfPlane(buffer, index);
        let height = CVPixelBufferGetHeightOfPlane(buffer, index);
        let base = NonNull::new(base.cast::<T>())?;
        if !base.as_ptr().is_aligned() || !stride.is_multiple_of(size) {
            return None;
        }
        // SAFETY: the buffer is locked until the caller unlocks it, after
        // the slice's last use; the plane holds `height` rows of `stride`
        // bytes, and its address is aligned for `T`, checked above.
        let samples =
            unsafe { slice::from_raw_parts(base.as_ptr().cast_const(), stride / size * height) };
        Some((samples, stride / size, width, height))
    };
    let (luma, luma_stride, width, height) = plane(0)?;
    let (chroma, chroma_stride, chroma_width, chroma_height) = plane(1)?;
    let fits = chroma_width == width.div_ceil(2)
        && chroma_height == height.div_ceil(2)
        && luma_stride >= width
        && chroma_stride >= chroma_width * 2;
    fits.then_some(Biplanar {
        width,
        height,
        luma,
        luma_stride,
        chroma,
        chroma_stride,
    })
}

/// What the session's callback hands back for the frame being decoded,
/// kept apart from the session so the callback can reach it from the
/// system's threads.
#[derive(Default)]
struct Output {
    picture: Option<Picture>,
    failure: Option<Error>,
}

/// One decompression session for one stream.
pub struct Decoder {
    session: CFRetained<VTDecompressionSession>,
    format: CFRetained<CMVideoFormatDescription>,
    /// Shared with the callback, which holds a pointer to it until the
    /// session is invalidated in `drop`.
    output: Arc<Mutex<Output>>,
    /// The callback and its pointer, kept for as long as the session.
    _callback: Box<VTDecompressionOutputCallbackRecord>,
}

// SAFETY: a decompression session may be called from any thread, one call
// at a time, which `&mut self` on every call that touches it ensures; the
// output it shares with the system's threads is behind a mutex, and the
// pictures in it are `Send`.
unsafe impl Send for Decoder {}

impl Decoder {
    /// A session for the stream `record` configures, of `width` by
    /// `height` pixels, handing pictures back in the order the samples go
    /// in.
    ///
    /// # Errors
    /// When the system has no decoder for the stream or refuses its
    /// configuration.
    pub fn new(record: Record<'_>, width: u32, height: u32) -> Result<Decoder, Error> {
        let format = format_description(record, width, height)?;
        let output = Arc::new(Mutex::new(Output::default()));
        let callback = Box::new(VTDecompressionOutputCallbackRecord {
            decompressionOutputCallback: Some(decoded),
            decompressionOutputRefCon: Arc::as_ptr(&output).cast_mut().cast::<c_void>(),
        });
        let numbers: Vec<CFRetained<CFNumber>> = FORMATS
            .iter()
            .map(|&(format, _)| CFNumber::new_i64(i64::from(format)))
            .collect();
        let formats = CFArray::from_retained_objects(&numbers);
        // SAFETY: reading a constant of the framework.
        let key = unsafe { kCVPixelBufferPixelFormatTypeKey };
        let attributes = CFDictionary::<CFString, CFType>::from_slices(&[key], &[&formats]);
        let mut raw: *mut VTDecompressionSession = ptr::null_mut();
        // SAFETY: `format` is a video format description; the attributes
        // are keyed by strings with the values the framework documents;
        // `callback` has the callback's signature and points at the output,
        // and both outlive the session (see `drop`); `raw` is a valid place
        // for the result.
        let status = unsafe {
            VTDecompressionSession::create(
                None,
                &format,
                None,
                Some(attributes.as_opaque()),
                &raw const *callback,
                NonNull::from(&mut raw),
            )
        };
        check("creating a decompression session", status)?;
        let raw = NonNull::new(raw).ok_or(Error {
            doing: "creating a decompression session".into(),
            status: -1,
        })?;
        // SAFETY: a session from `create` is ours to release.
        let session = unsafe { CFRetained::from_raw(raw) };
        Ok(Decoder {
            session,
            format,
            output,
            _callback: callback,
        })
    }

    /// Decodes the next sample, in decode order, and returns its picture;
    /// `None` when the decoder dropped the frame.
    ///
    /// For H.264 and HEVC a sample is NAL units, each after its length in
    /// as many bytes as the record says.
    ///
    /// # Errors
    /// When the sample is empty or the system fails to decode it.
    pub fn decode(&mut self, sample: &[u8]) -> Result<Option<Picture>, Error> {
        let buffer = self.sample_buffer(sample)?;
        let mut info = VTDecodeInfoFlags(0);
        // SAFETY: `buffer` is a ready sample of the session's format; no
        // flags ask for asynchronous decoding or delayed frames, so the
        // callback runs once for it before the call returns; the frame's
        // reference is never read; `info` is a valid place for the flags.
        let status = unsafe {
            self.session
                .decode_frame(&buffer, VTDecodeFrameFlags(0), ptr::null_mut(), &mut info)
        };
        check("decoding a frame", status)?;
        // SAFETY: waiting on a valid session.
        let status = unsafe { self.session.wait_for_asynchronous_frames() };
        check("waiting for a decoded frame", status)?;
        let mut output = self.output.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(failure) = output.failure.take() {
            output.picture = None;
            return Err(failure);
        }
        Ok(output.picture.take())
    }

    /// A one-sample buffer of the session's format holding a copy of
    /// `sample`.
    fn sample_buffer(&self, sample: &[u8]) -> Result<CFRetained<CMSampleBuffer>, Error> {
        let failed = |doing: &str| Error {
            doing: doing.into(),
            status: -1,
        };
        if sample.is_empty() {
            return Err(failed("decoding an empty sample"));
        }
        let mut raw: *mut CMBlockBuffer = ptr::null_mut();
        // SAFETY: no memory or custom source is passed, so the block buffer
        // allocates `sample.len()` bytes of its own now; `raw` is a valid
        // place for the result.
        let status = unsafe {
            CMBlockBuffer::create_with_memory_block(
                None,
                ptr::null_mut(),
                sample.len(),
                None,
                ptr::null(),
                0,
                sample.len(),
                kCMBlockBufferAssureMemoryNowFlag,
                NonNull::from(&mut raw),
            )
        };
        check("making a block buffer", status)?;
        let raw = NonNull::new(raw).ok_or_else(|| failed("making a block buffer"))?;
        // SAFETY: a block buffer from `create_with_memory_block` is ours to
        // release.
        let block = unsafe { CFRetained::from_raw(raw) };
        // SAFETY: `sample` holds `sample.len()` bytes, and the block buffer
        // as many, allocated above.
        let status = unsafe {
            CMBlockBuffer::replace_data_bytes(
                NonNull::from(sample).cast::<c_void>(),
                &block,
                0,
                sample.len(),
            )
        };
        check("filling a block buffer", status)?;
        let sizes = [sample.len()];
        let mut raw: *mut CMSampleBuffer = ptr::null_mut();
        // SAFETY: one sample of `sizes[0]` bytes in `block`, of the
        // session's format, without timing; `raw` is a valid place for the
        // result.
        let status = unsafe {
            CMSampleBuffer::create_ready(
                None,
                Some(&block),
                Some(&self.format),
                1,
                0,
                ptr::null(),
                1,
                sizes.as_ptr(),
                NonNull::from(&mut raw),
            )
        };
        check("making a sample buffer", status)?;
        let raw = NonNull::new(raw).ok_or_else(|| failed("making a sample buffer"))?;
        // SAFETY: a sample buffer from `create_ready` is ours to release.
        Ok(unsafe { CFRetained::from_raw(raw) })
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: tearing the session down before the output and the
        // callback record it uses are dropped; no call reaches the callback
        // after this returns.
        unsafe { self.session.invalidate() };
    }
}

/// The format of a stream `record` configures, `width` by `height`: the
/// record goes in as the sample description's extension box, as a file
/// carries it.
fn format_description(
    record: Record<'_>,
    width: u32,
    height: u32,
) -> Result<CFRetained<CMVideoFormatDescription>, Error> {
    let (name, body) = record.parts();
    let name = CFString::from_static_str(name);
    let data = CFData::from_bytes(body);
    let atoms = CFDictionary::<CFString, CFType>::from_slices(&[&name], &[&data]);
    // SAFETY: reading a constant of the framework.
    let key = unsafe { kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms };
    let extensions = CFDictionary::<CFString, CFType>::from_slices(&[key], &[&atoms]);
    let mut raw: *const CMVideoFormatDescription = ptr::null();
    // SAFETY: the extensions are keyed by strings, with the dictionary of
    // boxes by name the framework documents for this key; `raw` is a valid
    // place for the result.
    let status = unsafe {
        CMVideoFormatDescriptionCreate(
            None,
            record.video_type(),
            i32::try_from(width).unwrap_or(i32::MAX),
            i32::try_from(height).unwrap_or(i32::MAX),
            Some(extensions.as_opaque()),
            NonNull::from(&mut raw),
        )
    };
    check("describing the stream", status)?;
    let raw = NonNull::new(raw.cast_mut()).ok_or(Error {
        doing: "describing the stream".into(),
        status: -1,
    })?;
    // SAFETY: a format description from a create function is ours to
    // release.
    Ok(unsafe { CFRetained::from_raw(raw) })
}

/// `Ok` when `status` is the system's success, else the error of `doing`.
fn check(doing: &str, status: i32) -> Result<(), Error> {
    match status {
        0 => Ok(()),
        status => Err(Error {
            doing: doing.to_owned(),
            status,
        }),
    }
}

/// The session's callback: one decoded picture, or a failure.
unsafe extern "C-unwind" fn decoded(
    output: *mut c_void,
    _frame: *mut c_void,
    status: i32,
    _flags: VTDecodeInfoFlags,
    image: *mut CVImageBuffer,
    _shown: CMTime,
    _duration: CMTime,
) {
    // SAFETY: `output` is the pointer `Decoder::new` gave the session, to
    // an output that outlives it.
    let output = unsafe { &*output.cast_const().cast::<Mutex<Output>>() };
    let mut output = output.lock().unwrap_or_else(PoisonError::into_inner);
    if status != 0 {
        output.failure.get_or_insert(Error {
            doing: "decoding a frame".into(),
            status,
        });
        return;
    }
    // A frame the decoder dropped has no picture.
    output.picture = NonNull::new(image).map(|image| Picture {
        // SAFETY: the system passes a valid image buffer for the length of
        // the call; retaining it keeps it past the call. A session hands
        // back pixel buffers, which the formats it was asked for are.
        buffer: unsafe { CFRetained::retain(image) },
    });
}

#[cfg(test)]
#[path = "../tests/unit/decoder.rs"]
mod tests;
