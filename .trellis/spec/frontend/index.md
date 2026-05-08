# Frontend Development Guidelines

> Project-specific frontend development guidelines for this Vue 3 / TypeScript app.

---

## Overview

This directory documents the frontend conventions currently used in this project. These files are intended to be injected into future Trellis implement/check agents, so each guide is concise, actionable, and based on observed source patterns rather than generic ideals.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | Documented |
| [Component Guidelines](./component-guidelines.md) | Component patterns, props, composition | Documented |
| [Hook Guidelines](./hook-guidelines.md) | Vue composables, cleanup, data fetching patterns | Documented |
| [State Management](./state-management.md) | Local state, Pinia, persistence, server/event state | Documented |
| [Quality Guidelines](./quality-guidelines.md) | Code standards, quality gates, forbidden patterns | Documented |
| [Type Safety](./type-safety.md) | Type organization, boundaries, validation | Documented |
| [Playback Contracts](./playback-contracts.md) | Vue/Tauri/Rust playback and Subsonic request contracts | Documented |

---

## How to Use These Guidelines

Before frontend implementation, load the guide files relevant to the task:

1. Use [Directory Structure](./directory-structure.md) when adding or moving files.
2. Use [Component Guidelines](./component-guidelines.md) when editing Vue SFCs.
3. Use [Hook Guidelines](./hook-guidelines.md) when adding composables or async fetching logic.
4. Use [State Management](./state-management.md) when touching Pinia stores, persistence, or shared state.
5. Use [Quality Guidelines](./quality-guidelines.md) for verification, cleanup, async handling, and styling guardrails.
6. Use [Type Safety](./type-safety.md) when adding or changing contracts, DTOs, or boundary payloads.

---

**Language**: All documentation should be written in **English**.
