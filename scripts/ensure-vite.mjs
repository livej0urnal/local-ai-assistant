import { request } from "node:http";
import { createServer } from "vite";

const host = "127.0.0.1";
const port = 1420;

function inspectPort() {
  return new Promise((resolve) => {
    const probe = request(
      {
        host,
        port,
        path: "/",
        method: "GET",
        timeout: 1000,
      },
      (response) => {
        let html = "";

        response.setEncoding("utf8");
        response.on("data", (chunk) => {
          html += chunk;
        });
        response.on("end", () => {
          const isC3po = html.includes("<title>C-3PO</title>") && html.includes("/src/main.ts");
          resolve(isC3po ? "c3po" : "other");
        });
      },
    );

    probe.on("timeout", () => {
      probe.destroy();
      resolve("other");
    });
    probe.on("error", () => resolve("available"));
    probe.end();
  });
}

const portState = await inspectPort();

if (portState === "c3po") {
  console.log(`C-3PO: интерфейс уже работает на http://${host}:${port}. Использую его повторно.`);
  process.exit(0);
}

if (portState === "other") {
  console.error(
    `C-3PO: порт ${port} занят другой программой. Остановите её или освободите порт перед запуском.`,
  );
  process.exit(1);
}

const server = await createServer();
await server.listen();
server.printUrls();

async function shutdown() {
  await server.close();
  process.exit(0);
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
