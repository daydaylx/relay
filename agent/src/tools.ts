import type { AgentTool } from "@earendil-works/pi-agent-core";
import { Type } from "typebox";

export function makeSpikeTool(): AgentTool {
  return {
    name: "relay_spike_status",
    label: "Relay Spike Status",
    description: "Return a fixed message proving that Relay-owned tools are isolated.",
    parameters: Type.Object({}),
    async execute() {
      return {
        content: [{ type: "text", text: "Relay Pi integration spike is running. No system was inspected or changed." }],
        details: {},
      };
    },
  };
}
