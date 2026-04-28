# opencode-flowdeck — Project Guide

## Project Overview

opencode-flowdeck is a multi-agent workflow plugin for OpenCode (opencode.ai). It turns OpenCode into a spec-driven, context-persistent development system with `.planning/` state management and guard rails that enforce structured workflows.

**Core Value:** Every session starts with full context. Agents always know current phase, pending steps, and project state — without the user re-explaining.

## Workflow

The plugin enforces a structured workflow:

```
/new-project → /discuss → /plan → /new-feature → /resume
```

1. **`/new-project`** — Initialize `.planning/` for greenfield projects
2. **`/map-codebase`** — Analyze existing code into `.codebase/` docs
3. **`/discuss [topic]`** — Extract requirements via deep Q&A with D-XX decision tracking
4. **`/plan [phase]`** — Create phased implementation plan from discussion
5. **`/new-feature`** — Execute plan with orchestrator + parallel agents
6. **`/resume`** — Pick up from where session was interrupted

## State Files

| File | Purpose |
|------|---------|
| `.planning/PROJECT.md` | Project description, goals, tech stack |
| `.planning/REQUIREMENTS.md` | Extracted requirements with REQ-IDs |
| `.planning/ROADMAP.md` | 6-phase milestone breakdown |
| `.planning/STATE.md` | Current status: phase, steps, blockers |
| `.planning/config.json` | Plugin config (model profiles, toggles) |
| `.codebase/*.md` | Codebase map (STACK, ARCHITECTURE, CONVENTIONS, etc.) |

## Phases

| Phase | Name | Focus |
|-------|------|-------|
| 1 | Plugin Infrastructure & Core Tools | Scaffold + state tools |
| 2 | Hooks & Session Lifecycle | Guard rails + session persistence |
| 3 | Agent Definitions | 8 agent markdown files |
| 4 | Setup & Planning Commands | /new-project, /map-codebase, /discuss, /plan |
| 5 | Execution & Review Commands | /new-feature, /fix-bug, /review-code |
| 6 | Installation & Documentation | install.sh + README |

## Guard Rails

The system enforces three rules:
1. **No coding without a plan.** Agents block if `.planning/` is missing.
2. **No planning without a codebase map.** Agents request `/map-codebase` first on existing projects.
3. **Every session starts by reading state.** Agents know exactly where work was left off.

## Key Decisions

- Plain Markdown + JSON state (simple, git-friendly)
- Guard rails: WARN during setup, BLOCK during execution
- Agent model mix: Claude Sonnet/Haiku/Opus + Gemini 2.5 Flash + GPT-4o
- Source files under 200 lines

## Current State

Run `/progress` at any time to see current phase and pending steps.

---

*Last updated: 2026-04-25 after roadmap creation*
