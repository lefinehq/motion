#!/usr/bin/env python3
"""Fetch the stock footage a showcase film plays.

The clips are free stock footage whose licences allow using them in a film
but not handing them on unaltered, so they are not in the repository. Each
film that plays footage has a manifest, `<name>.media.json`, beside its
scene; this script downloads every clip it lists, checks the file's sha256,
and, when ffmpeg is installed, writes the cut the scene plays.

    python3 examples/showcase/fetch-media.py examples/showcase/data-story.media.json
    python3 examples/showcase/fetch-media.py --status examples/showcase/*.media.json
    python3 examples/showcase/fetch-media.py --hash some-clip.mp4

Only the standard library is used, and ffmpeg when a clip asks to be cut.

A manifest is a JSON object:

    {
      "film": "data-story",
      "clips": [
        {
          "id": "crossing",
          "source": "https://www.pexels.com/video/...",    # the clip's page
          "url": "https://videos.pexels.com/video-files/...mp4",  # the file
          "author": "...", "author_url": "https://...",
          "licence": "Pexels License",
          "licence_url": "https://www.pexels.com/license/",
          "resolution": [3840, 2160], "fps": 25,
          "sha256": "...",                    # of the file `url` serves
          "out": "media/data-story/crossing.mov",   # what the scene names
          "cut": {"start": 2.0, "duration": 6.0, "size": [1920, 1080],
                  "fps": 30, "codec": "photo-jpeg"}
        }
      ]
    }

Paths are relative to the manifest. A download is kept in `media/.download/`
under its sha256, so a second run, or a second film using the same clip,
does not download it again. `out` is written from it:

- without `cut`, the download is copied as it is (its sound too, which
  nakkash does not play);
- with `cut`, ffmpeg writes it with no sound track: `start` and `duration`
  in seconds of the source, `size` (scaled to cover it, then centre-cropped),
  `fps`, `crop` `[x, y, w, h]` in source pixels before the scale, `vf`
  for any further ffmpeg filter, and `codec`:
  - `photo-jpeg` (the default): QuickTime Photo JPEG, which nakkash decodes
    on every platform; `quality` is ffmpeg's `-q:v`, 2 by default;
  - `h264`: H.264, which nakkash decodes on macOS only; `crf`, 16 by default;
  - `animation`: QuickTime Animation (`rle `), lossless and large.

An optional `out_sha256` is the sha256 of the `out` this manifest's author
wrote. Another ffmpeg may write other bytes, so a different one is reported,
not refused.

`--status` reads nothing from the network: it says which clips are present
and whether they hash as the manifest says. It exits 1 when one is not.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

USER_AGENT = "nakkash-fetch-media/1 (+https://github.com/lefinehq/nakkash)"
CODECS = {
    "photo-jpeg": ["-c:v", "mjpeg", "-pix_fmt", "yuvj420p", "-tag:v", "jpeg"],
    "h264": ["-c:v", "libx264", "-pix_fmt", "yuv420p", "-preset", "slow",
             "-colorspace", "bt709", "-color_primaries", "bt709",
             "-color_trc", "bt709", "-color_range", "tv", "-movflags", "+faststart"],
    "animation": ["-c:v", "qtrle", "-pix_fmt", "rgb24"],
}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def say(text: str) -> None:
    print(text, flush=True)


def download(url: str, dest: Path) -> None:
    """Downloads `url` to `dest` through a temporary file beside it."""
    dest.parent.mkdir(parents=True, exist_ok=True)
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    fd, tmp = tempfile.mkstemp(dir=dest.parent, prefix=".part-")
    try:
        with urllib.request.urlopen(req, timeout=60) as r, os.fdopen(fd, "wb") as out:
            total = int(r.headers.get("Content-Length") or 0)
            got = 0
            while True:
                block = r.read(1 << 20)
                if not block:
                    break
                out.write(block)
                got += len(block)
                if total and sys.stdout.isatty():
                    print(f"\r  {got / 1e6:.1f} / {total / 1e6:.1f} MB", end="", flush=True)
        if total and sys.stdout.isatty():
            print()
        os.chmod(tmp, 0o644)
        os.replace(tmp, dest)
    except BaseException:
        Path(tmp).unlink(missing_ok=True)
        raise


def cut_args(cut: dict, src: Path, out: Path) -> list[str]:
    """The ffmpeg command that writes `cut` of `src` to `out`, with no sound."""
    args = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y"]
    if "start" in cut:
        args += ["-ss", str(cut["start"])]
    args += ["-i", str(src)]
    if "duration" in cut:
        args += ["-t", str(cut["duration"])]
    filters = []
    if "crop" in cut:
        x, y, w, h = cut["crop"]
        filters.append(f"crop={w}:{h}:{x}:{y}")
    if "size" in cut:
        w, h = cut["size"]
        filters.append(f"scale={w}:{h}:force_original_aspect_ratio=increase:flags=lanczos")
        filters.append(f"crop={w}:{h}")
    if "fps" in cut:
        filters.append(f"fps={cut['fps']}")
    if "vf" in cut:
        filters.append(cut["vf"])
    if filters:
        args += ["-vf", ",".join(filters)]
    codec = cut.get("codec", "photo-jpeg")
    if codec not in CODECS:
        raise SystemExit(f"cut codec {codec!r}: expected one of {', '.join(CODECS)}")
    args += CODECS[codec]
    if codec == "photo-jpeg":
        args += ["-q:v", str(cut.get("quality", 2))]
    elif codec == "h264":
        args += ["-crf", str(cut.get("crf", 16))]
    args += ["-an", "-sn", "-dn", "-map_metadata", "-1", str(out)]
    return args


def fetch(manifest: Path, clip: dict, *, force: bool, cut_only: bool) -> bool:
    root = manifest.parent
    cid = clip["id"]
    want = clip.get("sha256", "").lower()
    if not want:
        raise SystemExit(f"{cid}: the manifest gives no sha256")
    store = root / "media" / ".download"
    name = Path(clip["url"].split("?")[0]).name or "clip"
    src = store / f"{want[:16]}-{name}"
    if not src.exists() or force:
        if cut_only:
            say(f"{cid}: not downloaded")
            return False
        say(f"{cid}: downloading {clip['url']}")
        download(clip["url"], src)
    got = sha256(src)
    if got != want:
        bad = src.with_name(src.name + ".bad")
        src.replace(bad)
        say(f"{cid}: sha256 {got}, the manifest says {want}; kept as {bad.name}")
        return False
    out = root / clip["out"]
    out.parent.mkdir(parents=True, exist_ok=True)
    cut = clip.get("cut")
    if cut is None:
        if force or not out.exists() or sha256(out) != want:
            shutil.copyfile(src, out)
        say(f"{cid}: {clip['out']} (as downloaded)")
        return True
    if not shutil.which("ffmpeg"):
        say(f"{cid}: downloaded and checked; ffmpeg is not installed, so {clip['out']} is not written")
        return False
    stamp = out.with_name(out.name + ".cut.json")
    recipe = json.dumps({"sha256": want, "cut": cut}, sort_keys=True)
    if force or not out.exists() or not stamp.exists() or stamp.read_text() != recipe:
        tmp = out.with_name(".part-" + out.name)
        subprocess.run(cut_args(cut, src, tmp), check=True)
        os.replace(tmp, out)
        stamp.write_text(recipe)
    note = ""
    if clip.get("out_sha256") and sha256(out) != clip["out_sha256"]:
        note = " (its bytes differ from the manifest author's; another ffmpeg writes other bytes)"
    say(f"{cid}: {clip['out']}{note}")
    return True


def status(manifest: Path, clip: dict) -> bool:
    root = manifest.parent
    out = root / clip["out"]
    if not out.exists():
        say(f"{clip['id']}: missing {clip['out']}")
        return False
    if "cut" not in clip and sha256(out) != clip["sha256"]:
        say(f"{clip['id']}: {clip['out']} does not hash as the manifest says")
        return False
    say(f"{clip['id']}: {clip['out']}")
    return True


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("manifests", nargs="*", type=Path, help="<name>.media.json files")
    ap.add_argument("--only", help="comma-separated clip ids")
    ap.add_argument("--force", action="store_true", help="download and cut again")
    ap.add_argument("--cut-only", action="store_true", help="cut what is downloaded, download nothing")
    ap.add_argument("--status", action="store_true", help="say what is present, read no network")
    ap.add_argument("--hash", type=Path, metavar="FILE", help="print a file's sha256 and exit")
    a = ap.parse_args()
    if a.hash:
        print(sha256(a.hash))
        return 0
    if not a.manifests:
        ap.error("give at least one manifest")
    only = set(a.only.split(",")) if a.only else None
    ok = True
    for manifest in a.manifests:
        clips = json.loads(manifest.read_text())["clips"]
        ids = [c["id"] for c in clips]
        if len(set(ids)) != len(ids):
            raise SystemExit(f"{manifest}: two clips share an id")
        for clip in clips:
            if only and clip["id"] not in only:
                continue
            if a.status:
                ok &= status(manifest, clip)
            else:
                ok &= fetch(manifest, clip, force=a.force, cut_only=a.cut_only)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
