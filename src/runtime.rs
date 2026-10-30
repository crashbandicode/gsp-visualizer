//! Switch half: version and guard checks, the arm/disarm inline hooks on the
//! character select's world GSP setter and the pane text hook that appends
//! the label. Hooks run on the game's UI thread and allocate nothing; the
//! diagnostic build formats events into a static ring that a writer thread
//! appends to `events.log`.

use crate::glicko::{self, Estimate, Params};
use crate::label;
use crate::model::{self, addr, key, Line, Settings, Site};
use core::cell::UnsafeCell;
use core::fmt::Write as _;
use skyline::hooks::{getRegionAddress, InlineCtx, Region};
use skyline::nn;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

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
/// The pane whose next text gets the label; 0 when disarmed.
static ARMED_PANE: AtomicU64 = AtomicU64::new(0);
/// The label for the armed pane: Elite Smash GSP, and the estimate as
/// `count | 1 << 31` for losses to drop.
static LABEL_GSP: AtomicU32 = AtomicU32::new(0);
static LABEL_ESTIMATE: AtomicU32 = AtomicU32::new(0);
/// The text handed to the game; static, so it outlives the call.
static COMPOSED: Owned<[u16; TEXT_LEN]> = Owned(UnsafeCell::new([0; TEXT_LEN]));

fn tick() -> u64 {
    unsafe { nn::os::GetSystemTick() as u64 }
}

fn base() -> u64 {
    TEXT_BASE.load(Ordering::Relaxed)
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

/// Number path of the world GSP setter. Arms the pane text hook when the
/// setter was called for the hovered fighter (`.1a19c44`).
#[skyline::hook(offset = model::ARM_OFFSET, inline)]
unsafe fn arm(ctx: &mut InlineCtx) {
    ARMED_PANE.store(0, Ordering::Release);
    let frame = ctx.registers[29].x();
    let returns_to = read_u64(frame + 8).wrapping_sub(base());
    if returns_to != model::HOVER_RETURN_OFFSET as u64 {
        #[cfg(feature = "diag")]
        diag::other_caller(returns_to);
        return;
    }
    // The pane handle object the setter fills at sp (= x29 - 0x20).
    let Some(pane) = ptr(frame - 0x20 + 8).and_then(|handle| ptr(handle + 0x10)) else {
        return;
    };
    let fighter = read_u64(ctx.registers[19].x() - 8);
    let mut inputs = Inputs::default();
    let result = compute(fighter, &mut inputs);
    #[cfg(feature = "diag")]
    diag::armed(fighter, ctx.registers[19].x(), &inputs, result);
    let Some((gsp, estimate)) = result else {
        return;
    };
    LABEL_GSP.store(gsp, Ordering::Relaxed);
    LABEL_ESTIMATE.store(
        match estimate {
            Estimate::Reach(n) => n,
            Estimate::Keep(n) => n | 1 << 31,
        },
        Ordering::Relaxed,
    );
    ARMED_PANE.store(pane, Ordering::Release);
}

/// After the setter's text call, on both paths.
#[skyline::hook(offset = model::DISARM_OFFSET, inline)]
unsafe fn disarm(_ctx: &mut InlineCtx) {
    ARMED_PANE.store(0, Ordering::Release);
}

/// The text of a set-text call: `units` long (or up to its NUL when
/// negative), cut at a NUL and at `TEXT_LEN - 1`.
unsafe fn text_at<'a>(text: *const u16, units: i32) -> &'a [u16] {
    if text.is_null() {
        return &[];
    }
    let max = if units < 0 {
        TEXT_LEN - 1
    } else {
        (units as usize).min(TEXT_LEN - 1)
    };
    model::until_nul(core::slice::from_raw_parts(text, max))
}

/// Every pane text. The armed pane's text gets the label appended.
#[skyline::hook(offset = model::SET_TEXT_OFFSET)]
unsafe fn set_text(pane: u64, text: *const u16, units: i32, flags: u32) {
    if pane == 0 || ARMED_PANE.load(Ordering::Acquire) != pane {
        return call_original!(pane, text, units, flags);
    }
    ARMED_PANE.store(0, Ordering::Release);
    let estimate = match LABEL_ESTIMATE.load(Ordering::Relaxed) {
        word if word >> 31 != 0 => Estimate::Keep(word & !(1 << 31)),
        word => Estimate::Reach(word),
    };
    let settings = settings();
    let original = text_at(text, units);
    // SAFETY: only the UI thread arms, and the pane is set on that thread.
    let out = &mut *COMPOSED.0.get();
    let len = label::compose(
        original,
        settings.joiner(),
        settings.dot(),
        LABEL_GSP.load(Ordering::Relaxed),
        estimate,
        out,
    );
    #[cfg(feature = "diag")]
    diag::composed(original, &out[..len]);
    call_original!(pane, out.as_ptr(), len as i32, flags)
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
    use std::sync::atomic::fence;

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

unsafe fn guard_matches(site: &Site) -> bool {
    let base = getRegionAddress(Region::Text) as usize;
    let end = getRegionAddress(Region::Rodata) as usize;
    let address = base + site.offset;
    address.is_multiple_of(4)
        && address + site.guard.len() <= end
        && core::slice::from_raw_parts(address as *const u8, site.guard.len()) == site.guard
}

fn version_is_supported() -> bool {
    let mut version = nn::oe::DisplayVersion { name: [0; 16] };
    unsafe { nn::oe::GetDisplayVersion(&mut version) };
    let end = version.name.iter().position(|v| *v == 0).unwrap_or(16);
    version.name[..end] == *model::SUPPORTED_VERSION.as_bytes()
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
    for site in model::SITES {
        if !unsafe { guard_matches(site) } {
            let _ = write!(report, " result=guard_mismatch site={}", site.name);
            report_install(&mut report);
            return;
        }
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
    skyline::install_hooks!(arm, disarm, set_text);
    let _ = write!(report, " result=installed hooks=arm,disarm,set_text");
    report_install(&mut report);
    #[cfg(feature = "diag")]
    if !diag::start_writer() {
        report.clear();
        let _ = write!(report, "{} writer=not_started", tick());
        report_install(&mut report);
    }
}
