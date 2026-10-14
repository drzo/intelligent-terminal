// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import assert from "node:assert/strict"
import { afterEach, beforeEach, describe, it } from "node:test"
import { mkdtemp, readFile, rm } from "node:fs/promises"
import { join } from "node:path"
import { fileURLToPath } from "node:url"
import { createContext, deferred, loadPlugin } from "../fixtures/OpenCodeHookContext.mjs"

const bundlePath = process.env.ITE2E_OPENCODE_BUNDLE ??
  fileURLToPath(new URL("../../../tools/wta/wt-agent-hooks/opencode/", import.meta.url))
const v1 = await loadPlugin(join(bundlePath, "v1", "wt-agent-hooks.js"))
const v2 = await loadPlugin(join(bundlePath, "v2", "wt-agent-hooks.js"))
const environmentKeys = ["WT_COM_CLSID", "WT_SESSION", "WTCLI_PATH", "OPENCODE_CLIENT", "WTA_HOOK_LOG_DIR"]
let originalEnvironment
let originalBun
let calls

describe("OpenCode bundled hook behavior", { skip: process.platform !== "win32" }, () => {
  beforeEach(() => {
    originalEnvironment = Object.fromEntries(environmentKeys.map((key) => [key, process.env[key]]))
    originalBun = globalThis.Bun
    process.env.WT_COM_CLSID = "{00000000-0000-0000-0000-000000000001}"
    process.env.WT_SESSION = "00000000-0000-0000-0000-000000000002"
    process.env.WTCLI_PATH = "C:\\fixture with spaces\\wtcli.exe"
    delete process.env.OPENCODE_CLIENT
    delete process.env.WTA_HOOK_LOG_DIR
    calls = []
    globalThis.Bun = {
      spawn(options) {
        calls.push({ topic: options.cmd.at(-1), payload: JSON.parse(new TextDecoder().decode(options.stdin)), options })
        return { exited: Promise.resolve(0) }
      },
    }
  })

  afterEach(() => {
    for (const key of environmentKeys) {
      if (originalEnvironment[key] === undefined) delete process.env[key]
      else process.env[key] = originalEnvironment[key]
    }
    if (originalBun === undefined) delete globalThis.Bun
    else globalThis.Bun = originalBun
  })

  it("V1 root lifecycle retains source, payloads, child suppression and idempotent disposal", async () => {
    const hooks = await v1({ directory: "C:\\root" })
    await hooks.event({ event: { type: "session.created", properties: { info: { id: "root", directory: "C:\\root", title: "Original" } } } })
    await hooks.event({ event: { type: "session.created", properties: { info: { id: "child", parentID: "root" } } } })
    await hooks["chat.message"]({ sessionID: "child" })
    await hooks["tool.execute.before"]({ sessionID: "child", tool: "bash" }, { args: {} })
    assert.equal(calls.length, 1)
    await hooks["chat.message"]({ sessionID: "root" })
    await hooks["tool.execute.before"]({ sessionID: "root", tool: "question" }, { args: { question: "Approve?" } })
    await hooks.event({ event: { type: "session.status", properties: { sessionID: "root", status: { type: "idle" } } } })
    assert.deepEqual(calls.map((call) => call.topic), [
      "agent.session.start", "agent.session.start", "agent.prompt.submit", "agent.tool.starting", "agent.stop",
    ])
    assert.deepEqual(calls[3].payload, { session_id: "root", cwd: "C:\\root", tool_name: "question", tool_input: { question: "Approve?" } })
    assert.deepEqual(Array.from(calls[0].options.cmd), [process.env.WTCLI_PATH, "agent-hook", "--cli-source", "opencode", "--event", "agent.session.start"])
    assert.equal(calls[0].options.windowsHide, true)
    await hooks.event({ event: { type: "session.updated", properties: { info: { id: "root", title: "Renamed" } } } })
    assert.equal(calls.at(-1).payload.title, "Renamed")
    await hooks.event({ event: { type: "session.error", properties: { sessionID: "root", error: { message: "V1 failure" } } } })
    assert.equal(calls.at(-1).payload.error, "V1 failure")
    await hooks.dispose()
    await hooks.dispose()
    assert.equal(calls.filter((call) => call.topic === "agent.session.end").length, 1)
    await hooks["chat.message"]({ sessionID: "root" })
    assert.equal(calls.at(-1).topic, "agent.session.end")
  })

  it("V2 forwards rename, waiting, replies and execution outcomes without treating children as roots", async () => {
    const sdk = createContext({ getSession: async (id) => ({ id, parentID: id === "child" ? "root" : undefined, location: sdk.location }) })
    const dispose = await v2.setup(sdk.context)
    try {
      await sdk.prompt("root")
      assert.equal(calls.length, 0, "the server environment cannot attribute an unbound session")
      await sdk.bind(["root"])
      await assert.rejects(sdk.bind(["child"]), /root sessions/)
      await assert.rejects(sdk.bind(["root"], {
        client_id: "00000000-0000-0000-0000-000000000003",
        pane_session_id: "00000000-0000-0000-0000-000000000003",
      }), /already bound to another terminal client/)
      await sdk.prompt("root")
      await sdk.prompt("child")
      await sdk.tool("child", "bash", {})
      await sdk.event("session.renamed", { sessionID: "root", title: "Renamed" })
      await sdk.tool("root", "question", { questions: ["Continue?"] })
      for (const type of ["permission.asked", "question.asked", "permission.replied", "question.replied"]) {
        await sdk.event(type, { sessionID: "root", action: "edit" })
      }
      for (const type of ["session.execution.succeeded", "session.execution.interrupted", "session.execution.failed"]) {
        await sdk.event(type, { sessionID: "root", error: { data: { message: "Fixture failure" } } })
      }
      assert.equal(calls.some((call) => call.payload.session_id === "child"), false)
      assert.deepEqual(calls.filter((call) => call.topic === "agent.notification").map((call) => call.payload.message),
        ["Permission required: edit", "OpenCode is waiting for input"])
      assert.equal(calls.filter((call) => call.topic === "agent.stop").length, 2)
      assert.equal(calls.find((call) => call.topic === "agent.error").payload.error, "Fixture failure")
      assert.equal(calls.find((call) => call.payload.title === "Renamed").topic, "agent.session.start")
      assert.ok(calls.every((call) => call.options.env.WT_SESSION === process.env.WT_SESSION))
      assert.deepEqual(calls.find((call) => call.topic === "agent.tool.starting").payload.tool_input, { questions: ["Continue?"] })
      await sdk.event("session.deleted", { sessionID: "root" })
      assert.equal(calls.at(-1).topic, "agent.session.end")
    } finally { await dispose() }
    assert.equal(sdk.aborted, true)
  })

  it("V2 location filtering rejects both foreign directories and foreign workspaces", async () => {
    const sdk = createContext()
    const dispose = await v2.setup(sdk.context)
    try {
      await sdk.bind(["owned"])
      calls.length = 0
      await sdk.event("session.status", { sessionID: "owned", status: { type: "busy" } }, { ...sdk.location, directory: "C:\\elsewhere" })
      await sdk.event("session.status", { sessionID: "owned", status: { type: "busy" } }, { ...sdk.location, workspaceID: "other" })
      assert.equal(calls.length, 0)
      await sdk.event("session.status", { sessionID: "owned", status: { type: "busy" } })
      assert.equal(calls.length, 1)
      assert.equal(calls[0].payload.session_id, "owned")
    } finally { await dispose() }
    assert.equal(calls.some((call) => call.topic === "agent.session.end"), false, "server disposal is not client exit")
  })

  for (const teardown of ["delete", "dispose"]) {
    it(`V2 ${teardown} invalidates a shared pending lookup before prompts and tools can emit`, async () => {
      const lookup = deferred()
      const sdk = createContext({ getSession: () => lookup.promise })
      const dispose = await v2.setup(sdk.context)
      const binding = sdk.bind(["resumed"])
      const rejected = assert.rejects(binding, /session changed|unloading/)
      const prompt = sdk.prompt("resumed")
      const tool = sdk.tool("resumed", "bash", {})
      assert.equal(sdk.lookupCount, 1)
      if (teardown === "delete") await sdk.event("session.deleted", { sessionID: "resumed" })
      else await dispose()
      lookup.resolve({ id: "resumed", location: sdk.location })
      await Promise.all([prompt, tool, rejected])
      assert.equal(calls.length, 0)
      await dispose()
    })
  }

  it("V2 deletion fences an older lookup even when the same ID starts a new lifecycle", async () => {
    const oldLookup = deferred()
    const newLookup = deferred()
    let first = true
    const sdk = createContext({ getSession: () => {
      const result = first ? oldLookup.promise : newLookup.promise
      first = false
      return result
    } })
    const dispose = await v2.setup(sdk.context)
    try {
      const oldBinding = sdk.bind(["reused"])
      const oldRejected = assert.rejects(oldBinding, /session changed/)
      await sdk.event("session.deleted", { sessionID: "reused" })
      const newBinding = sdk.bind(["reused"])
      oldLookup.resolve({ id: "reused", title: "Stale", location: sdk.location })
      await oldRejected
      assert.equal(calls.length, 0)
      newLookup.resolve({ id: "reused", title: "Current", location: sdk.location })
      await newBinding
      await sdk.prompt("reused")
      assert.equal(calls.some((call) => call.payload.title === "Stale"), false)
      assert.equal(calls.filter((call) => call.topic === "agent.prompt.submit").length, 1)
    } finally { await dispose() }
  })

  it("V2 binding lookup failures are surfaced, bridge failures are diagnosed, and lookup can retry", async (t) => {
    const logDirectory = await mkdtemp(join(fileURLToPath(new URL(".", import.meta.url)), ".opencode-log-"))
    t.after(() => rm(logDirectory, { recursive: true }))
    process.env.WTA_HOOK_LOG_DIR = logDirectory
    let failLookup = true
    const sdk = createContext({ getSession: async (id) => {
      if (failLookup) throw new Error("lookup unavailable")
      return { id, location: sdk.location }
    } })
    const dispose = await v2.setup(sdk.context)
    try {
      await assert.rejects(sdk.bind(["resumed"]), /lookup unavailable/)
      await sdk.prompt("resumed")
      assert.equal(calls.length, 0)
      failLookup = false
      await sdk.bind(["resumed"])
      await sdk.prompt("resumed")
      assert.equal(calls.at(-1).topic, "agent.prompt.submit")
      globalThis.Bun.spawn = () => { throw new Error("bridge unavailable") }
      await sdk.tool("resumed", "bash", {})
      const log = await readFile(join(logDirectory, "hook-trace.log"), "utf8")
      assert.match(log, /agent.tool.starting.*bridge unavailable/)
      assert.match(log, /opencode hook failed/)
    } finally { await dispose() }
  })

  it("ACP mode suppresses registration and missing client identity suppresses binding and V1 emission", async () => {
    for (const suppressed of ["acp", "WT_SESSION", "WT_COM_CLSID"]) {
      const previous = { ...process.env }
      if (suppressed === "acp") process.env.OPENCODE_CLIENT = "acp"
      else delete process.env[suppressed]
      const sdk = createContext()
      const dispose = await v2.setup(sdk.context)
      if (suppressed === "acp") {
        assert.equal(dispose, undefined)
        assert.equal(sdk.registered, 0)
      } else {
        assert.equal(sdk.registered, 2, "V2 may load without the background server owning a pane")
        await assert.rejects(sdk.bind(["suppressed"]), /Invalid Intelligent Terminal pane context/)
        await sdk.prompt("suppressed")
        await dispose()
      }
      const hooks = await v1({ directory: sdk.location.directory })
      await hooks["chat.message"]({ sessionID: "suppressed" })
      await hooks.dispose()
      for (const key of environmentKeys) {
        if (previous[key] === undefined) delete process.env[key]
        else process.env[key] = previous[key]
      }
    }
    assert.equal(calls.length, 0)
  })
})
