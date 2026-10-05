//! The sync loop. `ptr-sync` (initial sync) and the app (refresh) share it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use super::client::{decode, Client};
use super::db::PtrDb;
use super::update::{parse, Update};

/// Progress of one applied index.
pub struct Progress {
    pub index: u64,
    pub last_index: u64,
    pub files: usize,
    pub bytes: usize,
}

/// Apply all new updates. A download thread fetches and parses the next
/// indexes while this thread writes the current index. Returns the number of
/// indexes applied. Stops early when `cancel` is set. Pass `stop_at` to apply
/// only up to that index (for tests). Pass `max_bytes_per_day` to limit the
/// average download rate. The download thread sleeps when it is ahead.
pub fn sync(
    client: Client,
    db: &mut PtrDb,
    cancel: &AtomicBool,
    stop_at: Option<u64>,
    max_bytes_per_day: Option<u64>,
    mut on_progress: impl FnMut(&Progress),
) -> Result<usize, String> {
    let since = (db.last_index().map_err(|e| e.to_string())? + 1) as u64;
    let meta = client.metadata(since)?;
    let next_due = meta.next_update_due;
    let mut entries: Vec<_> = meta.entries.into_iter().filter(|e| e.index >= since).collect();
    if let Some(s) = stop_at {
        entries.retain(|e| e.index <= s);
    }
    let last_index = entries.last().map(|e| e.index).unwrap_or(0);

    // A small bound keeps the memory use low.
    let (tx, rx) = mpsc::sync_channel::<Result<(u64, Vec<Update>, usize, usize), String>>(4);
    let dl_cancel = std::sync::Arc::new(AtomicBool::new(false));
    let dl_flag = dl_cancel.clone();
    let worker = std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let mut total: u64 = 0;
        for e in entries {
            if dl_flag.load(Ordering::Relaxed) {
                return;
            }
            let mut ups = Vec::with_capacity(e.hashes.len());
            let mut bytes = 0;
            let res = (|| {
                for h in &e.hashes {
                    let raw = client.update_raw(h)?;
                    bytes += raw.len();
                    total += raw.len() as u64;
                    if let Some(limit) = max_bytes_per_day.filter(|l| *l > 0) {
                        // Time that `total` bytes may use at the limit.
                        let allowed = total as f64 / limit as f64 * 86400.0;
                        let ahead = allowed - start.elapsed().as_secs_f64();
                        // Sleep in short steps, so a cancel acts fast.
                        let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(ahead.max(0.0));
                        while std::time::Instant::now() < until && !dl_flag.load(Ordering::Relaxed) {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                        }
                    }
                    ups.push(parse(&decode(&raw)?)?);
                }
                Ok::<_, String>(())
            })();
            let msg = res.map(|_| (e.index, std::mem::take(&mut ups), e.hashes.len(), bytes));
            let failed = msg.is_err();
            if tx.send(msg).is_err() || failed {
                return;
            }
        }
    });

    let mut applied = 0;
    let mut result = Ok(());
    for msg in rx.iter() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match msg {
            Ok((index, ups, files, bytes)) => {
                if let Err(e) = db.apply_index(index, next_due, ups) {
                    result = Err(format!("index {index}: {e}"));
                    break;
                }
                applied += 1;
                on_progress(&Progress { index, last_index, files, bytes });
            }
            Err(e) => {
                result = Err(e);
                break;
            }
        }
    }
    dl_cancel.store(true, Ordering::Relaxed);
    drop(rx);
    let _ = worker.join();
    result.map(|_| applied)
}
