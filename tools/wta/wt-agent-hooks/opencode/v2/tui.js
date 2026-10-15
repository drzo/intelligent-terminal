// Managed by Intelligent Terminal: wt-agent-hooks

import { randomUUID } from "node:crypto"
import { createEffect } from "solid-js"
import { clientContext, noteFailure } from "./bridge.js"
import { PaneBinding } from "./rpc.js"

export default {
  id: "wt-agent-hooks.client",
  setup(ctx) {
    const context = clientContext()
    if (!context) return

    const clientID = randomUUID()
    const rpc = ctx.client.rpc(PaneBinding)
    const locations = new Map()
    const controller = new AbortController()
    let disposed = false
    let pending = Promise.resolve()
    const key = (location) => JSON.stringify([location.directory, location.workspaceID])

    function synchronize() {
      if (disposed) return
      // Native tabs are shared across TUI clients; only this route proves pane ownership.
      const ids = new Set()
      const route = ctx.ui.router.current()
      if (route.type === "session") ids.add(ctx.data.session.root(route.sessionID))
      const wanted = new Map()
      for (const id of ids) {
        const session = ctx.data.session.get(id)
        if (!session || session.parentID) continue
        const locationKey = key(session.location)
        const entry = wanted.get(locationKey) || { location: session.location, ids: [] }
        entry.ids.push(id)
        wanted.set(locationKey, entry)
      }
      pending = pending.then(async () => {
        if (disposed) return
        const all = new Map([...locations, ...wanted])
        for (const [locationKey, entry] of all) {
          if (disposed) return
          try {
            const sessionIDs = wanted.get(locationKey)?.ids || []
            const result = await rpc.sync(
              { ...context, client_id: clientID, session_ids: sessionIDs },
              { location: entry.location, signal: AbortSignal.any([controller.signal, AbortSignal.timeout(5000)]) },
            )
            if (result.bound !== sessionIDs.length) throw new Error("OpenCode pane binding acknowledgement is incomplete")
            if (wanted.has(locationKey)) locations.set(locationKey, entry)
            else locations.delete(locationKey)
          } catch (error) {
            if (!disposed) noteFailure(context, "pane binding", error)
          }
        }
      })
    }

    // The effect follows client-local route/tabs, not all sessions in the
    // shared server. The slot adds no visible UI.
    const removeSlot = ctx.ui.slot({
      append: "app",
      render() {
        createEffect(synchronize)
        return null
      },
    })
    const heartbeat = setInterval(synchronize, 10000)
    synchronize()

    return async () => {
      disposed = true
      controller.abort()
      clearInterval(heartbeat)
      removeSlot()
      await pending
      await Promise.all([...locations.values()].map(async (entry) => {
        try {
          await rpc.sync(
            { ...context, client_id: clientID, session_ids: [] },
            { location: entry.location, signal: AbortSignal.timeout(1000) },
          )
        } catch (error) {
          noteFailure(context, "pane unbinding", error)
        }
      }))
      locations.clear()
    }
  },
}
