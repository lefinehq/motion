#!/bin/bash
# Makes the test sounds and their reference decodes; see README.md.
set -e
cd "$(dirname "$0")"; rm -f ./*.wav ./*.flac ./*.mp3 ./*.ogg ./*.f32
S=(synth 0.1 sine 300-3000 sine 500-5000 gain -6)
M=(synth 0.1 sine 300-3000 gain -6)
sox -n -r 44100 -c 2 -b 16 s16.wav "${S[@]}"
sox -n -r 44100 -c 2 -b 24 s24.wav "${S[@]}"
sox -n -r 22050 -c 1 -b 8 -e unsigned u8.wav "${M[@]}"
sox -n -r 44100 -c 2 -b 32 -e floating-point f32.wav "${S[@]}"
sox -n -r 44100 -c 1 -b 64 -e floating-point f64.wav "${M[@]}"
sox -n -r 8000 -c 1 -e a-law alaw.wav "${M[@]}"
sox -n -r 8000 -c 1 -e u-law ulaw.wav "${M[@]}"
ffmpeg -v error -f lavfi -i "sine=f=440:r=48000:d=0.05" -filter_complex "[0]asplit=6[a][b][c][d][e][f];[a][b][c][d][e][f]join=inputs=6:channel_layout=5.1" -c:a pcm_s16le surround.wav
sox s16.wav -b 16 s16.flac
sox s24.wav -b 24 s24.flac
lame --quiet -b 128 s16.wav s16.mp3
sox s16.wav -C 5 s16.ogg
for f in s16.wav s24.wav u8.wav f32.wav f64.wav alaw.wav ulaw.wav surround.wav s16.flac s24.flac s16.mp3 s16.ogg; do
  ffmpeg -v error -i "$f" -f f32le -c:a pcm_f32le "$f.f32"
done
