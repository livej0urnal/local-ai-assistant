"""Private localhost TTS service used by C-3PO.

The service never sends text or the reference voice to the network. Chatterbox
may download its model files on the first run; later synthesis is local.
"""

from __future__ import annotations

import json
import hashlib
import os
import platform
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
VOICE_PIPELINE_VERSION = "2-natural-speech"

model = None
engine_name: str | None = None
voice_conditionals = None
model_error: str | None = None
model_lock = threading.Lock()


def load_model():
    """Load the large model once and prefer Apple/Windows GPU acceleration."""
    global model, model_error, engine_name, voice_conditionals
    if model is not None:
        return model
    with model_lock:
        if model is not None:
            return model
        try:
            # MLX читает веса напрямую в unified memory Apple Silicon и не
            # тащит PyTorch. На остальных процессорах импорт просто пропускается.
            if platform.system() == "Darwin" and platform.machine().lower() in {"arm64", "aarch64"}:
                try:
                    from mlx_audio.tts.utils import load_model as load_mlx_model

                    print("Loading Chatterbox Multilingual v3 on MLX", flush=True)
                    model = load_mlx_model("mlx-community/chatterbox-multilingual-v3")
                    # Голосовой отпечаток не зависит от текста. Вычисляем его
                    # один раз вместо повторной обработки reference.wav.
                    voice_conditionals = model.prepare_conditionals(str(REFERENCE), 0, exaggeration=0.45)
                    engine_name = "mlx"
                    model_error = None
                    print("MLX Chatterbox is ready", flush=True)
                    return model
                except ImportError:
                    print("MLX runtime is not installed; using portable Chatterbox", flush=True)

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
            engine_name = "pytorch"
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
            cache_directory = Path(__file__).with_name("audio-cache")
            cache_directory.mkdir(parents=True, exist_ok=True)
            reference_version = f"{REFERENCE.stat().st_size}:{REFERENCE.stat().st_mtime_ns}"
            cache_key = hashlib.sha256(
                f"{VOICE_PIPELINE_VERSION}|{engine_name}|{language}|{reference_version}|{text}".encode("utf-8")
            ).hexdigest()
            cache_path = cache_directory / f"{cache_key}.wav"
            if cache_path.is_file():
                audio = cache_path.read_bytes()
                print(f"Voice cache hit for {len(text)} characters", flush=True)
            else:
                with model_lock, tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as output:
                    output_path = Path(output.name)
                try:
                    if engine_name == "mlx":
                        import numpy as np
                        from mlx_audio.audio_io import write as audio_write

                        results = list(tts.generate(
                            text=text,
                            conds=voice_conditionals,
                            lang_code=language,
                            exaggeration=0.45,
                            cfg_weight=0.35,
                            temperature=0.65,
                            repetition_penalty=1.35,
                            verbose=False,
                        ))
                        if not results:
                            raise RuntimeError("MLX did not return audio")
                        result = results[-1]
                        audio_write(str(output_path), np.array(result.audio), result.sample_rate)
                    else:
                        import torchaudio

                        wav = tts.generate(
                            text,
                            language_id=language,
                            audio_prompt_path=str(REFERENCE),
                            exaggeration=0.45,
                            cfg_weight=0.35,
                            temperature=0.65,
                            repetition_penalty=1.35,
                        )
                        torchaudio.save(str(output_path), wav, tts.sr)
                    audio = output_path.read_bytes()
                    temporary_cache = cache_path.with_suffix(".tmp")
                    temporary_cache.write_bytes(audio)
                    temporary_cache.replace(cache_path)
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
            tts = load_model()
            if engine_name == "mlx":
                # Первый короткий проход компилирует Metal-графы в фоне. Без
                # него цена компиляции попадёт в первый ответ пользователя.
                with model_lock:
                    list(tts.generate(
                        text="Готов.",
                        conds=voice_conditionals,
                        lang_code="ru",
                        exaggeration=0.45,
                        cfg_weight=0.35,
                        temperature=0.65,
                        repetition_penalty=1.35,
                        verbose=False,
                    ))
                print("MLX voice graph is warmed up", flush=True)
        except Exception as error:
            print(f"Warm-up failed: {type(error).__name__}: {error}", flush=True)

    threading.Thread(target=warm_up, name="c3po-tts-warmup", daemon=True).start()
    server.serve_forever()
