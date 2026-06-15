---
name: best-practices
description: "Use when the user wants to ensure a task is done the RIGHT way — researching official docs, best practices, and codebase conventions BEFORE writing any code. Invoke this skill whenever the user says 'best practice', 'do it right', 'make sure this is correct', 'research first', 'what's the proper way', or when tackling unfamiliar libraries, complex integrations, migrations, or any task where getting it wrong would be costly. Also use when the user explicitly invokes /best-practices."
---

# Best Practices

Research deeply, validate thoroughly, then execute with confidence.

The default instinct is to jump straight into implementation — write code, see if it works, fix what breaks. This skill enforces the opposite: understand the problem space fully, find the established best practice, confirm the approach, and only then write code that you know is correct.

<HARD-GATE>
Do NOT write any implementation code, make any file changes, or take any implementation action until:
1. You have completed the Research Phase
2. You have presented your findings to the user
3. The user has approved the approach

This applies regardless of how simple the task appears. "Simple" tasks are where outdated assumptions cause the most damage.
</HARD-GATE>

## Anti-Pattern: "I Already Know How To Do This"

Your training data has a cutoff. Libraries release breaking changes. APIs get deprecated. Config formats evolve. The way you "know" how to do something may be 6-18 months out of date. The entire point of this skill is to verify against current reality before acting.

## The Three Phases

```
Phase 1: RESEARCH (mandatory)
├── Identify technologies and domains involved
├── Search official documentation (WebFetch/WebSearch)
├── Search codebase for existing patterns (Grep/Glob)
├── Search for known pitfalls and anti-patterns
├── Check version-specific considerations
└── Synthesize findings into an approach

    ▼ GATE: Present findings, get user approval

Phase 2: PLAN (mandatory)
├── Outline the specific changes to make
├── Identify files to create/modify
├── Note any dependencies or ordering constraints
└── Flag any risks or tradeoffs

    ▼ GATE: User confirms plan

Phase 3: EXECUTE
├── Implement following the researched approach
├── Verify against best practices checklist
└── Confirm result with user
```

---

## Phase 1: Research

This is the heart of the skill. Do not rush it. The time spent here saves multiples in debugging and rework.

### Step 1: Identify the Domain

Before searching anything, name what you're dealing with:
- What technologies, libraries, frameworks are involved?
- What versions are in use? (check `package.json`, `Cargo.toml`, `go.mod`, etc.)
- What's the task category? (new feature, migration, config, integration, refactor, etc.)

### Step 2: Search Official Documentation

For each technology involved, go to the source of truth:

1. **WebSearch** for the official docs page for the specific task
2. **WebFetch** the relevant doc page(s) and read them thoroughly
3. Look specifically for:
   - Recommended approach / getting started guide for this use case
   - API reference for the specific methods/components you'll use
   - Migration guides (if upgrading or changing approach)
   - Configuration reference (if config is involved)

Do NOT rely on your training data for API signatures, config options, or method names. They change frequently. Read the current docs.

### Step 3: Search the Codebase

Understand what already exists:

1. **Grep** for existing usage of the same libraries/patterns
2. **Glob** for related files (configs, similar components, tests)
3. Look for:
   - How this project already uses the technology (follow existing conventions)
   - Related tests that show expected behavior
   - Config files that may need updating
   - Similar solved problems you can learn from

### Step 4: Search for Pitfalls

Targeted research for what goes wrong:

1. **WebSearch** for common mistakes, gotchas, or "things I wish I knew" for the specific task
2. Look for:
   - Known breaking changes between versions
   - Common misconfiguration issues
   - Performance pitfalls
   - Security considerations
   - Deprecation warnings

### Step 5: Synthesize

Combine everything into a clear approach. You should now be able to answer:
- What is the officially recommended way to do this?
- How does this project already handle similar things?
- What are the known pitfalls to avoid?
- Are there version-specific considerations?

### Presenting Research Findings

Present a concise summary to the user. Structure it like this:

```
## Research Summary

**Task:** [what we're doing]
**Technologies:** [lib@version, framework@version, ...]

### Best Practice Approach
[The recommended approach from official docs, 2-4 sentences]

### Codebase Conventions
[How this project already handles similar things, or "no existing patterns found"]

### Key Pitfalls to Avoid
- [pitfall 1]
- [pitfall 2]

### Proposed Approach
[Your specific plan, combining best practices with project conventions]

### Sources
- [doc URL 1]
- [doc URL 2]
```

Wait for user approval before proceeding.

---

## Phase 2: Plan

After the user approves the research findings, outline the specific implementation:

1. **Files to modify/create** — list each file and what changes
2. **Order of operations** — what to do first, dependencies between steps
3. **Risks and tradeoffs** — anything the user should know
4. **Verification** — how you'll confirm it works (tests, manual check, etc.)

This can be brief for simple tasks. Scale the detail to the complexity.

Wait for user confirmation before executing.

---

## Phase 3: Execute

Now implement, following the researched approach exactly:

1. Make changes according to the plan
2. After implementation, do a self-check:
   - Does this match what the official docs recommend?
   - Does this follow the codebase conventions identified in research?
   - Did you avoid the pitfalls you identified?
3. Present the result to the user

---

## Red Flags — STOP and Return to Research

If you catch yourself thinking:
- "I know how this API works" (without having checked current docs)
- "This is straightforward, I don't need to look it up"
- "The docs are probably the same as what I remember"
- "Let me just try this and see if it works"
- "I'll research after I write the code"
- "This is a small change, best practices don't apply"

**ALL of these mean: STOP. You're skipping research.**

## Common Rationalizations

| Rationalization | Reality |
|----------------|---------|
| "It's just a one-liner" | One-liners with wrong API signatures cause hours of debugging |
| "I've done this a hundred times" | Your training data is frozen. The library may have changed. |
| "The user is in a hurry" | Researching for 2 minutes beats debugging for 20 |
| "I'll verify after I write it" | Post-hoc verification is confirmation bias. Research first. |
| "The docs won't have anything new" | Then research will be fast. Do it anyway. |
| "This is too simple for a research phase" | Simple tasks with wrong assumptions waste the most time |

## Parallel Research with Subagents

For tasks involving multiple technologies, spawn parallel research agents:

```
Agent 1: Research [technology A] best practices for [task]
Agent 2: Research [technology B] best practices for [task]
Agent 3: Search codebase for existing patterns
```

Synthesize all findings before presenting to user. This keeps research fast even for complex multi-tech tasks.

## When Research Finds Conflicting Advice

If official docs, community best practices, and codebase conventions disagree:

1. **Official docs win** for API usage, config format, and method signatures
2. **Codebase conventions win** for style, structure, and architectural patterns
3. **Community best practices win** for performance, security, and edge cases not covered by docs
4. **When truly conflicted**, present the options to the user with tradeoffs and let them decide

## Quick Reference

| Phase | What You Do | Gate |
|-------|------------|------|
| **1. Research** | Docs, codebase, pitfalls, synthesis | Present summary, get approval |
| **2. Plan** | Files, order, risks, verification | User confirms |
| **3. Execute** | Implement, self-check, present result | User verifies |
