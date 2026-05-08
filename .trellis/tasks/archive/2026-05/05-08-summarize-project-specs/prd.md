# Summarize Project Specs

## Goal

Summarize this existing project's actual frontend conventions into `.trellis/spec/frontend/` guideline files so future AI-assisted work follows the current codebase rather than generic defaults.

## What I already know

* The user asked to summarize specs from the existing project.
* The project is a single-repo frontend-focused Vue 3 / TypeScript app with Trellis spec layer `frontend`.
* Existing frontend guideline index lists files for directory structure, component guidelines, hook guidelines, state management, quality guidelines, and type safety, but they are marked as "To fill".
* Research has identified concrete conventions for source layout, Vue SFC component patterns, composables/data fetching, Pinia state, TypeScript typing, quality checks, async cleanup, and theme styling.

## Research References

* [`research/directory-structure.md`](research/directory-structure.md) — `src/` organization, route/view/component/service/store/utils/assets placement, and naming examples.
* [`research/component-guidelines.md`](research/component-guidelines.md) — Vue SFC Composition API patterns, props/emits, lifecycle cleanup, Teleport/overlay, TDesign/Naive usage, and scoped CSS/SCSS conventions.
* [`research/hooks-state-types.md`](research/hooks-state-types.md) — composables, direct async fetching, manual caches, Pinia persistence, shared/local types, IPC DTOs, and boundary validation patterns.
* [`research/quality-guidelines.md`](research/quality-guidelines.md) — current quality gate, TypeScript/import rules, async error handling, lifecycle cleanup, theme tokens, performance UI, and undesirable patterns.

## Assumptions (temporary)

* The target output is updating existing `.trellis/spec/frontend/*.md` files, not creating a new spec system.
* The specs should document actual conventions found in the current codebase with concise examples and forbidden patterns.
* No application behavior should change.
* The spec docs should be written in English, matching `.trellis/spec/frontend/index.md`.

## Open Questions

* None.

## Requirements (evolving)

* Inspect the existing project structure and representative source files.
* Fill frontend spec guideline files with project-specific conventions.
* Use concise high-signal documentation optimized for future AI context injection.
* Include concrete examples or file references from the current codebase where useful.
* Keep the specs useful for future implementation/check agents.
* Document observed caveats rather than inventing standards that are not configured, such as lint/test tools.

## Acceptance Criteria (evolving)

* [ ] `.trellis/spec/frontend/directory-structure.md` describes actual module/file organization.
* [ ] `.trellis/spec/frontend/component-guidelines.md` describes actual component patterns and composition rules.
* [ ] `.trellis/spec/frontend/hook-guidelines.md` describes actual hook and data-fetching patterns.
* [ ] `.trellis/spec/frontend/state-management.md` describes actual local/global/server state patterns.
* [ ] `.trellis/spec/frontend/quality-guidelines.md` captures code standards and forbidden patterns observed in the project.
* [ ] `.trellis/spec/frontend/type-safety.md` captures TypeScript/type-safety conventions observed in the project.
* [ ] No app source behavior is changed.
* [ ] Specs reference actual source examples and avoid unsupported claims.

## Definition of Done (team quality bar)

* Spec files updated.
* Research notes persisted under `research/`.
* Relevant context is curated in `implement.jsonl` and `check.jsonl` before implementation.
* Git diff reviewed for scope.

## Out of Scope (explicit)

* Refactoring application source code.
* Adding new runtime behavior.
* Reorganizing the source tree.
* Adding new lint/test tooling as part of this task.

## Expansion Sweep

### Future evolution

* These specs may later guide sub-agents for every frontend task, so they should be actionable and easy to scan.
* More exhaustive examples can be added later as new conventions are discovered; this first pass should avoid freezing accidental inconsistencies as hard rules.

### Related scenarios

* The docs should align with existing Trellis frontend index sections and be suitable for `implement.jsonl` / `check.jsonl` injection.
* Quality guidance should distinguish current hard requirements from preferences or undesirable patterns that still exist in legacy code.

### Failure / edge cases

* Overstating conventions could cause future agents to refactor unnecessarily or reject valid existing patterns.
* Under-documenting examples could make the specs too generic to help future implementation agents.

## Feasible Approaches

**Approach A: Concise high-signal specs** (Recommended)

* How it works: Write each spec file as a compact set of rules, actual examples, and caveats focused on what future agents need before coding/checking.
* Pros: Fast to use, lower risk of documenting incidental details, good for agent context injection.
* Cons: Less exhaustive as human onboarding documentation.

**Approach B: Exhaustive onboarding docs**

* How it works: Turn each guideline file into a detailed reference with many examples and caveats.
* Pros: More useful for new human contributors reading everything manually.
* Cons: Larger context cost for agents and more maintenance burden.

**Approach C: Two-layer docs**

* How it works: Keep guideline files concise, with optional longer appendix/research references for deep details.
* Pros: Balances agent usability and human depth.
* Cons: Slightly more structure to maintain.

## Decision (ADR-lite)

**Context**: The frontend spec files should guide future Trellis implement/check agents. Large onboarding-style documents would increase context cost and risk freezing incidental inconsistencies as hard rules.

**Decision**: Use Approach A, concise high-signal specs: each guideline file should contain actionable rules, source examples, and caveats focused on actual project conventions.

**Consequences**: The first pass will be optimized for AI-assisted development and may not cover every edge case. Additional details can be added later when a convention becomes important enough to preserve.

## Technical Notes

* Task directory: `.trellis/tasks/05-08-summarize-project-specs`.
* Spec index inspected: `.trellis/spec/frontend/index.md`.
* Thinking guides index inspected: `.trellis/spec/guides/index.md`.
* Research was performed by Trellis research agents and persisted under the task research directory.
