//! Title bytes put under flights' shader writes into a copy in the card's
//! memory by one masked pass (`VICTORY_MERGE_MASKED=1`, off by default).
//!
//! `Shadow::merge_under_writes` sent each run of changed bytes up as a copy
//! of its own, at most `VICTORY_MERGE_RUNS` (512) a submission; past that
//! the flights were settled. vf-all-3 had 13,000-54,000 runs of a few bytes
//! a frame, and 75 ms a frame of the submitting thread went on settles for
//! "title memory changed under shader writes". Here each changed dword is
//! an entry (offset, byte mask) and one dispatch puts the masked bytes of
//! the CPU's side into the card's side: the bytes around them, which the
//! flights' shaders may have written, stay as they are.

use std::sync::OnceLock;

use windows::Win32::Graphics::Direct3D12::*;

/// Entries one copy takes a submission at most; past it the flights settle.
pub(crate) const MOST: usize = 1 << 22;

/// `VICTORY_MERGE_MASKED=1`.
pub(crate) fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_MERGE_MASKED").is_some_and(|v| v == "1"))
}

/// The entries for `runs` (offset, bytes) of a slice at copy offset `at`:
/// per dword of the copy that holds a changed byte, its offset and a mask
/// of 0xff for each changed byte, in order.
pub(crate) fn dword_masks(at: usize, runs: &[(usize, usize)]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    dword_masks_into(at, runs, &mut out);
    out
}

/// [`dword_masks`], appended to `out`.
pub(crate) fn dword_masks_into(at: usize, runs: &[(usize, usize)], out: &mut Vec<(u32, u32)>) {
    for &(r, n) in runs {
        // a dword at a time, not a byte at a time
        let (mut byte, end) = (at + r, at + r + n);
        while byte < end {
            let word = byte & !3;
            let upto = (word + 4).min(end);
            let mut bits = 0u32;
            for b in byte..upto {
                bits |= 0xffu32 << ((b & 3) * 8);
            }
            match out.last_mut() {
                Some(last) if last.0 == word as u32 => last.1 |= bits,
                _ => out.push((word as u32, bits)),
            }
            byte = upto;
        }
    }
}

/// The bytes the entries put, as runs (offsets), touching ones joined: for
/// the relay's notes of what went up and what is still to go, which must
/// hold no byte the pass leaves alone.
pub(crate) fn byte_runs(entries: &[(u32, u32)]) -> Vec<(usize, usize)> {
    // sorted by dword, not byte by byte: the same runs for a quarter of the sort
    let mut words = entries.to_vec();
    words.sort_unstable_by_key(|e| e.0);
    // a dword noted twice: one mask, so its bytes come in order
    words.dedup_by(|later, kept| {
        let same = later.0 == kept.0;
        if same {
            kept.1 |= later.1;
        }
        same
    });
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (word, mask) in words {
        for b in 0..4usize {
            if mask >> (b * 8) & 0xff == 0 {
                continue;
            }
            let at = word as usize + b;
            match out.last_mut() {
                Some(last) if at <= last.1 => last.1 = last.1.max(at + 1),
                _ => out.push((at, at + 1)),
            }
        }
    }
    out
}

/// Whether the relay's notes of masked bytes are kept per send, sorted, and
/// looked up by range (`VICTORY_MERGE_MASKED_INDEX=1`).
///
/// Why: fast-2 put 581k changed runs a frame through the masked pass; each
/// went into the copy's `sent` list as a byte run, and every piece the relay
/// merged (1,100-1,400 a frame) scanned that whole list and re-sorted the
/// dwords still to go up byte by byte. The relay's landing time grew from
/// 74-91 ms (vf-all-3) to 100-165 ms.
pub(crate) fn indexed() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let on = std::env::var_os("VICTORY_MERGE_MASKED_INDEX").is_some_and(|v| v == "1");
        if on {
            tracing::info!("the relay's notes of masked bytes are kept per send and looked up by range (VICTORY_MERGE_MASKED_INDEX=1)");
        }
        on
    })
}

/// A copy's notes for the masked pass: the dwords still to go up, and with
/// [`indexed`] the byte runs each send took (send number, sorted runs).
#[derive(Default)]
pub(crate) struct Notes {
    pub(crate) pending: Vec<(u32, u32)>,
    pub(crate) sent: Vec<(u64, Group)>,
}

/// A send's masked dwords as it took them, made into sorted byte runs by
/// the relay the first time it looks (not on the submitting thread: 9.8 ms
/// a frame there in s2-prep-1).
pub(crate) enum Group {
    Taken(Vec<(u32, u32)>),
    Runs(Vec<(usize, usize)>),
}

impl Notes {
    /// Send `sent_as` takes what is pending; with [`indexed`] its dwords are
    /// kept as a group, and the groups every fetch still to merge came after
    /// go.
    pub(crate) fn send(&mut self, sent_as: u64, fetched: u64) -> Vec<(u32, u32)> {
        // With `merge_buffers`: the next submission's notes start with room
        // for as many as this one's (taken whole, they grew from nothing
        // again every submission, reallocating as they went).
        let room = if crate::shadow::merge_buffers() { self.pending.len() } else { 0 };
        let pending = std::mem::replace(&mut self.pending, Vec::with_capacity(room));
        self.sent.retain(|g| g.0 > fetched);
        if !pending.is_empty() {
            self.sent.push((sent_as, Group::Taken(pending.clone())));
        }
        pending
    }

    /// The masked bytes in `[from, to)` that are the title's for a fetch sent
    /// after send `after`: those still to go up and those a later send took.
    pub(crate) fn skip(
        &mut self,
        from: usize,
        to: usize,
        after: u64,
        out: &mut Vec<(usize, usize)>,
    ) {
        let near: Vec<(u32, u32)> = self
            .pending
            .iter()
            .copied()
            .filter(|&(w, _)| (w as usize) < to && w as usize + 4 > from)
            .collect();
        out.extend(byte_runs(&near));
        for (s, group) in &mut self.sent {
            if *s <= after {
                continue;
            }
            if let Group::Taken(taken) = group {
                *group = Group::Runs(byte_runs(taken));
            }
            let Group::Runs(runs) = group else {
                continue;
            };
            let first = runs.partition_point(|r| r.1 <= from);
            out.extend(runs[first..].iter().take_while(|r| r.0 < to).copied());
        }
    }
}

/// Ranges (offsets) the entries' dwords cover, near ones joined: for the
/// hashes of the pieces they touch (hashing more is harmless).
pub(crate) fn spans(entries: &[(u32, u32)]) -> Vec<(usize, usize)> {
    let mut words: Vec<usize> = entries.iter().map(|e| e.0 as usize).collect();
    words.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for w in words {
        match out.last_mut() {
            Some(last) if w <= last.1 + 4096 => last.1 = last.1.max(w + 4),
            _ => out.push((w, w + 4)),
        }
    }
    out
}

pub(crate) const SCATTER: &str = r#"
#define RS "RootConstants(num32BitConstants=1, b0), SRV(t0), SRV(t1), UAV(u0)"
cbuffer C : register(b0) { uint count; };
ByteAddressBuffer staging : register(t0);
ByteAddressBuffer entries : register(t1);
RWByteAddressBuffer card : register(u0);
[RootSignature(RS)]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    if (id.x >= count) return;
    uint2 e = entries.Load2(id.x * 8u);
    card.Store(e.x, (card.Load(e.x) & ~e.y) | (staging.Load(e.x) & e.y));
}
"#;

pub(crate) struct Program {
    root: ID3D12RootSignature,
    pso: ID3D12PipelineState,
}

// SAFETY: D3D12 root signatures and pipeline states are free-threaded.
unsafe impl Send for Program {}
// SAFETY: as above.
unsafe impl Sync for Program {}

impl Program {
    pub(crate) fn make(device: &ID3D12Device) -> windows::core::Result<Program> {
        use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
        use windows::Win32::Graphics::Direct3D::ID3DBlob;
        let mut code: Option<ID3DBlob> = None;
        // SAFETY: the source and names live through the call.
        unsafe {
            D3DCompile(
                SCATTER.as_ptr().cast(),
                SCATTER.len(),
                windows::core::s!("card masked merge"),
                None,
                None,
                windows::core::s!("main"),
                windows::core::s!("cs_5_1"),
                0,
                0,
                &mut code,
                None,
            )?;
        }
        let code = code.ok_or_else(windows::core::Error::empty)?;
        // SAFETY: the blob's own bytes; the root signature is inside it.
        unsafe {
            let bytes = std::slice::from_raw_parts(
                code.GetBufferPointer().cast::<u8>(),
                code.GetBufferSize(),
            );
            let root: ID3D12RootSignature = device.CreateRootSignature(0, bytes)?;
            let pso: ID3D12PipelineState =
                device.CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                    pRootSignature: std::mem::transmute_copy(&root),
                    CS: D3D12_SHADER_BYTECODE {
                        pShaderBytecode: bytes.as_ptr().cast(),
                        BytecodeLength: bytes.len(),
                    },
                    ..Default::default()
                })?;
            Ok(Program { root, pso })
        }
    }

    /// Records the pass: `count` entries at `entries`, bytes from `staging`
    /// into `card`, which is in the unordered-access state.
    ///
    /// # Safety
    /// An open list; live buffers; the entries' offsets lie in both.
    pub(crate) unsafe fn record(
        &self,
        list: &ID3D12GraphicsCommandList,
        staging: u64,
        entries: u64,
        card: u64,
        count: u32,
    ) {
        // SAFETY: as the caller promises.
        unsafe {
            list.SetComputeRootSignature(&self.root);
            list.SetPipelineState(&self.pso);
            list.SetComputeRoot32BitConstants(0, 1, std::ptr::from_ref(&count).cast(), 0);
            list.SetComputeRootShaderResourceView(1, staging);
            list.SetComputeRootShaderResourceView(2, entries);
            list.SetComputeRootUnorderedAccessView(3, card);
            list.Dispatch(count.div_ceil(64), 1, 1);
        }
    }
}

/// The host's program, made once; `None` if it cannot be.
pub(crate) fn program(device: &ID3D12Device) -> Option<&'static Program> {
    static PROGRAM: OnceLock<Option<Program>> = OnceLock::new();
    PROGRAM
        .get_or_init(|| match Program::make(device) {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(error = %e, "no masked merge for copies in the card's memory: its program could not be made");
                None
            }
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_cover_the_changed_bytes_and_only_them() {
        let was: Vec<u8> = (0..300u32).map(|i| (i * 13 + 5) as u8).collect();
        for at in [0usize, 4, 8192] {
            for changes in [
                vec![],
                vec![0],
                vec![299],
                vec![7, 8, 9],
                vec![7, 9],
                vec![15, 16, 17, 23, 24, 64, 65, 66, 67, 298, 299],
            ] {
                let mut now = was.clone();
                for &c in &changes {
                    now[c] ^= 0x41;
                }
                let runs = crate::shadow::changed_runs(&now, &was);
                let entries = dword_masks(at, &runs);
                let mut put = was.clone();
                let mut masked = 0;
                for (word, mask) in &entries {
                    for b in 0..4 {
                        if mask >> (b * 8) & 0xff != 0 {
                            let i = *word as usize + b - at;
                            put[i] = now[i];
                            masked += 1;
                        }
                    }
                }
                assert_eq!(put, now);
                assert_eq!(masked, changes.len());
                assert!(entries.windows(2).all(|w| w[0].0 < w[1].0));
            }
        }
    }

    /// The runs as the byte-by-byte sort made them.
    fn byte_runs_one_by_one(entries: &[(u32, u32)]) -> Vec<(usize, usize)> {
        let mut bytes: Vec<usize> = entries
            .iter()
            .flat_map(|&(word, mask)| {
                (0..4usize)
                    .filter(move |b| mask >> (b * 8) & 0xff != 0)
                    .map(move |b| word as usize + b)
            })
            .collect();
        bytes.sort_unstable();
        let mut out: Vec<(usize, usize)> = Vec::new();
        for b in bytes {
            match out.last_mut() {
                Some(last) if b <= last.1 => last.1 = last.1.max(b + 1),
                _ => out.push((b, b + 1)),
            }
        }
        out
    }

    /// The entries as the byte-at-a-time loop made them.
    fn dword_masks_one_by_one(at: usize, runs: &[(usize, usize)]) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for &(r, n) in runs {
            for byte in at + r..at + r + n {
                let word = (byte & !3) as u32;
                let bit = 0xffu32 << ((byte & 3) * 8);
                match out.last_mut() {
                    Some(last) if last.0 == word => last.1 |= bit,
                    _ => out.push((word, bit)),
                }
            }
        }
        out
    }

    #[test]
    fn masks_a_dword_at_a_time_are_those_a_byte_at_a_time() {
        let mut seed = 0x2545_f491u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as usize
        };
        for _ in 0..300 {
            let at = next() % 9;
            let mut runs = Vec::new();
            let mut r = next() % 7;
            for _ in 0..next() % 12 {
                let n = 1 + next() % 13;
                runs.push((r, n));
                r += n + next() % 6;
            }
            assert_eq!(
                dword_masks(at, &runs),
                dword_masks_one_by_one(at, &runs),
                "{at} {runs:?}"
            );
        }
    }

    #[test]
    fn runs_sorted_by_dword_are_those_sorted_by_byte() {
        let mut seed = 0x9e37_79b9u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..200 {
            let entries: Vec<(u32, u32)> = (0..next() % 40)
                .map(|_| ((next() % 64) * 4, next() & 0xff00_ff0f))
                .collect();
            assert_eq!(byte_runs(&entries), byte_runs_one_by_one(&entries));
        }
    }

    /// What the relay leaves out of a piece, looked up per send, is what the
    /// whole list of byte runs gave.
    #[test]
    fn notes_per_send_skip_what_the_full_list_skipped() {
        let mut notes = Notes::default();
        let mut flat: Vec<(usize, usize, u64)> = Vec::new();
        for (send, words) in [
            (5u64, [0u32, 8, 64]),
            (6, [4, 200, 400]),
            (7, [12, 64, 1000]),
        ] {
            notes.pending = words.iter().map(|&w| (w, 0x00ff_00ff)).collect();
            flat.extend(
                byte_runs(&notes.pending)
                    .into_iter()
                    .map(|(a, b)| (a, b, send)),
            );
            notes.send(send, 4);
        }
        notes.pending = vec![(96, 0xffff_0000), (2000, 0xff)];
        for (from, to, after) in [
            (0, 2048, 4),
            (0, 70, 5),
            (60, 210, 6),
            (100, 999, 7),
            (0, 100, 6),
        ] {
            let mut got = Vec::new();
            notes.skip(from, to, after, &mut got);
            let mut want: Vec<(usize, usize)> = byte_runs(&notes.pending)
                .into_iter()
                .chain(flat.iter().filter(|s| s.2 > after).map(|s| (s.0, s.1)))
                .filter(|&(a, b)| a < to && b > from)
                .collect();
            let bytes = |runs: &mut Vec<(usize, usize)>| {
                let mut on = vec![false; to - from];
                for &(a, b) in runs.iter() {
                    for x in a.max(from)..b.min(to) {
                        on[x - from] = true;
                    }
                }
                on
            };
            assert_eq!(
                bytes(&mut got),
                bytes(&mut want),
                "{from}..{to} after {after}"
            );
        }
        // send 5 is dropped once every fetch came after it
        notes.send(8, 5);
        assert!(notes.sent.iter().all(|g| g.0 > 5));
    }

    #[test]
    fn byte_runs_hold_only_masked_bytes() {
        assert_eq!(
            byte_runs(&[(0, 0x0000_ff00), (4, 0xffff_0000), (8, 0x0000_00ff)]),
            vec![(1, 2), (6, 9)]
        );
        assert_eq!(byte_runs(&[]), vec![]);
    }

    #[test]
    fn spans_join_near_words() {
        assert_eq!(spans(&[(8, 0xff), (0, 0xff00)]), vec![(0, 12)]);
        assert_eq!(
            spans(&[(0, 0xff), (65536, 0xff)]),
            vec![(0, 4), (65536, 65540)]
        );
    }

    /// On the software adapter: the pass leaves every unmasked byte of the
    /// card's side as the "shaders" wrote it.
    #[test]
    fn the_pass_puts_only_masked_bytes() {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
        use windows::Win32::Graphics::Dxgi::*;
        use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
        use windows::core::Interface;
        const LEN: usize = 64 << 10;
        // SAFETY: plain WARP device, buffers and one list, waited for.
        unsafe {
            let Ok(factory) = CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_FLAGS(0))
            else {
                return;
            };
            let Ok(adapter) = factory.EnumWarpAdapter::<IDXGIAdapter>() else {
                return;
            };
            let mut device: Option<ID3D12Device> = None;
            if D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut device).is_err() {
                return;
            }
            let device = device.unwrap();
            let shared = |size: usize| -> (ID3D12Resource, *mut u8) {
                let heap = D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_CUSTOM,
                    CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_WRITE_BACK,
                    MemoryPoolPreference: D3D12_MEMORY_POOL_L0,
                    ..Default::default()
                };
                let desc = D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                    Width: size as u64,
                    Height: 1,
                    DepthOrArraySize: 1,
                    MipLevels: 1,
                    SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                    Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                    ..Default::default()
                };
                let mut buffer: Option<ID3D12Resource> = None;
                device
                    .CreateCommittedResource(
                        &heap,
                        D3D12_HEAP_FLAG_NONE,
                        &desc,
                        D3D12_RESOURCE_STATE_COMMON,
                        None,
                        &mut buffer,
                    )
                    .unwrap();
                let buffer = buffer.unwrap();
                let mut data = std::ptr::null_mut();
                buffer.Map(0, None, Some(&mut data)).unwrap();
                (buffer, data.cast())
            };
            let (card, card_data) = shared(LEN);
            let (staging, staging_data) = shared(LEN);
            let shaders: Vec<u8> = (0..LEN).map(|i| (i * 7 + 1) as u8).collect();
            let title: Vec<u8> = (0..LEN).map(|i| (i * 11 + 3) as u8 | 0x80).collect();
            std::ptr::copy_nonoverlapping(shaders.as_ptr(), card_data, LEN);
            std::ptr::copy_nonoverlapping(title.as_ptr(), staging_data, LEN);
            let runs = vec![(0usize, 1usize), (5, 2), (17, 9), (4093, 6), (60000, 1)];
            let entries = dword_masks(0, &runs);
            let (table, table_data) = shared(entries.len() * 8);
            for (i, (w, m)) in entries.iter().enumerate() {
                table_data.cast::<u32>().add(i * 2).write(*w);
                table_data.cast::<u32>().add(i * 2 + 1).write(*m);
            }
            let program = Program::make(&device).expect("the pass compiles");
            let queue: ID3D12CommandQueue = device
                .CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                    Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                    ..Default::default()
                })
                .unwrap();
            let allocator: ID3D12CommandAllocator = device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                .unwrap();
            let list: ID3D12GraphicsCommandList = device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
                .unwrap();
            program.record(
                &list,
                staging.GetGPUVirtualAddress(),
                table.GetGPUVirtualAddress(),
                card.GetGPUVirtualAddress(),
                entries.len() as u32,
            );
            list.Close().unwrap();
            queue.ExecuteCommandLists(&[Some(list.cast().unwrap())]);
            let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
            queue.Signal(&fence, 1).unwrap();
            let event: HANDLE = CreateEventW(None, false, false, None).unwrap();
            fence.SetEventOnCompletion(1, event).unwrap();
            WaitForSingleObject(event, INFINITE);
            let out = std::slice::from_raw_parts(card_data, LEN);
            let mut want = shaders.clone();
            for &(r, n) in &runs {
                want[r..r + n].copy_from_slice(&title[r..r + n]);
            }
            assert!(
                out == want.as_slice(),
                "only the masked bytes are the title's"
            );
        }
    }
}
