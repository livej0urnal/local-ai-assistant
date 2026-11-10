"""Install C-3PO's optional local voice runtime for the current user.

Run with Python 3.11. The script writes only to the operating system's normal
per-user application-data folder and never uploads the chosen voice sample.
"""

from __future__ import annotations

import argparse
import os
import platform
import shutil
import subprocess
import sys
import venv
from pathlib import Path


def data_directory() -> Path:
    if sys.platform == "darwin":
        return Path.home() / "Library/Application Support/com.igorfilippov.C-3PO/tts"
    if sys.platform == "win32":
        return Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData/Local")) / "igorfilippov/C-3PO/data/tts"
    return Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share")) / "c-3po/tts"


def main() -> None:
    parser = argparse.ArgumentParser(description="Install the private, local C-3PO voice runtime")
    parser.add_argument("--voice", type=Path, help="Your own/licensed WAV, MP3, M4A, FLAC or OGG reference")
    args = parser.parse_args()
    if sys.version_info[:2] != (3, 11):
        raise SystemExit("Python 3.11 is required. Install it, then run this script with Python 3.11.")

    target = data_directory()
    use_mlx = sys.platform == "darwin" and platform.machine().lower() in {"arm64", "aarch64"}
    environment = target / ("mlx-voice-venv" if use_mlx else "chatterbox-venv")
    target.mkdir(parents=True, exist_ok=True)
    if not environment.exists():
        print(f"Creating isolated environment: {environment}")
        venv.EnvBuilder(with_pip=True).create(environment)
    python = environment / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
    if use_mlx:
        print("Installing the Apple Silicon MLX voice engine…")
        subprocess.run([str(python), "-m", "pip", "install", "mlx-audio==0.6.3"], check=True)
    else:
        print("Installing the portable Chatterbox speech engine…")
        subprocess.run([str(python), "-m", "pip", "install", "chatterbox-tts==0.1.7"], check=True)

    if args.voice:
        source = args.voice.expanduser().resolve()
        if not source.is_file() or source.suffix.lower() not in {".wav", ".mp3", ".m4a", ".flac", ".ogg"}:
            raise SystemExit("The voice reference must be an existing WAV, MP3, M4A, FLAC or OGG file.")
        for extension in ("wav", "mp3", "m4a", "flac", "ogg"):
            (target / f"reference.{extension}").unlink(missing_ok=True)
        destination = target / f"reference{source.suffix.lower()}"
        shutil.copy2(source, destination)
        print(f"Private voice reference copied to: {destination}")
    else:
        print("Runtime installed. Add your own voice later with --voice /path/to/sample.wav")
    print("Done. Restart C-3PO and open Settings → Voice.")


if __name__ == "__main__":
    main()
