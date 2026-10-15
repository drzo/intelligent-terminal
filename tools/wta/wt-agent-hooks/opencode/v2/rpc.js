// Managed by Intelligent Terminal: wt-agent-hooks

// Keep the installed RPC contract independent of SDK package resolution.
export const PaneBinding = {
  id: "wt-agent-hooks.pane",
  methods: {
    sync: {
      output: {
        type: "object",
        properties: { bound: { type: "integer", minimum: 0 } },
        required: ["bound"],
        additionalProperties: false,
      },
      input: {
        type: "object",
        properties: {
          client_id: { type: "string", format: "uuid" },
          pane_session_id: { type: "string", minLength: 1, maxLength: 64 },
          com_clsid: { type: "string", minLength: 1, maxLength: 64 },
          bridge_path: { type: "string", minLength: 1, maxLength: 32768 },
          log_dir: { type: "string", maxLength: 32768 },
          session_ids: {
            type: "array",
            uniqueItems: true,
            items: { type: "string", minLength: 1, maxLength: 1024 },
          },
        },
        required: ["client_id", "pane_session_id", "com_clsid", "bridge_path", "session_ids"],
        additionalProperties: false,
      },
    },
  },
  events: {},
}
