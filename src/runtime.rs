//! Switch half: version and guard checks, the one inline hook in the
//! character select's world GSP setter that re-sets the pane with the label,
//! and the plugin API v1 exports (`gsp_visualizer_hooked_sites_v1`,
//! `gsp_visualizer_label_v1`). The hook runs on the game's UI thread and
//! allocates nothing; the diagnostic build formats events into a static ring
//! that a writer thread appends to `events.log`.

use crate::glicko::{self, Estimate, Params};
use crate::label;
use crate::model::{self, addr, key, Kind, Line, Settings, Site, ENTRY_PATCH};
use crate::share;
use core::cell::UnsafeCell;
use core::fmt::Write as _;
use skyline::hooks::{getRegionAddress, InlineCtx, Region};
use skyline::nn;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

const DATA_DIR: &str = "sd:/ultimate/gsp-visualizer";
const OPEN_WRITE_APPEND: i32 = 2 | 4;
/// Longest pane text read or written (UTF-16 units, NUL included).
const TEXT_LEN: usize = 256;

/// Counts calls into the heap. Nothing in the plugin is meant to make one.
struct CountingAllocator;
static HEAP_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HEAP_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        HEAP_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        HEAP_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        System.realloc(ptr, layout, size)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Shared cell whose accesses are ordered by the surrounding atomics.
struct Owned<T>(UnsafeCell<T>);
// SAFETY: see each use; only the UI thread touches the label state.
unsafe impl<T> Sync for Owned<T> {}

static TEXT_BASE: AtomicU64 = AtomicU64::new(0);
/// `Settings::to_bits`; diagnostic builds reload it from `diag.txt`.
static SETTINGS: AtomicU64 = AtomicU64::new(0);
/// Set once the label runs (this plugin's hook or a site owner's); the
/// exported label function needs it.
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Set when this plugin installed the hook itself.
static OWNER: AtomicBool = AtomicBool::new(false);
/// Version and guard check result: 0 not run, 1 passed, 2 failed.
static CHECKS: AtomicU8 = AtomicU8::new(0);
/// The text handed to the game; static, so it outlives the call.
static COMPOSED: Owned<[u16; TEXT_LEN]> = Owned(UnsafeCell::new([0; TEXT_LEN]));

fn tick() -> u64 {
    unsafe { nn::os::GetSystemTick() as u64 }
}

fn base() -> u64 {
    TEXT_BASE.load(Ordering::Relaxed)
}

unsafe fn read_u16(address: u64) -> u16 {
    (address as *const u16).read_unaligned()
}

unsafe fn read_u32(address: u64) -> u32 {
    (address as *const u32).read_unaligned()
}

unsafe fn read_u64(address: u64) -> u64 {
    (address as *const u64).read_unaligned()
}

/// The pointer stored at `address`, if `address` and it are non-null.
unsafe fn ptr(address: u64) -> Option<u64> {
    if address == 0 {
        return None;
    }
    Some(read_u64(address)).filter(|p| *p != 0)
}

unsafe fn game<F: Copy>(offset: usize) -> F {
    let code = (base() as usize + offset) as *const ();
    core::mem::transmute_copy(&code)
}

fn settings() -> Settings {
    Settings::from_bits(SETTINGS.load(Ordering::Acquire))
}

/// Everything the label is computed from, kept for the diagnostic log.
#[derive(Clone, Copy, Default)]
struct Inputs {
    slot: i32,
    a: i32,
    b: i32,
    c: i32,
    threshold: i32,
    rule: [u32; 4],
    k_key: u32,
    params: [f32; 3],
    default_b: i32,
    opponent_b: i32,
}

/// The Elite Smash GSP and estimate for `fighter`, from the game's own
/// settings, rating cache and GSP converter. `None` when any of them is
/// missing or Elite Smash is off.
unsafe fn compute(fighter: u64, inputs: &mut Inputs) -> Option<(u32, Estimate)> {
    let get_int: extern "C" fn(u64, u32) -> i32 = game(model::GET_INT_OFFSET);
    let get_float: extern "C" fn(u64, u32) -> f32 = game(model::GET_FLOAT_OFFSET);
    let fighter_slot: extern "C" fn(u64, u64) -> i32 = game(model::FIGHTER_SLOT_OFFSET);
    let to_gsp: extern "C" fn(u64, u32, u32) -> u32 = game(model::TO_GSP_OFFSET);
    let base = base();

    let rating = ptr(ptr(base + addr::RATING as u64)? + 8)?;
    let manager = ptr(rating + addr::RATING_SETTINGS as u64)?;
    if get_int(manager, key::ELITE_ENABLED) & 0xff != 1 {
        return None;
    }
    let threshold = settings()
        .threshold
        .unwrap_or_else(|| get_int(manager, key::ELITE_THRESHOLD));
    inputs.threshold = threshold;
    let rule = [
        u32::from(*((base as usize + addr::RULE) as *const u8)),
        u32::from(*((base as usize + addr::RULE_SECONDS) as *const u16)),
        u32::from(*((base as usize + addr::RULE_A) as *const u8)),
        u32::from(*((base as usize + addr::RULE_B) as *const u8)),
    ];
    inputs.rule = rule;
    let k_key = model::ruleset_key(rule[0] as u8, rule[1] as u16, rule[2] as u8, rule[3] as u8);
    inputs.k_key = k_key.unwrap_or(0);
    let params = Params {
        r0: get_int(manager, key::R0) as f32,
        sigma: get_float(manager, key::VOLATILITY),
        k: k_key.map_or(1.0, |k| get_float(manager, k)),
    };
    inputs.params = [params.r0, params.sigma, params.k];
    let default_b = get_int(manager, key::DEFAULT_DEVIATION);
    inputs.default_b = default_b;
    let usable = |x: f32| x.is_finite() && x > 0.0;
    if threshold <= 0 || default_b <= 0 || !usable(params.sigma) || !usable(params.k) {
        return None;
    }

    let fighters = ptr(ptr(ptr(base + addr::FIGHTERS as u64)? + 8)? + 0x168)?;
    let slot = fighter_slot(fighters, fighter);
    inputs.slot = slot;
    if !(0..addr::SLOTS).contains(&slot) {
        return None;
    }
    let record = ptr(rating + addr::RATING_RECORDS as u64)?
        + (addr::RECORD_BASE + slot as usize * addr::RECORD_STRIDE) as u64;
    let field = |offset: usize| read_u32(record + offset as u64) as i32;
    let (a, b, c) = (
        field(addr::RECORD_A),
        field(addr::RECORD_B),
        field(addr::RECORD_C),
    );
    (inputs.a, inputs.b, inputs.c) = (a, b, c);
    if a <= 0 || b <= 0 || c <= 0 {
        return None;
    }

    let owner = ptr(ptr(ptr(ptr(base + addr::GSP_OWNER as u64)?)? + 0x68)?)?;
    if read_u32(ptr(owner + 8)? + addr::GSP_TABLE_COUNT as u64) == 0 {
        return None;
    }
    let elite_gsp = to_gsp(owner, model::KIND_FIGHTER, threshold as u32);
    let opponent_b = glicko::settled_deviation(&params, default_b);
    inputs.opponent_b = opponent_b;
    Some((
        elite_gsp,
        glicko::estimate(a, b, c, threshold, opponent_b, &params),
    ))
}

/// The world GSP setter after its text call: the one hook, installed only
/// when this plugin owns the site (see `share`). Runs the label, then every
/// callback other plugins registered for the site.
#[skyline::hook(offset = model::APPLY_OFFSET, inline)]
unsafe fn apply(ctx: &mut InlineCtx) {
    label_callback(ctx);
    SHARED.for_each(|callback| {
        let callback: unsafe extern "C" fn(*mut InlineCtx) = core::mem::transmute(callback);
        callback(ctx);
    });
}

/// For the hovered fighter's GSP (called from `.1a19c44`, GSP path) sets the
/// pane again through the game's own setter: the text the game just set, then
/// the label. Every other call passes untouched. Called from this plugin's
/// hook, or from the site owner's when another plugin owns `.1a1c118`.
unsafe extern "C" fn label_callback(ctx: *mut InlineCtx) {
    let ctx = &mut *ctx;
    let info = ctx.registers[19].x();
    if !model::shows_number(read_u32(info), read_u32(info + 0xc)) {
        return;
    }
    let frame = ctx.registers[29].x();
    let returns_to = read_u64(frame + 8).wrapping_sub(base());
    if returns_to != model::HOVER_RETURN_OFFSET as u64 {
        #[cfg(feature = "diag")]
        diag::other_caller(returns_to);
        return;
    }
    // The pane handle object the setter filled at sp (= x29 - 0x20).
    let Some(pane) = ptr(frame - 0x20 + 8).and_then(|handle| ptr(handle + 0x10)) else {
        return;
    };
    let Some(text) = ptr(pane + model::textbox::STRING as u64) else {
        return;
    };
    let units = usize::from(read_u16(pane + model::textbox::LENGTH as u64)).min(TEXT_LEN - 1);
    let original = model::until_nul(core::slice::from_raw_parts(text as *const u16, units));
    let fighter = read_u64(info - 8);
    let mut inputs = Inputs::default();
    let result = compute(fighter, &mut inputs);
    #[cfg(feature = "diag")]
    diag::armed(fighter, info, &inputs, result);
    let Some((gsp, estimate)) = result else {
        return;
    };
    let settings = settings();
    // SAFETY: only the game's UI thread runs the setter; the game's text is
    // copied into `out` before the setter replaces the pane's buffer.
    let out = &mut *COMPOSED.0.get();
    let len = label::compose(
        original,
        settings.joiner(),
        settings.dot(),
        gsp,
        estimate,
        out,
    );
    #[cfg(feature = "diag")]
    diag::composed(original, &out[..len]);
    let set_text: extern "C" fn(u64, *const u16, i32, u32) = game(model::SET_TEXT_OFFSET);
    set_text(pane, out.as_ptr(), len as i32, 0);
}

/// Callbacks other plugins registered for `.1a1c118` (see `share`).
static SHARED: share::Callbacks = share::Callbacks::new();
static SITE_SHARE: share::SiteShareV1 = share::SiteShareV1 {
    abi_version: share::ABI_VERSION,
    offset: model::APPLY_OFFSET as u64,
    register: register_apply,
};

/// Inline-hook sharing v1 for `.1a1c118` (see `share`).
#[no_mangle]
pub extern "C" fn ssbu_inline_site_1a1c118_v1() -> *const share::SiteShareV1 {
    &SITE_SHARE
}

/// Another plugin's callback for `.1a1c118`; accepted when this plugin will
/// hook the site (13.0.5, every site as expected).
unsafe extern "C" fn register_apply(callback: usize) -> bool {
    checks_pass() && SHARED.push(callback)
}

/// Plugin API v1, found by other plugins with `nn::ro::LookupSymbol`: the
/// main-image offsets this plugin has hooked (none unless it owns them), so a
/// plugin can leave them alone or detect gsp-visualizer. Writes up to
/// `capacity` offsets to `out` and returns how many it hooked.
#[no_mangle]
pub unsafe extern "C" fn gsp_visualizer_hooked_sites_v1(out: *mut usize, capacity: usize) -> usize {
    if !OWNER.load(Ordering::Acquire) {
        return 0;
    }
    if !out.is_null() {
        for (i, site) in model::HOOKED_SITES.iter().take(capacity).enumerate() {
            *out.add(i) = *site;
        }
    }
    model::HOOKED_SITES.len()
}

/// Plugin API v1: the label for a fighter (`Elite <GSP> · <n> wins|losses`,
/// UTF-16, NUL-terminated) so another plugin can show it without hooking.
/// Returns the units written before the NUL; 0 when unavailable. Call on the
/// game's UI thread, like the game's own settings and rating reads.
#[no_mangle]
pub unsafe extern "C" fn gsp_visualizer_label_v1(
    fighter: u64,
    out: *mut u16,
    capacity: usize,
) -> usize {
    if !INSTALLED.load(Ordering::Acquire) || out.is_null() || capacity == 0 {
        return 0;
    }
    let out = core::slice::from_raw_parts_mut(out, capacity);
    match compute(fighter, &mut Inputs::default()) {
        Some((gsp, estimate)) => label::compose(&[], &[], settings().dot(), gsp, estimate, out),
        None => {
            out[0] = 0;
            0
        }
    }
}

type Path = Line<96>;

fn path(args: core::fmt::Arguments) -> Path {
    let mut path = Path::new();
    let _ = path.write_fmt(args);
    path
}

/// An `nn::fs` file opened to append; closed on drop. A closed file is on
/// the card at once, while Eden keeps an open file's writes until the host
/// file is closed (its `FlushFile` does not reach the host).
struct NxFile {
    handle: nn::fs::FileHandle,
    offset: i64,
}

impl NxFile {
    fn append(path: &mut Path) -> Option<Self> {
        let path = path.with_nul()?.as_ptr();
        unsafe {
            // Fails harmlessly when the file exists.
            nn::fs::CreateFile(path, 0);
            let mut handle = nn::fs::FileHandle { handle: 0 };
            if nn::fs::OpenFile(&mut handle, path, OPEN_WRITE_APPEND) != 0 {
                return None;
            }
            let mut offset = 0;
            if nn::fs::GetFileSize(&mut offset, handle) != 0 {
                nn::fs::CloseFile(handle);
                return None;
            }
            Some(Self { handle, offset })
        }
    }

    fn write(&mut self, bytes: &[u8]) -> bool {
        let option = nn::fs::WriteOption { flags: 0 };
        let written = unsafe {
            nn::fs::WriteFile(
                self.handle,
                self.offset,
                bytes.as_ptr(),
                bytes.len() as u64,
                &option,
            )
        } == 0;
        if written {
            self.offset += bytes.len() as i64;
        }
        written
    }
}

impl Drop for NxFile {
    fn drop(&mut self) {
        unsafe {
            nn::fs::FlushFile(self.handle);
            nn::fs::CloseFile(self.handle);
        }
    }
}

fn append_log(bytes: &[u8]) {
    if let Some(mut log) = NxFile::append(&mut path(format_args!("{DATA_DIR}/events.log"))) {
        log.write(bytes);
    }
}

/// Eden-only diagnostics: an event ring the hooks format into, a writer
/// thread that appends it to `events.log` and reloads `diag.txt`.
#[cfg(feature = "diag")]
mod diag {
    use super::*;
    use std::sync::atomic::{fence, AtomicU32};

    const EVENT_RING: usize = 16;
    const EVENT_LEN: usize = 512;
    const WRITER_STACK: usize = 32 * 1024;
    const OPEN_READ: i32 = 1;

    /// One formatted line; `seq` is the event index + 1 once it is complete.
    struct EventSlot {
        seq: AtomicU64,
        text: Owned<Line<EVENT_LEN>>,
    }

    static EVENTS: [EventSlot; EVENT_RING] = [const {
        EventSlot {
            seq: AtomicU64::new(0),
            text: Owned(UnsafeCell::new(Line::new())),
        }
    }; EVENT_RING];
    static EVENT_HEAD: AtomicU64 = AtomicU64::new(0);

    /// The writer thread's object and stack, static so no heap is involved.
    #[repr(C, align(4096))]
    struct ThreadStack(UnsafeCell<[u8; WRITER_STACK]>);
    #[repr(C, align(16))]
    struct ThreadObject(UnsafeCell<[u8; 0x1c0]>);
    // SAFETY: handed to nn::os once, at startup, and never touched again.
    unsafe impl Sync for ThreadStack {}
    unsafe impl Sync for ThreadObject {}
    static WRITER_STACK_MEMORY: ThreadStack = ThreadStack(UnsafeCell::new([0; WRITER_STACK]));
    static WRITER_THREAD: ThreadObject = ThreadObject(UnsafeCell::new([0; 0x1c0]));

    pub fn push(args: core::fmt::Arguments) {
        let index = EVENT_HEAD.fetch_add(1, Ordering::AcqRel);
        let slot = &EVENTS[index as usize % EVENT_RING];
        slot.seq.store(0, Ordering::Release);
        // SAFETY: this index owns the slot until it publishes `seq`; the
        // writer re-checks `seq` after copying and drops an overwritten line.
        let text = unsafe { &mut *slot.text.0.get() };
        text.clear();
        let _ = write!(text, "{} ", tick());
        let _ = text.write_fmt(args);
        let _ = text.write_char('\n');
        slot.seq.store(index + 1, Ordering::Release);
    }

    /// Return offsets of other callers that reached the number path,
    /// logged once each.
    static OTHER_SEEN: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
    static OTHER_NEXT: AtomicU32 = AtomicU32::new(0);

    pub fn other_caller(returns_to: u64) {
        if OTHER_SEEN
            .iter()
            .any(|seen| seen.load(Ordering::Relaxed) == returns_to)
        {
            return;
        }
        let slot = OTHER_NEXT.fetch_add(1, Ordering::Relaxed) as usize % OTHER_SEEN.len();
        OTHER_SEEN[slot].store(returns_to, Ordering::Relaxed);
        push(format_args!("diag other_caller returns_to={returns_to:#x}"));
    }

    pub fn armed(fighter: u64, info: u64, inputs: &Inputs, result: Option<(u32, Estimate)>) {
        let i = inputs;
        let shown = unsafe { read_u32(info + 0x10) };
        push(format_args!(
            "diag armed fighter={fighter:#x} shown_gsp={shown} slot={} a={} b={} c={} threshold={} rule={},{},{},{} k_key={} r0={} sigma={} k={} b0={} opp_b={} result={:?}",
            i.slot, i.a, i.b, i.c, i.threshold, i.rule[0], i.rule[1], i.rule[2], i.rule[3],
            i.k_key, i.params[0], i.params[1], i.params[2], i.default_b, i.opponent_b, result
        ));
    }

    fn ascii(units: &[u16], out: &mut Line<200>) {
        for unit in units {
            let c = char::from_u32(u32::from(*unit)).filter(char::is_ascii_graphic);
            let _ = out.write_char(match (*unit, c) {
                (0x0a, _) => '|',
                (0x20, _) => ' ',
                (0xb7, _) => '.',
                (_, Some(c)) => c,
                _ => '?',
            });
        }
    }

    /// Resolves this plugin's exports the way another plugin would and logs
    /// what the sites export returns.
    pub fn check_exports() {
        let lookup = |name: &[u8]| {
            let mut address = 0usize;
            let status = unsafe { nn::ro::LookupSymbol(&mut address, name.as_ptr()) };
            (status, address)
        };
        let (status, sites) = lookup(b"gsp_visualizer_hooked_sites_v1\0");
        let (_, label) = lookup(b"gsp_visualizer_label_v1\0");
        let mut out = [0usize; 4];
        let count = if sites == 0 {
            0
        } else {
            let sites_v1: unsafe extern "C" fn(*mut usize, usize) -> usize =
                unsafe { core::mem::transmute(sites) };
            unsafe { sites_v1(out.as_mut_ptr(), out.len()) }
        };
        push(format_args!(
            "diag exports lookup_status={status} hooked_sites_v1={} label_v1={} count={count} first={:#x}",
            u8::from(sites != 0),
            u8::from(label != 0),
            out[0]
        ));
    }

    pub fn composed(original: &[u16], composed: &[u16]) {
        let (mut before, mut after) = (Line::<200>::new(), Line::<200>::new());
        ascii(original, &mut before);
        ascii(composed, &mut after);
        push(format_args!(
            "diag composed units={}/{} before={} after={}",
            original.len(),
            composed.len(),
            core::str::from_utf8(before.as_bytes()).unwrap_or(""),
            core::str::from_utf8(after.as_bytes()).unwrap_or("")
        ));
    }

    /// `diag.txt` into `buf`; the bytes read (0 when it is missing).
    fn read_diag(buf: &mut [u8; 256]) -> usize {
        let mut file = path(format_args!("{DATA_DIR}/diag.txt"));
        let Some(file) = file.with_nul() else {
            return 0;
        };
        let mut read = 0u64;
        unsafe {
            let mut handle = nn::fs::FileHandle { handle: 0 };
            if nn::fs::OpenFile(&mut handle, file.as_ptr(), OPEN_READ) != 0 {
                return 0;
            }
            let ok =
                nn::fs::ReadFile1(&mut read, handle, 0, buf.as_mut_ptr(), buf.len() as u64) == 0;
            nn::fs::CloseFile(handle);
            if !ok {
                return 0;
            }
        }
        (read as usize).min(buf.len())
    }

    pub fn load_settings() -> Settings {
        let mut buf = [0u8; 256];
        let len = read_diag(&mut buf);
        Settings::parse(&buf[..len])
    }

    fn reload_settings() {
        let loaded = load_settings();
        if SETTINGS.swap(loaded.to_bits(), Ordering::AcqRel) != loaded.to_bits() {
            push(format_args!("diag settings {loaded:?}"));
        }
    }

    /// Copies every published event after `tail` into `events.log`.
    fn drain(tail: &mut u64) {
        let head = EVENT_HEAD.load(Ordering::Acquire);
        if head == *tail {
            return;
        }
        if head - *tail > EVENT_RING as u64 {
            *tail = head - EVENT_RING as u64;
        }
        let Some(mut file) = NxFile::append(&mut path(format_args!("{DATA_DIR}/events.log")))
        else {
            return;
        };
        while *tail < head {
            let slot = &EVENTS[*tail as usize % EVENT_RING];
            let seq = slot.seq.load(Ordering::Acquire);
            if seq == 0 || seq < *tail + 1 {
                break; // still being written
            }
            let mut copy = Line::<EVENT_LEN>::new();
            // SAFETY: read under the sequence check below.
            let text = unsafe { &*slot.text.0.get() };
            let _ = copy.write_str(core::str::from_utf8(text.as_bytes()).unwrap_or(""));
            fence(Ordering::Acquire);
            if seq == *tail + 1 && slot.seq.load(Ordering::Relaxed) == seq {
                file.write(copy.as_bytes());
            }
            *tail += 1;
        }
    }

    extern "C" fn writer_main(_: *mut skyline::libc::c_void) {
        let mut tail = 0;
        let mut round = 0u32;
        loop {
            unsafe {
                nn::os::SleepThread(nn::TimeSpan {
                    nanoseconds: 200_000_000,
                })
            };
            round = round.wrapping_add(1);
            if round.is_multiple_of(10) {
                reload_settings();
            }
            drain(&mut tail);
        }
    }

    pub fn start_writer() -> bool {
        unsafe {
            let thread = WRITER_THREAD.0.get().cast::<nn::os::ThreadType>();
            let priority = nn::os::GetThreadPriority(nn::os::GetCurrentThread());
            let created = nn::os::CreateThread(
                thread,
                writer_main,
                core::ptr::null_mut(),
                WRITER_STACK_MEMORY.0.get().cast::<u8>(),
                WRITER_STACK,
                priority,
            );
            if created != 0 {
                return false;
            }
            nn::os::SetThreadNamePointer(thread, c"gsp-visualizer".as_ptr().cast());
            nn::os::StartThread(thread);
            true
        }
    }
}

/// The site's bytes in the running image, if it lies inside `.text`.
unsafe fn site_bytes(site: &Site) -> Option<&'static [u8]> {
    let base = getRegionAddress(Region::Text) as usize;
    let end = getRegionAddress(Region::Rodata) as usize;
    let address = base + site.offset;
    (address.is_multiple_of(4) && address + site.guard.len() <= end)
        .then(|| core::slice::from_raw_parts(address as *const u8, site.guard.len()))
}

/// The bytes the site's kind requires are the vanilla ones.
unsafe fn guard_matches(site: &Site) -> bool {
    let range = site.checked();
    site_bytes(site).is_some_and(|bytes| bytes[range.clone()] == site.guard[range])
}

/// A called function whose entry another plugin has hooked (calls go
/// through that hook, so both plugins keep working).
unsafe fn entry_hooked(site: &Site) -> bool {
    site.kind == Kind::Called
        && site_bytes(site).is_some_and(|bytes| bytes[..ENTRY_PATCH] != site.guard[..ENTRY_PATCH])
}

fn version_is_supported() -> bool {
    let mut version = nn::oe::DisplayVersion { name: [0; 16] };
    unsafe { nn::oe::GetDisplayVersion(&mut version) };
    let end = version.name.iter().position(|v| *v == 0).unwrap_or(16);
    version.name[..end] == *model::SUPPORTED_VERSION.as_bytes()
}

/// The game version and every site's guard, checked once; also asked by
/// another plugin's registration, which can come before this `main`.
fn checks_pass() -> bool {
    match CHECKS.load(Ordering::Acquire) {
        1 => true,
        2 => false,
        _ => {
            let pass = version_is_supported()
                && model::SITES
                    .iter()
                    .all(|site| unsafe { guard_matches(site) });
            CHECKS.store(if pass { 1 } else { 2 }, Ordering::Release);
            pass
        }
    }
}

/// The `SiteShareV1` table whoever's `ssbu_inline_site_1a1c118_v1` resolves
/// first returns (0 when no plugin exports it).
fn site_owner_table() -> usize {
    let mut export = 0usize;
    unsafe { nn::ro::LookupSymbol(&mut export, b"ssbu_inline_site_1a1c118_v1\0".as_ptr()) };
    if export == 0 {
        return 0;
    }
    let export: extern "C" fn() -> *const share::SiteShareV1 =
        unsafe { core::mem::transmute(export) };
    export() as usize
}

/// Appends the install line to `events.log` and the Skyline log.
fn report_install(report: &mut Line<256>) {
    let _ = write!(
        report,
        " heap_allocations={}",
        HEAP_ALLOCATIONS.load(Ordering::Relaxed)
    );
    skyline::logging::log(core::str::from_utf8(report.as_bytes()).unwrap_or(""));
    let _ = report.write_char('\n');
    for directory in ["sd:/ultimate", DATA_DIR] {
        if let Some(directory) = path(format_args!("{directory}")).with_nul() {
            unsafe { nn::fs::CreateDirectory(directory.as_ptr()) };
        }
    }
    append_log(report.as_bytes());
}

#[skyline::main(name = "gsp-visualizer")]
pub fn main() {
    let mut report = Line::<256>::new();
    let _ = write!(
        report,
        "{} plugin=gsp-visualizer/{}",
        tick(),
        env!("CARGO_PKG_VERSION")
    );
    if !version_is_supported() {
        let _ = write!(report, " result=unsupported_version");
        report_install(&mut report);
        return;
    }
    // Election first (see `share`): when another plugin owns `.1a1c118` its
    // hook has already changed those bytes, so only the owner guards the
    // shared site itself; everything else this plugin relies on is guarded
    // either way.
    let own_table = &SITE_SHARE as *const share::SiteShareV1 as usize;
    let role = share::role(site_owner_table(), own_table);
    let owner = role == share::Role::Owner;
    for site in model::SITES {
        if (owner || site.offset != model::APPLY_OFFSET) && !unsafe { guard_matches(site) } {
            if owner {
                CHECKS.store(2, Ordering::Release);
            }
            let _ = write!(report, " result=guard_mismatch site={}", site.name);
            report_install(&mut report);
            return;
        }
    }
    if owner {
        CHECKS.store(1, Ordering::Release);
    }
    TEXT_BASE.store(
        unsafe { getRegionAddress(Region::Text) as u64 },
        Ordering::Relaxed,
    );
    SETTINGS.store(Settings::DEFAULT.to_bits(), Ordering::Release);
    #[cfg(feature = "diag")]
    {
        let loaded = diag::load_settings();
        SETTINGS.store(loaded.to_bits(), Ordering::Release);
        let _ = write!(report, " diag {loaded:?}");
    }
    // One hook per instruction: install it when this plugin owns `.1a1c118`,
    // otherwise run the label from the owner's hook.
    match role {
        share::Role::Owner => {
            skyline::install_hooks!(apply);
            OWNER.store(true, Ordering::Release);
            INSTALLED.store(true, Ordering::Release);
            let _ = write!(
                report,
                " result=installed role=owner hooks=apply guests={}",
                SHARED.len()
            );
        }
        share::Role::Guest(table) => {
            let api = unsafe { &*(table as *const share::SiteShareV1) };
            if share::compatible(api, model::APPLY_OFFSET)
                && unsafe { (api.register)(label_callback as *const () as usize) }
            {
                INSTALLED.store(true, Ordering::Release);
                let _ = write!(report, " result=installed role=guest owner={table:#x}");
            } else {
                let _ = write!(report, " result=owner_refused owner={table:#x}");
                report_install(&mut report);
                return;
            }
        }
    }
    for site in model::SITES {
        if unsafe { entry_hooked(site) } {
            let _ = write!(report, " entry_hooked={}", site.name);
        }
    }
    report_install(&mut report);
    #[cfg(feature = "diag")]
    diag::check_exports();
    #[cfg(feature = "diag")]
    if !diag::start_writer() {
        report.clear();
        let _ = write!(report, "{} writer=not_started", tick());
        report_install(&mut report);
    }
}
