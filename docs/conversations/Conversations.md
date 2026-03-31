# Conversations — Session Index

> Indexed log of Claude Code sessions on microgrid-agent.
> Each entry captures branch, focus area, and key outcomes.
> Search: `grep -rl "keyword" docs/conversations/`

---

## 2026-03-31 — Bootstrap & restructure session

**Branch:** `main`
**Focus:** Full governance bootstrap + project restructure
**Outcomes:**
- Fixed `session-stop.sh` macOS compatibility (`grep -oP` → `sed`-based parsing)
- Bootstrapped Claude auto-memory system (MEMORY.md + memory files)
- Created `docs/conversations/` with this index
- Installed full control metalayer (METALAYER.md, schemas, audit scripts, githooks)
- **Project restructure**: `prototype/` → `reference/`, `sim/` → `simulation/`, `ml/` → `forecast/`, `schema/`+`schemas/` → `.control/schemas/`, `evals/` → `.control/evals/`, deleted empty `src/`
- Installed conversation bridge (conversation-history.py + Stop hook)
- All paths updated across CLAUDE.md, AGENTS.md, METALAYER.md, README.md, Makefile, hooks, policy, topology, evals, genome, diy-guide
- **Verification**: 116 Python tests pass, 39 Rust tests pass, control-audit 19/19, bstack-check 13/13
