// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { readFile } from "node:fs/promises"
import { spawn } from "node:child_process"
import { dirname, resolve } from "node:path"
import vm from "node:vm"
import * as fs from "node:fs"

export async function loadPlugin(path) {
  const context = vm.createContext({
    process, TextEncoder, AbortController, console, setInterval, clearInterval,
    get Bun() { return globalThis.Bun },
  })
  const modules = new Map()
  async function load(name) {
    if (modules.has(name)) return modules.get(name)
    if (name === "node:fs" || name === "@opencode/plugin/rpc") {
      const exports = name === "node:fs" ? fs : { Rpc: { define: (definition) => definition } }
      const module = new vm.SyntheticModule(Object.keys(exports), function () {
        for (const [key, value] of Object.entries(exports)) this.setExport(key, value)
      }, { context, identifier: name })
      modules.set(name, module)
      return module
    }
    const module = new vm.SourceTextModule(await readFile(name, "utf8"), { context, identifier: name })
    modules.set(name, module)
    await module.link((specifier, parent) => {
      if (!specifier.startsWith(".")) return load(specifier)
      let target = resolve(dirname(parent.identifier), specifier)
      // Installed adapters carry their bridge locally; source adapters share it.
      if (!fs.existsSync(target) && specifier.endsWith("bridge.js")) {
        target = resolve(dirname(parent.identifier), "..", "shared", "bridge.js")
      }
      return load(target)
    })
    return module
  }
  const module = await load(resolve(path))
  await module.evaluate()
  return module.namespace.default ?? module.namespace.WtAgentHooks
}

export function deferred() {
  let resolve
  let reject
  const promise = new Promise((onResolve, onReject) => {
    resolve = onResolve
    reject = onReject
  })
  return { promise, resolve, reject }
}

export function createContext({
  directory = "C:\\ite2e\\opencode",
  workspaceID = "fixture-workspace",
  getSession,
} = {}) {
  const promptHooks = new Map()
  const toolHooks = new Map()
  const queue = []
  let wake
  let signal
  let lookupCount = 0
  let binding
  const location = { directory, workspaceID }

  const context = {
    location,
    rpc: {
      async register(definition, handlers) {
        if (definition.id !== "wt-agent-hooks.pane") throw new Error("Unexpected pane binding RPC")
        binding = handlers
      },
    },
    session: {
      async get({ sessionID }) {
        lookupCount++
        return getSession ? getSession(sessionID) : { id: sessionID, location }
      },
      async hook(name, callback) { promptHooks.set(name, callback) },
    },
    tool: {
      async hook(name, callback) { toolHooks.set(name, callback) },
    },
    event: {
      async *subscribe(options) {
        signal = options.signal
        const abort = () => wake?.()
        signal.addEventListener("abort", abort)
        try {
          while (!signal.aborted) {
            if (!queue.length) await new Promise((resolve) => { wake = resolve })
            if (signal.aborted) return
            const item = queue.shift()
            if (!item) continue
            yield item.event
            item.handled.resolve()
          }
        } finally {
          signal.removeEventListener("abort", abort)
        }
      },
    },
  }

  return {
    context,
    location,
    sync: (input) => binding.sync(input),
    bind: (ids, overrides = {}) => binding.sync({
      client_id: process.env.WT_SESSION,
      pane_session_id: process.env.WT_SESSION,
      com_clsid: process.env.WT_COM_CLSID,
      bridge_path: process.env.WTCLI_PATH,
      log_dir: process.env.WTA_HOOK_LOG_DIR,
      session_ids: ids,
      ...overrides,
    }),
    prompt: (sessionID) => promptHooks.get("prompt")({ sessionID }),
    tool: (sessionID, tool, input) => toolHooks.get("execute.before")({ sessionID, tool, input }),
    async event(type, data, eventLocation = location) {
      const handled = deferred()
      queue.push({ event: { type, data, location: eventLocation }, handled })
      wake?.()
      await handled.promise
    },
    get lookupCount() { return lookupCount },
    get aborted() { return signal?.aborted },
    get registered() { return promptHooks.size + toolHooks.size },
  }
}

export function installNativeBridge() {
  const pending = []
  globalThis.Bun = {
    spawn(options) {
      const child = spawn(options.cmd[0], options.cmd.slice(1), {
        stdio: ["pipe", "ignore", "ignore"],
        windowsHide: options.windowsHide,
        env: options.env,
      })
      const exited = new Promise((resolve, reject) => {
        child.once("error", reject)
        child.stdin.once("error", reject)
        child.once("exit", (code, signal) => {
          if (code === 0) resolve(code)
          else reject(new Error(`hook bridge exited with code=${code}, signal=${signal}`))
        })
      })
      child.stdin.end(options.stdin)
      pending.push(exited)
      return { exited }
    },
  }
  return async () => {
    const results = await Promise.allSettled(pending)
    const failures = results.filter((result) => result.status === "rejected")
    if (failures.length) throw new AggregateError(failures.map((result) => result.reason), "Native hook delivery failed")
  }
}
