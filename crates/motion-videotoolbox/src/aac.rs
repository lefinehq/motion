//! AAC-LC from the system's AudioToolbox: interleaved 32-bit float samples
//! in, the packets of an MPEG-4 audio stream out, 1,024 frames each.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{self, NonNull};

use objc2_audio_toolbox::{
    AudioConverterDispose, AudioConverterFillComplexBuffer, AudioConverterGetProperty,
    AudioConverterNew, AudioConverterPrimeInfo, AudioConverterRef, AudioConverterSetProperty,
    kAudioConverterEncodeBitRate, kAudioConverterPrimeInfo,
    kAudioConverterPropertyMaximumOutputPacketSize,
};
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription, AudioStreamPacketDescription,
    MPEG4ObjectID, kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatLinearPCM,
    kAudioFormatMPEG4AAC,
};

use crate::Error;

/// Frames in each AAC packet.
pub const FRAMES_PER_PACKET: u32 = 1024;

/// An encoded AAC-LC stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aac {
    /// The packets, in order.
    pub packets: Vec<Vec<u8>>,
    /// Frames of priming the encoder puts before the first input frame:
    /// a player skips them through the file's edit list.
    pub leading: u32,
}

/// The input left to hand the converter, and its layout.
struct Feed<'a> {
    samples: &'a [f32],
    channels: u32,
    /// The next frame to hand over.
    next: usize,
}

/// Encodes `samples`, interleaved `channels` at `rate` Hz, as AAC-LC at
/// `bitrate` bits a second.
///
/// # Errors
/// An [`Error`] naming the call that failed, with its status: the system
/// has no AAC encoder for the format, or refuses the bit rate.
pub fn encode_aac(samples: &[f32], rate: f64, channels: u32, bitrate: u32) -> Result<Aac, Error> {
    let input = AudioStreamBasicDescription {
        mSampleRate: rate,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
        mBytesPerPacket: 4 * channels,
        mFramesPerPacket: 1,
        mBytesPerFrame: 4 * channels,
        mChannelsPerFrame: channels,
        mBitsPerChannel: 32,
        mReserved: 0,
    };
    let output = AudioStreamBasicDescription {
        mSampleRate: rate,
        mFormatID: kAudioFormatMPEG4AAC,
        mFormatFlags: MPEG4ObjectID::AAC_LC.0 as u32,
        mBytesPerPacket: 0,
        mFramesPerPacket: FRAMES_PER_PACKET,
        mBytesPerFrame: 0,
        mChannelsPerFrame: channels,
        mBitsPerChannel: 0,
        mReserved: 0,
    };
    let mut converter: AudioConverterRef = ptr::null_mut();
    // SAFETY: both descriptions and the out pointer are live locals for the
    // call, which only reads the descriptions and writes the converter.
    let status = unsafe {
        AudioConverterNew(
            NonNull::from(&input),
            NonNull::from(&output),
            NonNull::from(&mut converter),
        )
    };
    check("creating the AAC encoder", status)?;
    let converter = Converter(converter);
    converter.set_u32(
        kAudioConverterEncodeBitRate,
        bitrate,
        "setting the AAC bit rate",
    )?;
    let largest = converter.get_u32(
        kAudioConverterPropertyMaximumOutputPacketSize,
        "asking the largest AAC packet",
    )?;
    let mut feed = Feed {
        samples,
        channels,
        next: 0,
    };
    let mut packets = Vec::new();
    let mut buffer = vec![0u8; largest as usize];
    loop {
        let mut list = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: channels,
                mDataByteSize: largest,
                mData: buffer.as_mut_ptr().cast(),
            }],
        };
        let mut count: u32 = 1;
        let mut description = AudioStreamPacketDescription {
            mStartOffset: 0,
            mVariableFramesInPacket: 0,
            mDataByteSize: 0,
        };
        // SAFETY: the converter is live; `supply` matches the callback's
        // signature and reads `feed`, a live local that outlives the call;
        // `list` points at `buffer`, which holds `largest` bytes, the most
        // one packet takes, and `description` has room for the one packet
        // asked for.
        let status = unsafe {
            AudioConverterFillComplexBuffer(
                converter.0,
                Some(supply),
                (&raw mut feed).cast::<c_void>(),
                NonNull::from(&mut count),
                NonNull::from(&mut list),
                &raw mut description,
            )
        };
        check("encoding AAC", status)?;
        if count == 0 {
            break;
        }
        let size = list.mBuffers[0].mDataByteSize as usize;
        packets.push(buffer[..size].to_vec());
    }
    let none = AudioConverterPrimeInfo {
        leadingFrames: 0,
        trailingFrames: 0,
    };
    let prime = converter.get(kAudioConverterPrimeInfo, none, "asking the AAC priming")?;
    Ok(Aac {
        packets,
        leading: prime.leadingFrames,
    })
}

/// Hands the converter the input it asks for: as many of the frames left
/// as it wants, or none once all are given, which ends the stream.
///
/// # Safety
/// `frames` and `data` are the converter's, and `user` is the `Feed` that
/// [`encode_aac`] passes, live for the call.
unsafe extern "C-unwind" fn supply(
    _converter: AudioConverterRef,
    frames: NonNull<u32>,
    data: NonNull<AudioBufferList>,
    _descriptions: *mut *mut AudioStreamPacketDescription,
    user: *mut c_void,
) -> i32 {
    // SAFETY: `user` is the `&mut Feed` that `encode_aac` passed, and
    // nothing else touches it while the converter calls back.
    let feed = unsafe { &mut *user.cast::<Feed<'_>>() };
    let channels = feed.channels as usize;
    let left = feed.samples.len() / channels - feed.next;
    // SAFETY: the converter hands a valid pointer to how many frames it
    // asks for, which it reads back as how many it got.
    let asked = unsafe { frames.read() } as usize;
    let given = asked.min(left);
    let start = feed.next * channels;
    // SAFETY: `data` is the converter's list of one buffer, valid for the
    // call; the buffer then points into `feed.samples`, which outlives the
    // encode, and the converter only reads input.
    let list = unsafe { &mut *data.as_ptr() };
    list.mBuffers[0] = AudioBuffer {
        mNumberChannels: feed.channels,
        mDataByteSize: (given * channels * 4) as u32,
        mData: feed.samples[start..].as_ptr().cast_mut().cast(),
    };
    // SAFETY: as above, the converter's count, now how many frames it gets.
    unsafe { frames.write(given as u32) };
    feed.next += given;
    0
}

/// A converter, disposed of when dropped.
struct Converter(AudioConverterRef);

impl Converter {
    fn set_u32(&self, property: u32, value: u32, doing: &str) -> Result<(), Error> {
        // SAFETY: the converter is live, and `value` is a live u32 of the
        // size given, which the call only reads.
        let status = unsafe {
            AudioConverterSetProperty(
                self.0,
                property,
                size_of::<u32>() as u32,
                NonNull::from(&value).cast(),
            )
        };
        check(doing, status)
    }

    fn get_u32(&self, property: u32, doing: &str) -> Result<u32, Error> {
        self.get(property, 0, doing)
    }

    /// A property whose value is a plain `T`, read over `value`.
    fn get<T: Copy>(&self, property: u32, mut value: T, doing: &str) -> Result<T, Error> {
        let mut size = size_of::<T>() as u32;
        // SAFETY: the converter is live, and `value` has room for the
        // `size` bytes the call may write; it writes back how many.
        let status = unsafe {
            AudioConverterGetProperty(
                self.0,
                property,
                NonNull::from(&mut size),
                NonNull::from(&mut value).cast(),
            )
        };
        check(doing, status)?;
        Ok(value)
    }
}

impl Drop for Converter {
    fn drop(&mut self) {
        // SAFETY: the converter was made by `AudioConverterNew` and is
        // disposed of once, here.
        unsafe { AudioConverterDispose(self.0) };
    }
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

#[cfg(test)]
#[path = "../tests/unit/aac.rs"]
mod tests;
