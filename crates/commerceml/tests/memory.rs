//! Streaming keeps memory use constant: the peak heap size while streaming
//! a large file does not depend on the file size.

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use commerceml::stream::{Event, StreamReader};
use commerceml::synth::Config;
use commerceml::{ParseOptions, WriteOptions};

/// Counts live heap bytes and remembers the peak.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: delegates to the system allocator and only adds bookkeeping.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            if new_size > layout.size() {
                let live =
                    LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed) + new_size - layout.size();
                PEAK.fetch_max(live, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Peak heap growth while running `f`.
fn peak_during(f: impl FnOnce()) -> usize {
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    f();
    PEAK.load(Ordering::Relaxed) - base
}

fn stream_count(path: &std::path::Path) -> usize {
    let mut n = 0;
    for event in StreamReader::open(path, ParseOptions::default()).unwrap() {
        if let Event::Product(_) = event.unwrap() {
            n += 1;
        }
    }
    n
}

#[test]
fn streaming_memory_does_not_grow_with_file_size() {
    let dir = std::env::temp_dir();
    let small = dir.join(format!("commerceml-mem-small-{}.xml", std::process::id()));
    let large = dir.join(format!("commerceml-mem-large-{}.xml", std::process::id()));
    for (path, products) in [(&small, 1_000), (&large, 15_000)] {
        let cfg = Config {
            products,
            ..Config::default()
        };
        let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        cfg.write_import(file, &WriteOptions::default()).unwrap();
    }
    let large_size = std::fs::metadata(&large).unwrap().len();
    assert!(
        large_size > 40_000_000,
        "the large file should be > 40 MB, got {large_size}"
    );

    let mut counted = (0, 0);
    let small_peak = peak_during(|| counted.0 = stream_count(&small));
    let large_peak = peak_during(|| counted.1 = stream_count(&large));
    let full_peak = peak_during(|| {
        let parsed = commerceml::from_path(&large).unwrap();
        assert_eq!(parsed.info.products().len(), 15_000);
    });
    std::fs::remove_file(&small).unwrap();
    std::fs::remove_file(&large).unwrap();

    assert_eq!(counted, (1_000, 15_000));
    eprintln!(
        "peak heap: stream small {small_peak} B, stream large {large_peak} B ({large_size} B file), full parse {full_peak} B"
    );
    assert!(large_peak < 2 * 1024 * 1024, "streaming peak {large_peak} B");
    assert!(
        large_peak < small_peak + 512 * 1024,
        "{large_peak} vs {small_peak}"
    );
    assert!(full_peak > 20 * large_peak);
}
