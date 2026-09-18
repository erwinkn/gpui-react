import { mkdtempSync, readFileSync, copyFileSync, existsSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { spawn, spawnSync } from "node:child_process"
import { fileURLToPath } from "node:url"
import { describe, expect, it } from "vitest"

const fixture = fileURLToPath(
  new URL("./fixtures/native-host.ts", import.meta.url),
)
const worker = fileURLToPath(
  new URL("./fixtures/native-host-worker.tsx", import.meta.url),
)
const environment = { ...process.env, GPUIX_BACKGROUND: "1" }

describe.skipIf(process.platform !== "darwin")(
  "native-owned application",
  () => {
    for (const [mode, repeats] of [
      ["normal", 5],
      ["close", 3],
      ["menu", 3],
      ["first-frame", 1],
      ["commits", 1],
      ["blocked-shutdown", 1],
    ] as const) {
      it(`supports ${mode}`, () => {
        const directory = mkdtempSync(join(tmpdir(), "gpuix-application-"))
        const cleanup = join(directory, "cleanup.txt")
        const result = spawnSync(
          "bun",
          [fixture, "-NSAppSleepDisabled", "YES"],
          {
            env: {
              ...environment,
              GPUIX_HOST_TEST: mode,
              GPUIX_HOST_REPEATS: String(repeats),
              GPUIX_CLEANUP_FILE: cleanup,
              GPUIX_HOST_OUTPUT: join(directory, "probe.json"),
            },
            encoding: "utf8",
            timeout: 15000,
          },
        )
        expect(result.stderr).toBe("")
        expect(result.status, result.stdout).toBe(0)
        expect(result.stdout).toContain(`cycle ${repeats} complete`)
        expect(existsSync(cleanup)).toBe(mode !== "blocked-shutdown")
      }, 20000)
    }

    it("bounds startup when a worker does not attach", () => {
      const result = spawnSync("bun", [fixture], {
        env: { ...environment, GPUIX_HOST_TEST: "not-ready" },
        encoding: "utf8",
        timeout: 15000,
      })
      expect(result.status, result.stderr).toBe(1)
      expect(result.stderr).toContain(
        "did not call attachApplication() within 10 seconds",
      )
    }, 17000)

    it("reports a worker startup failure and exits", () => {
      const result = spawnSync("bun", [fixture], {
        env: { ...environment, GPUIX_HOST_TEST: "crash" },
        encoding: "utf8",
        timeout: 5000,
      })
      expect(result.status).toBe(1)
      expect(result.stderr).toContain("Intentional application worker failure")
    })

    for (const [mode, reason] of [
      ["missing", "missing-worker"],
      ["unload", "Application runtime unloaded"],
    ]) {
      it(`reports ${mode} worker and releases the native loop`, () => {
        const result = spawnSync("bun", [fixture], {
          env: { ...environment, GPUIX_HOST_TEST: mode },
          encoding: "utf8",
          timeout: 7000,
        })
        expect(result.status, result.stderr).toBe(1)
        expect(result.stderr).toContain(reason)
      }, 10000)
    }

    it("preserves default SIGTERM behavior after the host ends", async () => {
      const child = spawn("bun", [fixture], {
        env: {
          ...environment,
          GPUIX_HOST_TEST: "first-frame",
          GPUIX_AFTER_HOST_WAIT: "1",
        },
        stdio: ["ignore", "pipe", "pipe"],
      })
      let output = "",
        error = ""
      child.stdout.on("data", (chunk) => {
        output += chunk.toString()
        if (output.includes("waiting outside the application"))
          child.kill("SIGTERM")
      })
      child.stderr.on("data", (chunk) => {
        error += chunk.toString()
      })
      const timer = setTimeout(() => child.kill("SIGKILL"), 7000)
      const result = await new Promise<[number | null, string | null]>(
        (resolve) =>
          child.once("exit", (code, signal) => resolve([code, signal])),
      )
      clearTimeout(timer)
      expect(result, error).toEqual([null, "SIGTERM"])
    }, 10000)

    it("serves automation while the launcher stays inside AppKit", () => {
      const result = spawnSync("bun", [fixture], {
        env: { ...environment, GPUIX_HOST_TEST: "automation" },
        encoding: "utf8",
        timeout: 10000,
        input: 'data: {"id":1,"method":"getTree","params":{}}\n\n',
      })
      expect(result.status, result.stderr).toBe(0)
      expect(result.stdout).toContain('"id":1,"result":{"tree":')
      expect(result.stdout).toContain('"testId":"root"')
    }, 15000)

    it("runs the compiled worker after relocation", () => {
      const buildDirectory = mkdtempSync(
        join(tmpdir(), "gpuix-application-build-"),
      )
      const destination = mkdtempSync(
        join(tmpdir(), "gpuix-application-moved-"),
      )
      const binary = join(buildDirectory, "app")
      const build = spawnSync(
        "bun",
        ["build", "--compile", fixture, worker, "--outfile", binary],
        { encoding: "utf8", timeout: 30000 },
      )
      expect(build.status, build.stderr).toBe(0)
      const moved = join(destination, "app")
      copyFileSync(binary, moved)
      const result = spawnSync(moved, ["-NSAppSleepDisabled", "YES"], {
        cwd: destination,
        env: { ...environment, GPUIX_HOST_REPEATS: "2" },
        encoding: "utf8",
        timeout: 15000,
      })
      expect(result.status, result.stderr).toBe(0)
      expect(result.stdout).toContain("cycle 2 complete")
    }, 45000)

    it("runs exit cleanup on SIGTERM while AppKit owns the main thread", async () => {
      const directory = mkdtempSync(join(tmpdir(), "gpuix-signal-"))
      const cleanup = join(directory, "cleanup.txt")
      const child = spawn("bun", [fixture], {
        env: {
          ...environment,
          GPUIX_HOST_TEST: "automation",
          GPUIX_CLEANUP_FILE: cleanup,
        },
        stdio: ["pipe", "pipe", "pipe"],
      })
      let output = "",
        error = "",
        sent = false
      child.stdout.on("data", (chunk) => {
        output += chunk.toString()
        if (!sent && output.includes('"id":1')) {
          sent = true
          child.kill("SIGTERM")
        }
      })
      child.stderr.on("data", (chunk) => {
        error += chunk.toString()
      })
      child.stdin.write('data: {"id":1,"method":"getTree","params":{}}\n\n')
      const timer = setTimeout(() => child.kill("SIGKILL"), 7000)
      const result = await new Promise<number | null>((resolve) =>
        child.once("exit", resolve),
      )
      clearTimeout(timer)
      expect(result, error).toBe(0)
      expect(readFileSync(cleanup, "utf8")).toBe("worker cleanup ran")
    }, 10000)
  },
)
