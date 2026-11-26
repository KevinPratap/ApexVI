# ApexIV

A fork of [VIctory](https://github.com/Marvisak/VIctory), the compatibility
layer that runs an Xbox Series title natively on Windows from files the user
supplies, tuned for **laptops and graphics cards with 6-8 GB**. It brings the
work of several forks together and measures every change on one modest machine.

**This repository contains no game files, code or assets.** You need your own
copy of the game. It is not affiliated with or endorsed by the game's developer
or publisher. As in other forks, the title is not named here.

## Where it stands (2026-11-27)

Tested on one machine: Windows 11, NVIDIA RTX 4050 Laptop (**6 GB VRAM**, 32-lane waves),
Intel Core i7-12650H, 32 GB RAM, the Series S profile (`--console series-s`), 1920x1080.

| | |
|---|---|
| Menus, loading screens, account picker | visible, sharp and responsive at **30 fps** (both `PresentX` planes composed; UI and loading art kept at full size) |
| Character control, HUD, audio | controllable in the world (movement, camera, HUD prompts, pause/menus, audio); played from the opening room through the yard to the outdoor pier/parking scene |
| Picture in the world | lit and textured on 6 GB VRAM: whole character torsos and wire-mesh windows (`VICTORY_MESH_9BIT_CORNERS`), clean BC7 clothing (`VICTORY_BC7_ROTATION`), indirect garbage clamp (`VICTORY_INDIRECT_ZERO_GARBAGE`) |
| Frame rate in the world | **6.35-7.5 fps** by scene (133-158 ms/frame; outdoor pier ~6.35 fps, opening room ~7.5 fps), up from 4.8 fps (210 ms) at the start of this fork |
| Stability | **14-15 minute** continuous runs with no crash or device loss; the title's hang watchdog is stepped over |
| Known limits on this 6 GB laptop | CPU submit-thread preparation (~102-135 ms/frame) caps frame rate below full speed; interiors without direct light rely on the ambient stand-in while ray-traced passes do not run |

### Where a frame goes

The console's CPU and GPU share one memory; here every write of one that the
other reads must be noticed and copied. Of a 140 ms frame:

| Part | ms per frame |
|---|---|
| Our submit thread preparing the frame (refreshing what the title wrote, merging what shaders wrote, descriptors) | ~102 (the wall) |
| The card's work | ~78 |
| The card waiting for us | ~49 |

## What is different here

Each change is opt-in behind a `VICTORY_*` switch, described where it is read;
`scripts/play.ps1` sets the ones measured to help.

- **The UI plane is shown**: a Series S title presents its UI (menus, loading
  art, HUD) in a second `PresentX` plane; it is drawn over the scene in the
  present's own list (from upstream #1997 / Yaekai's #211).
- **Textures fit a 6 GB card**: host copies leave off their top levels
  (`VICTORY_TEXTURE_MIP_DROP=2`, `_FLOOR=128`, `_COLOR=1`; after upstream
  #2146): about 4,000 textures wait instead of 12,000, and rooms render.
- **The card waits for less**: masked merges with an index
  (`VICTORY_MERGE_MASKED`, `_INDEX`), copies of GPU results made after their
  flight lands (`VICTORY_COPY_AFTER_LANDING`), false-positive reads let ahead
  with a per-piece miss check (`VICTORY_COPY_AHEAD`, `VICTORY_FILL_AHEAD_COPY_MISS`),
  relay stage 2.
- **Cheaper write tracking**: joined and view-aware write-watch askings
  (`VICTORY_WATCH_TEXTURES_JOINED`, `VICTORY_WATCH_VIEWS_SKIP`), absent regions
  cached with a time limit (`VICTORY_GUARD_ABSENT_CACHE=4`), guards resting
  after a fault and opening only the written view (`VICTORY_GUARD_REST`,
  `VICTORY_OPEN_ONE_VIEW`, after the VIctory-7900XTX fork), clean blocks in a
  lock-free bitset (`VICTORY_GUARD_BITS`), 4 KiB refreshes (`VICTORY_FINE_PAGES`).
- **Less locking**: lock-free unchanged descriptor slots (`VICTORY_MIRROR_FAST`),
  in-place small reads (`VICTORY_COPY_PLAIN_LEAST`), leaner merges
  (`VICTORY_MERGE_BUFFERS`, `VICTORY_MERGE_CHUNKS`).
- **Light**: the ambient stand-in with reflections held and the sky followed
  (`VICTORY_AMBIENT_REFLECT`, `VICTORY_AMBIENT_SKY`, from Belobug's fork).
- **UI and loading art at full size** while world textures shrink (`VICTORY_TEXTURE_MIP_DROP_UI_FULL=2`: what a draw into a presented image samples is kept whole), **whole character torsos and wire-mesh geometry** (`VICTORY_MESH_9BIT_CORNERS=1`: 9-bit mesh primitive corner indices, dropping bit-9 edge flags so edge-flagged triangles are not culled, from upstream #1718 / `c0228d9`), **clean BC7 clothing blocks** (`VICTORY_BC7_ROTATION=1`: BC7 channel rotation restricted to modes 4 and 5, from upstream #3111), **clamped indirect mesh records and 16-bit SDWA sign-extension** (`VICTORY_INDIRECT_ZERO_GARBAGE=1`, `VICTORY_SDWA_SEXT_16=1`, from upstream #1799 / #3124 and `voidopcode/vilocity`), **full-view dirty-page and rotating-texture refreshes** (`VICTORY_PAGE_WRITES=all`, `VICTORY_TEXTURE_CHANGING=1`: every-frame rechecks for both tabled and bindless video ring-buffer textures, after `fxceless/victory-fork` #602 / #1314), **console sync UAV barriers** (`VICTORY_SYNC_BARRIER=1`, from `realblastex/flamingo`), and **zero camera jitter under spatial upscaling** (`VICTORY_SPATIAL_NO_JITTER=1`).
- **Parallel preparation and smaller physical sections**: heap mirroring and per-copy merges on wide threads (`VICTORY_PREP_SPLIT=2`), parallel staged-texture refresh checks (`VICTORY_PARALLEL_REFRESH=1`, from fxceless's fork), and 2 MiB physical-page sections (`VICTORY_SECTION_MB=2`, from upstream #3177: remap stalls 7.3 -> 1.3 ms/frame).
- **Stability**: the title's hang watchdog stepped over with a log of the
  stale heartbeat (`VICTORY_STEP_OVER`); four 64-lane programs the 32-lane host
  refused translated with wave-wide branches (`VICTORY_WAVE_WIDE_ONLY`).

What was tried and measured as no gain or a loss is recorded in the commit
messages (in-place GPU reads over PCIe, local fill-ahead, wide refresh with the
current journal lock, a texture size cap).

## How to run it

You need Windows 10/11 x64, an AVX2 CPU, a Direct3D 12 GPU with shader model
6.6 and mesh shaders, Rust (the toolchain is pinned), the MSVC build tools, and
your own copy of the game.

```powershell
git clone https://github.com/KevinPratap/ApexIV
cd ApexIV
cargo build --locked --profile diag -p victory --features diagnostics
scripts\play.ps1 -Game <path of your own copy's executable> -Cache D:\apex-cache -Data D:\apex-data
```

`-Safe` leaves out the newest switches. The first run makes pipelines and
stutters; later runs reuse the cache.

## Help wanted

Measure on your machine and say what you see (GPU, CPU, RAM, fps by scene).
`tools/frametime.py <log> [min draws]` gives the median frame parts of a run
log. The open work, biggest first:

1. **Split the submit thread's preparation** ([docs/prep-split-plan.md](docs/prep-split-plan.md)):
   Design A: step A0 (journal lock split, `VICTORY_JOURNAL_SPLIT`, with refreshes on
   several threads, `VICTORY_WIDE_REFRESH`) is in; heap sync and merges on the wide threads,
   about 9 fps here) is in progress; Design B (two submissions in preparation)
   after it, toward the card-bound ceiling of about 11-12 fps on this laptop.
2. **Ray-traced passes**: about 400 draws a frame are the ray-tracing structure
   builder, which does not run; rooms without direct light are dark.
3. **Two 64-lane compute programs** still refused on 32-lane NVIDIA hosts
   (`02384a60fda6f2d0`, `2da4beb386509b77`: multi-wave groups with LDS).
4. **Lighting at night and noon**: the sky-following stand-in is untested
   outside the opening's sunset.

Pull requests: one change each, behind a switch, off by default, with what it
was measured to do.

## Credits

A tree of other people's work; ported commits name their source.

- **[VIctory](https://github.com/Marvisak/VIctory)** by Marvisak and its contributors.
- **[fxceless/victory-fork](https://github.com/fxceless/victory-fork)** and **AryaBlue's VIctoryBlue**, the base.
- **[Belobug/VIctory-Belobug-AMD](https://github.com/Belobug/VIctory-Belobug-AMD)**: the ambient stand-in, reflections, sky light.
- **[PastiiJay/klbf-lab](https://github.com/PastiiJay/klbf-lab)** and **[myokex/victory-amd](https://github.com/myokex/victory-amd)**: small-mip detiling, remapped ranges, split dispatches.
- **[avatarmanz/VIctory-7900XTX](https://github.com/avatarmanz/VIctory-7900XTX)**: guard rest, one view opened at a fault, fine pages.
- **[fablinoo/kelvaroq](https://github.com/fablinoo/kelvaroq)**: write watch per reservation, frames ahead.
- **[realblastex/flamingo](https://github.com/realblastex/flamingo)**: the fast path settings and console pipeline sync barrier.
- **[voidopcode/vilocity](https://github.com/voidopcode/vilocity)**: remaining list budget preserved when clamping oversized indirect mesh records.
- Upstream pull requests #1718, #1799, #1997 (Yaekai, Calvix2k), #2146, #3111 (RVZO6), #3124, #3177 (KainanP, RyanCase94, CuteBooty).
- Changes in this fork were made with [Claude Code](https://claude.com/claude-code).

## License

GPL-3.0-or-later, as VIctory. See [LICENSE](LICENSE) and [docs/asset-policy.md](docs/asset-policy.md).
