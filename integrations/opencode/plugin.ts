// repoctx opencode plugin (tier-2 runtime interception).
//
// Installed by `repoctx hook install opencode` to `.opencode/plugin/repoctx.ts`.
// Intercepts the agent's bash `rg`/`grep <identifier>` calls before they run
// and rewrites them to the structural `repoctx` query — the same decision the
// Claude Code hook makes, delegated to the `repoctx rewrite` exit-code
// utility so the rules live in one place (the binary).
//
// opencode plugin API: a plugin is a function returning a hooks object.
// `tool.execute.before(input, output)` fires before a tool runs; mutating
// `output.args` changes what the tool receives.

import { spawnSync } from "node:child_process";

export const RepoctxPlugin = async () => {
  return {
    "tool.execute.before": async (
      input: { tool: string },
      output: { args: Record<string, unknown> },
    ) => {
      if (input.tool !== "bash") return;
      const command = output.args?.command;
      if (typeof command !== "string" || command.length === 0) return;

      // `repoctx rewrite <cmd>`: exit 0 + rewritten command on stdout when a
      // rule fires; exit 1 = passthrough (leave the command untouched).
      let res;
      try {
        res = spawnSync("repoctx", ["rewrite", command], { encoding: "utf8" });
      } catch {
        return; // repoctx not installed → passthrough
      }
      if (res.status === 0) {
        const rewritten = (res.stdout || "").trim();
        if (rewritten.length > 0 && rewritten !== command) {
          output.args.command = rewritten;
        }
      }
    },
  };
};
