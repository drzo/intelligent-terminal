import assert from "node:assert/strict"
import { randomUUID } from "node:crypto"
import { readFile } from "node:fs/promises"
import { test } from "node:test"
import vm from "node:vm"

const bundle = new URL("../wt-agent-hooks/opencode/", import.meta.url)
const paneA = "11111111-1111-1111-1111-111111111111"
const paneB = "22222222-2222-2222-2222-222222222222"
const clsid = "33333333-3333-3333-3333-333333333333"
const location = { directory: "C:\\same-project" }
const tick = () => new Promise((resolve) => setImmediate(resolve))

async function load(entry, env = {}) {
  const calls = []
  const failures = []
  const effects = []
  const timers = new Map()
  let now = Date.now()
  class Clock extends Date { static now() { return now } }
  const context = vm.createContext({
    process: { platform: "win32", env },
    TextEncoder,
    AbortController,
    AbortSignal,
    Date: Clock,
    console: { error: (...args) => failures.push(args) },
    setInterval: (callback) => { const id = {}; timers.set(id, callback); return id },
    clearInterval: (id) => timers.delete(id),
    Bun: {
      spawn: (options) => {
        calls.push({
          topic: options.cmd.at(-1),
          env: options.env,
          payload: JSON.parse(new TextDecoder().decode(options.stdin)),
        })
        return { exited: Promise.resolve(0) }
      },
    },
  })
  const modules = new Map()
  async function moduleFor(name) {
    if (modules.has(name)) return modules.get(name)
    if (name === "node:fs" || name === "node:crypto" || name === "solid-js") {
      const exports = name === "node:fs"
        ? { mkdirSync() {}, appendFileSync: (...args) => failures.push(args) }
        : name === "node:crypto"
          ? { randomUUID }
          : { createEffect: (effect) => { effects.push(effect); effect() } }
      const module = new vm.SyntheticModule(Object.keys(exports), function () {
        for (const [key, value] of Object.entries(exports)) this.setExport(key, value)
      }, { context, identifier: name })
      modules.set(name, module)
      return module
    }
    const source = await readFile(new URL(name, bundle), "utf8")
    const module = new vm.SourceTextModule(source, { context, identifier: name })
    modules.set(name, module)
    await module.link((specifier, parent) => {
      if (!specifier.startsWith(".")) return moduleFor(specifier)
      // Resolve the same paths installed by WTA, including V1's hidden helper.
      if (specifier === "./wt-agent-hooks/bridge.js" || specifier === "./bridge.js") {
        return moduleFor("shared/bridge.js")
      }
      return moduleFor(new URL(specifier, new URL(parent.identifier, bundle)).href.slice(bundle.href.length))
    })
    return module
  }
  const module = await moduleFor(entry)
  await module.evaluate()
  return { plugin: module.namespace, calls, failures, timers, effects, advance: (ms) => { now += ms } }
}

function serverContext(sessions) {
  const hooks = {}
  let handlers
  let push
  const queue = []
  const stream = {
    [Symbol.asyncIterator]() {
      return {
        next: () => queue.length ? Promise.resolve(queue.shift()) : new Promise((resolve) => { push = resolve }),
      }
    },
  }
  const ctx = {
    location,
    rpc: { register: async (_definition, value) => { handlers = value } },
    session: {
      get: async ({ sessionID }) => {
        const value = sessions[sessionID]
        if (!value) throw new Error("unknown session")
        return value
      },
      hook: async (name, callback) => { hooks[name] = callback },
    },
    tool: { hook: async (name, callback) => { hooks[name] = callback } },
    event: {
      subscribe: ({ signal }) => {
        signal.addEventListener("abort", () => push?.({ done: true }))
        return stream
      },
    },
  }
  return {
    ctx,
    hooks,
    sync: (input) => handlers.sync(input),
    async event(type, data, eventLocation = location) {
      const value = { done: false, value: { type, data, location: eventLocation } }
      if (push) { const resolve = push; push = undefined; resolve(value) }
      else queue.push(value)
      await tick()
    },
  }
}

const binding = (pane, ids) => ({
  client_id: pane,
  pane_session_id: pane,
  com_clsid: clsid,
  bridge_path: "C:\\Terminal\\wtcli.exe",
  session_ids: ids,
})

test("V2 pane binding definition loads without installing the OpenCode SDK", async () => {
  await assert.doesNotReject(import(new URL("v2/rpc.js", bundle)))
})

test("V1 loads as a function on older CLIs and preserves child/ACP suppression", async () => {
  const fixture = await load("v1/wt-agent-hooks.js", { WT_SESSION: paneA, WT_COM_CLSID: clsid })
  assert.equal(Object.keys(fixture.plugin).length, 1)
  const hooks = await fixture.plugin.WtAgentHooks({ directory: location.directory })
  await hooks.event({ event: { type: "session.created", properties: { info: { id: "root", directory: location.directory } } } })
  await hooks.event({ event: { type: "session.created", properties: { info: { id: "child", parentID: "root" } } } })
  await hooks["chat.message"]({ sessionID: "child" })
  assert.deepEqual(fixture.calls.map((call) => call.payload.session_id), ["root"])
  await hooks["chat.message"]({ sessionID: "root" })
  await hooks.event({ event: { type: "question.asked", properties: { sessionID: "root" } } })
  await hooks.dispose()
  assert.deepEqual(fixture.calls.map((call) => call.topic), [
    "agent.session.start", "agent.session.start", "agent.prompt.submit", "agent.notification", "agent.session.end",
  ])
  assert.ok(fixture.calls.every((call) => call.env.WT_SESSION === paneA))

  const acp = await load("v1/wt-agent-hooks.js", { WT_SESSION: paneA, WT_COM_CLSID: clsid, OPENCODE_CLIENT: "acp" })
  const acpHooks = await acp.plugin.WtAgentHooks({ directory: location.directory })
  await acpHooks["chat.message"]({ sessionID: "root" })
  await acpHooks.dispose()
  assert.equal(acp.calls.length, 0)
})

test("V2 uses explicit pane bindings, never shared server environment or cwd attribution", async () => {
  const fixture = await load("v2/wt-agent-hooks.js", { WT_SESSION: "wrong-server-pane", WT_COM_CLSID: clsid })
  const server = serverContext({
    a: { id: "a", title: "A", location },
    b: { id: "b", title: "B", location },
    child: { id: "child", parentID: "a", location },
    other: { id: "other", location: { directory: "C:\\other" } },
  })
  const dispose = await fixture.plugin.default.setup(server.ctx)
  await server.event("session.status", { sessionID: "a", status: { type: "busy" } })
  assert.equal(fixture.calls.length, 0)
  await server.sync(binding(paneA, ["a"]))
  assert.equal(fixture.calls.at(-1).topic, "agent.prompt.submit", "late client binding must recover observed busy status")
  await server.sync(binding(paneB, ["b"]))
  await assert.rejects(server.sync(binding(paneA, ["child"])))
  await assert.rejects(server.sync(binding(paneA, ["other"])))
  await assert.rejects(server.sync(binding(paneB, ["a"])))
  await server.hooks.prompt({ sessionID: "b" })
  await server.hooks["execute.before"]({ sessionID: "a", tool: "question", input: { question: "Which?" } })
  await server.event("question.asked", { sessionID: "a" })
  await server.event("session.execution.failed", { sessionID: "b", error: { message: "Provider failed" } })
  const count = fixture.calls.length
  await server.event("session.deleted", { sessionID: "a" }, { directory: "C:\\other" })
  assert.equal(fixture.calls.length, count)
  assert.ok(fixture.calls.filter((call) => call.payload.session_id === "a").every((call) => call.env.WT_SESSION === paneA))
  assert.ok(fixture.calls.filter((call) => call.payload.session_id === "b").every((call) => call.env.WT_SESSION === paneB))
  await server.sync(binding(paneA, []))
  assert.equal(fixture.calls.at(-1).topic, "agent.session.end")
  assert.equal(fixture.calls.at(-1).payload.session_id, "a")
  const beforeDispose = fixture.calls.length
  await dispose()
  assert.equal(fixture.calls.length, beforeDispose, "server unload is not a CLI exit")
})

test("V2 ACP-started server registers RPC but forwards only explicitly bound interactive sessions", async () => {
  const fixture = await load("v2/wt-agent-hooks.js", { OPENCODE_CLIENT: "acp" })
  const server = serverContext({ a: { id: "a", location } })
  const dispose = await fixture.plugin.default.setup(server.ctx)
  await server.hooks.prompt({ sessionID: "a" })
  await server.event("session.status", { sessionID: "a", status: { type: "busy" } })
  assert.equal(fixture.calls.length, 0)
  await server.sync(binding(paneA, ["a"]))
  assert.equal(fixture.calls.at(-1).topic, "agent.prompt.submit")
  assert.ok(fixture.calls.every((call) => call.env.WT_SESSION === paneA))
  await dispose()

  const acp = await load("v2/tui.js", { OPENCODE_CLIENT: "acp", WT_SESSION: paneA, WT_COM_CLSID: clsid })
  const result = acp.plugin.default.setup({})
  assert.equal(result, undefined, "ACP TUI must not access RPC or register UI hooks")
  assert.equal(acp.timers.size, 0)
  assert.equal(acp.effects.length, 0)
  assert.equal(acp.calls.length, 0)
})

test("V2 missing heartbeats suppress forwarding without inventing a CLI exit", async () => {
  const fixture = await load("v2/wt-agent-hooks.js")
  const server = serverContext({ a: { id: "a", location } })
  const dispose = await fixture.plugin.default.setup(server.ctx)
  await server.sync(binding(paneA, ["a"]))
  const before = fixture.calls.length
  fixture.advance(46000)
  for (const expire of fixture.timers.values()) expire()
  await server.event("session.status", { sessionID: "a", status: { type: "busy" } })
  assert.equal(fixture.calls.length, before)
  await server.sync(binding(paneA, ["a"]))
  assert.equal(fixture.calls.at(-1).topic, "agent.prompt.submit")
  await dispose()
})

test("V2 a delayed session lookup cannot rebind a deleted session", async () => {
  const fixture = await load("v2/wt-agent-hooks.js")
  const server = serverContext({ a: { id: "a", location } })
  let resolveLookup
  server.ctx.session.get = () => new Promise((resolve) => { resolveLookup = resolve })
  const dispose = await fixture.plugin.default.setup(server.ctx)
  const pending = server.sync(binding(paneA, ["a"]))
  await server.event("session.deleted", { sessionID: "a" })
  resolveLookup({ id: "a", location })
  await assert.rejects(pending, /session changed/)
  assert.equal(fixture.calls.length, 0)
  await dispose()
})

test("V2 two panes sharing native tabs retain independent bindings and activity", async () => {
  const sessions = { a: { id: "a", title: "A", location }, b: { id: "b", title: "B", location } }
  const fixture = await load("v2/wt-agent-hooks.js")
  const server = serverContext(sessions)
  const disposeServer = await fixture.plugin.default.setup(server.ctx)
  const clients = []
  const requests = []
  try {
    for (const [pane, id] of [[paneA, "a"], [paneB, "b"]]) {
      const client = await load("v2/tui.js", { WT_SESSION: pane, WT_COM_CLSID: clsid })
      const dispose = client.plugin.default.setup({
        client: { rpc: () => ({ sync: async (input) => {
          requests.push(input)
          return server.sync(input)
        } }) },
        data: { session: { root: (id) => id, get: (id) => sessions[id] } },
        ui: {
          tabs: { enabled: () => true, list: () => [{ sessionID: "a" }, { sessionID: "b" }] },
          router: { current: () => ({ type: "session", sessionID: id }) },
          slot: (claim) => { claim.render(); return () => {} },
        },
      })
      clients.push({ client, dispose })
    }
    await tick()
    assert.ok(requests.length >= 2)
    assert.equal(new Set(requests.map((request) => request.client_id)).size, 2)
    for (const request of requests) {
      assert.deepEqual(Array.from(request.session_ids), [request.pane_session_id === paneA ? "a" : "b"])
    }
    assert.ok(clients.every(({ client }) => client.failures.length === 0))
    await server.hooks.prompt({ sessionID: "a" })
    await server.event("question.asked", { sessionID: "b" })
    await server.event("session.execution.succeeded", { sessionID: "a" })
    assert.equal(fixture.calls.at(-1).topic, "agent.stop")
    assert.equal(fixture.calls.at(-1).env.WT_SESSION, paneA)
    assert.equal(fixture.calls.findLast((call) => call.payload.session_id === "b").topic, "agent.notification")
    assert.ok(fixture.calls.filter((call) => call.payload.session_id === "a").every((call) => call.env.WT_SESSION === paneA))
    assert.ok(fixture.calls.filter((call) => call.payload.session_id === "b").every((call) => call.env.WT_SESSION === paneB))
    await clients[0].dispose()
    assert.equal(fixture.calls.at(-1).topic, "agent.session.end")
    assert.equal(fixture.calls.at(-1).payload.session_id, "a")
    for (const heartbeat of clients[1].client.timers.values()) heartbeat()
    await tick()
    await server.event("session.execution.succeeded", { sessionID: "b" })
    assert.equal(fixture.calls.at(-1).topic, "agent.stop")
    assert.equal(fixture.calls.at(-1).env.WT_SESSION, paneB)
  } finally {
    for (const { dispose } of clients) await dispose()
    await disposeServer()
  }
})

test("V2 client binds only its routed root despite shared tabs, and unbinds on home", async () => {
  const fixture = await load("v2/tui.js", { WT_SESSION: paneA, WT_COM_CLSID: clsid, WTCLI_PATH: "C:\\Terminal\\wtcli.exe" })
  const requests = []
  let route = { type: "session", sessionID: "child" }
  const sessions = { a: { id: "a", location }, child: { id: "child", parentID: "a", location } }
  const ctx = {
    client: { rpc: () => ({ sync: async (input, options) => { requests.push({ input, options }); return { bound: input.session_ids.length } } }) },
    data: { session: { root: (id) => id === "child" ? "a" : id, get: (id) => sessions[id] } },
    ui: {
      tabs: { enabled: () => true, list: () => [{ sessionID: "a" }, { sessionID: "child" }] },
      router: { current: () => route },
      slot: (claim) => { claim.render(); return () => {} },
    },
  }
  const dispose = fixture.plugin.default.setup(ctx)
  await tick()
  assert.ok(requests.length > 0)
  assert.ok(requests.every((request) => request.input.pane_session_id === paneA))
  assert.deepEqual(Array.from(requests.at(-1).input.session_ids), ["a"])
  route = { type: "home" }
  fixture.effects[0]()
  await tick()
  assert.equal(requests.at(-1).input.session_ids.length, 0)
  await dispose()
  assert.equal(fixture.timers.size, 0)
})
