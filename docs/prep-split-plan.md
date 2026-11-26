# Splitting submission preparation (`VICTORY_PREP_SPLIT`): plan

Status: plan only, no code. Written after run batch36 (2026-11-26) for the
user to decide on. Source read: play/aa2 at 0ef6a1e6c (= play/full plus the
opt-in switches of that branch); `host.rs` (`Host::submit`,
`execute_late`), `queue.rs` (read only), `descriptor.rs` (`sync_heaps`),
`shadow.rs`, `watch.rs`.

## 1. Why: what limits a frame now

Run batch36 (launcher settings, profiled, no capture), the framelog's
usual frame late in the run:

| | per frame |
|---|---|
| frame | 137 ms (6.9-7.2 fps over batches 32-36) |
| our submitting thread (`victory-submit`, `queue::worker`) | at work 108.7 ms, 8 submissions, preparing 101.9 ms |
| the card | busy about 78 ms (title lists 70 ms), idle 59 ms, **49 ms of it waiting for us** |
| relay thread (copy-back) | 49.7 ms landing, 55 ms waiting for the card: beside us, not in the chain |
| recording threads | about 50 % busy |
| the title's own submitting thread | waiting about 96 % (held back by frames ahead) |

The profiler's "submit" group is two threads: the title's (waiting) and
ours. Per thread, `Host::submit` is about 78 % of ours. So **one thread's
serial preparation is the frame**, and the card idles waiting for it.

Incremental cuts of the parts of preparation are exhausted: every item
left is 2-9 % of that thread (section 3), and halving all of them gains
about 10-15 ms (about 8 fps). The next gain has to come from doing
preparation's parts **at the same time**.

Ceiling: with preparation no longer the limit, the frame becomes the card's
time, about 78-90 ms: **about 11-12 fps on this laptop (RTX 4050)**, if
nothing else surfaces. Past that, the card's own load (resolution,
scaling) is the lever.

## 2. What one submission's preparation is now

`execute_late` -> `Host::submit(runs, late = true)`, all of it under
`self.submitted` (one submission at a time "from preparation to
copy-back: snapshots and the ranges shaders write are per host copy, not
per submission"). In order, with each stage's share of our thread in
batch36 (per thread, i.e. the profile's figure doubled):

| # | Stage | Reads | Writes | Share |
|---|---|---|---|---|
| S0 | order against flights: `copies_after_landing`, `late_blocked` -> `relay_wait`/`settle` | the line of flights (`line.written`), the lists' declared reads | the line | about 0.3 % (waits) |
| S1 | `descriptor::sync_heaps`: records the title copied into its shader-visible heaps as bytes | title heap memory (write watch through `watch::Tracked`, then `read_into`), `shadow::find` for buffer views | host descriptor heaps (`HostHeap::mirror*`, the `made` lock shared with the recording threads) | about 9.5 % (13 ms) |
| S2 | `sweep` (only with nothing in the air) | | frees | small |
| S3 | `descriptor::make_waiting` | texture host copies | descriptor slots | small |
| S4 | `shadow::preparing(n)` | | the submission number for the copies' declared ranges | 0 |
| S5 | `watch::ask_once(ranges)`: one write-watch asking for every range the lists refresh (and staged textures) | kernel write watch, guards | the journal (`JOURNAL`: `written`, `ahead`, `ahead_asked`, pieces) | about 6.5 % (9 ms) |
| S6 | `stale_under_flights` -> `Shadow::merge_under_writes` (title bytes put under the flights' declared shader writes; `card_masked` for card copies) | the journal, title memory, each copy's snapshot/CPU side under its `before` lock | each copy's reference bytes, card notes (`card.masked`, shared with the relay) | about 18.5 % (25 ms) |
| S7 | `run_deferred` per list into a `Batch`; `batch.flush()`: the refreshes (`Shadow::update`: `watch::writes`, `copy_stale_fine`; byte moves wide with `WIDE_COPIES`) | the journal, guards, title memory | the copies' bytes and stamps (`copied`), upload memory | about 18.5 % (25 ms) |
| S8 | `take_snapshots` | copies | snapshots of ranges shaders may write | about 1.3 % |
| S9 | `card_before`: changed runs of card copies sent up, hash/compare list | card copies' CPU sides | upload lists | about 2-4 % |
| S10 | `ExecuteCommandLists` + `Signal`; the flight joins the line | | the line, the fence | small |

The rest of the thread: `queue::worker`'s own bookkeeping and waiting for
work (about 10 %).

## 3. Two designs

### A. Fan-out inside one submission (recommended first)

Keep "one submission at a time". Run the stages of that one submission
that touch disjoint state at the same time, on the wide threads (four,
about 3.5 % busy in batch36):

- **A1. S1 (heap sync) beside S5-S7 (the copies).** S1 writes only host
  descriptor heaps; S5-S7 write only copies, the journal and upload
  memory. S1 must be done before S10 (the lists index those slots), not
  before S5-S7. A wide thread starts S1 right after S0, and S10 waits for
  it. Hidden: up to 13 ms.
- **A2. S6 per copy, wide.** `merge_under_writes` of different copies
  touches different `before` locks and bytes. Shared: the journal (its
  answers come from `cached()` after S5's asking), the card notes per
  card, `runs_left` (a budget across copies; give each copy its share or
  take it atomically). 25 ms over four or five threads: about 15-18 ms
  saved.
- **Prerequisite, A0: the journal lock.** batch34 showed what happens when
  several threads use `watch::writes`: 6.2 % of our thread and 12.8 % of
  the wide threads waiting on `JOURNAL`, because `writes()` holds it
  across the guards' and the kernel's askings. A1 adds a second user (the
  heap's `Tracked::take`) and A2 several. So first: `writes()` takes the
  journal only to look up and to note, never across `guard()` or
  `kernel_pages()`. Asking numbers stay ordered: a number is taken under
  the lock before asking, and its answer is noted under the lock after
  (the 7900XTX fork's `journal()`/`journal_mut()` split, `next_asking`).
  This also makes `VICTORY_WIDE_REFRESH` worth measuring again.

Expected: preparation about 102 -> 70-75 ms, frame about 137 -> 105-110
ms: **about 9-9.5 fps.** Risk: moderate and local (no change to what
is prepared, only to which thread does it, and when).

### B. Two submissions in preparation at once (the 7900XTX fork's "what's next" #1)

"Split what one submission does over two threads in a row; ask for the
next submission while this one is sent." Thread A runs S0-S5 of
submission n+1 while thread B runs S6-S10 of submission n.

What it needs that we do not have:

- **Per-submission state where it is per copy now.** The `submitted`
  lock exists because a copy's snapshot (`before`), its declared shader
  ranges (`pending`) and its stamps (`copied`) assume one submission in
  preparation. n+1's S5 may run while n's S7 refreshes the same copy.
  Each would need a submission tag, or a rule that n+1's stages touching
  a copy wait for n's S7 on that copy.
- **The journal's single `ahead` slot.** n+1's asking replaces n's
  `ahead`/`ahead_asked` while n's refreshes still read `cached()`. The
  answer is newer, so refreshes stay correct if each uses the number it
  gets back. Needs per-submission ahead entries and a test.
- **Order against flights (S0).** n+1's `late_blocked` must see n as a
  flight before n is sent. n+1 waits for n's S10 before its own S6, or n
  is put on the line (as "being sent") at the end of its S5.

Expected: close to the ceiling if the halves balance (S0-S5 about 16 %,
S6-S10 about 40 %: they do not; S7 or S6 would have to move into the
first half, which brings back the per-copy problem). Realistically
**about 10-11 fps** on top of A. Risk: high (state model changes, subtle
ordering, rare wrong frames).

## 4. Recommendation and order of work

1. **A0** journal lock split (`VICTORY_JOURNAL_SPLIT=1`), measured alone
   and with `VICTORY_WIDE_REFRESH=1`.
2. **A1** heap sync beside the copies (`VICTORY_PREP_SPLIT=1`).
3. **A2** merges per copy on the wide threads (`VICTORY_PREP_SPLIT=2`,
   includes 1).
4. Measure. Only if preparation is still the frame by a clear margin:
   design B as its own plan, with a reviewer before code.

Each one is opt-in, off by default, one commit each, as with everything
else on this branch.

## 5. Invariants to keep (what a review checks)

- Everything a list reads is prepared before S10 sends it: S1's slots,
  S7's copies, S8's snapshots, S9's uploads. With A1 that becomes a join
  before S10, and it must also hold on every early return and settle
  path of `submit`.
- S6 before S7 for the same copy (the merge puts the title's bytes into
  the reference that the refresh and the later compare use). Fan-out is
  across copies, never across stages of one copy.
- Asking numbers stay totally ordered, and a page's journal entry never
  goes back to an older number (A0).
- `relay` and `card_masked` notes: the relay thread takes the same
  `before`/notes locks. Per-copy fan-out must not hold two copies' locks
  at once, in any order.
- `runs_left` (`settle_why(2)`): the budget across copies stays a budget:
  divide it, or take it atomically, so the fan-out cannot overspend and
  skip a settle that is needed.
- The guards' lock order is unchanged (`GUARDS` then nothing; never
  `JOURNAL` held while taking `GUARDS` after A0).
- With every switch off, behaviour is exactly as today.

## 6. Failure modes to look for

- A heap slot mirrored after the lists that index it are sent: a wrong or
  stale texture for one frame (flicker, a wrong material). Check: the
  `S1` join counts, and the existing "slots made null/neutral" counters.
- A merge or refresh on a wide thread racing the relay's landing on the
  same copy: the card's bytes taken for the title's (wrong geometry or
  physics, e.g. the ocean case of the 7900XTX fork's findings).
- Journal: a refresh that misses a page because a newer asking replaced
  the cached answer: a stale copy (visible as stale geometry, caught by
  `VICTORY_REFRESH_CHECK=1`).
- Contention moved, not removed: wide threads waiting on `made` (S1 vs
  the recording threads) or on the copies' `before` locks.
- A deadlock between the submit thread joining a wide job and a wide job
  waiting for a lock the submit thread holds. Rule: the submit thread
  holds no lock that a wide job takes while it waits for the join
  (today's `shadow::wide` users already follow this).

## 7. Test strategy

- **Unit tests** (CPU only, `watch::` / `shadow::` test modules):
  - A0: two threads asking overlapping ranges; every page written is in
    one of the answers and numbers are ordered; the existing guard/journal
    tests unchanged.
  - A2: the merge of N copies in parallel gives the same bytes and notes
    as in series (copies with overlapping flight ranges, a card copy, a
    snapshot copy); `runs_left` exhausted mid-way still settles.
  - A negative check per fix, as on this branch: set the fix aside, see
    the test fail.
- **In-game runs** (same scene, `run_ztrx.sh`, `NOCAPTURE=1`):
  - each step alone, then together; fps, `preparing`, the card's "idle
    behind us", the relay line, the settle reasons;
  - `VICTORY_REFRESH_CHECK=1` once per step (slow, checks every refresh
    against title memory);
  - a 20-30 minute session on the final set (the 7900XTX fork's lost
    write showed up in 6 of 200 stress runs);
  - one profiled run per step: did the time move off the submit thread,
    and did contention appear elsewhere.
- **Review:** a fresh reviewer on A0 and A2 before they go into the
  launcher (they touch ordering).

## 8. Size

| Step | Files | Size | Risk |
|---|---|---|---|
| A0 journal split | `watch.rs` | about 150-250 lines | medium (ordering) |
| A1 heap sync beside | `host.rs` (`submit`), `descriptor.rs` | about 60-100 | low-medium |
| A2 merges wide | `host.rs` (`stale_under_flights`), `shadow.rs` | about 120-200 | medium |
| B two in preparation | `host.rs`, `shadow.rs`, `watch.rs`, maybe `queue.rs` | 600+ | high |

`queue.rs` is not touched by A0-A2.
