import http from "node:http"
import { spawn } from "node:child_process"

const HOST = "127.0.0.1"
const PORT = 3210
const VITE_CLIENT_PATH = "/@vite/client"

function requestWithTimeout(pathname, timeoutMs = 1500) {
  return new Promise((resolve, reject) => {
    const req = http.get(
      {
        host: HOST,
        port: PORT,
        path: pathname,
      },
      (res) => {
        let body = ""
        res.setEncoding("utf8")
        res.on("data", (chunk) => {
          body += chunk
        })
        res.on("end", () => {
          resolve({ statusCode: res.statusCode ?? 0, body })
        })
      },
    )

    req.setTimeout(timeoutMs, () => {
      req.destroy(new Error(`Timed out after ${timeoutMs}ms`))
    })
    req.on("error", reject)
  })
}

async function hasReusableViteServer() {
  try {
    const response = await requestWithTimeout(VITE_CLIENT_PATH)
    return response.statusCode === 200 && response.body.includes("createHotContext")
  } catch {
    return false
  }
}

async function main() {
  if (await hasReusableViteServer()) {
    console.log(`[tauri-dev] Reusing existing Vite dev server on http://${HOST}:${PORT}`)
    return
  }

  console.log(`[tauri-dev] Starting Vite dev server on http://${HOST}:${PORT}`)
  const child =
    process.platform === "win32"
      ? spawn(
          "cmd.exe",
          [
            "/d",
            "/s",
            "/c",
            `npm run dev -- --host ${HOST} --port ${PORT} --strictPort`,
          ],
          { stdio: "inherit" },
        )
      : spawn(
          "npm",
          ["run", "dev", "--", "--host", HOST, "--port", String(PORT), "--strictPort"],
          { stdio: "inherit" },
        )

  child.on("exit", (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal)
      return
    }
    process.exit(code ?? 0)
  })
}

await main()
