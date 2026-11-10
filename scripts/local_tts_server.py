"""Private localhost TTS service used by C-3PO.

The service never sends text or the reference voice to the network. Chatterbox
may download its model files on the first run; later synthesis is local.
"""

from __future__ import annotations

import json
import os
import tempfile
import threading
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

os.environ.setdefault("NUMBA_CACHE_DIR", str(Path(__file__).with_name("numba-cache")))

HOST = "127.0.0.1"
PORT = int(os.environ.get("C3PO_TTS_PORT", "11438"))
REFERENCE = Path(os.environ["C3PO_TTS_REFERENCE"])
SUPPORTED_LANGUAGES = {"ru", "en", "es", "de", "fr", "it", "pt", "zh", "ja", "ar"}
MAX_TEXT_LENGTH = 1_200

model = None
model_error: str | None = None
model_lock = threading.Lock()


def load_model():
    """Load the large model once and prefer Apple/Windows GPU acceleration."""
    global model, model_error
    if model is not None:
        return model
    with model_lock:
        if model is not None:
            return model
        try:
            import torch
            import perth
            from chatterbox.mtl_tts import ChatterboxMultilingualTTS

            # resemble-perth currently ships no native implicit-watermarker
            # implementation for every Apple Silicon/Python combination. Its
            # own documented dummy implementation keeps synthesis available
            # instead of crashing while loading the model.
            if perth.PerthImplicitWatermarker is None:
                print("Native Perth watermarker unavailable; using its built-in fallback", flush=True)
                perth.PerthImplicitWatermarker = perth.DummyWatermarker

            if torch.backends.mps.is_available():
                device = "mps"
            elif torch.cuda.is_available():
                device = "cuda"
            else:
                device = "cpu"
            print(f"Loading Chatterbox Multilingual on {device}", flush=True)
            # PyPI 0.1.7 exposes the multilingual voice-cloning model without
            # the newer repository-only `t3_model` switch. Keeping this call
            # version-pinned makes installation reproducible on macOS/Windows.
            model = ChatterboxMultilingualTTS.from_pretrained(device=device)
            model_error = None
            print("Chatterbox is ready", flush=True)
            return model
        except Exception as error:  # The full traceback stays in the private server log.
            model_error = f"{type(error).__name__}: {error}"
            traceback.print_exc()
            raise


class Handler(BaseHTTPRequestHandler):
    server_version = "C3PO-TTS/1.0"

    def log_message(self, format_string, *args):
        # Do not log request bodies: generated speech can contain private text.
        print(f"{self.address_string()} {format_string % args}", flush=True)

    def cors_headers(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")

    def send_json(self, status: int, payload: dict):
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.cors_headers()
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_OPTIONS(self):
        self.send_response(204)
        self.cors_headers()
        self.end_headers()

    def do_GET(self):
        if self.path != "/health":
            self.send_json(404, {"error": "not found"})
            return
        state = "ready" if model is not None else ("error" if model_error else "idle")
        self.send_json(200, {"state": state, "referenceFound": REFERENCE.is_file(), "error": model_error})

    def do_POST(self):
        if self.path != "/synthesize":
            self.send_json(404, {"error": "not found"})
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if length <= 0 or length > 20_000:
                raise ValueError("invalid request size")
            request = json.loads(self.rfile.read(length))
            text = str(request.get("text", "")).strip()
            language = str(request.get("language", "ru"))
            if not text or len(text) > MAX_TEXT_LENGTH:
                raise ValueError(f"text must contain 1-{MAX_TEXT_LENGTH} characters")
            if language not in SUPPORTED_LANGUAGES:
                raise ValueError("unsupported language")
            if not REFERENCE.is_file():
                raise FileNotFoundError("voice reference is missing")

            tts = load_model()
            import torchaudio

            with model_lock, tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as output:
                output_path = Path(output.name)
                wav = tts.generate(text, language_id=language, audio_prompt_path=str(REFERENCE))
                torchaudio.save(str(output_path), wav, tts.sr)
            try:
                audio = output_path.read_bytes()
            finally:
                output_path.unlink(missing_ok=True)
            self.send_response(200)
            self.cors_headers()
            self.send_header("Content-Type", "audio/wav")
            self.send_header("Content-Length", str(len(audio)))
            self.end_headers()
            self.wfile.write(audio)
            print(f"Synthesized {len(text)} characters into {len(audio)} bytes", flush=True)
        except Exception as error:
            traceback.print_exc()
            print(f"Synthesis failed: {type(error).__name__}: {error}", flush=True)
            self.send_json(500, {"error": f"{type(error).__name__}: {error}"})


if __name__ == "__main__":
    Path(os.environ["NUMBA_CACHE_DIR"]).mkdir(parents=True, exist_ok=True)
    print(f"C-3PO TTS listening on http://{HOST}:{PORT}", flush=True)
    server = ThreadingHTTPServer((HOST, PORT), Handler)

    # Прогреваем модель сразу после запуска локального сервиса. HTTP уже
    # принимает /health, а первый настоящий ответ не тратит время на загрузку
    # весов с диска. Если прогрев завершится ошибкой, POST вернёт её в журнал.
    def warm_up():
        try:
            load_model()
        except Exception as error:
            print(f"Warm-up failed: {type(error).__name__}: {error}", flush=True)

    threading.Thread(target=warm_up, name="c3po-tts-warmup", daemon=True).start()
    server.serve_forever()
