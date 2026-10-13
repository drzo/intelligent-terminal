// Managed by Intelligent Terminal: wt-agent-hooks
// OpenCode API variant: v2

import { emit, eventMessage, noteFailure } from "./bridge.js"
import { PaneBinding } from "./rpc.js"

const leaseMs = 45000
const guid = /^(?:[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}|\{[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}\})$/i
const sameLocation = (left, right) =>
  left?.directory?.toLowerCase() === right?.directory?.toLowerCase() &&
  left?.workspaceID === right?.workspaceID

export default {
  id: "wt-agent-hooks",
  async setup(ctx) {
    if (process.platform !== "win32" || process.env.OPENCODE_CLIENT === "acp") return

    const owners = new Map()
    const lookups = new Map()
    const activity = new Map()
    const controller = new AbortController()
    let disposed = false
    function publish(topic, id, payload = {}) {
      const owner = owners.get(id)
      if (disposed || !owner || Date.now() - owner.renewed > leaseMs) return
      emit(owner.context, topic, id, { cwd: owner.session.location.directory, ...payload })
    }
    function release(id, owner, reason) {
      if (owners.get(id) !== owner) return
      emit(owner.context, "agent.session.end", id, { cwd: owner.session.location.directory, reason })
      owners.delete(id)
      lookups.delete(id)
    }
    function record(topic, id, payload = {}, forward = true) {
      if (!id) return
      activity.delete(id)
      activity.set(id, { topic, payload })
      if (activity.size > 512) activity.delete(activity.keys().next().value)
      if (forward) publish(topic, id, payload)
    }

    await ctx.rpc.register(PaneBinding, {
      sync: async (input) => {
        if (!guid.test(input.pane_session_id) || !guid.test(input.com_clsid)) {
          throw new Error("Invalid Intelligent Terminal pane context")
        }
        const tokens = input.session_ids.map((id) => {
          const token = lookups.get(id) || {}
          lookups.set(id, token)
          return token
        })
        try {
          const sessions = await Promise.all(input.session_ids.map((sessionID) => ctx.session.get({ sessionID })))
          if (disposed) throw new Error("OpenCode hook bridge is unloading")
          for (const [index, session] of sessions.entries()) {
            if (lookups.get(session.id) !== tokens[index]) {
              throw new Error("OpenCode session changed during pane binding")
            }
            if (session.parentID || !sameLocation(session.location, ctx.location)) {
              throw new Error("Pane bindings require root sessions in this plugin location")
            }
            const previous = owners.get(session.id)
            if (previous && previous.clientID !== input.client_id && Date.now() - previous.renewed <= leaseMs) {
              throw new Error("OpenCode session is already bound to another terminal client")
            }
          }
          const wanted = new Set(input.session_ids)
          for (const [id, owner] of owners) {
            if (owner.clientID === input.client_id && !wanted.has(id)) release(id, owner, "client detached")
          }
          for (const session of sessions) {
            const previous = owners.get(session.id)
            const owner = {
              clientID: input.client_id,
              context: input,
              session,
              renewed: Date.now(),
            }
            owners.set(session.id, owner)
            if (!previous || previous.clientID !== input.client_id || previous.session.title !== session.title) {
              publish("agent.session.start", session.id, { title: session.title || "" })
              const last = activity.get(session.id)
              if (last) publish(last.topic, session.id, last.payload)
            }
          }
          return { bound: sessions.length }
        } finally {
          for (const [index, id] of input.session_ids.entries()) {
            if (!owners.has(id) && lookups.get(id) === tokens[index]) lookups.delete(id)
          }
        }
      },
    })

    await ctx.session.hook("prompt", ({ sessionID }) => {
      publish("agent.session.start", sessionID)
      record("agent.prompt.submit", sessionID)
    })
    await ctx.tool.hook("execute.before", (event) => {
      if (event.tool === "question") {
        record("agent.notification", event.sessionID, { message: "OpenCode is waiting for input" }, false)
      } else {
        record("agent.prompt.submit", event.sessionID, {}, false)
      }
      publish("agent.tool.starting", event.sessionID, { tool_name: event.tool, tool_input: event.input })
    })

    const events = (async () => {
      try {
        for await (const event of ctx.event.subscribe({ signal: controller.signal })) {
          if (controller.signal.aborted) break
          if (event.location && !sameLocation(event.location, ctx.location)) continue
          const data = event.data || {}
          const id = data.sessionID
          const owner = owners.get(id)
          switch (event.type) {
            case "session.renamed":
              if (!owner) break
              owner.session = { ...owner.session, title: data.title }
              publish("agent.session.start", id, { title: data.title })
              break
            case "session.status":
              if (data.status?.type === "idle") record("agent.stop", id)
              else if (data.status?.type === "busy" || data.status?.type === "retry") {
                record("agent.prompt.submit", id)
              }
              break
            case "session.execution.started":
            case "permission.replied":
            case "question.replied":
              record("agent.prompt.submit", id)
              break
            case "session.idle":
            case "session.execution.succeeded":
            case "session.execution.interrupted":
              record("agent.stop", id)
              break
            case "session.execution.failed":
              record("agent.error", id, { error: eventMessage(data.error) || "OpenCode session error" })
              break
            case "permission.asked":
            case "question.asked":
              record("agent.notification", id, {
                message: event.type === "permission.asked"
                  ? `Permission required: ${data.action || "tool use"}`
                  : "OpenCode is waiting for input",
              })
              break
            case "session.deleted":
              if (owner) release(id, owner, "deleted")
              lookups.delete(id)
              activity.delete(id)
              break
          }
        }
      } catch (error) {
        if (!controller.signal.aborted) noteFailure(undefined, "event.subscribe", error)
      }
    })()
    const expiry = setInterval(() => {
      for (const [id, owner] of owners) {
        if (Date.now() - owner.renewed > leaseMs) {
          // A lost heartbeat is not proof that the CLI exited. Keep WTA's
          // row/binding intact; real pane lifecycle owns exit detection.
          owners.delete(id)
          lookups.delete(id)
        }
      }
    }, 5000)

    return async () => {
      disposed = true
      clearInterval(expiry)
      controller.abort()
      owners.clear()
      lookups.clear()
      activity.clear()
      await events
    }
  },
}
