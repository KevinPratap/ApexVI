## Task & Summary

- **Claim ID** (from #1 / `CLAIMS.md`, or `none` for docs/typos): ``
- **Switch** (`VICTORY_*`, must default to **off** on `main`, or `n/a`): ``
- **Source / Attribution** (`original`, or upstream/fork repo + commit/PR + author): ``

What does this change do, and why?

## Measurements (`tools/frametime.py`)

- **Machine**: GPU (VRAM), CPU, RAM, OS
- **Scene / test**: (describe generically — **never** name the title, executable, characters, or attach screenshots/dumps)

| Build | Samples | FPS | ms/frame | prep (ms) | copies (ms) | gpu (ms) | back (ms) | draws | VRAM |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Before (`main`) | | | | | | | | | |
| After (switch ON) | | | | | | | | | |

*(If not measured in a title, write "Not measured in a title" and describe the unit / synthetic GPU test that verifies it.)*

## Checklist

- [ ] **Asset policy (`docs/asset-policy.md`)**: `cargo xtask policy` passes; zero game files, dumps, screenshots, title/character names, or title shader hashes anywhere in code, commits, or this PR.
- [ ] **Single change, opt-in**: one logical change, gated behind a `VICTORY_*` switch that is **off by default** (unless purely a test/doc/xtask change).
- [ ] **Checks pass**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` and `cargo test --workspace --locked`.
- [ ] **Credits**: any ported or adapted work credits the author, repository, and commit/PR.
