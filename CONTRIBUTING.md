# Contributing to ApexVI

ApexVI is tuned for **laptops and graphics cards with 6-8 GB VRAM**, where every megabyte of video memory and millisecond of CPU preparation counts. Humans and coding agents coordinate here under the same transparent rules used in VIctory.

## 1. Rule #1: Asset policy (non-negotiable)

Read **[docs/asset-policy.md](docs/asset-policy.md)** first, then install the pre-commit hook right after cloning:

```powershell
cargo xtask install-hooks
```

- **Nothing from the game is ever stored or quoted in this repository, issues, or pull requests**: no game files, executables, shaders, dumps, decompiled snippets, extracted tables, or screenshots, and **do not name the title, its executable, its characters, or title-specific shader hashes** (write *"the title"* or *"an Xbox Series title"*).
- Every commit and CI run enforces `cargo xtask policy` (strict text-only allowlist, 512 KiB cap, magic-byte and hex/base64 payload rejection).

## 2. Claim before you write code

So two contributors (or two agents) do not write the same fix twice, check **[CLAIMS.md](https://github.com/KevinPratap/ApexVI/blob/forge/status/CLAIMS.md)** and open pull requests first, then comment in **[Issue #1 — Coordinating who works on what](https://github.com/KevinPratap/ApexVI/issues/1)**:

- `claiming: <ID>` — take an open task from Issue #1 or `CLAIMS.md` before starting.
- `claiming: <one line>` — claim a task not yet listed (assigns `NEW-n`).
- `done: <ID> -> #<PR>` — link your open pull request (or include `<ID>` in the PR title/body).
- `release: <ID>` — return a task you are no longer working on.
- `finding: <one line>` — log an actionable finding for someone else (`FIND-n`).

Claims expire after **14 days** without an open PR, and edited comments are ignored so claims stay strictly first-come, first-served. Full mechanics: **[docs/forge.md](docs/forge.md)**.

## 3. Pull request rules

1. **One change per pull request.** Keep scope surgical so regressions and speedups are unambiguous.
2. **Opt-in behind a `VICTORY_*` switch (off by default).** New runtime, GPU, memory, or shader behavior must be gated behind a `VICTORY_*` environment variable that defaults to off on `main`, documented right where `std::env::var` / `var_os` reads it. Proven switches are enabled together in `scripts/play.ps1`.
3. **Measure before and after.**
   - If your change targets frame time, VRAM, or visual correctness and you have hardware + your own copy of the title, build with `--profile diag` and report median numbers from `tools/frametime.py`:
     ```powershell
     cargo build --locked --profile diag -p victory --features diagnostics
     python tools/frametime.py <before.log>
     python tools/frametime.py <after.log>
     ```
     Include your GPU (and VRAM), CPU, RAM, resolution/profile, and scene description.
   - If you do not have the title, include a deterministic unit test or synthetic D3D12 hardware test (skipping cleanly when no adapter is present) and state *"not measured in a title"* in the PR description.
4. **Credit your sources (GPL-3.0-or-later).** When porting or adapting an idea or commit from upstream `Marvisak/VIctory` or another fork, name the author, repository, and commit/PR in both the commit message and PR body.
5. **Pass all local checks before opening the PR:**
   ```powershell
   cargo xtask policy
   cargo clippy --workspace --all-targets --locked -- -D warnings
   cargo clippy --workspace --all-targets --features victory/diagnostics --locked -- -D warnings
   cargo test --workspace --locked
   ```

## 4. The Forge (`forge/main`)

Every 12 hours, `.github/workflows/forge.yml` runs `cargo xtask forge` to build **`forge/main`** — `main` plus every open non-draft pull request that merges cleanly, compiles, passes `cargo xtask policy`, and does not touch `.github/workflows/`. Per-PR merge and test results are published to **[STATUS.md](https://github.com/KevinPratap/ApexVI/blob/forge/status/STATUS.md)**. Being in `forge/main` lets testers try open PRs together; review still decides merges into `main`.
