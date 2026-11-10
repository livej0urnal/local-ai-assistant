import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { basename, resolve } from "node:path";

const [, , sourceArgument, transcriptArgument] = process.argv;
if (!sourceArgument || !transcriptArgument) {
  console.error("Usage: node scripts/build-private-voice-catalog.mjs <source.mp3> <whisper.json>");
  process.exit(1);
}

const source = resolve(sourceArgument);
const transcript = JSON.parse(readFileSync(resolve(transcriptArgument), "utf8"));
const outputDirectory = resolve("public/private-audio");
mkdirSync(outputDirectory, { recursive: true });

function categoryFor(text) {
  const normalized = text.toLocaleLowerCase("ru-RU");
  if (/я .?c-?3po|я с-?3по|позвольте представиться|о, привет/.test(normalized)) return "startup";
  if (/получилось|невероятное достижение|всё по плечу|спасибо/.test(normalized)) return "success";
  if (/опас|шанс|вниматель|осторож|запрет|нарушение/.test(normalized)) return "warning";
  if (/не может быть|ужас|беда|нам конец|уничтож/.test(normalized)) return "error";
  if (/несколько минут|подожди|погоди/.test(normalized)) return "waiting";
  if (/чем могу|рад познакомиться|нас не представили|здравствуйте/.test(normalized)) return "introduction";
  if (/отключиться|память|масляная ванна|пора/.test(normalized)) return "shutdown";
  return "general";
}

function isRecommended(category, text) {
  if (category === "startup") return /я .?c-?3po|о, привет/.test(text.toLocaleLowerCase("ru-RU"));
  return category !== "general" && text.length <= 220;
}

const phrases = transcript.transcription
  .filter((segment) => segment.text.trim() && !/^\s*\[.*\]\s*$/u.test(segment.text))
  .map((segment, index) => {
    const startMs = Math.max(0, segment.offsets.from - 80);
    const endMs = segment.offsets.to + 100;
    const id = `phrase-${String(index + 1).padStart(3, "0")}`;
    const file = `${id}.mp3`;
    const text = segment.text.trim();
    const category = categoryFor(text);
    const result = spawnSync("ffmpeg", [
      "-hide_banner", "-loglevel", "error", "-y",
      "-ss", (startMs / 1000).toFixed(3),
      "-to", (endMs / 1000).toFixed(3),
      "-i", source,
      "-codec:a", "libmp3lame", "-q:a", "4",
      resolve(outputDirectory, file),
    ], { stdio: "inherit" });
    if (result.status !== 0) throw new Error(`ffmpeg failed for ${id}`);
    return {
      id,
      category,
      text,
      startMs,
      endMs,
      file,
      recommended: isRecommended(category, text),
    };
  });

const catalog = {
  version: 1,
  private: true,
  source: basename(source),
  generatedAt: new Date().toISOString(),
  phrases,
};
writeFileSync(resolve(outputDirectory, "catalog.json"), `${JSON.stringify(catalog, null, 2)}\n`);
console.log(`Created ${phrases.length} local phrases in ${outputDirectory}`);
