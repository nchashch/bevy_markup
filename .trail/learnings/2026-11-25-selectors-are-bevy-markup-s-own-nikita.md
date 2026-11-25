---
title: "Selectors are bevy_markup's own; keep them flat and read element fields one at a time"
author: Nikita
date: 2026-11-25
branch: own-selector-matcher
cost: { minutes: 98, tokens: 388286 }
id: l-c7b6d901
scope: repo
status: active
src: agent
touches:
  - ".aiwg/activity.log"
  - docs/agents/adr/0049-minijinja-with-fuel-instead-of-tera.md
  - docs/agents/adr/README.md
  - AGENTS.md
  - CHANGELOG.md
  - Cargo.lock
  - Cargo.toml
  - docs/agents/bugs/UPSTREAM.md
  - fuzz/Cargo.lock
  - fuzzcheck/Cargo.lock
---

## Decided
ADR 0050 replaced parcel_selectors with our own selector parser and matcher. The parser must accept exactly the grammar parcel accepted (a rejected selector fails the whole sheet), including its parse states after pseudo-elements and its odd end-of-input reading of `*|`/`|`. 4M differential cases against parcel agreed before parcel was removed.

## Tried and ruled out
- A selector as a Vec of compound Vecs, deep-cloned by the cascade: 6% slower than parcel on plain sheets (twice the libc time: malloc and memmove). One flat Vec per selector with combinator separators, plus Arc-shared sheet selectors, fixed the allocations.
- Building a whole element view (~100 bytes) per simple-selector test: the optimizer kept the copy. Per-field accessors on Node made matching faster than parcel.

## Watch out
- perf call graphs (`-g`, `--call-graph dwarf`) fail here: kernel.perf_event_paranoid is 2. Flat `perf record -e cpu-clock:u` works; attribute libc time by comparing dso shares between builds.
- Benchmark builds in the scratchpad live on a RAM-backed tmpfs: delete old target dirs, or a link fails with "No space left on device" and memory runs short.

In the code: src/css/select.rs (parser, `Simple::Combinator` layout), src/cascade/selector.rs (`matches_from`, `Outcome`, `Node::tag/classes/...`), benches/shared/mod.rs (selectors-deep/-siblings/-functional).
